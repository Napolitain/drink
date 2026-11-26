//! Preferences stored below `HKEY_CURRENT_USER`.
#![allow(unsafe_code)]

use windows::Win32::Foundation::ERROR_SUCCESS;
use windows::Win32::System::Registry::{
    HKEY_CURRENT_USER, RRF_RT_REG_DWORD, RRF_RT_REG_MULTI_SZ, RegGetValueW,
};
use windows::core::HSTRING;

/// Value naming games that defer breaks while in the foreground.
pub const PROTECTED_GAMES: &str = "ProtectedGameExecutables";

const PERSONALIZE: &str = "Software\\Microsoft\\Windows\\CurrentVersion\\Themes\\Personalize";

/// Configured game executables from `key`, as a `REG_MULTI_SZ` list.
pub fn protected_games(key: &str) -> Vec<String> {
    let (key, value) = (HSTRING::from(key), HSTRING::from(PROTECTED_GAMES));
    let mut bytes = 0u32;
    // SAFETY: a size query with no data buffer.
    let status = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            &key,
            &value,
            RRF_RT_REG_MULTI_SZ,
            None,
            None,
            Some(&raw mut bytes),
        )
    };
    if status != ERROR_SUCCESS || bytes == 0 {
        return Vec::new();
    }
    let mut data = vec![0u16; (bytes as usize).div_ceil(2)];
    // SAFETY: `data` holds at least `bytes` bytes and outlives the call.
    let status = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            &key,
            &value,
            RRF_RT_REG_MULTI_SZ,
            None,
            Some(data.as_mut_ptr().cast()),
            Some(&raw mut bytes),
        )
    };
    if status != ERROR_SUCCESS {
        return Vec::new();
    }
    data.truncate(bytes as usize / 2);
    data.split(|&unit| unit == 0)
        .filter(|entry| !entry.is_empty())
        .map(String::from_utf16_lossy)
        .collect()
}

/// Whether the taskbar uses the light theme, so the icon should be dark.
pub fn light_taskbar() -> bool {
    let mut light = 0u32;
    let mut bytes = 4u32;
    // SAFETY: `light` is a writable DWORD of `bytes` bytes.
    let status = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            &HSTRING::from(PERSONALIZE),
            &HSTRING::from("SystemUsesLightTheme"),
            RRF_RT_REG_DWORD,
            None,
            Some((&raw mut light).cast()),
            Some(&raw mut bytes),
        )
    };
    status == ERROR_SUCCESS && light != 0
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command;

    fn reg(args: &[&str]) {
        let status = Command::new("reg.exe")
            .args(args)
            .output()
            .map(|output| output.status);
        assert!(status.is_ok_and(|status| status.success()), "reg {args:?}");
    }

    #[test]
    fn reads_configured_games_from_the_registry() {
        let key = format!(
            "Software\\io.github.napolitain.drink.test-{}",
            std::process::id()
        );
        let full = format!("HKCU\\{key}");
        assert!(protected_games(&key).is_empty());
        reg(&[
            "add",
            &full,
            "/v",
            PROTECTED_GAMES,
            "/t",
            "REG_MULTI_SZ",
            "/d",
            "Game.exe\\0Other Game.exe",
            "/f",
        ]);
        let games = protected_games(&key);
        reg(&["delete", &full, "/f"]);
        assert_eq!(games, ["Game.exe", "Other Game.exe"]);
    }

    #[test]
    fn reads_the_taskbar_theme() {
        // Either theme is valid; the read must not fail on a missing value.
        let _ = light_taskbar();
    }
}
