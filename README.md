# ADB Broadcast Sender

A desktop client for sending Android broadcast intents via ADB, built with **Rust + egui (eframe)**.

Management-style UI with a sidebar menu: manage connected devices, send broadcast intents, inject text into a focused input, install APKs and take screenshots — all without leaving your desk.

## Features

- **Sidebar management UI** — dark sidebar + content area, five pages:
  - **Device List**: WiFi connect (IP / port / quick-IP tags), device cards in a responsive grid (select / screenshot / disconnect)
  - **Broadcast**: device picker, custom action & content, quick-content chips, QR code generation, send broadcast, send history
  - **Text Input**: inject text into the focused field on the device via `adb shell input text` (special chars and spaces are escaped automatically)
  - **Install APK**: pick a `.apk` file with a native file dialog and install it on the selected device (`adb install -r`)
  - **Settings**: ADB path, screenshot directory (with an "open folder" shortcut)
- Single-instance enforcement — launching a second copy focuses the existing window
- Persistent configuration (`config.json` under the user data dir): ADB path, quick contents, WiFi IP tags, last device
- Windows-only (uses native system fonts for Chinese UI and `explorer` for opening folders)

## Requirements

- **Windows 10/11** (x64)
- [Android SDK platform-tools](https://developer.android.com/tools/releases/platform-tools) (`adb.exe`) — the app defaults to `D:\Program Files\Android\SDK\platform-tools\adb.exe` but any path works, configurable in Settings
- Rust toolchain (stable) if building from source

## Build

```bash
cargo build --release
```

The binary is produced at `target/release/adb-broadcast-sender.exe`.

## Usage

1. Launch the app and go to **Device List**.
2. Connect a device:
   - **USB**: plug the device in, enable USB debugging, and click **刷新设备** (Refresh Devices).
   - **WiFi**: enter the device IP (default port `5555`) and click **连接设备** (Connect), or tap a saved quick IP tag.
3. Switch to the page you need:
   - **Broadcast** — pick the target device, enter the action and content, then **发送广播** (Send Broadcast). Use **生成二维码** (Generate QR) to preview the content as a QR code.
   - **Text Input** — focus an input field on the device, type your text and click **写入** (Write). Spaces become `%s` and shell metacharacters are escaped; non-ASCII characters are converted to `?`.
   - **Install APK** — click **选择文件** (Choose File), select the APK, then **安装到设备** (Install).

## How it works

Each action spawns a background thread that invokes `adb` and reports the result back to the UI (status bar + toast), so the interface never blocks.

- Broadcast: `adb -s <serial> shell am broadcast -a <action> --es data "<content>"`
- Text input: `adb -s <serial> shell input text <escaped>`
- Install: `adb -s <serial> install -r <apk>`
- Screenshot: `adb -s <serial> exec-out screencap -p`

## Tech stack

| Component | Choice |
|---|---|
| Language | Rust (edition 2021) |
| GUI | [eframe / egui](https://github.com/emilk/egui) 0.29 |
| Native file dialog | [rfd](https://github.com/PolyMeilex/rfd) 0.14 |
| QR code | [qrcode](https://github.com/kennytm/rust-qrcode) 0.14 |
| Config / serialization | serde + serde_json, stored via `dirs` |

## Project layout

```
src/
├── main.rs      # App entry, UI pages, ADB command spawning
├── app.rs       # Shared AppState (device/broadcast managers, status)
├── config.rs    # Persistent config (JSON)
├── adb/         # adb command wrappers
│   ├── device_manager.rs
│   └── broadcast_manager.rs
└── models/      # Device / Broadcast models
assets/          # App icon (png / ico / rgba)
build.rs         # Windows resource embedding (icon, metadata)
```

## License

[MIT](LICENSE)
