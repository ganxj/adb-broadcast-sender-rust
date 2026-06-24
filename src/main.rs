#![windows_subsystem = "windows"]

use eframe::egui;
use crate::app::{AppState, StatusType};
use crate::models::device::Device;
use crate::adb::device_manager::DeviceManager;
use std::os::windows::process::CommandExt;
use std::sync::{Arc, Mutex};
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
            .with_inner_size([900.0, 720.0])
            .with_min_inner_size([600.0, 400.0])
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
const C_PRIMARY: egui::Color32 = egui::Color32::from_rgb(33, 150, 243);
const C_PRIMARY_HOVER: egui::Color32 = egui::Color32::from_rgb(25, 118, 210);
const C_SUCCESS: egui::Color32 = egui::Color32::from_rgb(76, 175, 80);
const C_SUCCESS_HOVER: egui::Color32 = egui::Color32::from_rgb(56, 142, 60);
const C_DANGER: egui::Color32 = egui::Color32::from_rgb(244, 67, 54);
const C_DANGER_HOVER: egui::Color32 = egui::Color32::from_rgb(211, 47, 47);
const C_SECONDARY: egui::Color32 = egui::Color32::from_rgb(117, 117, 117);
const C_SECONDARY_HOVER: egui::Color32 = egui::Color32::from_rgb(97, 97, 97);
const C_BG: egui::Color32 = egui::Color32::from_rgb(248, 249, 250);
const C_CARD: egui::Color32 = egui::Color32::from_rgb(255, 255, 255);
const C_TEXT: egui::Color32 = egui::Color32::from_rgb(51, 51, 51);
const C_TEXT_MUTED: egui::Color32 = egui::Color32::from_rgb(102, 102, 102);
const C_TEXT_LIGHT: egui::Color32 = egui::Color32::from_rgb(153, 153, 153);
const C_WIFI_BG: egui::Color32 = egui::Color32::from_rgb(240, 247, 255);
const C_QUICK_BG: egui::Color32 = egui::Color32::from_rgb(227, 242, 253);
const C_QUICK_BORDER: egui::Color32 = egui::Color32::from_rgb(144, 202, 249);
const C_QUICK_TEXT: egui::Color32 = egui::Color32::from_rgb(25, 118, 210);
const C_SUCCESS_BG: egui::Color32 = egui::Color32::from_rgb(232, 245, 233);
const C_SUCCESS_TEXT: egui::Color32 = egui::Color32::from_rgb(46, 125, 50);
const C_ERROR_BG: egui::Color32 = egui::Color32::from_rgb(255, 235, 238);
const C_ERROR_TEXT: egui::Color32 = egui::Color32::from_rgb(198, 40, 40);
const C_INFO_BG: egui::Color32 = egui::Color32::from_rgb(227, 242, 253);
const C_INFO_TEXT: egui::Color32 = egui::Color32::from_rgb(21, 101, 192);
const C_INFO_BOX_BG: egui::Color32 = egui::Color32::from_rgb(249, 249, 249);
const C_INPUT_BORDER: egui::Color32 = egui::Color32::from_rgb(210, 213, 218);
const C_INPUT_FOCUS: egui::Color32 = egui::Color32::from_rgb(33, 150, 243);
const MAX_CARD_WIDTH: f32 = 860.0;

fn setup_custom_style(ctx: &egui::Context) {
    let mut style = (*ctx.style()).clone();
    style.spacing.item_spacing = egui::vec2(8.0, 8.0);
    style.spacing.button_padding = egui::vec2(16.0, 8.0);
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

struct AdbBroadcastApp {
    state: AppState,
    devices: Vec<Device>,
    action_input: String,
    content_input: String,
    wifi_ip: String,
    wifi_port: String,
    custom_content: String,
    quick_contents: Vec<String>,
    selected_device_serial: String,
    show_adb_path_dialog: bool,
    adb_path_input: String,
    last_connected_ip: Option<String>,
    pending_action: String,
    bg_queue: Arc<Mutex<Vec<BgTaskResult>>>,
}

impl AdbBroadcastApp {
    fn new() -> Self {
        let mut app = Self {
            state: AppState::new().expect("Failed to initialize app state"),
            devices: Vec::new(),
            action_input: "com.sunmi.scanner.ACTION_DATA_CODE_RECEIVED".to_string(),
            content_input: String::new(),
            wifi_ip: String::new(),
            wifi_port: "5555".to_string(),
            custom_content: String::new(),
            quick_contents: vec![
                r#"{"dock":"D-1"}"#.to_string(),
                r#"{"dock":"D-7"}"#.to_string(),
                "fjsdhf6545651a".to_string(),
                r#"{"cashId":1,"locationCode":"N1"}"#.to_string(),
                "https://example.com".to_string(),
                r#"{"code":"12345","type":"qr"}"#.to_string(),
            ],
            selected_device_serial: String::new(),
            show_adb_path_dialog: false,
            adb_path_input: String::new(),
            last_connected_ip: None,
            pending_action: String::new(),
            bg_queue: Arc::new(Mutex::new(Vec::new())),
        };

        // Merge saved quick contents
        for item in &app.state.config_manager.get().quick_contents {
            if !app.quick_contents.contains(item) {
                app.quick_contents.push(item.clone());
            }
        }

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
                    // Add to history
                    if let (Some(content), Some(device)) = (&result.broadcast_content, &result.broadcast_device) {
                        let error = if result.success { None } else { Some(result.message.as_str()) };
                        self.state.add_broadcast_history(content, device, result.success, error);
                    }
                    if result.success {
                        self.content_input.clear();
                        self.state.set_status("Broadcast sent successfully!", StatusType::Success);
                    } else {
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
}

impl eframe::App for AdbBroadcastApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Process completed background tasks
        self.process_bg_results();
        
        egui::CentralPanel::default()
            .frame(egui::Frame::none().fill(C_BG))
            .show(ctx, |ui| {
            // Title
            ui.vertical_centered(|ui| {
                ui.add_space(20.0);
                if !self.pending_action.is_empty() {
                    ui.horizontal(|ui| {
                        ui.add(egui::Spinner::new());
                        ui.add_space(8.0);
                        ui.label(
                            egui::RichText::new(&self.pending_action)
                                .size(13.0)
                                .color(C_PRIMARY)
                        );
                    });
                    ui.add_space(6.0);
                } else {
                    ui.label(
                        egui::RichText::new("ADB Broadcast Sender")
                            .size(24.0)
                            .color(C_PRIMARY)
                            .strong()
                    );
                    ui.add_space(6.0);
                    ui.label(
                        egui::RichText::new("Send Android broadcast intents via ADB")
                            .size(12.0)
                            .color(C_TEXT_LIGHT)
                    );
                }
                ui.add_space(20.0);
            });

            egui::ScrollArea::vertical()
                .id_salt("main_scroll")
                .show(ui, |ui| {
                let available_width = ui.available_width();
                let card_width = available_width.min(MAX_CARD_WIDTH);
                let side_margin = ((available_width - card_width) / 2.0).max(16.0);

                ui.spacing_mut().item_spacing = egui::vec2(0.0, 20.0);

                // ============ DEVICE SELECTION CARD ============
                ui.horizontal(|ui| {
                    ui.add_space(side_margin);
                    ui.allocate_ui_with_layout(
                        egui::vec2(card_width, 0.0),
                        egui::Layout::top_down(egui::Align::LEFT),
                        |ui| {
                            card_frame().show(ui, |ui| {
                                card_header(ui, "Device Selection", C_PRIMARY);
                                ui.add_space(16.0);

                                // Device dropdown
                                ui.horizontal(|ui| {
                                    ui.label(egui::RichText::new("Select Device:").strong().color(C_TEXT));
                                    ui.add_space(8.0);
                                    
                                    let selected_text = if self.selected_device_serial.is_empty() {
                                        "-- No devices found --".to_string()
                                    } else {
                                        self.devices
                                            .iter()
                                            .find(|d| d.serial == self.selected_device_serial)
                                            .map(|d| d.display_name())
                                            .unwrap_or_else(|| "-- Select a device --".to_string())
                                    };
                                    
                                    let combo_width = (ui.available_width() - 4.0).max(200.0);
                                    input_frame().show(ui, |ui| {
                                        ui.set_min_width(combo_width - 20.0);
                                        egui::ComboBox::from_id_salt("device_select")
                                            .width(combo_width - 20.0)
                                            .selected_text(selected_text)
                                            .show_ui(ui, |ui| {
                                                if self.devices.is_empty() {
                                                    let _ = ui.selectable_label(true, "-- No devices found --");
                                                } else {
                                                    for device in &self.devices {
                                                        let selected = self.selected_device_serial == device.serial;
                                                        if ui.selectable_label(selected, device.display_name()).clicked() {
                                                            self.selected_device_serial = device.serial.clone();
                                                            self.state.select_device_by_serial(&device.serial, &self.devices);
                                                            self.state.set_status("Device selected", StatusType::Success);
                                                        }
                                                    }
                                                }
                                            });
                                    });
                                });

                                ui.add_space(10.0);
                                ui.horizontal(|ui| {
                                    let is_busy = !self.pending_action.is_empty();
                                    if primary_button(ui, "Refresh").clicked() && !is_busy {
                                        self.pending_action = "Refreshing devices...".to_string();
                                        self.spawn_refresh();
                                    }
                                    ui.add_space(8.0);
                                    if secondary_button(ui, "Set ADB Path").clicked() {
                                        self.adb_path_input = self.state.config_manager.get().adb_path.clone();
                                        self.show_adb_path_dialog = true;
                                    }
                                });

                                // Device info
                                if let Some(ref device) = self.state.selected_device {
                                    ui.add_space(10.0);
                                    info_box(ui, |ui| {
                                        ui.label(egui::RichText::new(&device.display_name()).strong().size(14.0).color(C_TEXT));
                                        ui.add_space(2.0);
                                        ui.label(egui::RichText::new(format!("Serial: {}", device.serial)).size(13.0).color(C_TEXT_MUTED));
                                        ui.label(egui::RichText::new(format!("State: {}", device.state)).size(13.0).color(C_TEXT_MUTED));
                                        ui.label(egui::RichText::new(format!("Connection: {}", device.connection_info())).size(13.0).color(C_TEXT_MUTED));
                                    });
                                }

                                // WiFi Connect Section
                                section_divider(ui);
                                wifi_frame().show(ui, |ui| {
                                    ui.label(
                                        egui::RichText::new("WiFi Connect")
                                            .strong()
                                            .color(C_QUICK_TEXT)
                                            .size(14.0)
                                    );
                                    ui.add_space(10.0);
                                    
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
                                            ui.add(egui::TextEdit::singleline(&mut self.wifi_port).desired_width(90.0));
                                        });
                                    });

                                    ui.add_space(8.0);
                                    ui.horizontal_wrapped(|ui| {
                                        quick_ip_btn(ui, "192.168.137.", &mut self.wifi_ip);
                                        ui.add_space(6.0);
                                        quick_ip_btn(ui, "192.168.137.56", &mut self.wifi_ip);
                                        ui.add_space(6.0);
                                        quick_ip_btn(ui, "192.168.137.204", &mut self.wifi_ip);
                                    });

                                    ui.add_space(10.0);
                                    ui.horizontal(|ui| {
                                        let is_busy = !self.pending_action.is_empty();
                                        if success_button(ui, "Connect").clicked() && !is_busy {
                                            let port = self.wifi_port.parse::<u16>().unwrap_or(5555);
                                            let ip = self.wifi_ip.trim().to_string();
                                            if ip.is_empty() {
                                                self.state.set_status("Please enter IP address", StatusType::Error);
                                            } else {
                                                self.pending_action = "Connecting...".to_string();
                                                self.spawn_connect(ip.clone(), port);
                                            }
                                        }
                                        ui.add_space(8.0);
                                        if danger_button(ui, "Disconnect").clicked() && !is_busy {
                                            let port = self.wifi_port.parse::<u16>().unwrap_or(5555);
                                            let ip = self.wifi_ip.trim();
                                            if ip.is_empty() {
                                                self.state.set_status("Please enter IP address", StatusType::Error);
                                            } else {
                                                self.pending_action = "Disconnecting...".to_string();
                                                self.spawn_disconnect(ip.to_string(), port);
                                            }
                                        }
                                    });
                                });
                            });
                        }
                    );
                    ui.add_space(side_margin);
                });

                // ============ BROADCAST SETTINGS CARD ============
                ui.horizontal(|ui| {
                    ui.add_space(side_margin);
                    ui.allocate_ui_with_layout(
                        egui::vec2(card_width, 0.0),
                        egui::Layout::top_down(egui::Align::LEFT),
                        |ui| {
                            card_frame().show(ui, |ui| {
                                card_header(ui, "Broadcast Settings", C_PRIMARY);
                                ui.add_space(16.0);

                                ui.label(egui::RichText::new("Action (default: com.sunmi.scanner.ACTION_DATA_CODE_RECEIVED):").strong().color(C_TEXT).size(13.0));
                                ui.add_space(4.0);
                                let input_width = (ui.available_width() - 4.0).max(200.0);
                                input_frame().show(ui, |ui| {
                                    ui.add(egui::TextEdit::singleline(&mut self.action_input).desired_width(input_width));
                                });

                                ui.add_space(12.0);
                                ui.label(egui::RichText::new("Content:").strong().color(C_TEXT).size(13.0));
                                ui.add_space(4.0);
                                input_frame().show(ui, |ui| {
                                    ui.add(
                                        egui::TextEdit::multiline(&mut self.content_input)
                                            .desired_rows(3)
                                            .desired_width(input_width),
                                    );
                                });

                                // Quick Content
                                section_divider(ui);
                                ui.label(egui::RichText::new("Quick Content").strong().color(C_QUICK_TEXT).size(14.0));
                                ui.add_space(8.0);
                                ui.horizontal_wrapped(|ui| {
                                    for content in self.quick_contents.clone() {
                                        let label = if content.len() > 22 {
                                            format!("{}...", &content[..22])
                                        } else {
                                            content.clone()
                                        };
                                        if quick_content_btn(ui, &label).clicked() {
                                            self.content_input = content.clone();
                                            self.state.set_status(&format!("Content set: {}", content), StatusType::Success);
                                        }
                                        ui.add_space(6.0);
                                    }
                                });

                                ui.add_space(8.0);
                                ui.horizontal(|ui| {
                                    let add_width = (ui.available_width() - 76.0).max(150.0);
                                    input_frame().show(ui, |ui| {
                                        ui.add(egui::TextEdit::singleline(&mut self.custom_content).desired_width(add_width).hint_text("Add custom content..."));
                                    });
                                    ui.add_space(6.0);
                                    if secondary_button(ui, "Add").clicked() {
                                        let content = self.custom_content.trim();
                                        if !content.is_empty() {
                                            self.quick_contents.push(content.to_string());
                                            // Save to config for persistence
                                            let _ = self.state.config_manager.add_quick_content(content.to_string());
                                            self.custom_content.clear();
                                            self.state.set_status("Custom content added", StatusType::Success);
                                        }
                                    }
                                });

                                ui.add_space(4.0);
                                section_divider(ui);
                                ui.horizontal(|ui| {
                                    let is_busy = !self.pending_action.is_empty();
                                    if primary_button(ui, "Send Broadcast").clicked() && !is_busy {
                                        if self.content_input.trim().is_empty() {
                                            self.state.set_status("Please enter broadcast content", StatusType::Error);
                                        } else if self.state.selected_device.is_none() {
                                            self.state.set_status("Please select a device", StatusType::Error);
                                        } else {
                                            self.pending_action = "Sending broadcast...".to_string();
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
                                    if secondary_button(ui, "Clear History").clicked() {
                                        self.state.clear_history();
                                        self.state.set_status("History cleared", StatusType::Success);
                                    }
                                });
                            });
                        }
                    );
                    ui.add_space(side_margin);
                });

                // ============ HISTORY CARD ============
                ui.horizontal(|ui| {
                    ui.add_space(side_margin);
                    ui.allocate_ui_with_layout(
                        egui::vec2(card_width, 0.0),
                        egui::Layout::top_down(egui::Align::LEFT),
                        |ui| {
                            card_frame().show(ui, |ui| {
                                card_header(ui, "Broadcast History", C_PRIMARY);
                                ui.add_space(16.0);

                                let history = self.state.get_history();
                                if history.is_empty() {
                                    ui.vertical_centered(|ui| {
                                        ui.add_space(20.0);
                                        ui.label(egui::RichText::new("No broadcast history yet").color(C_TEXT_LIGHT).size(13.0));
                                        ui.add_space(4.0);
                                        ui.label(egui::RichText::new("Sent broadcasts will appear here").color(C_TEXT_LIGHT).size(12.0));
                                        ui.add_space(20.0);
                                    });
                                } else {
                                    egui::ScrollArea::vertical()
                                        .id_salt("history_scroll")
                                        .max_height(240.0)
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

                                                // History item background card
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
                                                // Draw history item with left accent bar
                                                ui.horizontal(|ui| {
                                                    // Left accent bar (4px wide)
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
                                                }); // end frame.show
                                                ui.add_space(6.0);
                                            }
                                        });
                                }
                            });
                        }
                    );
                    ui.add_space(side_margin);
                });

                // Status bar
                if self.state.status_type != StatusType::None {
                    let (bg_color, text_color) = match self.state.status_type {
                        StatusType::Success => (C_SUCCESS_BG, C_SUCCESS_TEXT),
                        StatusType::Error => (C_ERROR_BG, C_ERROR_TEXT),
                        StatusType::Info => (C_INFO_BG, C_INFO_TEXT),
                        StatusType::None => return,
                    };

                    ui.add_space(8.0);
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
                            }
                        );
                        ui.add_space(side_margin);
                    });
                }
                
                ui.add_space(16.0);
            });

            // ADB Path Dialog
            if self.show_adb_path_dialog {
                egui::Window::new("Set ADB Path")
                    .collapsible(false)
                    .resizable(false)
                    .default_size([480.0, 120.0])
                    .show(ctx, |ui| {
                        ui.label("ADB executable path:");
                        ui.add_space(4.0);
                        input_frame().show(ui, |ui| {
                            ui.add(egui::TextEdit::singleline(&mut self.adb_path_input).desired_width(ui.available_width().max(200.0)));
                        });
                        ui.add_space(12.0);
                        ui.horizontal(|ui| {
                            if primary_button(ui, "Save").clicked() {
                                self.state.update_adb_path(&self.adb_path_input);
                                self.show_adb_path_dialog = false;
                                self.state.set_status("ADB path updated", StatusType::Success);
                            }
                            ui.add_space(8.0);
                            if secondary_button(ui, "Cancel").clicked() {
                                self.show_adb_path_dialog = false;
                            }
                        });
                    });
            }
        });
    }
}

// ===== UI HELPER FUNCTIONS =====

fn card_frame() -> egui::Frame {
    egui::Frame::none()
        .fill(C_CARD)
        .stroke(egui::Stroke::new(1.0, egui::Color32::from_rgb(228, 231, 236)))
        .rounding(egui::Rounding::same(10.0))
        .inner_margin(egui::Margin::symmetric(24.0, 20.0))
        .shadow(egui::epaint::Shadow {
            offset: egui::Vec2::new(0.0, 1.0),
            blur: 8.0,
            spread: 0.0,
            color: egui::Color32::from_black_alpha(12),
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
    ui.add_space(6.0);
    ui.separator();
    ui.add_space(6.0);
}

fn input_frame() -> egui::Frame {
    egui::Frame::none()
        .fill(C_CARD)
        .stroke(egui::Stroke::new(1.0, C_INPUT_BORDER))
        .rounding(egui::Rounding::same(6.0))
        .inner_margin(egui::Margin::symmetric(10.0, 6.0))
}

fn wifi_frame() -> egui::Frame {
    egui::Frame::none()
        .fill(C_WIFI_BG)
        .stroke(egui::Stroke::new(1.0, egui::Color32::from_rgb(190, 218, 240)))
        .rounding(egui::Rounding::same(8.0))
        .inner_margin(egui::Margin::symmetric(16.0, 14.0))
}

fn info_box<R>(ui: &mut egui::Ui, add_contents: impl FnOnce(&mut egui::Ui) -> R) -> egui::InnerResponse<R> {
    egui::Frame::none()
        .fill(C_INFO_BOX_BG)
        .stroke(egui::Stroke::new(1.0, egui::Color32::from_rgb(224, 227, 232)))
        .rounding(egui::Rounding::same(6.0))
        .inner_margin(egui::Margin::symmetric(14.0, 10.0))
        .show(ui, add_contents)
}

fn status_frame(bg_color: egui::Color32) -> egui::Frame {
    egui::Frame::none()
        .fill(bg_color)
        .stroke(egui::Stroke::new(1.0, bg_color))
        .rounding(egui::Rounding::same(8.0))
        .inner_margin(egui::Margin::symmetric(16.0, 12.0))
}

fn primary_button(ui: &mut egui::Ui, label: &str) -> egui::Response {
    ui.add(
        egui::Button::new(egui::RichText::new(label).color(egui::Color32::WHITE).size(13.0))
            .fill(C_PRIMARY)
            .rounding(egui::Rounding::same(6.0))
            .min_size(egui::vec2(80.0, 34.0))
    )
}

fn success_button(ui: &mut egui::Ui, label: &str) -> egui::Response {
    ui.add(
        egui::Button::new(egui::RichText::new(label).color(egui::Color32::WHITE).size(13.0))
            .fill(C_SUCCESS)
            .rounding(egui::Rounding::same(6.0))
            .min_size(egui::vec2(80.0, 34.0))
    )
}

fn danger_button(ui: &mut egui::Ui, label: &str) -> egui::Response {
    ui.add(
        egui::Button::new(egui::RichText::new(label).color(egui::Color32::WHITE).size(13.0))
            .fill(C_DANGER)
            .rounding(egui::Rounding::same(6.0))
            .min_size(egui::vec2(80.0, 34.0))
    )
}

fn secondary_button(ui: &mut egui::Ui, label: &str) -> egui::Response {
    ui.add(
        egui::Button::new(egui::RichText::new(label).color(egui::Color32::WHITE).size(13.0))
            .fill(C_SECONDARY)
            .rounding(egui::Rounding::same(6.0))
            .min_size(egui::vec2(80.0, 34.0))
    )
}

fn quick_ip_btn(ui: &mut egui::Ui, ip: &str, target: &mut String) {
    let btn = egui::Button::new(
        egui::RichText::new(ip).color(C_QUICK_TEXT).size(12.0)
    )
    .fill(C_QUICK_BG)
    .stroke(egui::Stroke::new(1.0, C_QUICK_BORDER))
    .rounding(egui::Rounding::same(5.0))
    .min_size(egui::vec2(0.0, 30.0));
    
    if ui.add(btn).clicked() {
        *target = ip.to_string();
    }
}

fn quick_content_btn(ui: &mut egui::Ui, label: &str) -> egui::Response {
    let btn = egui::Button::new(
        egui::RichText::new(label).color(C_QUICK_TEXT).size(12.0)
    )
    .fill(C_QUICK_BG)
    .stroke(egui::Stroke::new(1.0, C_QUICK_BORDER))
    .rounding(egui::Rounding::same(5.0))
    .min_size(egui::vec2(0.0, 30.0));
    
    ui.add(btn)
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
