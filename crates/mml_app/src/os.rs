//! What the operating system can do for a running machine: stay awake
//! while a job streams (a sleeping computer stops the machine half way)
//! and tell the operator when a job ends.

use std::process::{Child, Command, Stdio};

/// Keeps the computer from sleeping while alive.
pub struct KeepAwake {
    #[allow(dead_code)]
    child: Option<Child>,
}

impl KeepAwake {
    pub fn new() -> Self {
        #[cfg(windows)]
        {
            use windows_sys::Win32::System::Power::{
                ES_CONTINUOUS, ES_SYSTEM_REQUIRED, SetThreadExecutionState,
            };
            // SAFETY: plain flag call, no pointers.
            unsafe {
                SetThreadExecutionState(ES_CONTINUOUS | ES_SYSTEM_REQUIRED);
            }
            Self { child: None }
        }
        #[cfg(not(windows))]
        {
            let pid = std::process::id().to_string();
            let child = if cfg!(target_os = "macos") {
                // Ends by itself if the application dies.
                Command::new("caffeinate")
                    .args(["-i", "-s", "-w", &pid])
                    .stdout(Stdio::null())
                    .spawn()
                    .ok()
            } else {
                Command::new("systemd-inhibit")
                    .args([
                        "--what=sleep:idle",
                        "--who=MyMaxmakeLab",
                        "--why=A machine job is running",
                        "--mode=block",
                        "sleep",
                        "infinity",
                    ])
                    .stdout(Stdio::null())
                    .spawn()
                    .ok()
            };
            Self { child }
        }
    }
}

impl Drop for KeepAwake {
    fn drop(&mut self) {
        #[cfg(windows)]
        {
            use windows_sys::Win32::System::Power::{ES_CONTINUOUS, SetThreadExecutionState};
            // SAFETY: plain flag call, no pointers.
            unsafe {
                SetThreadExecutionState(ES_CONTINUOUS);
            }
        }
        if let Some(child) = &mut self.child {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

/// System notification (macOS, Linux desktops); the window also asks
/// for attention (Dock bounce, taskbar flash) on every system.
pub fn notify(title: &str, body: &str) {
    let quote = |text: &str| text.replace('\\', "\\\\").replace('"', "\\\"");
    if cfg!(target_os = "macos") {
        let script = format!(
            "display notification \"{}\" with title \"{}\"",
            quote(body),
            quote(title)
        );
        let _ = Command::new("osascript")
            .args(["-e", &script])
            .stdout(Stdio::null())
            .spawn();
    } else if cfg!(target_os = "linux") {
        let _ = Command::new("notify-send")
            .args(["--app-name=MyMaxmakeLab", title, body])
            .spawn();
    }
}
