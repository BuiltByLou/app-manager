//! A tiny preferences file. One `key = value` per line, no dependencies.

use std::io;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Preferences {
    /// Ask before removing an app from the three-dot menu.
    pub confirm_removals: bool,
}

impl Default for Preferences {
    fn default() -> Self {
        Self {
            confirm_removals: true,
        }
    }
}

impl Preferences {
    pub fn load() -> Self {
        Self::load_from(&path())
    }

    pub fn save(&self) {
        if let Err(error) = self.save_to(&path()) {
            eprintln!("appmgr: could not save preferences: {error}");
        }
    }

    fn load_from(path: &Path) -> Self {
        let Ok(text) = std::fs::read_to_string(path) else {
            return Self::default();
        };
        let mut preferences = Self::default();
        for line in text.lines() {
            let Some((key, value)) = line.split_once('=') else {
                continue;
            };
            if key.trim() == "confirm-removals" {
                preferences.confirm_removals = value.trim() != "false";
            }
        }
        preferences
    }

    fn save_to(&self, path: &Path) -> io::Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let text = format!(
            "confirm-removals = {}\n",
            if self.confirm_removals { "true" } else { "false" }
        );
        std::fs::write(path, text)
    }
}

fn path() -> PathBuf {
    if let Some(config) = std::env::var_os("XDG_CONFIG_HOME") {
        return PathBuf::from(config).join("app-manager").join("preferences");
    }
    let home = std::env::var_os("HOME").map(PathBuf::from).unwrap_or_default();
    home.join(".config").join("app-manager").join("preferences")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp(name: &str) -> PathBuf {
        std::env::temp_dir()
            .join("appmgr-prefs-tests")
            .join(format!("{name}-{}", std::process::id()))
    }

    #[test]
    fn missing_file_means_asking() {
        let preferences = Preferences::load_from(&temp("missing").join("preferences"));
        assert!(preferences.confirm_removals);
    }

    #[test]
    fn round_trips_the_choice() {
        let path = temp("roundtrip").join("preferences");
        let preferences = Preferences {
            confirm_removals: false,
        };
        preferences.save_to(&path).unwrap();
        assert_eq!(Preferences::load_from(&path), preferences);
    }

    #[test]
    fn ignores_unknown_lines() {
        let path = temp("unknown").join("preferences");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, "# comment\nsomething = yes\nconfirm-removals = false\n").unwrap();
        assert!(!Preferences::load_from(&path).confirm_removals);
    }
}
