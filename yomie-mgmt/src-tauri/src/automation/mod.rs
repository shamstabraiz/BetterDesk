/// Yomie Agent — Automation module
///
/// Handles remote command execution and script running
/// triggered by the admin console.

pub mod command_channel;
pub mod script_runner;

pub use command_channel::CommandChannel;
pub use script_runner::ScriptRunner;
