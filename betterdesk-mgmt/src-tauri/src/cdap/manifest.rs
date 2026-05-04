//! Dynamic CDAP manifest builder for desktop device type.
//!
//! Generates 30+ widgets covering system telemetry, management,
//! terminal, file browser, desktop, clipboard, and security.

use sysinfo::{Disks, System};

use super::protocol::{
    AlertDef, DeviceDescriptor, DeviceManifest, TableColumn, Widget,
};
use super::CdapConfig;

/// Build the full CDAP manifest for this desktop machine.
pub fn build_manifest(config: &CdapConfig) -> DeviceManifest {
    let mut sys = System::new_all();
    sys.refresh_all();

    let _hostname = System::host_name().unwrap_or_else(|| "Unknown".into());
    let os_name = System::name().unwrap_or_else(|| "Windows".into());
    let os_version = System::os_version().unwrap_or_default();
    let arch = std::env::consts::ARCH;
    let _total_mem_gb = sys.total_memory() as f64 / 1_073_741_824.0;

    let disks = Disks::new_with_refreshed_list();
    let _total_disk_gb: f64 = disks
        .iter()
        .map(|d| d.total_space() as f64 / 1_073_741_824.0)
        .sum();

    let device = DeviceDescriptor {
        id: config.device_id.clone(),
        name: config.device_name.clone(),
        device_type: "desktop".into(),
        firmware: Some(env!("CARGO_PKG_VERSION").into()),
        model: Some(format!(
            "{} {} ({})",
            os_name, os_version, arch
        )),
        manufacturer: Some("Yomie".into()),
        tags: Some(vec!["windows".into(), "desktop".into()]),
    };

    let mut capabilities = vec!["telemetry".into(), "commands".into()];
    if config.enable_terminal {
        capabilities.push("terminal".into());
    }
    if config.enable_file_browser {
        capabilities.push("file_transfer".into());
    }
    if config.enable_clipboard {
        capabilities.push("clipboard".into());
    }
    if config.enable_remote_desktop {
        capabilities.push("remote_desktop".into());
    }

    let mut widgets = Vec::new();

    // ── System Telemetry Gauges ──────────────────────────────────────
    widgets.push(Widget {
        id: "sys_cpu".into(),
        widget_type: "gauge".into(),
        label: "CPU Usage".into(),
        group: Some("System".into()),
        unit: Some("%".into()),
        min: Some(0.0),
        max: Some(100.0),
        readonly: Some(true),
        icon: Some("cpu".into()),
        ..Default::default()
    });

    widgets.push(Widget {
        id: "sys_memory".into(),
        widget_type: "gauge".into(),
        label: "Memory Usage".into(),
        group: Some("System".into()),
        unit: Some("%".into()),
        min: Some(0.0),
        max: Some(100.0),
        readonly: Some(true),
        icon: Some("memory".into()),
        ..Default::default()
    });

    widgets.push(Widget {
        id: "sys_disk".into(),
        widget_type: "gauge".into(),
        label: "Disk Usage".into(),
        group: Some("System".into()),
        unit: Some("%".into()),
        min: Some(0.0),
        max: Some(100.0),
        readonly: Some(true),
        icon: Some("storage".into()),
        ..Default::default()
    });

    widgets.push(Widget {
        id: "sys_network_tx".into(),
        widget_type: "gauge".into(),
        label: "Network TX".into(),
        group: Some("System".into()),
        unit: Some("KB/s".into()),
        min: Some(0.0),
        max: Some(125_000.0), // ~1 Gbps
        readonly: Some(true),
        icon: Some("upload".into()),
        ..Default::default()
    });

    widgets.push(Widget {
        id: "sys_network_rx".into(),
        widget_type: "gauge".into(),
        label: "Network RX".into(),
        group: Some("System".into()),
        unit: Some("KB/s".into()),
        min: Some(0.0),
        max: Some(125_000.0),
        readonly: Some(true),
        icon: Some("download".into()),
        ..Default::default()
    });

    // ── System Info Text Widgets ─────────────────────────────────────
    widgets.push(Widget {
        id: "sys_hostname".into(),
        widget_type: "text".into(),
        label: "Hostname".into(),
        group: Some("Info".into()),
        readonly: Some(true),
        icon: Some("computer".into()),
        ..Default::default()
    });

    widgets.push(Widget {
        id: "sys_os".into(),
        widget_type: "text".into(),
        label: "Operating System".into(),
        group: Some("Info".into()),
        readonly: Some(true),
        icon: Some("info".into()),
        ..Default::default()
    });

    widgets.push(Widget {
        id: "sys_uptime".into(),
        widget_type: "text".into(),
        label: "Uptime".into(),
        group: Some("Info".into()),
        readonly: Some(true),
        icon: Some("schedule".into()),
        ..Default::default()
    });

    widgets.push(Widget {
        id: "sys_total_ram".into(),
        widget_type: "text".into(),
        label: "Total RAM".into(),
        group: Some("Info".into()),
        readonly: Some(true),
        icon: Some("memory".into()),
        ..Default::default()
    });

    widgets.push(Widget {
        id: "sys_total_disk".into(),
        widget_type: "text".into(),
        label: "Total Disk".into(),
        group: Some("Info".into()),
        readonly: Some(true),
        icon: Some("storage".into()),
        ..Default::default()
    });

    widgets.push(Widget {
        id: "sys_arch".into(),
        widget_type: "text".into(),
        label: "Architecture".into(),
        group: Some("Info".into()),
        readonly: Some(true),
        ..Default::default()
    });

    widgets.push(Widget {
        id: "sys_ip_address".into(),
        widget_type: "text".into(),
        label: "IP Address".into(),
        group: Some("Info".into()),
        readonly: Some(true),
        icon: Some("lan".into()),
        ..Default::default()
    });

    // ── Action Buttons ───────────────────────────────────────────────
    widgets.push(Widget {
        id: "btn_restart".into(),
        widget_type: "button".into(),
        label: "Restart".into(),
        group: Some("Power".into()),
        icon: Some("restart_alt".into()),
        confirm: Some("Are you sure you want to restart this computer?".into()),
        dangerous: Some(true),
        ..Default::default()
    });

    widgets.push(Widget {
        id: "btn_shutdown".into(),
        widget_type: "button".into(),
        label: "Shutdown".into(),
        group: Some("Power".into()),
        icon: Some("power_settings_new".into()),
        confirm: Some("Are you sure you want to shut down this computer?".into()),
        dangerous: Some(true),
        ..Default::default()
    });

    widgets.push(Widget {
        id: "btn_lock".into(),
        widget_type: "button".into(),
        label: "Lock Screen".into(),
        group: Some("Power".into()),
        icon: Some("lock".into()),
        ..Default::default()
    });

    widgets.push(Widget {
        id: "btn_logoff".into(),
        widget_type: "button".into(),
        label: "Log Off".into(),
        group: Some("Power".into()),
        icon: Some("logout".into()),
        confirm: Some("Log off the current user?".into()),
        ..Default::default()
    });

    widgets.push(Widget {
        id: "btn_flush_dns".into(),
        widget_type: "button".into(),
        label: "Flush DNS".into(),
        group: Some("Network".into()),
        icon: Some("dns".into()),
        ..Default::default()
    });

    widgets.push(Widget {
        id: "btn_clear_temp".into(),
        widget_type: "button".into(),
        label: "Clear Temp Files".into(),
        group: Some("Maintenance".into()),
        icon: Some("cleaning_services".into()),
        ..Default::default()
    });

    // ── Table Widgets (queryable) ────────────────────────────────────
    widgets.push(Widget {
        id: "process_list".into(),
        widget_type: "table".into(),
        label: "Processes".into(),
        group: Some("Management".into()),
        icon: Some("list_alt".into()),
        columns: Some(vec![
            TableColumn {
                key: "pid".into(),
                label: "PID".into(),
                sortable: Some(true),
            },
            TableColumn {
                key: "name".into(),
                label: "Name".into(),
                sortable: Some(true),
            },
            TableColumn {
                key: "cpu".into(),
                label: "CPU %".into(),
                sortable: Some(true),
            },
            TableColumn {
                key: "memory_mb".into(),
                label: "Mem (MB)".into(),
                sortable: Some(true),
            },
            TableColumn {
                key: "status".into(),
                label: "Status".into(),
                sortable: Some(false),
            },
        ]),
        ..Default::default()
    });

    widgets.push(Widget {
        id: "service_list".into(),
        widget_type: "table".into(),
        label: "Services".into(),
        group: Some("Management".into()),
        icon: Some("settings_suggest".into()),
        columns: Some(vec![
            TableColumn {
                key: "name".into(),
                label: "Name".into(),
                sortable: Some(true),
            },
            TableColumn {
                key: "display_name".into(),
                label: "Display Name".into(),
                sortable: Some(true),
            },
            TableColumn {
                key: "status".into(),
                label: "Status".into(),
                sortable: Some(true),
            },
            TableColumn {
                key: "start_type".into(),
                label: "Startup".into(),
                sortable: Some(true),
            },
        ]),
        ..Default::default()
    });

    widgets.push(Widget {
        id: "network_info".into(),
        widget_type: "table".into(),
        label: "Network Interfaces".into(),
        group: Some("Network".into()),
        icon: Some("settings_ethernet".into()),
        columns: Some(vec![
            TableColumn {
                key: "name".into(),
                label: "Interface".into(),
                sortable: Some(true),
            },
            TableColumn {
                key: "ip".into(),
                label: "IP Address".into(),
                sortable: Some(false),
            },
            TableColumn {
                key: "mac".into(),
                label: "MAC".into(),
                sortable: Some(false),
            },
            TableColumn {
                key: "status".into(),
                label: "Status".into(),
                sortable: Some(true),
            },
        ]),
        ..Default::default()
    });

    widgets.push(Widget {
        id: "firewall_rules".into(),
        widget_type: "table".into(),
        label: "Firewall Rules".into(),
        group: Some("Security".into()),
        icon: Some("shield".into()),
        columns: Some(vec![
            TableColumn {
                key: "name".into(),
                label: "Rule".into(),
                sortable: Some(true),
            },
            TableColumn {
                key: "direction".into(),
                label: "Direction".into(),
                sortable: Some(true),
            },
            TableColumn {
                key: "action".into(),
                label: "Action".into(),
                sortable: Some(true),
            },
            TableColumn {
                key: "enabled".into(),
                label: "Enabled".into(),
                sortable: Some(true),
            },
        ]),
        ..Default::default()
    });

    widgets.push(Widget {
        id: "installed_software".into(),
        widget_type: "table".into(),
        label: "Installed Software".into(),
        group: Some("Management".into()),
        icon: Some("apps".into()),
        columns: Some(vec![
            TableColumn {
                key: "name".into(),
                label: "Name".into(),
                sortable: Some(true),
            },
            TableColumn {
                key: "version".into(),
                label: "Version".into(),
                sortable: Some(true),
            },
            TableColumn {
                key: "publisher".into(),
                label: "Publisher".into(),
                sortable: Some(true),
            },
            TableColumn {
                key: "install_date".into(),
                label: "Installed".into(),
                sortable: Some(true),
            },
        ]),
        ..Default::default()
    });

    widgets.push(Widget {
        id: "startup_programs".into(),
        widget_type: "table".into(),
        label: "Startup Programs".into(),
        group: Some("Management".into()),
        icon: Some("rocket_launch".into()),
        columns: Some(vec![
            TableColumn {
                key: "name".into(),
                label: "Name".into(),
                sortable: Some(true),
            },
            TableColumn {
                key: "command".into(),
                label: "Command".into(),
                sortable: Some(false),
            },
            TableColumn {
                key: "location".into(),
                label: "Location".into(),
                sortable: Some(true),
            },
        ]),
        ..Default::default()
    });

    widgets.push(Widget {
        id: "user_accounts".into(),
        widget_type: "table".into(),
        label: "User Accounts".into(),
        group: Some("Security".into()),
        icon: Some("people".into()),
        columns: Some(vec![
            TableColumn {
                key: "name".into(),
                label: "Username".into(),
                sortable: Some(true),
            },
            TableColumn {
                key: "full_name".into(),
                label: "Full Name".into(),
                sortable: Some(true),
            },
            TableColumn {
                key: "active".into(),
                label: "Active".into(),
                sortable: Some(true),
            },
            TableColumn {
                key: "admin".into(),
                label: "Admin".into(),
                sortable: Some(true),
            },
        ]),
        ..Default::default()
    });

    widgets.push(Widget {
        id: "event_log".into(),
        widget_type: "table".into(),
        label: "Event Log".into(),
        group: Some("Security".into()),
        icon: Some("event_note".into()),
        columns: Some(vec![
            TableColumn {
                key: "time".into(),
                label: "Time".into(),
                sortable: Some(true),
            },
            TableColumn {
                key: "level".into(),
                label: "Level".into(),
                sortable: Some(true),
            },
            TableColumn {
                key: "source".into(),
                label: "Source".into(),
                sortable: Some(true),
            },
            TableColumn {
                key: "message".into(),
                label: "Message".into(),
                sortable: Some(false),
            },
        ]),
        ..Default::default()
    });

    widgets.push(Widget {
        id: "scheduled_tasks".into(),
        widget_type: "table".into(),
        label: "Scheduled Tasks".into(),
        group: Some("Automation".into()),
        icon: Some("event_repeat".into()),
        columns: Some(vec![
            TableColumn {
                key: "name".into(),
                label: "Task".into(),
                sortable: Some(true),
            },
            TableColumn {
                key: "status".into(),
                label: "Status".into(),
                sortable: Some(true),
            },
            TableColumn {
                key: "next_run".into(),
                label: "Next Run".into(),
                sortable: Some(true),
            },
            TableColumn {
                key: "last_result".into(),
                label: "Last Result".into(),
                sortable: Some(false),
            },
        ]),
        ..Default::default()
    });

    // ── LED Status Indicators ────────────────────────────────────────
    widgets.push(Widget {
        id: "defender_status".into(),
        widget_type: "led".into(),
        label: "Windows Defender".into(),
        group: Some("Security".into()),
        icon: Some("security".into()),
        ..Default::default()
    });

    // ── Special Session Widgets ──────────────────────────────────────
    if config.enable_terminal {
        widgets.push(Widget {
            id: "terminal".into(),
            widget_type: "terminal".into(),
            label: "Terminal".into(),
            group: Some("Sessions".into()),
            icon: Some("terminal".into()),
            ..Default::default()
        });
    }

    if config.enable_file_browser {
        widgets.push(Widget {
            id: "file_browser".into(),
            widget_type: "file_browser".into(),
            label: "File Browser".into(),
            group: Some("Sessions".into()),
            icon: Some("folder".into()),
            ..Default::default()
        });
    }

    if config.enable_remote_desktop {
        widgets.push(Widget {
            id: "desktop".into(),
            widget_type: "desktop".into(),
            label: "Remote Desktop".into(),
            group: Some("Sessions".into()),
            icon: Some("desktop_windows".into()),
            ..Default::default()
        });
    }

    if config.enable_clipboard {
        widgets.push(Widget {
            id: "clipboard".into(),
            widget_type: "clipboard".into(),
            label: "Clipboard".into(),
            group: Some("Sessions".into()),
            icon: Some("content_paste".into()),
            ..Default::default()
        });
    }

    // ── Select Widgets ───────────────────────────────────────────────
    if config.enable_automation {
        widgets.push(Widget {
            id: "automation".into(),
            widget_type: "button".into(),
            label: "Run Script".into(),
            group: Some("Automation".into()),
            icon: Some("code".into()),
            ..Default::default()
        });
    }

    // ── Alerts ───────────────────────────────────────────────────────
    let alerts = vec![
        AlertDef {
            id: "high_cpu".into(),
            severity: "warning".into(),
            condition: "sys_cpu > 90".into(),
            message: "CPU usage above 90%".into(),
            auto_resolve: Some(true),
        },
        AlertDef {
            id: "high_memory".into(),
            severity: "warning".into(),
            condition: "sys_memory > 90".into(),
            message: "Memory usage above 90%".into(),
            auto_resolve: Some(true),
        },
        AlertDef {
            id: "critical_disk".into(),
            severity: "critical".into(),
            condition: "sys_disk > 95".into(),
            message: "Disk usage above 95%".into(),
            auto_resolve: Some(true),
        },
    ];

    DeviceManifest {
        device,
        capabilities,
        widgets,
        alerts: Some(alerts),
        heartbeat_interval: Some(config.heartbeat_interval_secs),
    }
}

// ---------------------------------------------------------------------------
//  Default impl for Widget
// ---------------------------------------------------------------------------

impl Default for Widget {
    fn default() -> Self {
        Self {
            id: String::new(),
            widget_type: String::new(),
            label: String::new(),
            group: None,
            unit: None,
            min: None,
            max: None,
            readonly: None,
            icon: None,
            columns: None,
            options: None,
            confirm: None,
            dangerous: None,
        }
    }
}
