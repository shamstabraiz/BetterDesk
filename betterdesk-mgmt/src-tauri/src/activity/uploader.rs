//! Yomie Activity Uploader — periodic HTTP upload of activity data
//!
//! Runs in a background async loop, periodically draining the session
//! aggregator's completed sessions and uploading them to the Console API.

use serde::Serialize;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use super::session::{ActivitySession, SessionAggregator};
use super::tracker::ActivityTracker;

/// Default upload interval in seconds (60s).
const UPLOAD_INTERVAL_SECS: u64 = 60;

/// Payload sent to the server.
#[derive(Debug, Serialize)]
struct ActivityPayload {
    device_id: String,
    sessions: Vec<ActivitySession>,
    idle_seconds: u64,
    timestamp: String,
}

/// Background activity uploader service.
pub struct ActivityUploader {
    device_id: String,
    api_url: String,
    upload_interval: Duration,
    running: Arc<Mutex<bool>>,
}

impl ActivityUploader {
    pub fn new(device_id: &str, api_url: &str) -> Self {
        Self {
            device_id: device_id.to_string(),
            api_url: api_url.trim_end_matches('/').to_string(),
            upload_interval: Duration::from_secs(UPLOAD_INTERVAL_SECS),
            running: Arc::new(Mutex::new(false)),
        }
    }

    /// Start the background polling + upload loop.
    pub fn start(
        &self,
        tracker: Arc<Mutex<ActivityTracker>>,
        aggregator: Arc<Mutex<SessionAggregator>>,
    ) {
        {
            let mut running = self.running.lock().unwrap();
            if *running {
                return;
            }
            *running = true;
        }

        let device_id = self.device_id.clone();
        let api_url = self.api_url.clone();
        let upload_interval = self.upload_interval;
        let running = self.running.clone();

        tokio::spawn(async move {
            let client = reqwest::Client::builder()
                .timeout(Duration::from_secs(15))
                .build()
                .unwrap_or_default();

            let mut poll_ticker = tokio::time::interval(Duration::from_secs(5));
            let mut upload_ticker = tokio::time::interval(upload_interval);

            loop {
                {
                    let r = running.lock().unwrap();
                    if !*r {
                        break;
                    }
                }

                tokio::select! {
                    _ = poll_ticker.tick() => {
                        // Poll foreground + feed aggregator
                        let snap = {
                            let mut t = tracker.lock().unwrap_or_else(|e| e.into_inner());
                            if t.should_poll() {
                                Some(t.get_foreground())
                            } else {
                                None
                            }
                        };

                        if let Some(snap) = snap {
                            let mut agg = aggregator.lock().unwrap_or_else(|e| e.into_inner());
                            agg.feed(&snap);
                        }
                    }
                    _ = upload_ticker.tick() => {
                        // Drain completed sessions and upload
                        // Note: always lock tracker before aggregator to match poll branch order
                        let (sessions, idle_secs) = {
                            let t = tracker.lock().unwrap_or_else(|e| e.into_inner());
                            let idle = t.get_idle();
                            let mut agg = aggregator.lock().unwrap_or_else(|e| e.into_inner());
                            let sessions = agg.drain_completed();
                            (sessions, idle.idle_seconds)
                        };

                        if sessions.is_empty() {
                            continue;
                        }

                        let payload = ActivityPayload {
                            device_id: device_id.clone(),
                            sessions,
                            idle_seconds: idle_secs,
                            timestamp: chrono::Utc::now().to_rfc3339(),
                        };

                        let url = format!("{}/api/bd/activity", api_url);
                        match client
                            .post(&url)
                            .header("X-Device-Id", &device_id)
                            .json(&payload)
                            .send()
                            .await
                        {
                            Ok(resp) if resp.status().is_success() => {
                                // Successfully uploaded
                            }
                            Ok(resp) => {
                                eprintln!(
                                    "[Activity] Upload failed: HTTP {}",
                                    resp.status()
                                );
                            }
                            Err(e) => {
                                eprintln!("[Activity] Upload error: {}", e);
                            }
                        }
                    }
                }
            }
        });
    }

    /// Stop the background loop.
    #[allow(dead_code)]
    pub fn stop(&self) {
        let mut running = self.running.lock().unwrap();
        *running = false;
    }

    /// Check if the uploader is running.
    #[allow(dead_code)]
    pub fn is_running(&self) -> bool {
        *self.running.lock().unwrap()
    }
}
