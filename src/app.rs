use crate::adb::broadcast_manager::BroadcastManager;
use crate::adb::device_manager::DeviceManager;
use crate::config::ConfigManager;
use crate::models::broadcast::Broadcast;
use crate::models::device::Device;

pub struct AppState {
    pub config_manager: ConfigManager,
    pub device_manager: DeviceManager,
    pub broadcast_manager: BroadcastManager,
    pub selected_device: Option<Device>,
    pub is_refreshing: bool,
    pub status_message: String,
    pub status_type: StatusType,
}

#[derive(Debug, Clone, PartialEq)]
pub enum StatusType {
    None,
    Info,
    Success,
    Error,
}

impl AppState {
    pub fn new() -> Result<Self, String> {
        let config_manager = ConfigManager::new()?;
        let adb_path = config_manager.get().adb_path.clone();

        Ok(Self {
            config_manager,
            device_manager: DeviceManager::new(&adb_path),
            broadcast_manager: BroadcastManager::new(&adb_path),
            selected_device: None,
            is_refreshing: false,
            status_message: String::new(),
            status_type: StatusType::None,
        })
    }

    pub fn refresh_devices(&mut self) -> Result<Vec<Device>, String> {
        self.is_refreshing = true;
        let result = self.device_manager.refresh_devices();
        self.is_refreshing = false;
        result
    }

    pub fn select_device(&mut self, device: Option<Device>) {
        self.selected_device = device.clone();
        if let Some(ref d) = device {
            let _ = self.config_manager.set_last_device(&d.serial);
        }
    }

    pub fn select_device_by_serial(&mut self, serial: &str, devices: &[Device]) {
        let device = devices.iter().find(|d| d.serial == serial).cloned();
        self.select_device(device);
    }

    pub fn send_broadcast(&mut self, content: &str, action: Option<&str>) -> Result<Broadcast, String> {
        let serial = match &self.selected_device {
            Some(d) => d.serial.clone(),
            None => return Err("No device selected".to_string()),
        };

        self.broadcast_manager.send_broadcast(&serial, content, action)
    }

    pub fn get_history(&self) -> Vec<Broadcast> {
        self.broadcast_manager.get_history().to_vec()
    }

    pub fn clear_history(&mut self) {
        self.broadcast_manager.clear_history();
    }

    pub fn set_status(&mut self, message: &str, status_type: StatusType) {
        self.status_message = message.to_string();
        self.status_type = status_type;
    }

    pub fn clear_status(&mut self) {
        self.status_message.clear();
        self.status_type = StatusType::None;
    }

    pub fn add_broadcast_history(&mut self, content: &str, device_serial: &str, success: bool, error: Option<&str>) {
        let mut broadcast = Broadcast::new(content, device_serial);
        if success {
            broadcast.mark_success("Broadcast sent successfully");
        } else {
            broadcast.mark_failed(error.unwrap_or("Unknown error"));
        }
        self.broadcast_manager.add_to_history_public(broadcast);
    }

    pub fn update_adb_path(&mut self, path: &str) {
        self.device_manager.set_adb_path(path);
        self.broadcast_manager.set_adb_path(path);
        let _ = self.config_manager.set_adb_path(path);
    }

    pub fn update_screenshot_dir(&mut self, path: &str) {
        let _ = self.config_manager.set_screenshot_dir(path);
    }

    pub fn connect_wifi(&mut self, ip: &str, port: u16) -> Result<String, String> {
        self.device_manager.connect_wifi(ip, port)
    }

    pub fn disconnect_wifi(&mut self, ip: &str, port: u16) -> Result<String, String> {
        self.device_manager.disconnect_wifi(ip, port)
    }
}
