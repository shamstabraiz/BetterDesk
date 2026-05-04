//! Terminal session management for CDAP.
//!
//! Spawns cmd.exe or powershell.exe child processes, relays I/O to/from
//! the CDAP gateway via WebSocket.

use anyhow::{Context, Result};
use log::{debug, info};
use std::process::Stdio;
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::process::{Child, Command};
use tokio::sync::Mutex;

use super::connection::CdapSender;
use super::protocol::CdapMessage;

/// An active terminal session backed by a child process.
pub struct TerminalSession {
    session_id: String,
    child: Child,
    stdin_tx: tokio::sync::mpsc::Sender<String>,
    output_task: tokio::task::JoinHandle<()>,
}

impl TerminalSession {
    /// Start a new terminal session.
    ///
    /// `shell` may be "powershell", "pwsh", "cmd", or a full path.
    pub async fn start(
        session_id: String,
        shell: &str,
        cols: u16,
        rows: u16,
        sender: Arc<Mutex<CdapSender>>,
    ) -> Result<Self> {
        let (program, args) = resolve_shell(shell);

        info!(
            "CDAP-term: Starting {} for session {} ({}x{})",
            program, session_id, cols, rows
        );

        let mut child = Command::new(&program)
            .args(&args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .creation_flags(0x08000000) // CREATE_NO_WINDOW
            .spawn()
            .context(format!("Spawn {} failed", program))?;

        let stdout = child.stdout.take().context("No stdout")?;
        let stderr = child.stderr.take().context("No stderr")?;

        let (stdin_tx, mut stdin_rx) = tokio::sync::mpsc::channel::<String>(256);

        // Forward stdin
        let mut stdin_handle = child.stdin.take().context("No stdin")?;
        tokio::spawn(async move {
            while let Some(data) = stdin_rx.recv().await {
                if stdin_handle.write_all(data.as_bytes()).await.is_err() {
                    break;
                }
            }
        });

        // Forward stdout + stderr → CDAP via channel merge
        let (out_tx, mut out_rx) = tokio::sync::mpsc::channel::<Vec<u8>>(256);

        // Spawn stdout reader
        let out_tx2 = out_tx.clone();
        tokio::spawn(async move {
            let mut stdout = stdout;
            let mut buf = vec![0u8; 4096];
            loop {
                match stdout.read(&mut buf).await {
                    Ok(0) | Err(_) => break,
                    Ok(n) => {
                        if out_tx2.send(buf[..n].to_vec()).await.is_err() {
                            break;
                        }
                    }
                }
            }
        });

        // Spawn stderr reader
        tokio::spawn(async move {
            let mut stderr = stderr;
            let mut buf = vec![0u8; 4096];
            loop {
                match stderr.read(&mut buf).await {
                    Ok(0) | Err(_) => break,
                    Ok(n) => {
                        if out_tx.send(buf[..n].to_vec()).await.is_err() {
                            break;
                        }
                    }
                }
            }
        });

        // Merge and send to CDAP
        let sid = session_id.clone();
        let output_task = tokio::spawn(async move {
            while let Some(data) = out_rx.recv().await {
                let text = String::from_utf8_lossy(&data).to_string();
                let msg = CdapMessage::terminal_output(&sid, &text);
                let mut tx = sender.lock().await;
                if tx.send_message(&msg).await.is_err() {
                    break;
                }
            }

            // Send terminal_end
            let msg = CdapMessage::terminal_end(&sid, None);
            let mut tx = sender.lock().await;
            let _ = tx.send_message(&msg).await;
        });

        Ok(Self {
            session_id,
            child,
            stdin_tx,
            output_task,
        })
    }

    /// Write user input to the terminal.
    pub async fn write_input(&self, data: &str) -> Result<()> {
        self.stdin_tx
            .send(data.into())
            .await
            .context("Terminal stdin send failed")?;
        Ok(())
    }

    /// Resize terminal (no-op on Windows without ConPTY).
    pub fn resize(&mut self, _cols: u16, _rows: u16) -> Result<()> {
        // ConPTY resize would go here for full PTY support
        debug!(
            "CDAP-term: Resize request for {} (not supported without ConPTY)",
            self.session_id
        );
        Ok(())
    }

    /// Kill the terminal process.
    pub async fn kill(&mut self) -> Result<()> {
        info!("CDAP-term: Killing session {}", self.session_id);
        let _ = self.child.kill().await;
        self.output_task.abort();
        Ok(())
    }
}

/// Resolve shell name to (program, args).
fn resolve_shell(shell: &str) -> (String, Vec<String>) {
    match shell.to_lowercase().as_str() {
        "powershell" | "ps" => (
            "powershell.exe".into(),
            vec![
                "-NoLogo".into(),
                "-NoProfile".into(),
                "-NonInteractive".into(),
            ],
        ),
        "pwsh" => (
            "pwsh.exe".into(),
            vec!["-NoLogo".into(), "-NoProfile".into()],
        ),
        "cmd" => ("cmd.exe".into(), vec!["/Q".into()]),
        other => {
            // Could be a full path
            (other.into(), vec![])
        }
    }
}
