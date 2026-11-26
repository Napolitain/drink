//! Build identities. Each one has its own preferences, window class and
//! instance lock, so a development build can run beside an installed release.

/// Identifier shared by release builds on every platform.
pub const RELEASE_IDENTIFIER: &str = "io.github.napolitain.drink";

/// Which build of Drink is running.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Flavor {
    /// Name shown in the notification area and menu.
    pub name: &'static str,
    /// Window class, instance lock and preference key.
    pub identifier: &'static str,
    /// `CalVer` version of a signed release build.
    pub release: Option<&'static str>,
}

impl Flavor {
    /// A build from a source checkout. It never installs release updates.
    pub const DEVELOPMENT: Self = Self {
        name: "Drink Dev",
        identifier: "io.github.napolitain.drink.dev",
        release: None,
    };

    /// The interactive smoke test.
    pub const SMOKE: Self = Self {
        name: "DrinkSmoke",
        identifier: "io.github.napolitain.drink.smoke",
        release: None,
    };

    /// A release build stamped with `version`.
    #[must_use]
    pub const fn release(version: &'static str) -> Self {
        Self {
            name: "Drink",
            identifier: RELEASE_IDENTIFIER,
            release: Some(version),
        }
    }

    /// The identity compiled into this binary. The release workflow sets
    /// `DRINK_RELEASE_VERSION`; every other build is a development build.
    #[must_use]
    pub const fn compiled() -> Self {
        match option_env!("DRINK_RELEASE_VERSION") {
            Some(version) => Self::release(version),
            None => Self::DEVELOPMENT,
        }
    }

    /// Preference key below `HKEY_CURRENT_USER`.
    #[must_use]
    pub fn registry_key(&self) -> String {
        format!("Software\\{}", self.identifier)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn release_and_development_builds_are_distinct() {
        let release = Flavor::release("2026.11.25");
        assert_eq!(release.identifier, RELEASE_IDENTIFIER);
        assert_eq!(release.release, Some("2026.11.25"));
        assert_eq!(release.name, "Drink");
        for other in [Flavor::DEVELOPMENT, Flavor::SMOKE] {
            assert!(other.release.is_none());
            assert!(other.identifier.starts_with(RELEASE_IDENTIFIER));
            assert_ne!(other.identifier, RELEASE_IDENTIFIER);
        }
        assert_eq!(
            Flavor::DEVELOPMENT.registry_key(),
            "Software\\io.github.napolitain.drink.dev"
        );
    }

    #[test]
    fn source_builds_compile_as_development() {
        let compiled = Flavor::compiled();
        match option_env!("DRINK_RELEASE_VERSION") {
            Some(version) => assert_eq!(compiled, Flavor::release(version)),
            None => assert_eq!(compiled, Flavor::DEVELOPMENT),
        }
    }
}
