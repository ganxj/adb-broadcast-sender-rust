use chrono::{DateTime, Local};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Broadcast {
    pub id: String,
    pub content: String,
    pub device_id: String,
    pub timestamp: DateTime<Local>,
    pub result: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

impl Broadcast {
    pub fn new(content: &str, device_id: &str) -> Self {
        Self {
            id: format!("broadcast_{}", Local::now().timestamp_millis()),
            content: content.to_string(),
            device_id: device_id.to_string(),
            timestamp: Local::now(),
            result: "pending".to_string(),
            output: None,
            error: None,
        }
    }

    pub fn mark_success(&mut self, output: &str) {
        self.result = "success".to_string();
        self.output = Some(output.to_string());
        self.error = None;
    }

    pub fn mark_failed(&mut self, error: &str) {
        self.result = "failed".to_string();
        self.error = Some(error.to_string());
        self.output = None;
    }

    pub fn is_success(&self) -> bool {
        self.result == "success"
    }

    pub fn is_failed(&self) -> bool {
        self.result == "failed"
    }

    pub fn is_pending(&self) -> bool {
        self.result == "pending"
    }

    pub fn status(&self) -> String {
        match self.result.as_str() {
            "success" => "Success".to_string(),
            "failed" => "Failed".to_string(),
            "pending" => "Pending".to_string(),
            _ => "Unknown".to_string(),
        }
    }

    pub fn formatted_timestamp(&self) -> String {
        self.timestamp.format("%Y-%m-%d %H:%M:%S").to_string()
    }

    pub fn summary(&self) -> String {
        format!(
            "Broadcast to device {}: {} ({})",
            self.device_id,
            self.status(),
            self.formatted_timestamp()
        )
    }
}
