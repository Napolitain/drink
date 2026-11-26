//! Preferences stored below `HKEY_CURRENT_USER`.
#![allow(unsafe_code)]

use windows::Win32::Foundation::ERROR_SUCCESS;
use windows::Win32::System::Registry::{
    HKEY, HKEY_CURRENT_USER, KEY_READ, RRF_RT_REG_DWORD, RRF_RT_REG_MULTI_SZ, RRF_RT_REG_SZ,
    RegEnumKeyExW, RegGetValueW, RegOpenKeyExW,
};
use windows::core::{HSTRING, Owned, PWSTR};

/// Value naming games that defer breaks while in the foreground.
pub const PROTECTED_GAMES: &str = "ProtectedGameExecutables";

/// Where the Xbox Game Bar records the games it has recognised.
pub const GAME_BAR_GAMES: &str = "System\\GameConfigStore\\Children";

/// Full executable paths of the games recorded below `key`, read from each
/// subkey's `MatchedExeFullPath`. Entries without a path are skipped.
pub fn recognised_games(key: &str) -> Vec<String> {
    let mut parent = HKEY::default();
    // SAFETY: opens a key for reading into a handle that is owned below.
    let status = unsafe {
        RegOpenKeyExW(
            HKEY_CURRENT_USER,
            &HSTRING::from(key),
            None,
            KEY_READ,
            &raw mut parent,
        )
    };
    if status != ERROR_SUCCESS {
        return Vec::new();
    }
    // SAFETY: the handle was just opened and is closed on drop.
    let parent = unsafe { Owned::new(parent) };
    let path = HSTRING::from("MatchedExeFullPath");
    let mut games = Vec::new();
    for index in 0..u32::MAX {
        let mut name = [0u16; 256];
        let mut len = u32::try_from(name.len()).unwrap_or_default();
        // SAFETY: writes at most `len` characters of the subkey name.
        let status = unsafe {
            RegEnumKeyExW(
                *parent,
                index,
                Some(PWSTR(name.as_mut_ptr())),
                &raw mut len,
                None,
                None,
                None,
                None,
            )
        };
        if status != ERROR_SUCCESS {
            break;
        }
        let child = HSTRING::from_wide(&name[..len as usize]);
        if let Some(exe) = string_value(*parent, &child, &path) {
            games.push(exe);
        }
    }
    games
}

/// A non-empty `REG_SZ` value of `subkey`.
fn string_value(key: HKEY, subkey: &HSTRING, value: &HSTRING) -> Option<String> {
    let mut data = vec![0u16; 1024];
    let mut bytes = u32::try_from(data.len() * 2).ok()?;
    // SAFETY: `data` holds `bytes` bytes and outlives the call.
    let status = unsafe {
        RegGetValueW(
            key,
            subkey,
            value,
            RRF_RT_REG_SZ,
            None,
            Some(data.as_mut_ptr().cast()),
            Some(&raw mut bytes),
        )
    };
    if status != ERROR_SUCCESS {
        return None;
    }
    let len = (bytes as usize / 2).saturating_sub(1);
    let text = String::from_utf16_lossy(&data[..len.min(data.len())]);
    (!text.is_empty()).then_some(text)
}

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
    fn reads_recognised_games_with_paths_only() {
        let key = format!(
            "Software\\io.github.napolitain.drink.test-games-{}",
            std::process::id()
        );
        let full = format!("HKCU\\{key}");
        let value = "MatchedExeFullPath";
        reg(&[
            "add",
            &format!("{full}\\a"),
            "/v",
            value,
            "/d",
            "C:\\Games\\A\\a.exe",
            "/f",
        ]);
        reg(&[
            "add",
            &format!("{full}\\b"),
            "/v",
            "TitleId",
            "/t",
            "REG_DWORD",
            "/d",
            "1",
            "/f",
        ]);
        reg(&[
            "add",
            &format!("{full}\\c"),
            "/v",
            value,
            "/d",
            "D:\\c.exe",
            "/f",
        ]);
        let games = recognised_games(&key);
        reg(&["delete", &full, "/f"]);
        assert_eq!(games, ["C:\\Games\\A\\a.exe", "D:\\c.exe"]);
        assert!(
            recognised_games(&key).is_empty(),
            "a missing key has no games"
        );
        // The real store may be empty, e.g. on CI; reading it must not fail.
        let _ = recognised_games(GAME_BAR_GAMES);
    }

    #[test]
    fn reads_the_taskbar_theme() {
        // Either theme is valid; the read must not fail on a missing value.
        let _ = light_taskbar();
    }
}
