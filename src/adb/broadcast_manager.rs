use crate::models::broadcast::Broadcast;
use std::process::Command;
use std::os::windows::process::CommandExt;

pub struct BroadcastManager {
    adb_path: String,
    history: Vec<Broadcast>,
    max_history: usize,
}

impl BroadcastManager {
    pub fn new(adb_path: &str) -> Self {
        Self {
            adb_path: adb_path.to_string(),
            history: Vec::new(),
            max_history: 100,
        }
    }

    pub fn send_broadcast(
        &mut self,
        device_serial: &str,
        content: &str,
        action: Option<&str>,
    ) -> Result<Broadcast, String> {
        let mut broadcast = Broadcast::new(content, device_serial);

        if content.trim().is_empty() {
            broadcast.mark_failed("Broadcast content cannot be empty");
            self.add_to_history(broadcast.clone());
            return Err("Broadcast content cannot be empty".to_string());
        }

        if content.contains('`') || content.contains("$(") {
            broadcast.mark_failed("Broadcast content contains invalid characters");
            self.add_to_history(broadcast.clone());
            return Err("Broadcast content contains invalid characters".to_string());
        }

        if content.len() > 1024 * 1024 {
            broadcast.mark_failed("Broadcast content is too large");
            self.add_to_history(broadcast.clone());
            return Err("Broadcast content is too large".to_string());
        }

        let action = action.unwrap_or("com.sunmi.scanner.ACTION_DATA_CODE_RECEIVED");
        let escaped_content = content.replace('"', "\\\"");
        let shell_cmd = format!(
            r#"am broadcast -a {} --es data "{}""#,
            action, escaped_content
        );

        let output = Command::new(&self.adb_path)
            .creation_flags(0x08000000)
            .args(["-s", device_serial, "shell", &shell_cmd])
            .output();

        match output {
            Ok(output) => {
                let stdout = String::from_utf8_lossy(&output.stdout);
                let stderr = String::from_utf8_lossy(&output.stderr);
                let result_text = format!("{}{}", stdout, stderr);

                if result_text.contains("result=0") || result_text.contains("Broadcast completed") {
                    broadcast.mark_success(&result_text);
                } else {
                    broadcast.mark_failed(&result_text);
                }

                self.add_to_history(broadcast.clone());

                if broadcast.is_success() {
                    Ok(broadcast)
                } else {
                    Err(broadcast.error.clone().unwrap_or_else(|| "Broadcast failed".to_string()))
                }
            }
            Err(e) => {
                let msg = format!("Failed to execute adb command: {}", e);
                broadcast.mark_failed(&msg);
                self.add_to_history(broadcast.clone());
                Err(msg)
            }
        }
    }

    pub fn get_history(&self) -> &[Broadcast] {
        &self.history
    }

    pub fn clear_history(&mut self) {
        self.history.clear();
    }

    pub fn add_to_history_public(&mut self, broadcast: Broadcast) {
        self.add_to_history(broadcast);
    }

    fn add_to_history(&mut self, broadcast: Broadcast) {
        self.history.push(broadcast);
        if self.history.len() > self.max_history {
            self.history.remove(0);
        }
    }

    pub fn set_max_history(&mut self, max: usize) {
        self.max_history = max;
    }

    pub fn set_adb_path(&mut self, path: &str) {
        self.adb_path = path.to_string();
    }
}
