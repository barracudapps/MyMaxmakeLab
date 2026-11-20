//! Automatic updates from the GitHub releases of the project. A release
//! is published by CI for every production tag (`v1.2.3`, no suffix);
//! the application checks the latest one at start and every few hours,
//! downloads the archive of its platform over HTTPS, verifies its
//! SHA-256 and replaces itself. The new version runs at next start.
//!
//! `curl` does the HTTPS (it ships with macOS, Windows 10+ and every
//! Linux desktop), which keeps a TLS stack out of the binary.

use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::mpsc::{Receiver, channel};
use std::time::{Duration, Instant};

const REPOSITORY: &str = "barracudapps/MyMaxmakeLab";
const CHECK_EVERY: Duration = Duration::from_secs(6 * 3600);

/// Archive name of this platform in a release.
pub fn asset_name() -> Option<&'static str> {
    match (std::env::consts::OS, std::env::consts::ARCH) {
        ("macos", _) => Some("MyMaxmakeLab-macos-universal.zip"),
        ("linux", "x86_64") => Some("MyMaxmakeLab-linux-x86_64.tar.gz"),
        ("windows", "x86_64") => Some("MyMaxmakeLab-windows-x86_64.zip"),
        _ => None,
    }
}

/// `v1.2.3` as numbers; tags with a suffix (`-beta`, `-rc1`) are not
/// production releases.
pub fn parse_version(tag: &str) -> Option<[u64; 3]> {
    let mut parts = tag.strip_prefix('v').unwrap_or(tag).split('.');
    let version = [
        parts.next()?.parse().ok()?,
        parts.next()?.parse().ok()?,
        parts.next()?.parse().ok()?,
    ];
    parts.next().is_none().then_some(version)
}

#[derive(Debug, Clone, PartialEq)]
pub struct Release {
    pub tag: String,
    pub archive_url: String,
    pub checksum_url: Option<String>,
}

/// The release to install, when the latest one is a newer production
/// version with an archive for this platform.
pub fn pick_release(json: &str, current: &str) -> Option<Release> {
    let value: serde_json::Value = serde_json::from_str(json).ok()?;
    if value["draft"].as_bool() == Some(true) || value["prerelease"].as_bool() == Some(true) {
        return None;
    }
    let tag = value["tag_name"].as_str()?;
    if parse_version(tag)? <= parse_version(current)? {
        return None;
    }
    let name = asset_name()?;
    let url_of = |wanted: &str| {
        value["assets"].as_array()?.iter().find_map(|asset| {
            (asset["name"].as_str() == Some(wanted))
                .then(|| asset["browser_download_url"].as_str().map(str::to_owned))
                .flatten()
        })
    };
    let archive_url = url_of(name)?;
    // Only files of this repository's releases are ever downloaded.
    let allowed = format!("https://github.com/{REPOSITORY}/releases/download/");
    if !archive_url.starts_with(&allowed) {
        return None;
    }
    let checksum_url = url_of(&format!("{name}.sha256")).filter(|url| url.starts_with(&allowed));
    Some(Release {
        tag: tag.to_owned(),
        archive_url,
        checksum_url,
    })
}

fn curl(arguments: &[&str]) -> Result<Vec<u8>, String> {
    let output = Command::new("curl")
        .args([
            "--silent",
            "--show-error",
            "--fail",
            "--location",
            "--max-time",
            "600",
        ])
        .args(arguments)
        .output()
        .map_err(|error| format!("curl: {error}"))?;
    if output.status.success() {
        Ok(output.stdout)
    } else {
        Err(String::from_utf8_lossy(&output.stderr).trim().to_owned())
    }
}

/// Where the running copy lives: the `.app` bundle on macOS, the
/// executable elsewhere. `None` for a development build (`cargo run`),
/// which must never replace itself.
pub fn installed_location() -> Option<PathBuf> {
    let executable = std::env::current_exe().ok()?.canonicalize().ok()?;
    if executable
        .components()
        .any(|part| part.as_os_str() == "target")
    {
        return None;
    }
    if cfg!(target_os = "macos") {
        // .../MyMaxmakeLab.app/Contents/MacOS/mymaxmakelab
        let bundle = executable.parent()?.parent()?.parent()?;
        return (bundle.extension()? == "app").then(|| bundle.to_path_buf());
    }
    Some(executable)
}

#[derive(Debug, Clone, PartialEq)]
pub enum Outcome {
    UpToDate,
    /// Installed, active at next start.
    Installed(String),
    /// A newer version exists but this copy cannot replace itself.
    Available(String),
    Failed(String),
}

fn install(release: &Release, location: &Path) -> Result<(), String> {
    let work = std::env::temp_dir().join(format!("mymaxmakelab-update-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&work);
    std::fs::create_dir_all(&work).map_err(|error| error.to_string())?;
    let archive = curl(&[&release.archive_url])?;
    if let Some(url) = &release.checksum_url {
        let expected = String::from_utf8_lossy(&curl(&[url])?)
            .split_whitespace()
            .next()
            .unwrap_or_default()
            .to_lowercase();
        let actual: String = Sha256::digest(&archive)
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect();
        if expected != actual {
            return Err("checksum mismatch, update refused".into());
        }
    }
    let archive_path = work.join(asset_name().unwrap_or("update"));
    std::fs::write(&archive_path, &archive).map_err(|error| error.to_string())?;
    let unpacked = work.join("unpacked");
    std::fs::create_dir_all(&unpacked).map_err(|error| error.to_string())?;
    let status = if cfg!(target_os = "macos") {
        Command::new("ditto")
            .arg("-x")
            .arg("-k")
            .arg(&archive_path)
            .arg(&unpacked)
            .status()
    } else {
        // bsdtar (Windows 10+, macOS) and GNU tar read both formats.
        Command::new("tar")
            .arg("-xf")
            .arg(&archive_path)
            .arg("-C")
            .arg(&unpacked)
            .status()
    }
    .map_err(|error| error.to_string())?;
    if !status.success() {
        return Err("cannot unpack the update".into());
    }
    let wanted = location.file_name().ok_or("bad install location")?;
    let fresh = unpacked.join(wanted);
    if !fresh.exists() {
        return Err(format!(
            "{} missing from the update",
            wanted.to_string_lossy()
        ));
    }
    // Keep the old copy aside until the new one is in place: a running
    // executable can be renamed on every system, not overwritten.
    let old = location.with_extension("old");
    let _ = remove(&old);
    std::fs::rename(location, &old)
        .map_err(|error| format!("cannot replace the application: {error}"))?;
    if let Err(error) = std::fs::rename(&fresh, location).or_else(|_| copy(&fresh, location)) {
        let _ = std::fs::rename(&old, location);
        return Err(format!("cannot install the update: {error}"));
    }
    let _ = remove(&old);
    let _ = std::fs::remove_dir_all(&work);
    Ok(())
}

fn remove(path: &Path) -> std::io::Result<()> {
    if path.is_dir() {
        std::fs::remove_dir_all(path)
    } else {
        std::fs::remove_file(path)
    }
}

/// Copy across volumes; `ditto` keeps bundle permissions and links.
fn copy(from: &Path, to: &Path) -> std::io::Result<()> {
    if cfg!(target_os = "macos") {
        let status = Command::new("ditto").arg(from).arg(to).status()?;
        if status.success() {
            return Ok(());
        }
        return Err(std::io::Error::other("ditto failed"));
    }
    std::fs::copy(from, to).map(|_| ())
}

pub fn check_and_install() -> Outcome {
    let api = format!("https://api.github.com/repos/{REPOSITORY}/releases/latest");
    let json = match curl(&["-H", "Accept: application/vnd.github+json", &api]) {
        Ok(json) => json,
        // No release yet (404) or offline: nothing to do.
        Err(_) => return Outcome::UpToDate,
    };
    let Some(release) = pick_release(&String::from_utf8_lossy(&json), env!("CARGO_PKG_VERSION"))
    else {
        return Outcome::UpToDate;
    };
    let Some(location) = installed_location() else {
        return Outcome::Available(release.tag);
    };
    match install(&release, &location) {
        Ok(()) => Outcome::Installed(release.tag),
        Err(error) => Outcome::Failed(error),
    }
}

#[derive(Default)]
pub struct Updater {
    last_check: Option<Instant>,
    pending: Option<Receiver<Outcome>>,
    pub outcome: Option<Outcome>,
}

impl Updater {
    /// Starts a background check when due (or when `now` is asked);
    /// returns a finished outcome once.
    pub fn poll(&mut self, enabled: bool, now: bool) -> Option<Outcome> {
        if let Some(receiver) = &self.pending
            && let Ok(outcome) = receiver.try_recv()
        {
            self.pending = None;
            self.outcome = Some(outcome.clone());
            return Some(outcome);
        }
        let due = self
            .last_check
            .is_none_or(|time| time.elapsed() > CHECK_EVERY);
        if self.pending.is_none() && (now || (enabled && due)) {
            self.last_check = Some(Instant::now());
            let (sender, receiver) = channel();
            std::thread::spawn(move || {
                let _ = sender.send(check_and_install());
            });
            self.pending = Some(receiver);
        }
        None
    }

    pub fn checking(&self) -> bool {
        self.pending.is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn release_json(tag: &str, prerelease: bool) -> String {
        let name = asset_name().unwrap_or("none");
        format!(
            r#"{{"tag_name":"{tag}","draft":false,"prerelease":{prerelease},"assets":[
                {{"name":"{name}","browser_download_url":"https://github.com/{REPOSITORY}/releases/download/{tag}/{name}"}},
                {{"name":"{name}.sha256","browser_download_url":"https://github.com/{REPOSITORY}/releases/download/{tag}/{name}.sha256"}}]}}"#
        )
    }

    #[test]
    fn only_newer_production_releases_are_picked() {
        assert_eq!(parse_version("v1.2.3"), Some([1, 2, 3]));
        assert_eq!(parse_version("v1.2.3-beta"), None);
        assert_eq!(parse_version("v1.2"), None);
        let newer = pick_release(&release_json("v0.2.0", false), "0.1.0").unwrap();
        assert_eq!(newer.tag, "v0.2.0");
        assert!(newer.checksum_url.is_some());
        assert_eq!(pick_release(&release_json("v0.1.0", false), "0.1.0"), None);
        assert_eq!(pick_release(&release_json("v0.0.9", false), "0.1.0"), None);
        assert_eq!(pick_release(&release_json("v0.2.0", true), "0.1.0"), None);
        assert_eq!(
            pick_release(&release_json("v0.2.0-rc1", false), "0.1.0"),
            None
        );
    }

    #[test]
    fn downloads_outside_the_project_releases_are_refused() {
        let json =
            release_json("v9.0.0", false).replace("https://github.com/", "https://evil.example/");
        assert_eq!(pick_release(&json, "0.1.0"), None);
    }

    #[test]
    fn a_development_build_never_replaces_itself() {
        // Tests run from target/: the updater must not touch them.
        assert_eq!(installed_location(), None);
    }
}
