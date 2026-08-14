#![windows_subsystem = "windows"]

use eframe::egui;
use crate::app::{AppState, StatusType};
use crate::models::device::Device;
use crate::adb::device_manager::DeviceManager;
use std::os::windows::process::CommandExt;
use std::sync::{Arc, Mutex};
use std::fs;
use std::path::PathBuf;
use std::thread;
use std::ffi::OsStr;
use std::os::windows::ffi::OsStrExt;

mod adb;
mod app;
mod config;
mod models;

// ===== Windows single-instance check =====
#[link(name = "kernel32")]
extern "system" {
    fn CreateMutexW(
        lp_mutex_attributes: *const std::ffi::c_void,
        b_initial_owner: i32,
        lp_name: *const u16,
    ) -> *const std::ffi::c_void;
    fn GetLastError() -> u32;
}

#[link(name = "user32")]
extern "system" {
    fn FindWindowW(
        lp_class_name: *const u16,
        lp_window_name: *const u16,
    ) -> *const std::ffi::c_void;
    fn SetForegroundWindow(h_wnd: *const std::ffi::c_void) -> i32;
    fn ShowWindow(h_wnd: *const std::ffi::c_void, n_cmd_show: i32) -> i32;
    fn IsIconic(h_wnd: *const std::ffi::c_void) -> i32;
}

const ERROR_ALREADY_EXISTS: u32 = 183;
const SW_RESTORE: i32 = 9;
const MUTEX_NAME: &str = "Global\\adb-broadcast-sender-instance";

fn to_wide(s: &str) -> Vec<u16> {
    OsStr::new(s).encode_wide().chain(std::iter::once(0)).collect()
}

fn check_single_instance() -> bool {
    unsafe {
        let name = to_wide(MUTEX_NAME);
        CreateMutexW(std::ptr::null(), 1, name.as_ptr());
        if GetLastError() == ERROR_ALREADY_EXISTS {
            // App already running -- bring existing window to front
            let title = to_wide("ADB Broadcast Sender");
            let hwnd = FindWindowW(std::ptr::null(), title.as_ptr());
            if !hwnd.is_null() {
                if IsIconic(hwnd) != 0 {
                    ShowWindow(hwnd, SW_RESTORE);
                }
                SetForegroundWindow(hwnd);
            }
            return false;
        }
    }
    true
}

// ===== Background Task System =====
#[derive(Clone, Debug)]
enum BgOp {
    RefreshDevices,
    ConnectWifi,
    DisconnectWifi,
    SendBroadcast,
    TakeScreenshot,
    InputText,
    InstallApk,
}

#[derive(Clone, Debug)]
struct BgTaskResult {
    op: BgOp,
    success: bool,
    message: String,
    devices: Option<Vec<Device>>,
    broadcast_content: Option<String>,
    broadcast_device: Option<String>,
}

fn main() -> Result<(), eframe::Error> {
    // Single-instance: if already running, bring existing window to front and exit
    if !check_single_instance() {
        return Ok(());
    }

    // Load app icon (256x256 RGBA)
    let icon_data = {
        let raw = include_bytes!("../assets/icon_256.rgba");
        egui::IconData {
            rgba: raw.to_vec(),
            width: 256,
            height: 256,
        }
    };

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1080.0, 760.0])
            .with_min_inner_size([760.0, 480.0])
            .with_icon(icon_data),
        ..Default::default()
    };

    eframe::run_native(
        "ADB Broadcast Sender",
        options,
        Box::new(|cc| {
            // Apply custom fonts and style
            setup_custom_style(&cc.egui_ctx);
            Ok(Box::new(AdbBroadcastApp::new()))
        }),
    )
}

// ===== Color Palette (matching original web UI) =====
const C_PRIMARY: egui::Color32 = egui::Color32::from_rgb(28, 126, 214);
const C_PRIMARY_HOVER: egui::Color32 = egui::Color32::from_rgb(24, 100, 171);
const C_SUCCESS: egui::Color32 = egui::Color32::from_rgb(47, 158, 68);
const C_SUCCESS_HOVER: egui::Color32 = egui::Color32::from_rgb(43, 138, 62);
const C_DANGER: egui::Color32 = egui::Color32::from_rgb(224, 49, 49);
const C_DANGER_HOVER: egui::Color32 = egui::Color32::from_rgb(201, 42, 42);
const C_SECONDARY: egui::Color32 = egui::Color32::from_rgb(91, 103, 117);
const C_SECONDARY_HOVER: egui::Color32 = egui::Color32::from_rgb(73, 84, 97);
const C_BG: egui::Color32 = egui::Color32::from_rgb(245, 247, 250);
const C_CARD: egui::Color32 = egui::Color32::from_rgb(255, 255, 255);
const C_TEXT: egui::Color32 = egui::Color32::from_rgb(33, 37, 41);
const C_TEXT_MUTED: egui::Color32 = egui::Color32::from_rgb(92, 99, 112);
const C_TEXT_LIGHT: egui::Color32 = egui::Color32::from_rgb(134, 142, 150);
const C_WIFI_BG: egui::Color32 = egui::Color32::from_rgb(239, 247, 255);
const C_QUICK_BG: egui::Color32 = egui::Color32::from_rgb(232, 244, 255);
const C_QUICK_BORDER: egui::Color32 = egui::Color32::from_rgb(144, 202, 249);
const C_QUICK_TEXT: egui::Color32 = egui::Color32::from_rgb(25, 118, 210);
const C_SUCCESS_BG: egui::Color32 = egui::Color32::from_rgb(232, 245, 233);
const C_SUCCESS_TEXT: egui::Color32 = egui::Color32::from_rgb(46, 125, 50);
const C_ERROR_BG: egui::Color32 = egui::Color32::from_rgb(255, 235, 238);
const C_ERROR_TEXT: egui::Color32 = egui::Color32::from_rgb(198, 40, 40);
const C_INFO_BG: egui::Color32 = egui::Color32::from_rgb(227, 242, 253);
const C_INFO_TEXT: egui::Color32 = egui::Color32::from_rgb(21, 101, 192);
const C_INFO_BOX_BG: egui::Color32 = egui::Color32::from_rgb(248, 250, 252);
const C_INPUT_BORDER: egui::Color32 = egui::Color32::from_rgb(206, 212, 218);
const C_INPUT_FOCUS: egui::Color32 = egui::Color32::from_rgb(33, 150, 243);
const MAX_CARD_WIDTH: f32 = 840.0;

// Sidebar palette
const SIDEBAR_BG: egui::Color32 = egui::Color32::from_rgb(30, 41, 59);
const SIDEBAR_HOVER_BG: egui::Color32 = egui::Color32::from_rgb(44, 58, 80);
const SIDEBAR_ACTIVE_BG: egui::Color32 = egui::Color32::from_rgb(30, 62, 98);
const SIDEBAR_TEXT: egui::Color32 = egui::Color32::from_rgb(203, 213, 225);
const SIDEBAR_TEXT_MUTED: egui::Color32 = egui::Color32::from_rgb(148, 163, 184);
const SIDEBAR_DIVIDER: egui::Color32 = egui::Color32::from_rgb(71, 85, 105);

fn setup_custom_style(ctx: &egui::Context) {
    // Load a CJK-capable system font so Chinese text renders correctly
    let font_candidates = [
        r"C:\Windows\Fonts\msyh.ttc",
        r"C:\Windows\Fonts\msyhbd.ttc",
        r"C:\Windows\Fonts\msyhl.ttc",
        r"C:\Windows\Fonts\simhei.ttf",
        r"C:\Windows\Fonts\simsun.ttc",
    ];
    for path in font_candidates {
        if let Ok(bytes) = fs::read(path) {
            let mut fonts = egui::FontDefinitions::default();
            fonts.font_data.insert("cjk".to_owned(), egui::FontData::from_owned(bytes));
            for family in [egui::FontFamily::Proportional, egui::FontFamily::Monospace] {
                if let Some(list) = fonts.families.get_mut(&family) {
                    list.push("cjk".to_owned());
                }
            }
            ctx.set_fonts(fonts);
            break;
        }
    }

    let mut style = (*ctx.style()).clone();
    style.spacing.item_spacing = egui::vec2(8.0, 8.0);
    style.spacing.button_padding = egui::vec2(14.0, 9.0);
    style.visuals.panel_fill = C_BG;
    style.visuals.window_fill = C_BG;
    style.visuals.window_rounding = egui::Rounding::same(8.0);
    style.visuals.widgets.inactive.weak_bg_fill = C_SECONDARY;
    style.visuals.widgets.inactive.bg_fill = C_SECONDARY;
    style.visuals.widgets.inactive.fg_stroke = egui::Stroke::new(1.0, egui::Color32::WHITE);
    style.visuals.widgets.hovered.weak_bg_fill = C_SECONDARY_HOVER;
    style.visuals.widgets.hovered.bg_fill = C_SECONDARY_HOVER;
    style.visuals.widgets.hovered.fg_stroke = egui::Stroke::new(1.0, egui::Color32::WHITE);
    style.visuals.widgets.active.weak_bg_fill = C_SECONDARY_HOVER;
    style.visuals.widgets.active.bg_fill = C_SECONDARY_HOVER;
    style.visuals.widgets.active.fg_stroke = egui::Stroke::new(1.0, egui::Color32::WHITE);
    style.visuals.button_frame = true;
    ctx.set_style(style);
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Page {
    Devices,
    Broadcast,
    TextInput,
    InstallApk,
    Settings,
}

struct AdbBroadcastApp {
    state: AppState,
    current_page: Page,
    devices: Vec<Device>,
    action_input: String,
    content_input: String,
    wifi_ip: String,
    wifi_port: String,
    custom_content: String,
    quick_contents: Vec<String>,
    wifi_tags: Vec<String>,
    custom_wifi_tag: String,
    selected_device_serial: String,
    adb_path_input: String,
    screenshot_dir_input: String,
    apk_path_input: String,
    text_input: String,
    last_connected_ip: Option<String>,
    pending_action: String,
    bg_queue: Arc<Mutex<Vec<BgTaskResult>>>,
    toast_message: Option<String>,
    toast_remaining: f64,
    toast_is_error: bool,
    show_qr_dialog: bool,
    qr_content: String,
}

impl AdbBroadcastApp {
    fn new() -> Self {
        let mut app = Self {
            current_page: Page::Devices,
            state: AppState::new().expect("Failed to initialize app state"),
            devices: Vec::new(),
            action_input: "com.sunmi.scanner.ACTION_DATA_CODE_RECEIVED".to_string(),
            content_input: String::new(),
            wifi_ip: String::new(),
            wifi_port: "5555".to_string(),
            custom_content: String::new(),
            custom_wifi_tag: String::new(),
            quick_contents: vec![
                r#"{"dock":"D-1"}"#.to_string(),
                r#"{"dock":"D-7"}"#.to_string(),
                "fjsdhf6545651a".to_string(),
                r#"{"cashId":1,"locationCode":"N1"}"#.to_string(),
                "https://example.com".to_string(),
                r#"{"code":"12345","type":"qr"}"#.to_string(),
            ],
            wifi_tags: Vec::new(),
            selected_device_serial: String::new(),
            adb_path_input: String::new(),
            screenshot_dir_input: String::new(),
            apk_path_input: String::new(),
            text_input: String::new(),
            last_connected_ip: None,
            pending_action: String::new(),
            bg_queue: Arc::new(Mutex::new(Vec::new())),
            toast_message: None,
            toast_remaining: 0.0,
            toast_is_error: false,
            show_qr_dialog: false,
            qr_content: String::new(),
        };

        // Merge saved quick contents
        for item in &app.state.config_manager.get().quick_contents {
            if !app.quick_contents.contains(item) {
                app.quick_contents.push(item.clone());
            }
        }

        // Load wifi IP tags from config
        app.wifi_tags = app.state.config_manager.get().wifi_ips.clone();

        // Pre-fill settings page inputs from config
        app.adb_path_input = app.state.config_manager.get().adb_path.clone();
        app.screenshot_dir_input = app.state.config_manager.get().screenshot_dir.clone();

        // Try to load initial devices
        if let Ok(devices) = app.state.refresh_devices() {
            app.devices = devices;
            let last_device = app.state.config_manager.get().last_device_id.clone();
            if !last_device.is_empty() {
                app.state.select_device_by_serial(&last_device, &app.devices);
                if app.state.selected_device.is_some() {
                    app.selected_device_serial = last_device;
                }
            }
            // Auto-select first device if none selected and devices exist
            if app.selected_device_serial.is_empty() && !app.devices.is_empty() {
                if let Some(first) = app.devices.first() {
                    app.selected_device_serial = first.serial.clone();
                    app.state.select_device_by_serial(&first.serial, &app.devices);
                }
            }
        }

        app
    }

    fn refresh_devices(&mut self) {
        match self.state.refresh_devices() {
            Ok(devices) => {
                self.devices = devices;
                
                // If we just connected via WiFi, try to auto-select that device
                if let Some(ref ip) = self.last_connected_ip {
                    let port = self.wifi_port.parse::<u16>().unwrap_or(5555);
                    let expected_serial = format!("{}:{}", ip, port);
                    if let Some(device) = self.devices.iter().find(|d| d.serial == expected_serial) {
                        self.selected_device_serial = device.serial.clone();
                        self.state.select_device_by_serial(&device.serial, &self.devices);
                        self.state.set_status(
                            &format!("Connected and selected: {}", device.display_name()),
                            StatusType::Success,
                        );
                        self.last_connected_ip = None;
                        return;
                    }
                    // Also try without port for already-connected devices
                    if let Some(device) = self.devices.iter().find(|d| d.serial.starts_with(ip)) {
                        self.selected_device_serial = device.serial.clone();
                        self.state.select_device_by_serial(&device.serial, &self.devices);
                        self.state.set_status(
                            &format!("Connected and selected: {}", device.display_name()),
                            StatusType::Success,
                        );
                        self.last_connected_ip = None;
                        return;
                    }
                    self.last_connected_ip = None;
                }
                
                // Maintain existing selection if device still exists
                if !self.selected_device_serial.is_empty() {
                    let exists = self.devices.iter().any(|d| d.serial == self.selected_device_serial);
                    if !exists {
                        self.selected_device_serial.clear();
                        self.state.select_device(None);
                        // Auto-select first device if available
                        if let Some(first) = self.devices.first() {
                            self.selected_device_serial = first.serial.clone();
                            self.state.select_device_by_serial(&first.serial, &self.devices);
                        }
                    } else {
                        self.state.select_device_by_serial(&self.selected_device_serial, &self.devices);
                    }
                } else if !self.devices.is_empty() {
                    // Auto-select first device if none selected
                    if let Some(first) = self.devices.first() {
                        self.selected_device_serial = first.serial.clone();
                        self.state.select_device_by_serial(&first.serial, &self.devices);
                    }
                }
                
                self.state.set_status(
                    &format!("Found {} device(s)", self.devices.len()),
                    StatusType::Success,
                );
            }
            Err(e) => {
                self.state.set_status(&format!("Failed to refresh devices: {}", e), StatusType::Error);
            }
        }
    }

    fn process_bg_results(&mut self) {
        let mut queue = self.bg_queue.lock().unwrap();
        while let Some(result) = queue.pop() {
            match result.op {
                BgOp::RefreshDevices => {
                    self.pending_action.clear();
                    if result.success {
                        if let Some(devices) = result.devices {
                            self.devices = devices;
                            // Maintain selection logic
                            if !self.selected_device_serial.is_empty() {
                                let exists = self.devices.iter().any(|d| d.serial == self.selected_device_serial);
                                if !exists {
                                    self.selected_device_serial.clear();
                                    self.state.select_device(None);
                                    if let Some(first) = self.devices.first() {
                                        self.selected_device_serial = first.serial.clone();
                                        self.state.select_device_by_serial(&first.serial, &self.devices);
                                    }
                                } else {
                                    self.state.select_device_by_serial(&self.selected_device_serial, &self.devices);
                                }
                            } else if !self.devices.is_empty() {
                                if let Some(first) = self.devices.first() {
                                    self.selected_device_serial = first.serial.clone();
                                    self.state.select_device_by_serial(&first.serial, &self.devices);
                                }
                            }
                        }
                        self.state.set_status(&result.message, StatusType::Success);
                    } else {
                        self.state.set_status(&result.message, StatusType::Error);
                    }
                }
                BgOp::ConnectWifi => {
                    self.pending_action.clear();
                    if result.success {
                        // Refresh devices after connect
                        self.pending_action = "Refreshing...".to_string();
                        let adb_path = self.state.config_manager.get().adb_path.clone();
                        let queue = self.bg_queue.clone();
                        thread::spawn(move || {
                        let output = std::process::Command::new(&adb_path)
                            .creation_flags(0x08000000)
                            .args(["devices", "-l"])
                                .output();
                            match output {
                                Ok(o) => {
                                    let stdout = String::from_utf8_lossy(&o.stdout);
                                    let devices = crate::adb::device_manager::DeviceManager::parse_device_list_static(&stdout);
                                    let _ = queue.lock().map(|mut q| {
                                        q.push(BgTaskResult {
                                            op: BgOp::RefreshDevices,
                                            success: true,
                                            message: format!("Found {} device(s)", devices.len()),
                                            devices: Some(devices),
                                            broadcast_content: None,
                            broadcast_device: None,
                                        });
                                    });
                                }
                                Err(e) => {
                                    let _ = queue.lock().map(|mut q| {
                                        q.push(BgTaskResult {
                                            op: BgOp::RefreshDevices,
                                            success: false,
                                            message: format!("Refresh failed: {}", e),
                                            devices: None,
                                            broadcast_content: None,
                            broadcast_device: None,
                                        });
                                    });
                                }
                            }
                        });
                        self.state.set_status(&result.message, StatusType::Success);
                    } else {
                        self.state.set_status(&result.message, StatusType::Error);
                    }
                }
                BgOp::DisconnectWifi => {
                    self.pending_action.clear();
                    if result.success {
                        self.state.set_status(&result.message, StatusType::Success);
                    } else {
                        self.state.set_status(&result.message, StatusType::Error);
                    }
                    // Auto-refresh after disconnect too
                    self.spawn_refresh();
                }
                BgOp::SendBroadcast => {
                    self.pending_action.clear();
                    if let (Some(content), Some(device)) = (&result.broadcast_content, &result.broadcast_device) {
                        let error = if result.success { None } else { Some(result.message.as_str()) };
                        self.state.add_broadcast_history(content, device, result.success, error);
                    }
                    if result.success {
                        self.content_input.clear();
                        self.state.set_status("Broadcast sent successfully!", StatusType::Success);
                    } else {
                        self.toast_message = Some(result.message.clone());
                        self.toast_remaining = 4.0;
                        self.toast_is_error = true;
                        self.state.set_status(&result.message, StatusType::Error);
                    }
                }
                BgOp::TakeScreenshot => {
                    self.pending_action.clear();
                    if result.success {
                        self.toast_message = Some(result.message.clone());
                        self.toast_remaining = 4.0;
                        self.toast_is_error = false;
                        self.state.set_status(&result.message, StatusType::Success);
                    } else {
                        self.state.set_status(&result.message, StatusType::Error);
                    }
                }
                BgOp::InputText => {
                    self.pending_action.clear();
                    if result.success {
                        self.state.set_status("文本已写入", StatusType::Success);
                    } else {
                        self.state.set_status(&result.message, StatusType::Error);
                    }
                }
                BgOp::InstallApk => {
                    self.pending_action.clear();
                    if result.success {
                        self.toast_message = Some("APK 安装成功".to_string());
                        self.toast_remaining = 4.0;
                        self.toast_is_error = false;
                        self.state.set_status("APK 安装成功", StatusType::Success);
                    } else {
                        self.toast_message = Some(result.message.clone());
                        self.toast_remaining = 4.0;
                        self.toast_is_error = true;
                        self.state.set_status(&result.message, StatusType::Error);
                    }
                }
            }
        }
    }

    fn spawn_refresh(&self) {
        let adb_path = self.state.config_manager.get().adb_path.clone();
        let queue = self.bg_queue.clone();
        thread::spawn(move || {
            let output = std::process::Command::new(&adb_path)
                .creation_flags(0x08000000)
                .args(["devices", "-l"])
                .output();
            match output {
                Ok(o) => {
                    let stdout = String::from_utf8_lossy(&o.stdout);
                    let devices = DeviceManager::parse_device_list_static(&stdout);
                    let _ = queue.lock().map(|mut q| {
                        q.push(BgTaskResult {
                            op: BgOp::RefreshDevices,
                            success: true,
                            message: format!("Found {} device(s)", devices.len()),
                            devices: Some(devices),
                            broadcast_content: None,
                            broadcast_device: None,
                        });
                    });
                }
                Err(e) => {
                    let _ = queue.lock().map(|mut q| {
                        q.push(BgTaskResult {
                            op: BgOp::RefreshDevices,
                            success: false,
                            message: format!("Refresh failed: {}", e),
                            devices: None,
                            broadcast_content: None,
                            broadcast_device: None,
                        });
                    });
                }
            }
        });
    }

    fn screenshot_dir(path: &str) -> Result<PathBuf, String> {
        let screenshot_dir = PathBuf::from(path.trim());
        if screenshot_dir.as_os_str().is_empty() {
            return Err("Screenshot directory cannot be empty".to_string());
        }
        fs::create_dir_all(&screenshot_dir)
            .map_err(|e| format!("Failed to create screenshot directory: {}", e))?;
        Ok(screenshot_dir)
    }

    fn screenshot_file_name(device_serial: &str) -> String {
        let safe_serial: String = device_serial
            .chars()
            .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
            .collect();
        let timestamp = chrono::Local::now().format("%Y%m%d_%H%M%S");
        format!("screenshot_{}_{}.png", safe_serial, timestamp)
    }

    fn spawn_screenshot(&self, device_serial: String) {
        let adb_path = self.state.config_manager.get().adb_path.clone();
        let screenshot_dir = self.state.config_manager.get().screenshot_dir.clone();
        let queue = self.bg_queue.clone();
        thread::spawn(move || {
            let output = std::process::Command::new(&adb_path)
                .creation_flags(0x08000000)
                .args(["-s", &device_serial, "exec-out", "screencap", "-p"])
                .output();

            match output {
                Ok(o) => {
                    let stderr = String::from_utf8_lossy(&o.stderr).trim().to_string();
                    let result = if !o.status.success() {
                        Err(if stderr.is_empty() {
                            "Screenshot command failed".to_string()
                        } else {
                            stderr
                        })
                    } else if o.stdout.is_empty() {
                        Err("Screenshot returned empty image data".to_string())
                    } else {
                        Self::screenshot_dir(&screenshot_dir).and_then(|dir| {
                            let file_path = dir.join(Self::screenshot_file_name(&device_serial));
                            fs::write(&file_path, &o.stdout)
                                .map_err(|e| format!("Failed to save screenshot: {}", e))?;
                            Ok(file_path)
                        })
                    };

                    let _ = queue.lock().map(|mut q| match result {
                        Ok(file_path) => q.push(BgTaskResult {
                            op: BgOp::TakeScreenshot,
                            success: true,
                            message: format!("Screenshot saved successfully: {}", file_path.display()),
                            devices: None,
                            broadcast_content: None,
                            broadcast_device: None,
                        }),
                        Err(e) => q.push(BgTaskResult {
                            op: BgOp::TakeScreenshot,
                            success: false,
                            message: e,
                            devices: None,
                            broadcast_content: None,
                            broadcast_device: None,
                        }),
                    });
                }
                Err(e) => {
                    let _ = queue.lock().map(|mut q| {
                        q.push(BgTaskResult {
                            op: BgOp::TakeScreenshot,
                            success: false,
                            message: format!("Screenshot failed: {}", e),
                            devices: None,
                            broadcast_content: None,
                            broadcast_device: None,
                        });
                    });
                }
            }
        });
    }
    fn connect_wifi_from_inputs(&mut self) {
        let port = self.wifi_port.parse::<u16>().unwrap_or(5555);
        let ip = self.wifi_ip.trim().to_string();
        if ip.is_empty() {
            self.state.set_status("Please enter IP address", StatusType::Error);
        } else {
            self.pending_action = "Connecting...".to_string();
            self.spawn_connect(ip, port);
        }
    }
    fn spawn_connect(&self, ip: String, port: u16) {
        let adb_path = self.state.config_manager.get().adb_path.clone();
        let queue = self.bg_queue.clone();
        thread::spawn(move || {
            let addr = format!("{}:{}", ip, port);
            let output = std::process::Command::new(&adb_path)
                .creation_flags(0x08000000)
                .args(["connect", &addr])
                .output();
            match output {
                Ok(o) => {
                    let stdout = String::from_utf8_lossy(&o.stdout);
                    let stderr = String::from_utf8_lossy(&o.stderr);
                    let result = format!("{}{}", stdout, stderr);
                    let success = result.contains("connected to") || result.contains("already connected");
                    let _ = queue.lock().map(|mut q| {
                        q.push(BgTaskResult {
                            op: BgOp::ConnectWifi,
                            success,
                            message: if success { result.trim().to_string() } else { result.trim().to_string() },
                            devices: None,
                            broadcast_content: None,
                            broadcast_device: None,
                        });
                    });
                }
                Err(e) => {
                    let _ = queue.lock().map(|mut q| {
                        q.push(BgTaskResult {
                            op: BgOp::ConnectWifi,
                            success: false,
                            message: format!("Connection failed: {}", e),
                            devices: None,
                            broadcast_content: None,
                            broadcast_device: None,
                        });
                    });
                }
            }
        });
    }

    fn spawn_disconnect(&self, ip: String, port: u16) {
        let adb_path = self.state.config_manager.get().adb_path.clone();
        let queue = self.bg_queue.clone();
        thread::spawn(move || {
            let addr = format!("{}:{}", ip, port);
            let output = std::process::Command::new(&adb_path)
                .creation_flags(0x08000000)
                .args(["disconnect", &addr])
                .output();
            match output {
                Ok(o) => {
                    let stdout = String::from_utf8_lossy(&o.stdout);
                    let stderr = String::from_utf8_lossy(&o.stderr);
                    let result = format!("{}{}", stdout, stderr);
                    let success = result.contains("disconnected") || result.contains("not connected");
                    let _ = queue.lock().map(|mut q| {
                        q.push(BgTaskResult {
                            op: BgOp::DisconnectWifi,
                            success,
                            message: result.trim().to_string(),
                            devices: None,
                            broadcast_content: None,
                            broadcast_device: None,
                        });
                    });
                }
                Err(e) => {
                    let _ = queue.lock().map(|mut q| {
                        q.push(BgTaskResult {
                            op: BgOp::DisconnectWifi,
                            success: false,
                            message: format!("Disconnect failed: {}", e),
                            devices: None,
                            broadcast_content: None,
                            broadcast_device: None,
                        });
                    });
                }
            }
        });
    }

    fn spawn_broadcast(&self, content: String, action: Option<String>, device_serial: String) {
        let adb_path = self.state.config_manager.get().adb_path.clone();
        let queue = self.bg_queue.clone();
        let bc = content.clone();
        let ds = device_serial.clone();
        thread::spawn(move || {
            if content.trim().is_empty() {
                let _ = queue.lock().map(|mut q| {
                    q.push(BgTaskResult {
                        op: BgOp::SendBroadcast,
                        success: false,
                        message: "Broadcast content cannot be empty".to_string(),
                        devices: None,
                        broadcast_content: Some(bc.clone()),
                        broadcast_device: Some(ds.clone()),
                    });
                });
                return;
            }
            let action = action.as_deref().unwrap_or("com.sunmi.scanner.ACTION_DATA_CODE_RECEIVED");
            let escaped_content = content.replace('"', "\\\"");
            let shell_cmd = format!(r#"am broadcast -a {} --es data "{}""#, action, escaped_content);
            
            let output = std::process::Command::new(&adb_path)
                .creation_flags(0x08000000)
                .args(["-s", &device_serial, "shell", &shell_cmd])
                .output();
            
            match output {
                Ok(o) => {
                    let stdout = String::from_utf8_lossy(&o.stdout);
                    let stderr = String::from_utf8_lossy(&o.stderr);
                    let result_text = format!("{}{}", stdout, stderr);
                    let (success, msg) = if result_text.contains("result=0") || result_text.contains("Broadcast completed") {
                        (true, "Broadcast sent successfully!".to_string())
                    } else {
                        (false, result_text.trim().to_string())
                    };
                    let _ = queue.lock().map(|mut q| {
                        q.push(BgTaskResult {
                            op: BgOp::SendBroadcast,
                            success,
                            message: msg.clone(),
                            devices: None,
                            broadcast_content: Some(bc.clone()),
                            broadcast_device: Some(ds.clone()),
                        });
                    });
                }
                Err(e) => {
                    let _ = queue.lock().map(|mut q| {
                        q.push(BgTaskResult {
                            op: BgOp::SendBroadcast,
                            success: false,
                            message: format!("Failed: {}", e),
                            devices: None,
                            broadcast_content: Some(bc.clone()),
                            broadcast_device: Some(ds.clone()),
                        });
                    });
                }
            }
        });
    }

    fn spawn_input_text(&self, content: String, device_serial: String) {
        let adb_path = self.state.config_manager.get().adb_path.clone();
        let queue = self.bg_queue.clone();
        thread::spawn(move || {
            if content.trim().is_empty() {
                let _ = queue.lock().map(|mut q| {
                    q.push(BgTaskResult {
                        op: BgOp::InputText,
                        success: false,
                        message: "输入内容不能为空".to_string(),
                        devices: None,
                        broadcast_content: None,
                        broadcast_device: None,
                    });
                });
                return;
            }
            let escaped = escape_input_text(&content);
            let shell_cmd = format!("input text {}", escaped);
            let output = std::process::Command::new(&adb_path)
                .creation_flags(0x08000000)
                .args(["-s", &device_serial, "shell", &shell_cmd])
                .output();
            match output {
                Ok(o) => {
                    let stdout = String::from_utf8_lossy(&o.stdout);
                    let stderr = String::from_utf8_lossy(&o.stderr);
                    let result_text = format!("{}{}", stdout, stderr);
                    let (success, msg) = if o.status.success() {
                        (true, "文本已写入".to_string())
                    } else {
                        (false, result_text.trim().to_string())
                    };
                    let _ = queue.lock().map(|mut q| {
                        q.push(BgTaskResult {
                            op: BgOp::InputText,
                            success,
                            message: msg,
                            devices: None,
                            broadcast_content: None,
                            broadcast_device: None,
                        });
                    });
                }
                Err(e) => {
                    let _ = queue.lock().map(|mut q| {
                        q.push(BgTaskResult {
                            op: BgOp::InputText,
                            success: false,
                            message: format!("写入失败: {}", e),
                            devices: None,
                            broadcast_content: None,
                            broadcast_device: None,
                        });
                    });
                }
            }
        });
    }

    fn spawn_install_apk(&self, apk_path: String, device_serial: String) {
        let adb_path = self.state.config_manager.get().adb_path.clone();
        let queue = self.bg_queue.clone();
        thread::spawn(move || {
            let output = std::process::Command::new(&adb_path)
                .creation_flags(0x08000000)
                .args(["-s", &device_serial, "install", "-r", &apk_path])
                .output();
            match output {
                Ok(o) => {
                    let stdout = String::from_utf8_lossy(&o.stdout);
                    let stderr = String::from_utf8_lossy(&o.stderr);
                    let result_text = format!("{}{}", stdout, stderr);
                    let (success, msg) = if result_text.contains("Success") {
                        (true, "APK 安装成功".to_string())
                    } else {
                        (false, result_text.trim().to_string())
                    };
                    let _ = queue.lock().map(|mut q| {
                        q.push(BgTaskResult {
                            op: BgOp::InstallApk,
                            success,
                            message: msg,
                            devices: None,
                            broadcast_content: None,
                            broadcast_device: None,
                        });
                    });
                }
                Err(e) => {
                    let _ = queue.lock().map(|mut q| {
                        q.push(BgTaskResult {
                            op: BgOp::InstallApk,
                            success: false,
                            message: format!("安装失败: {}", e),
                            devices: None,
                            broadcast_content: None,
                            broadcast_device: None,
                        });
                    });
                }
            }
        });
    }
}

impl AdbBroadcastApp {
    // ===== Page helpers =====
    fn page_header(&mut self, ui: &mut egui::Ui, title: &str, subtitle: &str) {
        let (_, side_margin) = self.card_widths(ui);
        ui.horizontal(|ui| {
            ui.add_space(side_margin);
            ui.label(egui::RichText::new(title).size(20.0).strong().color(C_TEXT));
            ui.add_space(10.0);
            if !self.pending_action.is_empty() {
                ui.add(egui::Spinner::new());
                ui.add_space(4.0);
                ui.label(egui::RichText::new(&self.pending_action).size(12.0).color(C_PRIMARY));
            }
        });
        ui.add_space(2.0);
        ui.horizontal(|ui| {
            ui.add_space(side_margin);
            ui.label(egui::RichText::new(subtitle).size(12.0).color(C_TEXT_LIGHT));
        });
        ui.add_space(18.0);
    }

    fn card_widths(&self, ui: &egui::Ui) -> (f32, f32) {
        let available_width = ui.available_width();
        let card_width = available_width.min(MAX_CARD_WIDTH);
        let side_margin = ((available_width - card_width) / 2.0).max(12.0);
        (card_width, side_margin)
    }

    // ===== Page: Device List =====
    fn page_devices(&mut self, ui: &mut egui::Ui) {
        self.page_header(ui, "设备列表", "查看与管理已连接的 ADB 设备");
        let (card_width, side_margin) = self.card_widths(ui);
        ui.spacing_mut().item_spacing = egui::vec2(0.0, 16.0);

        // WiFi connect card (top)
        centered_card(ui, card_width, side_margin, |ui| {
            card_frame().show(ui, |ui| {
                    ui.set_min_width(ui.available_width());
                card_header(ui, "WiFi 连接", C_QUICK_TEXT);
                ui.add_space(14.0);

                let mut port_enter_pressed = false;
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new("IP:").strong().color(C_TEXT).size(13.0));
                    ui.add_space(4.0);
                    let ip_w = (ui.available_width() * 0.55 - 60.0).max(140.0);
                    input_frame().show(ui, |ui| {
                        ui.add(egui::TextEdit::singleline(&mut self.wifi_ip).desired_width(ip_w));
                    });
                    ui.add_space(8.0);
                    ui.label(egui::RichText::new("Port:").strong().color(C_TEXT).size(13.0));
                    ui.add_space(4.0);
                    input_frame().show(ui, |ui| {
                        let port_response = ui.add(egui::TextEdit::singleline(&mut self.wifi_port).desired_width(90.0));
                        port_enter_pressed = port_response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                    });
                });

                if port_enter_pressed && self.pending_action.is_empty() {
                    self.connect_wifi_from_inputs();
                }

                ui.add_space(10.0);
                ui.label(egui::RichText::new("快速 IP").strong().color(C_QUICK_TEXT).size(12.0));
                ui.add_space(4.0);
                ui.horizontal_wrapped(|ui| {
                    let mut to_remove: Option<String> = None;
                    for ip in &self.wifi_tags {
                        let label = if ip.len() > 14 {
                            format!("{}..", &ip[..14])
                        } else {
                            ip.clone()
                        };
                        let btn_response = ui.add(
                            egui::Button::new(
                                egui::RichText::new(format!("{}  ×", label)).size(11.0).color(C_QUICK_TEXT)
                            )
                            .fill(C_QUICK_BG)
                            .stroke(egui::Stroke::new(1.0, C_QUICK_BORDER))
                            .rounding(egui::Rounding::same(5.0))
                            .min_size(egui::vec2(0.0, 28.0))
                        );
                        if btn_response.clicked() {
                            if let Some(pointer) = ui.ctx().pointer_interact_pos() {
                                let rect = btn_response.rect;
                                let x_zone = egui::Rect::from_min_max(
                                    egui::pos2(rect.right() - 24.0, rect.top()),
                                    rect.right_bottom(),
                                );
                                if x_zone.contains(pointer) {
                                    to_remove = Some(ip.clone());
                                } else {
                                    self.wifi_ip = ip.clone();
                                    self.state.set_status(&format!("IP 已设置: {}", ip), StatusType::Success);
                                }
                            }
                        }
                        ui.add_space(4.0);
                    }
                    if let Some(ip) = to_remove {
                        self.wifi_tags.retain(|i| i != &ip);
                        let _ = self.state.config_manager.set_wifi_ips(self.wifi_tags.clone());
                        self.state.set_status(&format!("已移除: {}", ip), StatusType::Info);
                    }
                });

                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    let add_width = (ui.available_width() - 120.0).max(120.0);
                    input_frame().show(ui, |ui| {
                        ui.add(egui::TextEdit::singleline(&mut self.custom_wifi_tag).desired_width(add_width).hint_text("添加 IP..."));
                    });
                    ui.add_space(6.0);
                    if secondary_button(ui, "添加").clicked() {
                        let ip = self.custom_wifi_tag.trim();
                        if !ip.is_empty() {
                            self.wifi_tags.push(ip.to_string());
                            let _ = self.state.config_manager.add_wifi_ip(ip.to_string());
                            self.custom_wifi_tag.clear();
                            self.state.set_status("IP 地址已添加", StatusType::Success);
                        }
                    }
                });

                ui.add_space(12.0);
                ui.horizontal(|ui| {
                    let is_busy = !self.pending_action.is_empty();
                    if success_button(ui, "连接设备").clicked() && !is_busy {
                        self.connect_wifi_from_inputs();
                    }
                });
            });
        });

        // Device info grid card
        centered_card(ui, card_width, side_margin, |ui| {
            card_frame().show(ui, |ui| {
                    ui.set_min_width(ui.available_width());
                ui.horizontal(|ui| {
                    let bar_rect = egui::Rect::from_min_size(
                        ui.next_widget_position(),
                        egui::vec2(3.0, 18.0),
                    );
                    ui.painter().rect_filled(bar_rect, egui::Rounding::same(1.5), C_SUCCESS);
                    ui.add_space(10.0);
                    ui.label(egui::RichText::new("设备信息").size(15.0).color(C_TEXT).strong());
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let is_busy = !self.pending_action.is_empty();
                        if secondary_button(ui, "刷新设备").clicked() && !is_busy {
                            self.pending_action = "正在刷新设备...".to_string();
                            self.spawn_refresh();
                        }
                        ui.add_space(8.0);
                        status_badge(ui, &format!("{} 台设备", self.devices.len()), C_QUICK_BG, C_QUICK_TEXT);
                    });
                });
                ui.add_space(14.0);

                let devices = self.devices.clone();
                if devices.is_empty() {
                    info_box(ui, |ui| {
                        ui.set_min_width(ui.available_width());
                        ui.vertical(|ui| {
                            ui.label(egui::RichText::new("暂无设备，请通过上方 WiFi 连接或点击刷新").color(C_TEXT_MUTED).size(13.0));
                            ui.add_space(10.0);
                            ui.horizontal(|ui| {
                                let is_busy = !self.pending_action.is_empty();
                                if primary_button(ui, "刷新设备").clicked() && !is_busy {
                                    self.pending_action = "正在刷新设备...".to_string();
                                    self.spawn_refresh();
                                }
                                ui.add_space(8.0);
                                if secondary_button(ui, "ADB 路径").clicked() {
                                    self.adb_path_input = self.state.config_manager.get().adb_path.clone();
                                    self.current_page = Page::Settings;
                                }
                                ui.add_space(8.0);
                                if secondary_button(ui, "截图目录").clicked() {
                                    self.screenshot_dir_input = self.state.config_manager.get().screenshot_dir.clone();
                                    self.current_page = Page::Settings;
                                }
                            });
                        });
                    });
                } else {
                    let total = ui.available_width();
                    let gap = 12.0;
                    let item_w = ((total - gap) / 2.0).max(240.0);
                    let row_h = 170.0;
                    let n = devices.len();
                    let rows = (n + 1) / 2;
                    let total_h = rows as f32 * row_h + (rows.saturating_sub(1) as f32) * gap;
                    let (_, rect) = ui.allocate_space(egui::vec2(total, total_h));
                    let origin = rect.min;

                    for (i, device) in devices.iter().enumerate() {
                        let col = i % 2;
                        let row = i / 2;
                        let cell_min = egui::pos2(
                            origin.x + (item_w + gap) * col as f32,
                            origin.y + (row_h + gap) * row as f32,
                        );
                        let cell_rect = egui::Rect::from_min_size(cell_min, egui::vec2(item_w, row_h));
                        ui.allocate_new_ui(
                            egui::UiBuilder::new().max_rect(cell_rect).layout(egui::Layout::top_down(egui::Align::LEFT)),
                            |ui| {
                                self.render_device_card(ui, device);
                            },
                        );
                    }
                }
            });
        });
    }

    // Render a single device card inside the grid
    fn render_device_card(&mut self, ui: &mut egui::Ui, device: &Device) {
        let selected = self.selected_device_serial == device.serial;
        let wifi_endpoint = device.wifi_endpoint();
        let is_busy = !self.pending_action.is_empty();

        egui::Frame::none()
            .fill(if selected { C_WIFI_BG } else { C_CARD })
            .stroke(egui::Stroke::new(
                if selected { 2.0 } else { 1.0 },
                if selected { C_PRIMARY } else { egui::Color32::from_rgb(228, 232, 240) },
            ))
            .rounding(egui::Rounding::same(8.0))
            .inner_margin(egui::Margin::symmetric(14.0, 12.0))
            .show(ui, |ui| {
                ui.set_min_width(ui.available_width());

                // Header: device name + state badge
                ui.horizontal(|ui| {
                    ui.add(
                        egui::Label::new(
                            egui::RichText::new(&device.display_name()).strong().size(13.5).color(C_TEXT),
                        )
                        .truncate(),
                    );
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        status_badge(ui, &device.state, C_QUICK_BG, C_QUICK_TEXT);
                    });
                });

                ui.add_space(8.0);

                // Info: two-column layout for Serial/State and Connection
                ui.label(egui::RichText::new(format!("Serial: {}", device.serial)).size(12.0).color(C_TEXT_MUTED));
                ui.add_space(2.0);
                ui.label(egui::RichText::new(format!("Connection: {}", device.connection_info())).size(12.0).color(C_TEXT_MUTED));

                ui.add_space(10.0);

                // Actions row
                ui.horizontal(|ui| {
                    if selected {
                        status_badge(ui, "已选中", C_SUCCESS_BG, C_SUCCESS_TEXT);
                    } else if secondary_button(ui, "选择").clicked() && !is_busy {
                        self.selected_device_serial = device.serial.clone();
                        self.state.select_device_by_serial(&device.serial, &self.devices);
                        self.state.set_status(&format!("已选择: {}", device.display_name()), StatusType::Success);
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if let Some((ip, port)) = wifi_endpoint.clone() {
                            if danger_button(ui, "断开").clicked() && !is_busy {
                                self.pending_action = "正在断开...".to_string();
                                self.spawn_disconnect(ip.clone(), port);
                            }
                            ui.add_space(6.0);
                        }
                        if secondary_button(ui, "截图").clicked() && !is_busy {
                            self.pending_action = "正在截图...".to_string();
                            self.spawn_screenshot(device.serial.clone());
                        }
                    });
                });
            });
    }

    // Shared device selection card (dropdown + refresh)
    fn device_select_card(&mut self, ui: &mut egui::Ui, card_width: f32, side_margin: f32) {
        centered_card(ui, card_width, side_margin, |ui| {
            card_frame().show(ui, |ui| {
                    ui.set_min_width(ui.available_width());
                card_header(ui, "选择设备", C_PRIMARY);
                ui.add_space(14.0);

                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new("选择设备:").strong().color(C_TEXT));
                    ui.add_space(8.0);

                    let selected_text = if self.selected_device_serial.is_empty() {
                        "-- 未找到设备 --".to_string()
                    } else {
                        self.devices
                            .iter()
                            .find(|d| d.serial == self.selected_device_serial)
                            .map(|d| d.display_name())
                            .unwrap_or_else(|| "-- 请选择设备 --".to_string())
                    };

                    let combo_width = (ui.available_width() - 20.0).max(200.0);
                    input_frame().show(ui, |ui| {
                        ui.set_min_width(combo_width - 20.0);
                        egui::ComboBox::from_id_salt("device_select")
                            .width(combo_width - 20.0)
                            .selected_text(selected_text)
                            .show_ui(ui, |ui| {
                                if self.devices.is_empty() {
                                    let _ = ui.selectable_label(true, "-- 未找到设备 --");
                                } else {
                                    for device in &self.devices {
                                        let selected = self.selected_device_serial == device.serial;
                                        if ui.selectable_label(selected, device.display_name()).clicked() {
                                            self.selected_device_serial = device.serial.clone();
                                            self.state.select_device_by_serial(&device.serial, &self.devices);
                                            self.state.set_status("设备已选择", StatusType::Success);
                                        }
                                    }
                                }
                            });
                    });
                });

                ui.add_space(12.0);
                ui.horizontal(|ui| {
                    let is_busy = !self.pending_action.is_empty();
                    if primary_button(ui, "刷新设备").clicked() && !is_busy {
                        self.pending_action = "正在刷新设备...".to_string();
                        self.spawn_refresh();
                    }
                    ui.add_space(8.0);
                    if self.devices.is_empty() {
                        ui.label(egui::RichText::new("未检测到设备，请先在设备列表页通过 WiFi 连接设备").color(C_TEXT_LIGHT).size(12.0));
                    }
                });
            });
        });
    }

    // ===== Page: Text Input =====
    fn page_text_input(&mut self, ui: &mut egui::Ui) {
        self.page_header(ui, "文本输入", "向选定设备输入文本（adb shell input text）");
        let (card_width, side_margin) = self.card_widths(ui);
        ui.spacing_mut().item_spacing = egui::vec2(0.0, 16.0);

        self.device_select_card(ui, card_width, side_margin);

        centered_card(ui, card_width, side_margin, |ui| {
            card_frame().show(ui, |ui| {
                    ui.set_min_width(ui.available_width());
                card_header(ui, "输入内容", C_PRIMARY);
                ui.add_space(14.0);

                let input_width = (ui.available_width() - 4.0).max(200.0);
                input_frame().show(ui, |ui| {
                    ui.add(
                        egui::TextEdit::multiline(&mut self.text_input)
                            .desired_rows(4)
                            .desired_width(input_width)
                            .hint_text("输入要写入设备的文本..."),
                    );
                });

                ui.add_space(10.0);
                ui.label(
                    egui::RichText::new("提示：仅支持 ASCII 字符，空格与换行自动转义为 %s，中文等字符将转换为 ?")
                        .size(11.0)
                        .color(C_TEXT_LIGHT),
                );

                ui.add_space(12.0);
                ui.horizontal(|ui| {
                    let is_busy = !self.pending_action.is_empty();
                    if primary_button(ui, "写入").clicked() && !is_busy {
                        if self.text_input.trim().is_empty() {
                            self.state.set_status("请输入要写入的文本", StatusType::Error);
                        } else if self.state.selected_device.is_none() {
                            self.state.set_status("请先选择设备", StatusType::Error);
                        } else {
                            self.pending_action = "正在写入文本...".to_string();
                            if let Some(ref device) = self.state.selected_device {
                                self.spawn_input_text(self.text_input.clone(), device.serial.clone());
                            }
                        }
                    }
                });
            });
        });
    }

    // ===== Page: Install APK =====
    fn page_install_apk(&mut self, ui: &mut egui::Ui) {
        self.page_header(ui, "安装 APK", "选择 APK 文件并安装到选定设备");
        let (card_width, side_margin) = self.card_widths(ui);
        ui.spacing_mut().item_spacing = egui::vec2(0.0, 16.0);

        self.device_select_card(ui, card_width, side_margin);

        centered_card(ui, card_width, side_margin, |ui| {
            card_frame().show(ui, |ui| {
                    ui.set_min_width(ui.available_width());
                card_header(ui, "APK 文件", C_PRIMARY);
                ui.add_space(14.0);

                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new("APK 路径:").strong().color(C_TEXT));
                    ui.add_space(8.0);
                    let input_width = (ui.available_width() - 180.0).max(160.0);
                    input_frame().show(ui, |ui| {
                        ui.add(
                            egui::TextEdit::singleline(&mut self.apk_path_input)
                                .desired_width(input_width)
                                .hint_text("未选择 APK 文件"),
                        );
                    });
                });

                ui.add_space(12.0);
                ui.horizontal(|ui| {
                    if secondary_button(ui, "选择文件").clicked() {
                        if let Some(path) = rfd::FileDialog::new()
                            .set_title("选择 APK 文件")
                            .add_filter("APK 文件", &["apk"])
                            .pick_file()
                        {
                            self.apk_path_input = path.display().to_string();
                            self.state.set_status(&format!("已选择: {}", path.display()), StatusType::Success);
                        }
                    }
                    ui.add_space(8.0);
                    let is_busy = !self.pending_action.is_empty();
                    if success_button(ui, "安装到设备").clicked() && !is_busy {
                        let path = self.apk_path_input.trim();
                        if path.is_empty() {
                            self.state.set_status("请先选择 APK 文件", StatusType::Error);
                        } else if !std::path::Path::new(path).exists() {
                            self.state.set_status("APK 文件不存在，请重新选择", StatusType::Error);
                        } else if self.state.selected_device.is_none() {
                            self.state.set_status("请先选择设备", StatusType::Error);
                        } else {
                            self.pending_action = "正在安装 APK...".to_string();
                            if let Some(ref device) = self.state.selected_device {
                                self.spawn_install_apk(path.to_string(), device.serial.clone());
                            }
                        }
                    }
                });
            });
        });
    }

    // ===== Page: Broadcast =====
    fn page_broadcast(&mut self, ui: &mut egui::Ui) {
        self.page_header(ui, "广播发送", "向选定设备发送 Android 广播 Intent");
        let (card_width, side_margin) = self.card_widths(ui);
        ui.spacing_mut().item_spacing = egui::vec2(0.0, 16.0);

        self.device_select_card(ui, card_width, side_margin);

        // Broadcast settings card
        centered_card(ui, card_width, side_margin, |ui| {
            card_frame().show(ui, |ui| {
                    ui.set_min_width(ui.available_width());
                card_header(ui, "广播设置", C_PRIMARY);
                ui.add_space(14.0);

                ui.label(egui::RichText::new("Action（默认: com.sunmi.scanner.ACTION_DATA_CODE_RECEIVED）:").strong().color(C_TEXT).size(13.0));
                ui.add_space(4.0);
                let input_width = (ui.available_width() - 4.0).max(200.0);
                input_frame().show(ui, |ui| {
                    ui.add(egui::TextEdit::singleline(&mut self.action_input).desired_width(input_width));
                });

                ui.add_space(12.0);
                ui.label(egui::RichText::new("Content（广播内容）:").strong().color(C_TEXT).size(13.0));
                ui.add_space(4.0);
                input_frame().show(ui, |ui| {
                    ui.add(
                        egui::TextEdit::multiline(&mut self.content_input)
                            .desired_rows(3)
                            .desired_width(input_width),
                    );
                });

                // Quick content
                ui.add_space(14.0);
                ui.separator();
                ui.add_space(14.0);
                ui.label(egui::RichText::new("快捷内容").strong().color(C_QUICK_TEXT).size(14.0));
                ui.add_space(8.0);
                ui.horizontal_wrapped(|ui| {
                    let mut to_remove: Option<String> = None;
                    for content in &self.quick_contents {
                        let label = if content.len() > 16 {
                            format!("{}..", &content[..16])
                        } else {
                            content.clone()
                        };
                        let btn_response = ui.add(
                            egui::Button::new(
                                egui::RichText::new(format!("{}  ×", label)).size(12.0).color(C_QUICK_TEXT)
                            )
                            .fill(C_QUICK_BG)
                            .stroke(egui::Stroke::new(1.0, C_QUICK_BORDER))
                            .rounding(egui::Rounding::same(5.0))
                            .min_size(egui::vec2(0.0, 28.0))
                        );
                        if btn_response.clicked() {
                            if let Some(pointer) = ui.ctx().pointer_interact_pos() {
                                let rect = btn_response.rect;
                                let x_zone = egui::Rect::from_min_max(
                                    egui::pos2(rect.right() - 24.0, rect.top()),
                                    rect.right_bottom(),
                                );
                                if x_zone.contains(pointer) {
                                    to_remove = Some(content.clone());
                                } else {
                                    self.content_input = content.clone();
                                    self.state.set_status(&format!("内容已设置: {}", content), StatusType::Success);
                                }
                            }
                        }
                        ui.add_space(4.0);
                    }
                    if let Some(content) = to_remove {
                        self.quick_contents.retain(|c| c != &content);
                        let _ = self.state.config_manager.set_quick_contents(self.quick_contents.clone());
                        self.state.set_status(&format!("已移除: {}", content), StatusType::Info);
                    }
                });

                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    let add_width = (ui.available_width() - 130.0).max(150.0);
                    input_frame().show(ui, |ui| {
                        ui.add(egui::TextEdit::singleline(&mut self.custom_content).desired_width(add_width).hint_text("添加自定义内容..."));
                    });
                    ui.add_space(6.0);
                    if secondary_button(ui, "添加").clicked() {
                        let content = self.custom_content.trim();
                        if !content.is_empty() {
                            self.quick_contents.push(content.to_string());
                            let _ = self.state.config_manager.add_quick_content(content.to_string());
                            self.custom_content.clear();
                            self.state.set_status("自定义内容已添加", StatusType::Success);
                        }
                    }
                });

                ui.add_space(14.0);
                ui.separator();
                ui.add_space(14.0);
                ui.horizontal(|ui| {
                    let is_busy = !self.pending_action.is_empty();
                    if primary_button(ui, "发送广播").clicked() && !is_busy {
                        if self.content_input.trim().is_empty() {
                            self.state.set_status("请输入广播内容", StatusType::Error);
                        } else if self.state.selected_device.is_none() {
                            self.state.set_status("请先选择设备", StatusType::Error);
                        } else {
                            self.pending_action = "正在发送广播...".to_string();
                            let action = if self.action_input.trim().is_empty() {
                                None
                            } else {
                                Some(self.action_input.trim().to_string())
                            };
                            if let Some(ref device) = self.state.selected_device {
                                self.spawn_broadcast(
                                    self.content_input.clone(),
                                    action,
                                    device.serial.clone(),
                                );
                            }
                        }
                    }
                    ui.add_space(8.0);
                    if secondary_button(ui, "生成二维码").clicked() {
                        self.qr_content = self.content_input.clone();
                        self.show_qr_dialog = true;
                    }
                });
            });
        });

        // History card
        centered_card(ui, card_width, side_margin, |ui| {
            card_frame().show(ui, |ui| {
                    ui.set_min_width(ui.available_width());
                ui.horizontal(|ui| {
                    let bar_rect = egui::Rect::from_min_size(
                        ui.next_widget_position(),
                        egui::vec2(3.0, 18.0),
                    );
                    ui.painter().rect_filled(bar_rect, egui::Rounding::same(1.5), C_PRIMARY);
                    ui.add_space(10.0);
                    ui.label(egui::RichText::new("广播历史").size(15.0).color(C_TEXT).strong());
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if secondary_button(ui, "清空历史").clicked() {
                            self.state.clear_history();
                            self.state.set_status("历史已清空", StatusType::Success);
                        }
                    });
                });
                ui.add_space(14.0);

                let history = self.state.get_history();
                if history.is_empty() {
                    ui.vertical_centered(|ui| {
                        ui.add_space(20.0);
                        ui.label(egui::RichText::new("暂无广播历史").color(C_TEXT_LIGHT).size(13.0));
                        ui.add_space(4.0);
                        ui.label(egui::RichText::new("发送的广播将显示在这里").color(C_TEXT_LIGHT).size(12.0));
                        ui.add_space(20.0);
                    });
                } else {
                    egui::ScrollArea::vertical()
                        .id_salt("history_scroll")
                        .max_height(260.0)
                        .show(ui, |ui| {
                            for broadcast in history.iter().rev() {
                                let left_border = if broadcast.is_success() {
                                    egui::Color32::from_rgb(76, 175, 80)
                                } else {
                                    egui::Color32::from_rgb(244, 67, 54)
                                };
                                let (status_bg, status_text) = if broadcast.is_success() {
                                    (C_SUCCESS_BG, C_SUCCESS_TEXT)
                                } else {
                                    (C_ERROR_BG, C_ERROR_TEXT)
                                };

                                egui::Frame::none()
                                    .fill(C_CARD)
                                    .rounding(egui::Rounding::same(6.0))
                                    .inner_margin(egui::Margin::symmetric(0.0, 10.0))
                                    .shadow(egui::epaint::Shadow {
                                        offset: egui::Vec2::new(0.0, 1.0),
                                        blur: 3.0,
                                        spread: 0.0,
                                        color: egui::Color32::from_black_alpha(8),
                                    })
                                    .show(ui, |ui| {
                                        ui.horizontal(|ui| {
                                            let bar_height = 60.0;
                                            let (bar_rect, _) = ui.allocate_exact_size(
                                                egui::vec2(4.0, bar_height),
                                                egui::Sense::hover(),
                                            );
                                            ui.painter().rect_filled(
                                                bar_rect,
                                                egui::Rounding {
                                                    nw: 3.0, sw: 3.0, ne: 0.0, se: 0.0,
                                                },
                                                left_border,
                                            );
                                            ui.add_space(10.0);

                                            ui.vertical(|ui| {
                                                ui.horizontal(|ui| {
                                                    ui.label(
                                                        egui::RichText::new(&broadcast.formatted_timestamp())
                                                            .color(C_TEXT_LIGHT)
                                                            .size(12.0),
                                                    );
                                                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                                        status_badge(ui, &broadcast.status(), status_bg, status_text);
                                                    });
                                                });
                                                ui.add_space(6.0);
                                                let content_preview = if broadcast.content.len() > 100 {
                                                    format!("{}...", &broadcast.content[..100])
                                                } else {
                                                    broadcast.content.clone()
                                                };
                                                ui.label(
                                                    egui::RichText::new(content_preview)
                                                        .color(C_TEXT)
                                                        .size(13.0)
                                                );
                                                if let Some(ref error) = broadcast.error {
                                                    ui.add_space(4.0);
                                                    ui.label(
                                                        egui::RichText::new(error)
                                                            .color(C_ERROR_TEXT)
                                                            .size(12.0),
                                                    );
                                                }
                                            });
                                        });
                                    });
                                ui.add_space(6.0);
                            }
                        });
                }
            });
        });
    }

    // ===== Page: Settings =====
    fn page_settings(&mut self, ui: &mut egui::Ui) {
        self.page_header(ui, "设置", "配置 ADB 与截图等应用选项");
        let (card_width, side_margin) = self.card_widths(ui);
        ui.spacing_mut().item_spacing = egui::vec2(0.0, 16.0);

        // ADB path
        centered_card(ui, card_width, side_margin, |ui| {
            card_frame().show(ui, |ui| {
                    ui.set_min_width(ui.available_width());
                card_header(ui, "ADB 路径", C_PRIMARY);
                ui.add_space(12.0);
                ui.label(egui::RichText::new("ADB 可执行文件路径：").strong().color(C_TEXT).size(13.0));
                ui.add_space(4.0);
                let input_width = (ui.available_width() - 4.0).max(200.0);
                input_frame().show(ui, |ui| {
                    ui.add(egui::TextEdit::singleline(&mut self.adb_path_input).desired_width(input_width));
                });
                ui.add_space(8.0);
                ui.label(
                    egui::RichText::new("用于执行 adb 命令的 platform-tools 路径（adb.exe）。")
                        .size(11.0)
                        .color(C_TEXT_LIGHT),
                );
                ui.add_space(12.0);
                ui.horizontal(|ui| {
                    if primary_button(ui, "保存").clicked() {
                        self.state.update_adb_path(&self.adb_path_input);
                        self.state.set_status("ADB 路径已更新", StatusType::Success);
                    }
                });
            });
        });

        // Screenshot dir
        centered_card(ui, card_width, side_margin, |ui| {
            card_frame().show(ui, |ui| {
                    ui.set_min_width(ui.available_width());
                card_header(ui, "截图保存目录", C_PRIMARY);
                ui.add_space(12.0);
                ui.label(egui::RichText::new("截图保存目录：").strong().color(C_TEXT).size(13.0));
                ui.add_space(4.0);
                let input_width = (ui.available_width() - 4.0).max(200.0);
                input_frame().show(ui, |ui| {
                    ui.add(egui::TextEdit::singleline(&mut self.screenshot_dir_input).desired_width(input_width));
                });
                ui.add_space(8.0);
                ui.label(
                    egui::RichText::new("设备截图的保存路径，目录不存在时会自动创建。")
                        .size(11.0)
                        .color(C_TEXT_LIGHT),
                );
                ui.add_space(12.0);
                ui.horizontal(|ui| {
                    if primary_button(ui, "保存").clicked() {
                        self.state.update_screenshot_dir(&self.screenshot_dir_input);
                        self.state.set_status("截图目录已更新", StatusType::Success);
                    }
                    ui.add_space(8.0);
                    if secondary_button(ui, "打开目录").clicked() {
                        self.open_screenshot_dir();
                    }
                });
            });
        });

        // About
        centered_card(ui, card_width, side_margin, |ui| {
            card_frame().show(ui, |ui| {
                    ui.set_min_width(ui.available_width());
                card_header(ui, "关于", C_SECONDARY);
                ui.add_space(12.0);
                info_box(ui, |ui| {
                    ui.set_min_width(ui.available_width());
                    ui.label(egui::RichText::new("ADB Broadcast Sender v1.0.0").strong().size(13.0).color(C_TEXT));
                    ui.add_space(6.0);
                    ui.label(
                        egui::RichText::new("基于 Rust + egui 的桌面客户端，用于通过 ADB 向 Android 设备发送广播 Intent，支持 WiFi 连接、二维码生成与截图等功能。")
                            .size(12.0)
                            .color(C_TEXT_MUTED),
                    );
                });
            });
        });
    }

    // ===== Status bar =====
    fn show_status_bar(&mut self, ui: &mut egui::Ui) {
        if self.state.status_type == StatusType::None {
            return;
        }
        let (bg_color, text_color) = match self.state.status_type {
            StatusType::Success => (C_SUCCESS_BG, C_SUCCESS_TEXT),
            StatusType::Error => (C_ERROR_BG, C_ERROR_TEXT),
            StatusType::Info => (C_INFO_BG, C_INFO_TEXT),
            StatusType::None => return,
        };
        let (card_width, side_margin) = self.card_widths(ui);
        ui.horizontal(|ui| {
            ui.add_space(side_margin);
            ui.allocate_ui_with_layout(
                egui::vec2(card_width, 0.0),
                egui::Layout::top_down(egui::Align::LEFT),
                |ui| {
                    status_frame(bg_color).show(ui, |ui| {
                        ui.set_min_width(ui.available_width());
                        ui.label(egui::RichText::new(&self.state.status_message).color(text_color).size(13.0));
                    });
                },
            );
            ui.add_space(side_margin);
        });
    }

    // ===== Sidebar =====
    fn show_sidebar(&mut self, ui: &mut egui::Ui) {
        ui.add_space(20.0);
        ui.vertical_centered(|ui| {
            let (rect, _) = ui.allocate_exact_size(egui::vec2(42.0, 42.0), egui::Sense::hover());
            let painter = ui.painter();
            painter.rect_filled(rect, egui::Rounding::same(11.0), C_PRIMARY);
            let c = rect.center();
            painter.circle_filled(c, 2.2, egui::Color32::WHITE);
            painter.circle_stroke(c, 6.5, egui::Stroke::new(1.6, egui::Color32::WHITE));
            painter.circle_stroke(
                c,
                11.0,
                egui::Stroke::new(1.3, egui::Color32::from_rgba_unmultiplied(255, 255, 255, 170)),
            );
            ui.add_space(10.0);
            ui.label(egui::RichText::new("ADB 广播助手").size(15.0).strong().color(egui::Color32::WHITE));
            ui.add_space(2.0);
            ui.label(egui::RichText::new("Broadcast Sender").size(10.0).color(SIDEBAR_TEXT_MUTED));
        });
        ui.add_space(22.0);
        // divider
        let y = ui.cursor().min.y;
        ui.painter().line_segment(
            [egui::pos2(ui.cursor().min.x + 14.0, y), egui::pos2(ui.max_rect().right() - 14.0, y)],
            egui::Stroke::new(1.0, SIDEBAR_DIVIDER),
        );
        ui.add_space(16.0);

        let items = [
            ("设备列表", MenuIcon::Devices, Page::Devices),
            ("广播发送", MenuIcon::Broadcast, Page::Broadcast),
            ("文本输入", MenuIcon::TextInput, Page::TextInput),
            ("安装 APK", MenuIcon::InstallApk, Page::InstallApk),
            ("设置", MenuIcon::Settings, Page::Settings),
        ];
        for (label, icon, page) in items {
            let selected = self.current_page == page;
            if sidebar_menu_item(ui, label, icon, selected) {
                if page == Page::Settings {
                    self.adb_path_input = self.state.config_manager.get().adb_path.clone();
                    self.screenshot_dir_input = self.state.config_manager.get().screenshot_dir.clone();
                }
                self.current_page = page;
            }
            ui.add_space(2.0);
        }

        ui.with_layout(egui::Layout::bottom_up(egui::Align::Center), |ui| {
            ui.add_space(16.0);
            ui.label(egui::RichText::new("v1.0.0").size(10.0).color(SIDEBAR_TEXT_MUTED));
            ui.add_space(4.0);
        });
    }

    // Open the screenshot directory in Windows Explorer
    fn open_screenshot_dir(&mut self) {
        let path = self.screenshot_dir_input.trim().to_string();
        if path.is_empty() {
            self.state.set_status("截图保存目录不能为空", StatusType::Error);
            return;
        }
        if let Err(e) = fs::create_dir_all(&path) {
            self.state.set_status(&format!("打开目录失败: {}", e), StatusType::Error);
            return;
        }
        match std::process::Command::new("explorer").arg(&path).spawn() {
            Ok(_) => self.state.set_status(&format!("已打开目录: {}", path), StatusType::Success),
            Err(e) => self.state.set_status(&format!("打开目录失败: {}", e), StatusType::Error),
        }
    }
}

impl eframe::App for AdbBroadcastApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Process completed background tasks
        self.process_bg_results();

        // ===== Left sidebar =====
        egui::SidePanel::left(egui::Id::new("sidebar_panel"))
            .exact_width(210.0)
            .frame(
                egui::Frame::none()
                    .fill(SIDEBAR_BG)
                    .inner_margin(egui::Margin::symmetric(10.0, 0.0)),
            )
            .show(ctx, |ui| {
                self.show_sidebar(ui);
            });

        // ===== Right content =====
        egui::CentralPanel::default()
            .frame(egui::Frame::none().fill(C_BG))
            .show(ctx, |ui| {
                egui::ScrollArea::vertical()
                    .id_salt("content_scroll")
                    .show(ui, |ui| {
                        ui.add_space(20.0);
                        match self.current_page {
                            Page::Devices => self.page_devices(ui),
                            Page::Broadcast => self.page_broadcast(ui),
                            Page::TextInput => self.page_text_input(ui),
                            Page::InstallApk => self.page_install_apk(ui),
                            Page::Settings => self.page_settings(ui),
                        }
                        ui.add_space(10.0);
                        self.show_status_bar(ui);
                        ui.add_space(20.0);
                    });
            });

        // ===== QR Code Dialog =====
        if self.show_qr_dialog {
            egui::Window::new("QR Code")
                .collapsible(false)
                .resizable(false)
                .default_size([360.0, 420.0])
                .show(ctx, |ui| {
                    ui.label(
                        egui::RichText::new("QR Code from content:")
                            .size(12.0)
                            .color(C_TEXT_MUTED)
                    );
                    ui.add_space(4.0);
                    ui.label(
                        egui::RichText::new(&self.qr_content)
                            .size(11.0)
                            .color(C_TEXT)
                    );
                    ui.add_space(12.0);

                    if !self.qr_content.is_empty() {
                        if let Ok(qr_code) = qrcode::QrCode::new(self.qr_content.as_bytes()) {
                            let module_count = qr_code.width();
                            let available = ui.available_width().min(300.0);
                            let cell_size = (available / module_count as f32).floor().max(2.0);
                            let img_size = cell_size * module_count as f32;

                            let (response, painter) = ui.allocate_painter(
                                egui::vec2(img_size, img_size),
                                egui::Sense::hover()
                            );
                            let rect = response.rect;
                            painter.rect_filled(rect, egui::Rounding::same(4.0), egui::Color32::WHITE);

                            for y in 0..module_count {
                                for x in 0..module_count {
                                    if qr_code[(x, y)] == qrcode::Color::Dark {
                                        let cell_rect = egui::Rect::from_min_size(
                                            egui::pos2(
                                                rect.min.x + x as f32 * cell_size,
                                                rect.min.y + y as f32 * cell_size,
                                            ),
                                            egui::vec2(cell_size, cell_size),
                                        );
                                        painter.rect_filled(cell_rect, egui::Rounding::ZERO, C_TEXT);
                                    }
                                }
                            }
                        } else {
                            ui.label(
                                egui::RichText::new("Failed to generate QR code")
                                    .color(C_ERROR_TEXT)
                                    .size(12.0)
                            );
                        }
                    } else {
                        ui.label(
                            egui::RichText::new("No content")
                                .color(C_TEXT_LIGHT)
                                .size(12.0)
                        );
                    }

                    ui.add_space(12.0);
                    ui.horizontal(|ui| {
                        if secondary_button(ui, "Close").clicked() {
                            self.show_qr_dialog = false;
                        }
                    });
                });
        }

        // ===== Toast notification overlay =====
        if let Some(ref msg) = self.toast_message.clone() {
            self.toast_remaining -= ctx.input(|i| i.stable_dt) as f64;
            if self.toast_remaining <= 0.0 {
                self.toast_message = None;
                self.toast_remaining = 0.0;
            } else {
                ctx.request_repaint_after(std::time::Duration::from_millis(100));
                let opacity = if self.toast_remaining < 1.0 {
                    self.toast_remaining as f32
                } else {
                    1.0
                };
                let (bg_r, bg_g, bg_b) = if self.toast_is_error {
                    (198, 40, 40)
                } else {
                    (46, 125, 50)
                };
                egui::Area::new("toast".into())
                    .anchor(egui::Align2::CENTER_TOP, egui::vec2(0.0, 12.0))
                    .order(egui::Order::Foreground)
                    .show(ctx, |ui| {
                        let bg = egui::Color32::from_rgba_premultiplied(
                            bg_r, bg_g, bg_b, (220.0 * opacity) as u8,
                        );
                        let text_color = egui::Color32::from_rgba_premultiplied(
                            255, 255, 255, (255.0 * opacity) as u8,
                        );
                        egui::Frame::none()
                            .fill(bg)
                            .rounding(egui::Rounding::same(8.0))
                            .inner_margin(egui::Margin::symmetric(20.0, 12.0))
                            .shadow(egui::epaint::Shadow {
                                offset: egui::Vec2::new(0.0, 2.0),
                                blur: 12.0,
                                spread: 0.0,
                                color: egui::Color32::from_black_alpha(40),
                            })
                            .show(ui, |ui| {
                                ui.label(
                                    egui::RichText::new(msg)
                                        .color(text_color)
                                        .size(13.0)
                                );
                            });
                    });
            }
        }
    }
}

// ===== Sidebar helpers =====
#[derive(Debug, Clone, Copy)]
enum MenuIcon {
    Devices,
    Broadcast,
    TextInput,
    InstallApk,
    Settings,
}

fn centered_card<R>(ui: &mut egui::Ui, card_width: f32, side_margin: f32, add_contents: impl FnOnce(&mut egui::Ui) -> R) {
    ui.horizontal(|ui| {
        ui.add_space(side_margin);
        ui.allocate_ui_with_layout(
            egui::vec2(card_width, 0.0),
            egui::Layout::top_down(egui::Align::LEFT),
            |ui| {
                ui.set_min_width(ui.available_width());
                add_contents(ui);
            },
        );
    });
}

fn sidebar_menu_item(ui: &mut egui::Ui, label: &str, icon: MenuIcon, selected: bool) -> bool {
    let full_width = ui.available_width();
    let height = 42.0;
    let (rect, response) = ui.allocate_exact_size(egui::vec2(full_width, height), egui::Sense::click());
    let painter = ui.painter();

    let bg = if selected {
        SIDEBAR_ACTIVE_BG
    } else if response.hovered() {
        SIDEBAR_HOVER_BG
    } else {
        egui::Color32::TRANSPARENT
    };
    painter.rect_filled(rect, egui::Rounding::same(8.0), bg);

    if selected {
        let accent = egui::Rect::from_min_size(
            egui::pos2(rect.min.x, rect.min.y + 10.0),
            egui::vec2(3.0, height - 20.0),
        );
        painter.rect_filled(accent, egui::Rounding::same(1.5), C_PRIMARY);
    }

    let icon_rect = egui::Rect::from_min_size(
        egui::pos2(rect.min.x + 16.0, rect.center().y - 9.0),
        egui::vec2(18.0, 18.0),
    );
    let icon_color = if selected { egui::Color32::WHITE } else { SIDEBAR_TEXT };
    draw_menu_icon(painter, icon, icon_rect, icon_color);

    painter.text(
        egui::pos2(rect.min.x + 44.0, rect.center().y),
        egui::Align2::LEFT_CENTER,
        label,
        egui::FontId::proportional(13.5),
        if selected { egui::Color32::WHITE } else { SIDEBAR_TEXT },
    );

    response.clicked()
}

fn draw_menu_icon(painter: &egui::Painter, icon: MenuIcon, rect: egui::Rect, color: egui::Color32) {
    let c = rect.center();
    let stroke = egui::Stroke::new(1.6, color);
    match icon {
        MenuIcon::Devices => {
            let w = 8.0;
            let h = 13.0;
            let body = egui::Rect::from_center_size(c, egui::vec2(w, h));
            painter.rect_stroke(body, egui::Rounding::same(2.0), stroke);
            painter.line_segment(
                [egui::pos2(body.min.x + 1.5, body.min.y + 3.0), egui::pos2(body.max.x - 1.5, body.min.y + 3.0)],
                egui::Stroke::new(1.2, color),
            );
            let a0 = egui::pos2(c.x + w * 0.5, body.min.y - 1.0);
            let a1 = egui::pos2(c.x + w, body.min.y - 4.5);
            painter.line_segment([a0, a1], egui::Stroke::new(1.3, color));
            painter.circle_filled(a1, 1.6, color);
        }
        MenuIcon::Broadcast => {
            painter.circle_filled(c, 2.0, color);
            painter.circle_stroke(c, 5.0, stroke);
            painter.circle_stroke(c, 8.5, egui::Stroke::new(1.3, color));
        }
        MenuIcon::Settings => {
            for i in 0..3 {
                let y = c.y - 6.0 + i as f32 * 6.0;
                painter.line_segment(
                    [egui::pos2(c.x - 8.0, y), egui::pos2(c.x + 8.0, y)],
                    egui::Stroke::new(1.6, color),
                );
                let kx = c.x - 4.0 + i as f32 * 5.0;
                painter.circle_filled(egui::pos2(kx, y), 2.2, color);
            }
        }
        MenuIcon::TextInput => {
            // input box with a cursor line
            let w = 12.5;
            let h = 8.0;
            let box_rect = egui::Rect::from_center_size(c + egui::vec2(0.0, -1.0), egui::vec2(w, h));
            painter.rect_stroke(box_rect, egui::Rounding::same(1.5), stroke);
            painter.line_segment(
                [egui::pos2(box_rect.min.x + 1.5, box_rect.min.y + 2.5), egui::pos2(box_rect.max.x - 1.5, box_rect.min.y + 2.5)],
                egui::Stroke::new(1.0, color),
            );
            painter.line_segment(
                [egui::pos2(c.x + 1.0, box_rect.min.y + 2.5), egui::pos2(c.x + 1.0, box_rect.max.y - 1.5)],
                egui::Stroke::new(1.4, color),
            );
        }
        MenuIcon::InstallApk => {
            // box with a downward arrow
            let w = 11.0;
            let h = 9.0;
            let box_rect = egui::Rect::from_center_size(c + egui::vec2(0.0, 2.0), egui::vec2(w, h));
            painter.rect_stroke(box_rect, egui::Rounding::same(1.5), stroke);
            painter.line_segment(
                [egui::pos2(box_rect.min.x, box_rect.min.y), egui::pos2(box_rect.max.x, box_rect.min.y)],
                egui::Stroke::new(1.2, color),
            );
            let tip_y = box_rect.min.y - 0.5;
            painter.line_segment(
                [egui::pos2(c.x, box_rect.min.y - 4.5), egui::pos2(c.x, tip_y)],
                egui::Stroke::new(1.4, color),
            );
            painter.line_segment(
                [egui::pos2(c.x - 2.5, tip_y - 2.5), egui::pos2(c.x, tip_y)],
                egui::Stroke::new(1.2, color),
            );
            painter.line_segment(
                [egui::pos2(c.x + 2.5, tip_y - 2.5), egui::pos2(c.x, tip_y)],
                egui::Stroke::new(1.2, color),
            );
        }
    }
}

// Escape text for `adb shell input text`:
// - space -> %s (Android input text convention)
// - shell special chars -> backslash escape
// - non-ASCII chars -> '?' (input text only supports ASCII)
fn escape_input_text(s: &str) -> String {
    let mut out = String::new();
    for c in s.chars() {
        match c {
            ' ' | '\n' | '\r' | '\t' => out.push_str("%s"),
            '\\' => out.push_str("\\\\"),
            '&' | '|' | '<' | '>' | ';' | '$' | '`' | '"' | '\'' | '(' | ')' | '*' | '?' | '!' | '[' | ']' | '{' | '}' | '~' | '^' => {
                out.push('\\');
                out.push(c);
            }
            c if !c.is_ascii() => out.push('?'),
            c => out.push(c),
        }
    }
    out
}


// ===== UI HELPER FUNCTIONS =====

fn card_frame() -> egui::Frame {
    egui::Frame::none()
        .fill(C_CARD)
        .stroke(egui::Stroke::new(1.0, egui::Color32::from_rgb(222, 227, 234)))
        .rounding(egui::Rounding::same(8.0))
        .inner_margin(egui::Margin::symmetric(24.0, 22.0))
        .shadow(egui::epaint::Shadow {
            offset: egui::Vec2::new(0.0, 1.0),
            blur: 4.0,
            spread: 0.0,
            color: egui::Color32::from_black_alpha(8),
        })
}

fn card_header(ui: &mut egui::Ui, title: &str, accent: egui::Color32) {
    ui.horizontal(|ui| {
        // Accent bar
        let bar_rect = egui::Rect::from_min_size(
            ui.next_widget_position(),
            egui::vec2(3.0, 18.0),
        );
        ui.painter().rect_filled(bar_rect, egui::Rounding::same(1.5), accent);
        ui.add_space(10.0);
        ui.label(
            egui::RichText::new(title)
                .size(15.0)
                .color(C_TEXT)
                .strong()
        );
    });
}

fn section_divider(ui: &mut egui::Ui) {
    ui.add_space(14.0);
    ui.separator();
    ui.add_space(14.0);
}

fn input_frame() -> egui::Frame {
    egui::Frame::none()
        .fill(C_CARD)
        .stroke(egui::Stroke::new(1.0, C_INPUT_BORDER))
        .rounding(egui::Rounding::same(7.0))
        .inner_margin(egui::Margin::symmetric(11.0, 7.0))
}

fn info_box<R>(ui: &mut egui::Ui, add_contents: impl FnOnce(&mut egui::Ui) -> R) -> egui::InnerResponse<R> {
    egui::Frame::none()
        .fill(C_INFO_BOX_BG)
        .stroke(egui::Stroke::new(1.0, egui::Color32::from_rgb(220, 225, 232)))
        .rounding(egui::Rounding::same(8.0))
        .inner_margin(egui::Margin::symmetric(16.0, 12.0))
        .show(ui, add_contents)
}

fn status_frame(bg_color: egui::Color32) -> egui::Frame {
    egui::Frame::none()
        .fill(bg_color)
        .stroke(egui::Stroke::new(1.0, bg_color))
        .rounding(egui::Rounding::same(8.0))
        .inner_margin(egui::Margin::symmetric(16.0, 12.0))
}

fn filled_button(
    ui: &mut egui::Ui,
    label: &str,
    fill: egui::Color32,
    hover_fill: egui::Color32,
    min_width: f32,
) -> egui::Response {
    let galley = ui.fonts(|f| {
        f.layout_no_wrap(
            label.to_string(),
            egui::FontId::new(13.0, egui::FontFamily::Proportional),
            egui::Color32::WHITE,
        )
    });
    let size = egui::vec2(min_width.max(galley.size().x + 28.0), 34.0);
    let (rect, response) = ui.allocate_exact_size(size, egui::Sense::click());
    let button_fill = if response.hovered() { hover_fill } else { fill };

    ui.painter().rect_filled(rect, egui::Rounding::same(7.0), button_fill);
    ui.painter().galley(
        rect.center() - galley.size() * 0.5,
        galley,
        egui::Color32::WHITE,
    );

    response
}

fn primary_button(ui: &mut egui::Ui, label: &str) -> egui::Response {
    filled_button(ui, label, C_PRIMARY, C_PRIMARY_HOVER, 80.0)
}

fn success_button(ui: &mut egui::Ui, label: &str) -> egui::Response {
    filled_button(ui, label, C_SUCCESS, C_SUCCESS_HOVER, 80.0)
}

fn danger_button(ui: &mut egui::Ui, label: &str) -> egui::Response {
    filled_button(ui, label, C_DANGER, C_DANGER_HOVER, 88.0)
}

fn secondary_button(ui: &mut egui::Ui, label: &str) -> egui::Response {
    filled_button(ui, label, C_SECONDARY, C_SECONDARY_HOVER, 88.0)
}

fn status_badge(ui: &mut egui::Ui, text: &str, bg: egui::Color32, text_color: egui::Color32) {
    let galley = ui.fonts(|f| {
        f.layout(
            text.to_string(),
            egui::FontId::new(11.0, egui::FontFamily::Proportional),
            text_color,
            f32::INFINITY,
        )
    });
    let size = galley.size();
    let padding = egui::vec2(12.0, 5.0);
    let rect = ui.allocate_exact_size(size + padding * 2.0, egui::Sense::hover()).0;
    ui.painter().rect_filled(rect, egui::Rounding::same(6.0), bg);
    ui.painter().galley(rect.min + padding, galley, text_color);
}
