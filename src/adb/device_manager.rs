use crate::models::device::Device;
use std::process::Command;
use std::os::windows::process::CommandExt;

pub struct DeviceManager {
    adb_path: String,
}

impl DeviceManager {
    pub fn new(adb_path: &str) -> Self {
        Self {
            adb_path: adb_path.to_string(),
        }
    }

    pub fn refresh_devices(&self) -> Result<Vec<Device>, String> {
        let output = Command::new(&self.adb_path)
            .creation_flags(0x08000000)
            .args(["devices", "-l"])
            .output()
            .map_err(|e| format!("Failed to execute adb devices: {}", e))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(format!("adb devices failed: {}", stderr));
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let devices = Self::parse_device_list(&stdout);
        Ok(devices)
    }

    pub fn parse_device_list(output: &str) -> Vec<Device> {
        Self::parse_device_list_static(output)
    }

    pub fn parse_device_list_static(output: &str) -> Vec<Device> {
        let mut devices = Vec::new();
        let lines: Vec<&str> = output.lines().collect();

        for line in lines.iter().skip(1) {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }

            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() < 2 {
                continue;
            }

            let serial = parts[0].to_string();
            let state = parts[1].to_string();

            let mut model = String::new();
            let mut product = String::new();
            for part in &parts[2..] {
                if let Some(val) = part.strip_prefix("model:") {
                    model = val.to_string();
                } else if let Some(val) = part.strip_prefix("product:") {
                    product = val.to_string();
                }
            }

            if model.is_empty() && !product.is_empty() {
                model = product;
            }

            let (connection, ip, port) = if serial.contains(':') {
                let ip_port: Vec<&str> = serial.split(':').collect();
                if ip_port.len() == 2 {
                    let ip = ip_port[0].to_string();
                    let port = ip_port[1].parse::<u16>().unwrap_or(5555);
                    ("wifi".to_string(), ip, Some(port))
                } else {
                    ("wifi".to_string(), serial.clone(), None)
                }
            } else {
                ("usb".to_string(), String::new(), None)
            };

            let device = Device {
                id: format!("device_{}", serial),
                serial: serial.clone(),
                model,
                state,
                connection,
                ip_address: ip,
                port,
                is_active: false,
            };

            devices.push(device);
        }

        devices
    }

    pub fn connect_wifi(&self, ip: &str, port: u16) -> Result<String, String> {
        let addr = format!("{}:{}", ip, port);
        let output = Command::new(&self.adb_path)
            .creation_flags(0x08000000)
            .args(["connect", &addr])
            .output()
            .map_err(|e| format!("Failed to execute adb connect: {}", e))?;

        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        let result = format!("{}{}", stdout, stderr);

        if result.contains("connected to") || result.contains("already connected") {
            Ok(result.trim().to_string())
        } else {
            Err(result.trim().to_string())
        }
    }

    pub fn disconnect_wifi(&self, ip: &str, port: u16) -> Result<String, String> {
        let addr = format!("{}:{}", ip, port);
        let output = Command::new(&self.adb_path)
            .creation_flags(0x08000000)
            .args(["disconnect", &addr])
            .output()
            .map_err(|e| format!("Failed to execute adb disconnect: {}", e))?;

        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        let result = format!("{}{}", stdout, stderr);

        if result.contains("disconnected") || result.contains("not connected") {
            Ok(result.trim().to_string())
        } else {
            Err(result.trim().to_string())
        }
    }

    pub fn set_adb_path(&mut self, path: &str) {
        self.adb_path = path.to_string();
    }
}
