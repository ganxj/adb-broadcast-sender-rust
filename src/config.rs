use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    #[serde(default = "default_adb_path")]
    pub adb_path: String,
    #[serde(default = "default_auto_refresh")]
    pub auto_refresh: bool,
    #[serde(default = "default_refresh_rate")]
    pub refresh_rate: u64,
    #[serde(default)]
    pub last_device_id: String,
    #[serde(default = "default_window_width")]
    pub window_width: u32,
    #[serde(default = "default_window_height")]
    pub window_height: u32,
    #[serde(default)]
    pub quick_contents: Vec<String>,
    #[serde(default = "default_wifi_ips")]
    pub wifi_ips: Vec<String>,
    #[serde(default = "default_screenshot_dir")]
    pub screenshot_dir: String,
}

fn default_adb_path() -> String {
    r#"D:\Program Files\Android\SDK\platform-tools\adb.exe"#.to_string()
}

fn default_auto_refresh() -> bool {
    true
}

fn default_refresh_rate() -> u64 {
    5
}

fn default_window_width() -> u32 {
    900
}

fn default_window_height() -> u32 {
    700
}

fn default_screenshot_dir() -> String {
    r#"D:\data\files\adb"#.to_string()
}

fn default_wifi_ips() -> Vec<String> {
    vec![
        "192.168.137.".to_string(),
        "192.168.137.56".to_string(),
        "192.168.137.204".to_string(),
    ]
}

impl Default for Config {
    fn default() -> Self {
        Self {
            adb_path: default_adb_path(),
            auto_refresh: default_auto_refresh(),
            refresh_rate: default_refresh_rate(),
            last_device_id: String::new(),
            window_width: default_window_width(),
            window_height: default_window_height(),
            quick_contents: Vec::new(),
            wifi_ips: default_wifi_ips(),
            screenshot_dir: default_screenshot_dir(),
        }
    }
}

pub struct ConfigManager {
    config: Config,
    config_path: PathBuf,
}

impl ConfigManager {
    pub fn new() -> Result<Self, String> {
        let config_dir = Self::get_config_dir()?;
        let config_path = config_dir.join("config.json");

        let mut manager = Self {
            config: Config::default(),
            config_path,
        };

        if let Err(e) = manager.load() {
            eprintln!("Failed to load config, using defaults: {}", e);
            let _ = manager.save();
        }

        Ok(manager)
    }

    fn get_config_dir() -> Result<PathBuf, String> {
        let app_data = dirs::data_dir()
            .or_else(dirs::config_dir)
            .ok_or("Could not find config directory")?;
        let config_dir = app_data.join("adb-broadcast-sender");
        fs::create_dir_all(&config_dir).map_err(|e| format!("Failed to create config dir: {}", e))?;
        Ok(config_dir)
    }

    pub fn load(&mut self) -> Result<(), String> {
        let data = fs::read_to_string(&self.config_path)
            .map_err(|e| format!("Failed to read config: {}", e))?;
        self.config = serde_json::from_str(&data)
            .map_err(|e| format!("Failed to parse config: {}", e))?;
        Ok(())
    }

    pub fn save(&self) -> Result<(), String> {
        let data = serde_json::to_string_pretty(&self.config)
            .map_err(|e| format!("Failed to serialize config: {}", e))?;
        fs::write(&self.config_path, data)
            .map_err(|e| format!("Failed to write config: {}", e))?;
        Ok(())
    }

    pub fn get(&self) -> &Config {
        &self.config
    }

    pub fn get_mut(&mut self) -> &mut Config {
        &mut self.config
    }

    pub fn set_adb_path(&mut self, path: &str) -> Result<(), String> {
        self.config.adb_path = path.to_string();
        self.save()
    }

    pub fn set_last_device(&mut self, device_id: &str) -> Result<(), String> {
        self.config.last_device_id = device_id.to_string();
        self.save()
    }

    pub fn set_window_size(&mut self, width: u32, height: u32) -> Result<(), String> {
        self.config.window_width = width;
        self.config.window_height = height;
        self.save()
    }

    pub fn set_quick_contents(&mut self, contents: Vec<String>) -> Result<(), String> {
        self.config.quick_contents = contents;
        self.save()
    }

    pub fn set_screenshot_dir(&mut self, path: &str) -> Result<(), String> {
        self.config.screenshot_dir = path.to_string();
        self.save()
    }

    pub fn add_quick_content(&mut self, content: String) -> Result<(), String> {
        if !self.config.quick_contents.contains(&content) {
            self.config.quick_contents.push(content);
            self.save()
        } else {
            Ok(())
        }
    }

    pub fn set_wifi_ips(&mut self, ips: Vec<String>) -> Result<(), String> {
        self.config.wifi_ips = ips;
        self.save()
    }

    pub fn add_wifi_ip(&mut self, ip: String) -> Result<(), String> {
        if !self.config.wifi_ips.contains(&ip) {
            self.config.wifi_ips.push(ip);
            self.save()
        } else {
            Ok(())
        }
    }
}
