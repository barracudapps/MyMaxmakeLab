//! Firmware update of open controllers through the standard flashing
//! tools: avrdude for GRBL on Arduino boards, esptool for grblHAL and
//! FluidNC on ESP32. Vendor firmwares (HiMill...) keep their own tool:
//! their update protocol is not public and a mistake can leave a board
//! unusable.

use crate::app::{Level, MyApp};
use crate::i18n::{tr, trf};
use eframe::egui::{self, RichText};
use std::io::{BufRead, BufReader};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::mpsc::{Receiver, channel};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Board {
    /// Arduino Uno / Nano (ATmega328P) with GRBL, `.hex` file.
    ArduinoGrbl,
    /// ESP32 with grblHAL or FluidNC, application `.bin` file.
    Esp32,
}

pub struct FirmwareState {
    pub open: bool,
    pub board: Board,
    pub file: Option<PathBuf>,
    pub port: String,
    pub understood: bool,
    pub log: Vec<String>,
    running: Option<Receiver<Option<String>>>,
}

impl Default for FirmwareState {
    fn default() -> Self {
        Self {
            open: false,
            board: Board::ArduinoGrbl,
            file: None,
            port: String::new(),
            understood: false,
            log: Vec::new(),
            running: None,
        }
    }
}

/// First available program among the candidates, searched in PATH.
fn find_tool(candidates: &[&str]) -> Option<String> {
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path).find_map(|directory| {
        candidates.iter().find_map(|name| {
            let executable = directory.join(name);
            executable
                .is_file()
                .then(|| executable.to_string_lossy().into_owned())
        })
    })
}

/// Program and arguments that flash `file` on `port`.
pub fn flash_command(board: Board, tool: &str, port: &str, file: &str) -> Vec<String> {
    let mut command = vec![tool.to_owned()];
    match board {
        Board::ArduinoGrbl => command.extend(
            [
                "-p",
                "atmega328p",
                "-c",
                "arduino",
                "-P",
                port,
                "-b",
                "115200",
                "-D",
                "-U",
            ]
            .iter()
            .map(|part| (*part).to_owned())
            .chain(std::iter::once(format!("flash:w:{file}:i"))),
        ),
        Board::Esp32 => command.extend(
            [
                "--chip",
                "esp32",
                "--port",
                port,
                "--baud",
                "460800",
                "write_flash",
                "0x10000",
                file,
            ]
            .iter()
            .map(|part| (*part).to_owned()),
        ),
    }
    command
}

fn tool_for(board: Board) -> Option<String> {
    match board {
        Board::ArduinoGrbl => find_tool(&["avrdude", "avrdude.exe"]),
        Board::Esp32 => find_tool(&["esptool", "esptool.py", "esptool.exe"]),
    }
}

pub fn show(app: &mut MyApp, ctx: &egui::Context) {
    if !app.firmware.open {
        return;
    }
    if let Some(receiver) = &app.firmware.running {
        while let Ok(line) = receiver.try_recv() {
            match line {
                Some(line) => app.firmware.log.push(line),
                None => {
                    app.firmware.running = None;
                    break;
                }
            }
        }
        ctx.request_repaint_after(std::time::Duration::from_millis(200));
    }
    let mut open = true;
    let mut start = false;
    let connected = app.device.is_connected();
    let ports = app.device.ports.clone();
    let state = &mut app.firmware;
    egui::Window::new(tr("firmware.title"))
        .open(&mut open)
        .default_width(520.0)
        .show(ctx, |ui| {
            ui.label(RichText::new(tr("firmware.vendor_note")).color(crate::theme::WARNING));
            ui.horizontal(|ui| {
                ui.selectable_value(&mut state.board, Board::ArduinoGrbl, tr("firmware.arduino"));
                ui.selectable_value(&mut state.board, Board::Esp32, tr("firmware.esp32"));
            });
            let tool = tool_for(state.board);
            match &tool {
                Some(tool) => ui.label(RichText::new(trf("firmware.tool_found", &[tool])).weak()),
                None => ui.label(
                    RichText::new(tr(match state.board {
                        Board::ArduinoGrbl => "firmware.need_avrdude",
                        Board::Esp32 => "firmware.need_esptool",
                    }))
                    .color(crate::theme::DANGER),
                ),
            };
            ui.horizontal(|ui| {
                if ui.button(tr("firmware.choose_file")).clicked()
                    && let Some(file) = rfd::FileDialog::new()
                        .add_filter(tr("firmware.file"), &["hex", "bin"])
                        .pick_file()
                {
                    state.file = Some(file);
                }
                if let Some(file) = &state.file {
                    ui.label(
                        file.file_name()
                            .map(|name| name.to_string_lossy().into_owned())
                            .unwrap_or_default(),
                    );
                }
            });
            ui.horizontal(|ui| {
                ui.label(tr("device.port"));
                egui::ComboBox::from_id_salt("firmware-port")
                    .selected_text(if state.port.is_empty() {
                        tr("device.choose_port").to_owned()
                    } else {
                        state.port.clone()
                    })
                    .show_ui(ui, |ui| {
                        for (name, label) in &ports {
                            ui.selectable_value(&mut state.port, name.clone(), label);
                        }
                    });
            });
            if connected {
                ui.label(
                    RichText::new(tr("firmware.disconnect_first")).color(crate::theme::WARNING),
                );
            }
            ui.checkbox(&mut state.understood, tr("firmware.understood"));
            let ready = tool.is_some()
                && state.file.is_some()
                && !state.port.is_empty()
                && state.understood
                && !connected
                && state.running.is_none();
            start = ui
                .add_enabled(
                    ready,
                    egui::Button::new(RichText::new(tr("firmware.flash")).strong()),
                )
                .clicked();
            if !state.log.is_empty() {
                egui::ScrollArea::vertical()
                    .max_height(200.0)
                    .stick_to_bottom(true)
                    .show(ui, |ui| {
                        for line in &state.log {
                            ui.monospace(line);
                        }
                    });
            }
        });
    if start
        && let (Some(tool), Some(file)) = (tool_for(app.firmware.board), app.firmware.file.clone())
    {
        let command = flash_command(
            app.firmware.board,
            &tool,
            &app.firmware.port,
            &file.to_string_lossy(),
        );
        app.firmware.log = vec![format!("$ {}", command.join(" "))];
        let (sender, receiver) = channel();
        std::thread::spawn(move || {
            let child = Command::new(&command[0])
                .args(&command[1..])
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn();
            match child {
                Ok(mut child) => {
                    // Both tools report progress on stderr.
                    if let Some(stderr) = child.stderr.take() {
                        for line in BufReader::new(stderr).lines().map_while(Result::ok) {
                            let _ = sender.send(Some(line));
                        }
                    }
                    let status = child.wait().map(|status| status.success()).unwrap_or(false);
                    let _ = sender.send(Some(if status { "OK".into() } else { "FAILED".into() }));
                }
                Err(error) => {
                    let _ = sender.send(Some(error.to_string()));
                }
            }
            let _ = sender.send(None);
        });
        app.firmware.running = Some(receiver);
        app.notify(tr("firmware.started"), Level::Info);
    }
    app.firmware.open = open;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flash_commands_target_the_right_board() {
        let arduino = flash_command(Board::ArduinoGrbl, "avrdude", "/dev/ttyUSB0", "grbl.hex");
        assert!(arduino.contains(&"atmega328p".to_owned()));
        assert_eq!(arduino.last().unwrap(), "flash:w:grbl.hex:i");
        let esp32 = flash_command(Board::Esp32, "esptool", "COM3", "firmware.bin");
        assert_eq!(
            &esp32[esp32.len() - 3..],
            ["write_flash", "0x10000", "firmware.bin"]
        );
    }
}
