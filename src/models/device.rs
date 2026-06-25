use serde::{Deserialize, Serialize};
use std::fmt;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Device {
    pub id: String,
    pub serial: String,
    pub model: String,
    pub state: String,
    pub connection: String,
    #[serde(skip_serializing_if = "String::is_empty", default)]
    pub ip_address: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub port: Option<u16>,
    pub is_active: bool,
}

impl Device {
    pub fn new(serial: &str, model: &str, state: &str) -> Self {
        Self {
            id: format!("device_{}", serial),
            serial: serial.to_string(),
            model: model.to_string(),
            state: state.to_string(),
            connection: "usb".to_string(),
            ip_address: String::new(),
            port: None,
            is_active: false,
        }
    }

    pub fn new_wifi(serial: &str, model: &str, state: &str, ip: &str, port: u16) -> Self {
        Self {
            id: format!("device_{}", serial),
            serial: serial.to_string(),
            model: model.to_string(),
            state: state.to_string(),
            connection: "wifi".to_string(),
            ip_address: ip.to_string(),
            port: Some(port),
            is_active: false,
        }
    }

    pub fn is_connected(&self) -> bool {
        self.state == "device" || self.state == "online"
    }

    pub fn is_offline(&self) -> bool {
        self.state == "offline"
    }

    pub fn is_unauthorized(&self) -> bool {
        self.state == "unauthorized"
    }

    pub fn display_name(&self) -> String {
        if self.model.is_empty() {
            self.serial.clone()
        } else {
            format!("{} ({})", self.model, self.serial)
        }
    }

    pub fn connection_info(&self) -> String {
        if self.connection == "wifi" {
            if let Some(port) = self.port {
                format!("WiFi: {}:{}", self.ip_address, port)
            } else {
                format!("WiFi: {}", self.ip_address)
            }
        } else {
            "USB".to_string()
        }
    }

    pub fn wifi_endpoint(&self) -> Option<(String, u16)> {
        if self.connection != "wifi" {
            return None;
        }

        if !self.ip_address.is_empty() {
            return Some((self.ip_address.clone(), self.port.unwrap_or(5555)));
        }

        let (ip, port) = self.serial.rsplit_once(':')?;
        let port = port.parse::<u16>().ok()?;
        Some((ip.to_string(), port))
    }
}

impl fmt::Display for Device {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.display_name())
    }
}
