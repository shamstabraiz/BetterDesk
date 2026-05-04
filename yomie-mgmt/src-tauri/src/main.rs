// Show console window only when --console flag is NOT present.
// This allows runtime monitoring via terminal when needed.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    let args: Vec<String> = std::env::args().collect();

    // On Windows release builds, re-attach to parent console if --console flag is present.
    #[cfg(all(windows, not(debug_assertions)))]
    if args.iter().any(|a| a == "--console") {
        unsafe {
            // AttachConsole(ATTACH_PARENT_PROCESS) — re-attach to the calling terminal
            windows_sys::Win32::System::Console::AttachConsole(
                windows_sys::Win32::System::Console::ATTACH_PARENT_PROCESS,
            );
        }
    }

    // Handle elevated CLI commands (--apply-config, --add-firewall-rules).
    // These are invoked by the main process via UAC / pkexec and must
    // exit before Tauri starts.
    betterdesk_mgmt_lib::service::handle_apply_config_cli();

    // Pass args awareness to lib::run() via env check
    let _ = &args; // suppress unused warning in non-windows builds

    betterdesk_mgmt_lib::run();
}
