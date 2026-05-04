/// Yomie Agent — DLP Policy Enforcer
///
/// Evaluates USB devices and file operations against server-defined
/// policies (whitelist/blacklist). Reports violations and optionally
/// blocks disallowed devices.

use super::usb_monitor::UsbDevice;

/// A DLP policy downloaded from the server.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct DlpPolicy {
    pub id: u64,
    pub name: String,
    pub description: String,
    pub enabled: bool,
    pub rules: Vec<PolicyRule>,
    pub updated_at: String,
}

/// A single rule within a policy.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct PolicyRule {
    pub rule_type: PolicyRuleType,
    pub action: PolicyAction,
    /// Filter criteria
    pub filter: RuleFilter,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub enum PolicyRuleType {
    /// Allow or block USB devices by VID/PID/serial/class
    UsbDevice,
    /// Allow or block file operations on certain drives/paths
    FileOperation,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub enum PolicyAction {
    Allow,
    Block,
    LogOnly,
}

/// Criteria for matching devices or file operations.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct RuleFilter {
    /// USB VID pattern (exact or wildcard "*")
    pub vid: Option<String>,
    /// USB PID pattern
    pub pid: Option<String>,
    /// USB serial number pattern
    pub serial: Option<String>,
    /// Device class filter (e.g. "mass_storage")
    pub device_class: Option<String>,
    /// Drive type filter ("removable", "fixed", "network")
    pub drive_type: Option<String>,
    /// File extension filter (e.g. ".exe,.msi")
    pub file_extensions: Option<String>,
}

/// Result of evaluating a device or operation against policies.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct PolicyResult {
    pub allowed: bool,
    pub action: String,
    pub matched_policy: Option<String>,
    pub matched_rule_index: Option<usize>,
    pub reason: String,
}

/// Evaluates items against a set of DLP policies.
pub struct PolicyEnforcer {
    policies: Vec<DlpPolicy>,
    /// URL of the Yomie server for policy sync
    server_url: String,
    device_id: String,
}

impl PolicyEnforcer {
    pub fn new(server_url: String, device_id: String) -> Self {
        Self {
            policies: Vec::new(),
            server_url,
            device_id,
        }
    }

    /// Replace all policies (e.g. after a server sync).
    pub fn set_policies(&mut self, policies: Vec<DlpPolicy>) {
        self.policies = policies.into_iter().filter(|p| p.enabled).collect();
    }

    /// Get current policy count.
    pub fn policy_count(&self) -> usize {
        self.policies.len()
    }

    /// Evaluate whether a USB device is allowed.
    pub fn evaluate_usb(&self, device: &UsbDevice) -> PolicyResult {
        for policy in &self.policies {
            for (idx, rule) in policy.rules.iter().enumerate() {
                if !matches!(rule.rule_type, PolicyRuleType::UsbDevice) {
                    continue;
                }

                if matches_usb_filter(&rule.filter, device) {
                    return PolicyResult {
                        allowed: matches!(rule.action, PolicyAction::Allow),
                        action: format_action(&rule.action),
                        matched_policy: Some(policy.name.clone()),
                        matched_rule_index: Some(idx),
                        reason: format!(
                            "USB device {}:{} matched rule #{} in policy '{}'",
                            device.vid, device.pid, idx, policy.name
                        ),
                    };
                }
            }
        }

        // Default: allow if no rule matches
        PolicyResult {
            allowed: true,
            action: "allow".into(),
            matched_policy: None,
            matched_rule_index: None,
            reason: "No matching policy rule".into(),
        }
    }

    /// Evaluate whether a file operation is allowed.
    pub fn evaluate_file_op(
        &self,
        filename: &str,
        drive_type: &str,
    ) -> PolicyResult {
        for policy in &self.policies {
            for (idx, rule) in policy.rules.iter().enumerate() {
                if !matches!(rule.rule_type, PolicyRuleType::FileOperation) {
                    continue;
                }

                if matches_file_filter(&rule.filter, filename, drive_type) {
                    return PolicyResult {
                        allowed: matches!(rule.action, PolicyAction::Allow),
                        action: format_action(&rule.action),
                        matched_policy: Some(policy.name.clone()),
                        matched_rule_index: Some(idx),
                        reason: format!(
                            "File '{}' on {} drive matched rule #{} in policy '{}'",
                            filename, drive_type, idx, policy.name
                        ),
                    };
                }
            }
        }

        PolicyResult {
            allowed: true,
            action: "allow".into(),
            matched_policy: None,
            matched_rule_index: None,
            reason: "No matching policy rule".into(),
        }
    }

    /// Fetch latest policies from the server.
    pub async fn sync_policies(&mut self) -> Result<usize, String> {
        let url = format!("{}/api/bd/dlp-policies", self.server_url);

        let client = reqwest::Client::new();
        let resp = client
            .get(&url)
            .header("X-Device-Id", &self.device_id)
            .send()
            .await
            .map_err(|e| format!("Policy sync request failed: {}", e))?;

        if !resp.status().is_success() {
            return Err(format!("Policy sync failed with status {}", resp.status()));
        }

        let policies: Vec<DlpPolicy> = resp
            .json()
            .await
            .map_err(|e| format!("Failed to parse policies: {}", e))?;

        let count = policies.len();
        self.set_policies(policies);
        Ok(count)
    }

    /// Report a policy violation to the server.
    pub async fn report_violation(&self, result: &PolicyResult, context: &str) -> Result<(), String> {
        let url = format!("{}/api/bd/dlp-events", self.server_url);

        let payload = serde_json::json!({
            "device_id": self.device_id,
            "action": result.action,
            "allowed": result.allowed,
            "policy_name": result.matched_policy,
            "reason": result.reason,
            "context": context,
            "timestamp": std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis() as u64,
        });

        let client = reqwest::Client::new();
        let resp = client
            .post(&url)
            .header("X-Device-Id", &self.device_id)
            .json(&payload)
            .send()
            .await
            .map_err(|e| format!("Violation report failed: {}", e))?;

        if resp.status().is_success() {
            Ok(())
        } else {
            Err(format!("Violation report failed with status {}", resp.status()))
        }
    }
}

// ---------------------------------------------------------------------------
//  Filter matching
// ---------------------------------------------------------------------------

fn matches_usb_filter(filter: &RuleFilter, device: &UsbDevice) -> bool {
    if let Some(ref vid) = filter.vid {
        if vid != "*" && !device.vid.eq_ignore_ascii_case(vid) {
            return false;
        }
    }
    if let Some(ref pid) = filter.pid {
        if pid != "*" && !device.pid.eq_ignore_ascii_case(pid) {
            return false;
        }
    }
    if let Some(ref serial) = filter.serial {
        if serial != "*" {
            match &device.serial {
                Some(s) if s.eq_ignore_ascii_case(serial) => {}
                _ => return false,
            }
        }
    }
    if let Some(ref cls) = filter.device_class {
        if cls != "*" && !device.device_class.eq_ignore_ascii_case(cls) {
            return false;
        }
    }
    true
}

fn matches_file_filter(filter: &RuleFilter, filename: &str, drive_type: &str) -> bool {
    if let Some(ref dt) = filter.drive_type {
        if dt != "*" && !drive_type.eq_ignore_ascii_case(dt) {
            return false;
        }
    }
    if let Some(ref exts) = filter.file_extensions {
        let lower = filename.to_lowercase();
        let ext_list: Vec<&str> = exts.split(',').map(|e| e.trim()).collect();
        if !ext_list.iter().any(|e| lower.ends_with(&e.to_lowercase())) {
            return false;
        }
    }
    true
}

fn format_action(action: &PolicyAction) -> String {
    match action {
        PolicyAction::Allow => "allow".into(),
        PolicyAction::Block => "block".into(),
        PolicyAction::LogOnly => "log_only".into(),
    }
}
