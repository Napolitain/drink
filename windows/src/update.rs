//! Install the latest signed release over the running executable.
//!
//! Before anything is extracted, the archive must carry a valid signature from
//! the release signer for this architecture and version. The running
//! executable is then renamed aside and the new one moved into its place,
//! which Windows allows while the old image is still running. The new version
//! starts when the app next relaunches.

use crate::menu::Outcome;
use crate::release;
use std::ffi::OsStr;
use std::fs::{self, File, OpenOptions};
use std::io;
use std::os::windows::fs::OpenOptionsExt;
use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{Mutex, PoisonError};

/// Allowed signers for release archives, shared with the macOS release.
pub const SIGNERS: &str = include_str!("../../macos/release-signers");

/// Held while executables are exchanged, so the app can wait for an
/// exchange to finish before it exits.
pub static EXCHANGE: Mutex<()> = Mutex::new(());

const CREATE_NO_WINDOW: u32 = 0x0800_0000;
const FILE_FLAG_DELETE_ON_CLOSE: u32 = 0x0400_0000;
const ERROR_SHARING_VIOLATION: i32 = 32;

/// Source of release metadata and assets.
pub trait Transport {
    /// The absolute URL that the latest-release page redirects to.
    ///
    /// # Errors
    ///
    /// Network or HTTP failures.
    fn latest(&self) -> io::Result<String>;

    /// Save `url` to `file`.
    ///
    /// # Errors
    ///
    /// Network or HTTP failures, including a missing asset.
    fn download(&self, url: &str, file: &Path) -> io::Result<()>;
}

/// Public GitHub release URLs fetched with the curl that ships with Windows,
/// without credentials or the GitHub API rate limit.
#[derive(Debug, Clone, Copy)]
pub struct Curl;

const CURL: [&str; 14] = [
    "-q",
    "--fail",
    "--silent",
    "--show-error",
    "--proto",
    "=https",
    "--proto-redir",
    "=https",
    "--connect-timeout",
    "10",
    "--max-time",
    "120",
    "--retry",
    "2",
];

impl Transport for Curl {
    fn latest(&self) -> io::Result<String> {
        let mut curl = Command::new(system_tool("curl.exe"));
        curl.args(CURL)
            .args([
                "--head",
                "--output",
                "NUL",
                "--write-out",
                "%{redirect_url}",
            ])
            .arg(release::latest_url());
        String::from_utf8(run(&mut curl)?).map_err(io::Error::other)
    }

    fn download(&self, url: &str, file: &Path) -> io::Result<()> {
        let mut curl = Command::new(system_tool("curl.exe"));
        curl.args(CURL)
            .args(["--location", "--output"])
            .arg(file)
            .arg(url);
        run(&mut curl).map(drop)
    }
}

/// An installed executable and what it trusts.
#[derive(Debug, Clone, Copy)]
pub struct Install<'a> {
    /// The executable to replace.
    pub exe: &'a Path,
    /// Its release version.
    pub version: &'a str,
    /// Allowed-signers file contents.
    pub signers: &'a str,
    /// Architecture label of the archive to install.
    pub arch: &'a str,
}

impl<'a> Install<'a> {
    /// The running release at `exe`.
    #[must_use]
    pub const fn running(exe: &'a Path, version: &'a str) -> Self {
        Self {
            exe,
            version,
            signers: SIGNERS,
            arch: release::ARCH,
        }
    }
}

/// Download, verify and install the latest release if it is newer.
pub fn install_latest(install: &Install, transport: &impl Transport) -> Outcome {
    let lock = match lock(install.exe) {
        Ok(lock) => lock,
        Err(error) if error.raw_os_error() == Some(ERROR_SHARING_VIOLATION) => {
            return Outcome::Busy;
        }
        Err(error) => {
            eprintln!("Drink update: {error}");
            return Outcome::Failed;
        }
    };
    remove_stages(install.exe);
    let stage = sibling(install.exe, &format!(".update.{}", std::process::id()));
    let outcome = stage_and_exchange(install, transport, &stage).unwrap_or_else(|error| {
        eprintln!("Drink update: {error}");
        Outcome::Failed
    });
    // The stage keeps the replaced image while it runs; a later launch removes it.
    let _ = fs::remove_dir_all(&stage);
    drop(lock);
    outcome
}

/// Remove stages left by earlier updates, unless an update is in progress.
pub fn clean(exe: &Path) {
    if let Ok(lock) = lock(exe) {
        remove_stages(exe);
        drop(lock);
    }
}

fn stage_and_exchange(
    install: &Install,
    transport: &impl Transport,
    stage: &Path,
) -> io::Result<Outcome> {
    let location = transport.latest()?;
    let version = release::version_from_location(location.trim())
        .ok_or_else(|| io::Error::other("unexpected latest-release location"))?;
    if !release::is_newer(version, install.version) {
        return Ok(Outcome::Current);
    }
    fs::create_dir(stage)?;
    let name = release::archive(install.arch);
    let archive = stage.join(&name);
    let signature = stage.join(format!("{name}.sig"));
    transport.download(&release::download_url(version, &name), &archive)?;
    transport.download(
        &release::download_url(version, &format!("{name}.sig")),
        &signature,
    )?;
    let signers = stage.join("release-signers");
    fs::write(&signers, install.signers)?;
    let mut verify = Command::new(system_tool("OpenSSH\\ssh-keygen.exe"));
    verify
        .args(["-Y", "verify", "-f"])
        .arg(&signers)
        .args([
            "-I",
            release::SIGNER,
            "-n",
            &release::namespace(install.arch, version),
            "-s",
        ])
        .arg(&signature)
        .stdin(File::open(&archive)?);
    run(&mut verify)?;
    let contents = stage.join("contents");
    fs::create_dir(&contents)?;
    let mut extract = Command::new(system_tool("tar.exe"));
    extract.arg("-xf").arg(&archive).arg("-C").arg(&contents);
    run(&mut extract)?;
    let candidate = contents.join(release::EXECUTABLE);
    if !candidate.is_file() {
        return Err(io::Error::other("the release archive has no Drink.exe"));
    }
    exchange(install.exe, &candidate, &stage.join("previous.exe"))?;
    Ok(Outcome::Installed)
}

/// Move `exe` to `previous` and `candidate` to `exe`, restoring `exe` if the
/// second move fails.
///
/// # Errors
///
/// Either rename failing; `exe` is then unchanged.
pub fn exchange(exe: &Path, candidate: &Path, previous: &Path) -> io::Result<()> {
    let _exchanging = EXCHANGE.lock().unwrap_or_else(PoisonError::into_inner);
    fs::rename(exe, previous)?;
    fs::rename(candidate, exe).inspect_err(|_| {
        let _ = fs::rename(previous, exe);
    })
}

/// Relaunch `exe` into an installed update. The new process waits for this
/// one to exit.
///
/// # Errors
///
/// The process could not be started.
pub fn relaunch(exe: &Path) -> io::Result<()> {
    Command::new(exe)
        .arg(crate::cli::RELAUNCH)
        .spawn()
        .map(drop)
}

/// Exclusive, self-deleting lock beside the executable.
fn lock(exe: &Path) -> io::Result<File> {
    OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .share_mode(0)
        .custom_flags(FILE_FLAG_DELETE_ON_CLOSE)
        .open(sibling(exe, ".update.lock"))
}

fn sibling(exe: &Path, suffix: &str) -> PathBuf {
    let mut name = exe
        .file_name()
        .unwrap_or_else(|| OsStr::new("Drink.exe"))
        .to_os_string();
    name.push(suffix);
    exe.with_file_name(name)
}

fn remove_stages(exe: &Path) {
    let prefix = sibling(exe, ".update.");
    let (Some(directory), Some(prefix)) =
        (exe.parent(), prefix.file_name().and_then(OsStr::to_str))
    else {
        return;
    };
    let Ok(entries) = fs::read_dir(if directory.as_os_str().is_empty() {
        Path::new(".")
    } else {
        directory
    }) else {
        return;
    };
    for entry in entries.flatten() {
        let stale = entry
            .file_name()
            .to_str()
            .is_some_and(|name| name.starts_with(prefix));
        if stale && entry.file_type().is_ok_and(|kind| kind.is_dir()) {
            let _ = fs::remove_dir_all(entry.path());
        }
    }
}

fn system_tool(relative: &str) -> PathBuf {
    crate::win::system_directory().join(relative)
}

/// Run a console tool without opening a window, returning its output.
fn run(command: &mut Command) -> io::Result<Vec<u8>> {
    let output = command
        .creation_flags(CREATE_NO_WINDOW)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()?;
    if output.status.success() {
        return Ok(output.stdout);
    }
    let tool = Path::new(command.get_program())
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .into_owned();
    Err(io::Error::other(format!(
        "{tool} failed ({}): {}",
        output.status,
        String::from_utf8_lossy(&output.stderr).trim()
    )))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;
    use std::sync::atomic::{AtomicU32, Ordering};

    struct Scratch(PathBuf);

    impl Scratch {
        fn new() -> Self {
            static NEXT: AtomicU32 = AtomicU32::new(0);
            let path = std::env::temp_dir().join(format!(
                "drink-update-test-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            let _ = fs::remove_dir_all(&path);
            fs::create_dir_all(&path).unwrap_or_else(|error| panic!("{error}"));
            Self(path)
        }
    }

    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    /// Serves a fixed tag and files from a directory, recording requests.
    struct Fake {
        location: String,
        assets: PathBuf,
        requests: RefCell<Vec<String>>,
    }

    impl Transport for Fake {
        fn latest(&self) -> io::Result<String> {
            Ok(self.location.clone())
        }
        fn download(&self, url: &str, file: &Path) -> io::Result<()> {
            self.requests.borrow_mut().push(url.to_owned());
            let name = url.rsplit('/').next().unwrap_or_default();
            fs::copy(self.assets.join(name), file).map(drop)
        }
    }

    fn tool(relative: &str, args: &[&OsStr]) {
        let mut command = Command::new(system_tool(relative));
        command.args(args);
        assert!(run(&mut command).is_ok(), "{relative} {args:?}");
    }

    struct Release {
        scratch: Scratch,
        exe: PathBuf,
        signers: String,
        fake: Fake,
    }

    impl Release {
        /// An install of 2025.01.01 and a signed archive of 2025.01.08.
        fn new() -> Self {
            let scratch = Scratch::new();
            let root = &scratch.0;
            let key = root.join("key");
            tool(
                "OpenSSH\\ssh-keygen.exe",
                &[
                    "-q".as_ref(),
                    "-t".as_ref(),
                    "ed25519".as_ref(),
                    "-N".as_ref(),
                    "".as_ref(),
                    "-f".as_ref(),
                    key.as_os_str(),
                ],
            );
            let public = fs::read_to_string(root.join("key.pub")).unwrap_or_default();
            let signers = format!("{} {public}", release::SIGNER);
            let exe = root.join("install").join("Drink.exe");
            fs::create_dir_all(exe.parent().unwrap_or(root)).unwrap_or_default();
            fs::write(&exe, "2025.01.01").unwrap_or_default();
            let assets = root.join("assets");
            let build = root.join("build");
            fs::create_dir_all(&assets).unwrap_or_default();
            fs::create_dir_all(&build).unwrap_or_default();
            let fake = Fake {
                location: format!("{}/tag/v2025.01.08", release::RELEASES),
                assets,
                requests: RefCell::new(Vec::new()),
            };
            let release = Self {
                scratch,
                exe,
                signers,
                fake,
            };
            release.publish("x64", "2025.01.08", Some("2025.01.08"));
            release
        }

        /// Sign an archive for `arch` and `signed` version holding `contents`.
        fn publish(&self, arch: &str, signed: &str, contents: Option<&str>) {
            let root = &self.scratch.0;
            let build = root.join("build");
            let _ = fs::remove_dir_all(&build);
            fs::create_dir_all(&build).unwrap_or_default();
            match contents {
                Some(text) => fs::write(build.join(release::EXECUTABLE), text).unwrap_or_default(),
                None => fs::write(build.join("README.txt"), "no executable").unwrap_or_default(),
            }
            let archive = self.fake.assets.join(release::archive("x64"));
            let _ = fs::remove_file(&archive);
            let entry = if contents.is_some() {
                release::EXECUTABLE
            } else {
                "README.txt"
            };
            tool(
                "tar.exe",
                &[
                    "-a".as_ref(),
                    "-cf".as_ref(),
                    archive.as_os_str(),
                    "-C".as_ref(),
                    build.as_os_str(),
                    entry.as_ref(),
                ],
            );
            let signature = self
                .fake
                .assets
                .join(format!("{}.sig", release::archive("x64")));
            let _ = fs::remove_file(&signature);
            let namespace = release::namespace(arch, signed);
            tool(
                "OpenSSH\\ssh-keygen.exe",
                &[
                    "-q".as_ref(),
                    "-Y".as_ref(),
                    "sign".as_ref(),
                    "-f".as_ref(),
                    root.join("key").as_os_str(),
                    "-n".as_ref(),
                    namespace.as_ref(),
                    archive.as_os_str(),
                ],
            );
        }

        fn install(&self) -> Outcome {
            let install = Install {
                exe: &self.exe,
                version: "2025.01.01",
                signers: &self.signers,
                arch: "x64",
            };
            install_latest(&install, &self.fake)
        }

        fn installed(&self) -> String {
            fs::read_to_string(&self.exe).unwrap_or_default()
        }

        fn stages(&self) -> usize {
            fs::read_dir(self.exe.parent().unwrap_or(&self.scratch.0))
                .map_or(0, |entries| entries.count() - 1)
        }
    }

    #[test]
    fn installs_a_newer_signed_release() {
        let release = Release::new();
        assert_eq!(release.install(), Outcome::Installed);
        assert_eq!(release.installed(), "2025.01.08");
        assert_eq!(release.stages(), 0, "the stage and lock are removed");
        assert_eq!(
            *release.fake.requests.borrow(),
            [
                release::download_url("2025.01.08", "Drink-Windows-x64.zip"),
                release::download_url("2025.01.08", "Drink-Windows-x64.zip.sig"),
            ]
        );
    }

    #[test]
    fn rejects_a_tampered_archive() {
        let release = Release::new();
        let archive = release.fake.assets.join(release::archive("x64"));
        let mut bytes = fs::read(&archive).unwrap_or_default();
        bytes.extend_from_slice(b"tampered");
        fs::write(&archive, bytes).unwrap_or_default();
        assert_eq!(release.install(), Outcome::Failed);
        assert_eq!(release.installed(), "2025.01.01");
        assert_eq!(release.stages(), 0);
    }

    #[test]
    fn rejects_genuine_signatures_for_another_version_or_architecture() {
        let mut release = Release::new();
        release.fake.location = format!("{}/tag/v2025.01.09", release::RELEASES);
        assert_eq!(
            release.install(),
            Outcome::Failed,
            "replayed under a newer tag"
        );
        release.fake.location = format!("{}/tag/v2025.01.08", release::RELEASES);
        release.publish("arm64", "2025.01.08", Some("2025.01.08"));
        assert_eq!(
            release.install(),
            Outcome::Failed,
            "archive for another architecture"
        );
        assert_eq!(release.installed(), "2025.01.01");
    }

    #[test]
    fn rejects_an_archive_without_the_executable() {
        let release = Release::new();
        release.publish("x64", "2025.01.08", None);
        assert_eq!(release.install(), Outcome::Failed);
        assert_eq!(release.installed(), "2025.01.01");
    }

    #[test]
    fn equal_older_or_invalid_tags_install_nothing() {
        let mut release = Release::new();
        for (tag, outcome) in [
            ("v2025.01.01", Outcome::Current),
            ("v2024.12.31", Outcome::Current),
            ("2025.01.08", Outcome::Failed),
        ] {
            release.fake.location = format!("{}/tag/{tag}", release::RELEASES);
            assert_eq!(release.install(), outcome, "{tag}");
        }
        assert!(release.fake.requests.borrow().is_empty());
        assert_eq!(release.installed(), "2025.01.01");
        fs::remove_file(release.fake.assets.join(release::archive("x64"))).unwrap_or_default();
        release.fake.location = format!("{}/tag/v2025.01.08", release::RELEASES);
        assert_eq!(release.install(), Outcome::Failed, "missing asset");
    }

    #[test]
    fn a_held_lock_reports_busy_and_skips_cleanup() {
        let release = Release::new();
        let stale = sibling(&release.exe, ".update.1");
        fs::create_dir_all(&stale).unwrap_or_default();
        let held = lock(&release.exe);
        assert!(held.is_ok());
        assert_eq!(release.install(), Outcome::Busy);
        clean(&release.exe);
        assert!(stale.is_dir());
        drop(held);
        clean(&release.exe);
        assert!(!stale.exists());
        assert_eq!(release.install(), Outcome::Installed);
    }

    #[test]
    fn exchanges_a_running_executable_and_rolls_back_failures() {
        let scratch = Scratch::new();
        let exe = scratch.0.join("Drink.exe");
        fs::copy(system_tool("PING.EXE"), &exe).unwrap_or_default();
        let mut running = Command::new(&exe)
            .args(["-n", "30", "127.0.0.1"])
            .creation_flags(CREATE_NO_WINDOW)
            .stdout(Stdio::null())
            .spawn()
            .unwrap_or_else(|error| panic!("{error}"));
        let candidate = scratch.0.join("candidate.exe");
        fs::write(&candidate, "new").unwrap_or_default();
        let previous = scratch.0.join("previous.exe");
        let alive = running.try_wait().is_ok_and(|status| status.is_none());
        let exchanged = exchange(&exe, &candidate, &previous);
        let _ = running.kill();
        let _ = running.wait();
        assert!(
            alive,
            "the replaced executable was running during the exchange"
        );
        assert!(exchanged.is_ok(), "{exchanged:?}");
        assert_eq!(fs::read_to_string(&exe).unwrap_or_default(), "new");
        assert!(previous.is_file());

        let missing = scratch.0.join("missing.exe");
        assert!(exchange(&exe, &missing, &scratch.0.join("again.exe")).is_err());
        assert_eq!(
            fs::read_to_string(&exe).unwrap_or_default(),
            "new",
            "rolled back"
        );
    }

    #[test]
    fn curl_reports_tool_failures() {
        let scratch = Scratch::new();
        let file = scratch.0.join("file");
        assert!(Curl.download("http://127.0.0.1:9/insecure", &file).is_err());
        assert!(relaunch(&scratch.0.join("missing.exe")).is_err());
        assert_eq!(
            sibling(Path::new("Drink.exe"), ".x"),
            PathBuf::from("Drink.exe.x")
        );
        remove_stages(Path::new("Drink.exe"));
    }
}
