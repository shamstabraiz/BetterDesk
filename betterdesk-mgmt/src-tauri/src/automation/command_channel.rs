/// Yomie Agent — Command Channel
///
/// Polls the Yomie server for pending remote commands and
/// executes them using the ScriptRunner.  Results are uploaded
/// back to the server.

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::time::Duration;

use super::script_runner::{CommandType, ScriptRunner};

/// A remote command received from the server.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemoteCommand {
    pub id: i64,
    pub device_id: String,
    pub command_type: String,
    pub payload: String,
    pub status: String,
    pub created_at: String,
}

/// Response from the server when fetching pending commands.
#[derive(Debug, Deserialize)]
struct PendingResponse {
    commands: Vec<RemoteCommand>,
}

/// Request body for submitting command results.
#[derive(Debug, Serialize)]
struct ResultPayload {
    status: String,
    result: String,
}

pub struct CommandChannel {
    base_url: String,
    device_id: String,
    client: reqwest::Client,
    poll_interval: Duration,
}

impl CommandChannel {
    pub fn new(base_url: &str, device_id: &str) -> Self {
        Self {
            base_url: base_url.trim_end_matches('/').to_string(),
            device_id: device_id.to_string(),
            client: reqwest::Client::builder()
                .timeout(Duration::from_secs(30))
                .build()
                .unwrap_or_default(),
            poll_interval: Duration::from_secs(15),
        }
    }

    /// Poll the server for pending commands.
    pub async fn fetch_pending(&self) -> Result<Vec<RemoteCommand>> {
        let url = format!("{}/api/bd/commands", self.base_url);
        let resp = self
            .client
            .get(&url)
            .header("X-Device-Id", &self.device_id)
            .send()
            .await?;

        if !resp.status().is_success() {
            anyhow::bail!("Server returned {}", resp.status());
        }

        let body: PendingResponse = resp.json().await?;
        Ok(body.commands)
    }

    /// Submit the result of a command execution back to the server.
    pub async fn submit_result(
        &self,
        command_id: i64,
        status: &str,
        result: &str,
    ) -> Result<()> {
        let url = format!(
            "{}/api/bd/commands/{}/result",
            self.base_url, command_id
        );

        let payload = ResultPayload {
            status: status.to_string(),
            result: result.to_string(),
        };

        let resp = self
            .client
            .post(&url)
            .header("X-Device-Id", &self.device_id)
            .json(&payload)
            .send()
            .await?;

        if !resp.status().is_success() {
            anyhow::bail!("Submit failed: {}", resp.status());
        }

        Ok(())
    }

    /// Background loop: poll → execute → report.
    /// Call this via `tokio::spawn`.
    pub async fn run_loop(self) {
        log::info!(
            "[CommandChannel] Started polling for {} every {:?}",
            self.device_id,
            self.poll_interval
        );

        loop {
            match self.fetch_pending().await {
                Ok(commands) => {
                    for cmd in commands {
                        log::info!(
                            "[CommandChannel] Executing command #{}: {} ({})",
                            cmd.id,
                            cmd.command_type,
                            cmd.payload.chars().take(60).collect::<String>()
                        );

                        // Mark as running
                        let _ = self.submit_result(cmd.id, "running", "").await;

                        // Execute
                        let cmd_type = CommandType::from_str(&cmd.command_type);
                        match ScriptRunner::execute(&cmd_type, &cmd.payload) {
                            Ok(result) => {
                                let output = format!(
                                    "exit_code: {}\n--- stdout ---\n{}\n--- stderr ---\n{}",
                                    result.exit_code, result.stdout, result.stderr
                                );
                                let status = if result.exit_code == 0 {
                                    "completed"
                                } else {
                                    "failed"
                                };
                                let _ = self.submit_result(cmd.id, status, &output).await;
                                log::info!(
                                    "[CommandChannel] Command #{} {}: exit {}",
                                    cmd.id,
                                    status,
                                    result.exit_code
                                );
                            }
                            Err(err) => {
                                let _ = self
                                    .submit_result(
                                        cmd.id,
                                        "failed",
                                        &format!("Execution error: {}", err),
                                    )
                                    .await;
                                log::error!(
                                    "[CommandChannel] Command #{} execution error: {}",
                                    cmd.id,
                                    err
                                );
                            }
                        }
                    }
                }
                Err(err) => {
                    log::debug!("[CommandChannel] Poll error: {}", err);
                }
            }

            tokio::time::sleep(self.poll_interval).await;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_remote_command_deserialize() {
        let json = r#"{
            "id": 1,
            "device_id": "ABC123",
            "command_type": "shell",
            "payload": "echo hi",
            "status": "pending",
            "created_at": "2026-01-01T00:00:00Z"
        }"#;
        let cmd: RemoteCommand = serde_json::from_str(json).unwrap();
        assert_eq!(cmd.id, 1);
        assert_eq!(cmd.command_type, "shell");
    }
}
