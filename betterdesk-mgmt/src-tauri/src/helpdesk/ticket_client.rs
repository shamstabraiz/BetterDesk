//! Yomie Helpdesk — Agent-side ticket client
//!
//! Allows the desktop agent to create, list, and comment on tickets
//! via the Yomie Console REST API.

use serde::{Deserialize, Serialize};

/// A support ticket as seen from the agent side.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Ticket {
    pub id: Option<u64>,
    pub title: String,
    pub description: String,
    pub status: String,
    pub priority: String,
    pub category: String,
    pub device_id: Option<String>,
    pub created_by: Option<String>,
    pub assigned_to: Option<String>,
    pub sla_due_at: Option<String>,
    pub created_at: Option<String>,
    pub updated_at: Option<String>,
}

/// Payload for creating a new ticket.
#[derive(Debug, Serialize)]
pub struct CreateTicketRequest {
    pub title: String,
    pub description: String,
    pub priority: String,
    pub category: String,
}

/// Response from ticket creation.
#[derive(Debug, Deserialize)]
pub struct CreateTicketResponse {
    pub success: bool,
    pub ticket_id: Option<u64>,
}

/// Response from listing tickets.
#[derive(Debug, Deserialize)]
pub struct ListTicketsResponse {
    pub tickets: Vec<Ticket>,
    pub total: usize,
}

/// Agent-side ticket client for the Yomie Console API.
pub struct TicketClient {
    http: reqwest::Client,
    base_url: String,
    device_id: String,
}

impl TicketClient {
    /// Create a new ticket client.
    ///
    /// # Arguments
    /// * `base_url` — Console API base URL, e.g. `http://192.168.0.110:5000`
    /// * `device_id` — This device's registration ID
    pub fn new(base_url: &str, device_id: &str) -> Self {
        Self {
            http: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(15))
                .build()
                .unwrap_or_default(),
            base_url: base_url.trim_end_matches('/').to_string(),
            device_id: device_id.to_string(),
        }
    }

    /// Create a new support ticket.
    pub async fn create_ticket(
        &self,
        title: &str,
        description: &str,
        priority: &str,
        category: &str,
    ) -> Result<CreateTicketResponse, String> {
        let url = format!("{}/api/tickets/bd", self.base_url);
        let body = CreateTicketRequest {
            title: title.to_string(),
            description: description.to_string(),
            priority: priority.to_string(),
            category: category.to_string(),
        };

        let resp = self
            .http
            .post(&url)
            .header("X-Device-Id", &self.device_id)
            .json(&body)
            .send()
            .await
            .map_err(|e| format!("HTTP request failed: {}", e))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            return Err(format!("Server returned {}: {}", status, text));
        }

        resp.json::<CreateTicketResponse>()
            .await
            .map_err(|e| format!("Failed to parse response: {}", e))
    }

    /// List tickets associated with this device.
    pub async fn list_tickets(&self) -> Result<ListTicketsResponse, String> {
        let url = format!("{}/api/tickets/bd", self.base_url);

        let resp = self
            .http
            .get(&url)
            .header("X-Device-Id", &self.device_id)
            .send()
            .await
            .map_err(|e| format!("HTTP request failed: {}", e))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            return Err(format!("Server returned {}: {}", status, text));
        }

        resp.json::<ListTicketsResponse>()
            .await
            .map_err(|e| format!("Failed to parse response: {}", e))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_request_serialization() {
        let req = CreateTicketRequest {
            title: "Test issue".to_string(),
            description: "Something broke".to_string(),
            priority: "high".to_string(),
            category: "software".to_string(),
        };
        let json = serde_json::to_string(&req).unwrap();
        assert!(json.contains("Test issue"));
        assert!(json.contains("high"));
    }
}
