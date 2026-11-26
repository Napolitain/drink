//! Release names and checks shared by the updater and the release workflow.
//!
//! Each Windows archive is signed with an SSH signature namespace that names
//! its architecture and version. A genuine signature therefore cannot be
//! replayed for a different version or architecture.

/// Public GitHub release pages.
pub const RELEASES: &str = "https://github.com/Napolitain/drink/releases";
/// Principal in the allowed-signers file.
pub const SIGNER: &str = "drink";
/// Executable name inside each release archive.
pub const EXECUTABLE: &str = "Drink.exe";

/// Architecture label used in asset names.
pub const ARCH: &str = if cfg!(target_arch = "aarch64") {
    "arm64"
} else {
    "x64"
};

/// Whether `text` is a `CalVer` release version such as `2026.11.25`.
#[must_use]
pub fn is_version(text: &str) -> bool {
    let parts: Vec<_> = text.split('.').collect();
    matches!(parts.as_slice(), [year, month, day]
        if [(year, 4), (month, 2), (day, 2)]
            .iter()
            .all(|(part, len)| part.len() == *len && part.bytes().all(|b| b.is_ascii_digit())))
}

/// The version that `releases/latest` redirected to, or `None` for any other
/// location.
#[must_use]
pub fn version_from_location(location: &str) -> Option<&str> {
    let tag = location.strip_prefix(RELEASES)?.strip_prefix("/tag/v")?;
    is_version(tag).then_some(tag)
}

/// Whether `candidate` is newer than the running `current` version.
#[must_use]
pub fn is_newer(candidate: &str, current: &str) -> bool {
    candidate > current
}

/// Archive asset name for `arch`.
#[must_use]
pub fn archive(arch: &str) -> String {
    format!("Drink-Windows-{arch}.zip")
}

/// Signature namespace for one architecture and version.
#[must_use]
pub fn namespace(arch: &str, version: &str) -> String {
    format!("drink-windows-{arch}-{version}")
}

/// URL that redirects to the latest release tag.
#[must_use]
pub fn latest_url() -> String {
    format!("{RELEASES}/latest")
}

/// Download URL for an asset of `version`.
#[must_use]
pub fn download_url(version: &str, file: &str) -> String {
    format!("{RELEASES}/download/v{version}/{file}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_calendar_versions_are_accepted() {
        assert!(is_version("2026.11.25"));
        for bad in [
            "",
            "2026.11",
            "2026.11.25.1",
            "26.11.25",
            "2026.1.25",
            "2026.11.2a",
            "v2026.11.25",
        ] {
            assert!(!is_version(bad), "{bad}");
        }
    }

    #[test]
    fn latest_location_must_be_a_release_tag() {
        let tag = format!("{RELEASES}/tag/v2026.11.25");
        assert_eq!(version_from_location(&tag), Some("2026.11.25"));
        for bad in [
            "https://example.com/Napolitain/drink/releases/tag/v2026.11.25".to_owned(),
            format!("{RELEASES}/tag/2026.11.25"),
            format!("{RELEASES}/tag/v2026.11"),
            format!("{RELEASES}/latest"),
            String::new(),
        ] {
            assert_eq!(version_from_location(&bad), None, "{bad}");
        }
    }

    #[test]
    fn versions_compare_by_date_and_never_downgrade() {
        assert!(is_newer("2026.12.01", "2026.11.25"));
        assert!(is_newer("2026.11.25", "0.1.0"));
        assert!(!is_newer("2026.11.25", "2026.11.25"));
        assert!(!is_newer("2026.11.18", "2026.11.25"));
    }

    #[test]
    fn asset_names_and_namespaces_bind_architecture_and_version() {
        assert_eq!(archive("x64"), "Drink-Windows-x64.zip");
        assert_eq!(
            namespace("arm64", "2026.11.25"),
            "drink-windows-arm64-2026.11.25"
        );
        assert_eq!(
            latest_url(),
            "https://github.com/Napolitain/drink/releases/latest"
        );
        assert_eq!(
            download_url("2026.11.25", "Drink-Windows-x64.zip"),
            "https://github.com/Napolitain/drink/releases/download/v2026.11.25/Drink-Windows-x64.zip"
        );
        assert!(["x64", "arm64"].contains(&ARCH));
    }
}
