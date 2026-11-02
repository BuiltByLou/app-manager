//! Which installed apps came from GitHub, and where they came from.
//!
//! The inventory only sees a package or a file. This registry is what remembers
//! the repository and the release a user installed, so updates can be checked
//! later. It lives under the user's data directory and is safe to edit by hand.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ManagedApp {
    /// `owner/repo`, as returned by [`crate::github::Repo::slug`].
    pub repo: String,
    /// `deb`, `appimage` or `flatpak`.
    pub format: String,
    /// Name of the release asset that was installed.
    #[serde(default)]
    pub asset: String,
    /// Release tag that was installed (e.g. `v0.45.0`).
    #[serde(default)]
    pub installed_version: String,
    /// Package name for `.deb`, app id for Flatpak.
    #[serde(default)]
    pub package: Option<String>,
    /// Installed file for an AppImage.
    #[serde(default)]
    pub path: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Registry {
    #[serde(default)]
    pub apps: BTreeMap<String, ManagedApp>,
}

impl Registry {
    /// The registry used by the running app.
    pub fn load() -> Self {
        Self::load_from(&registry_path())
    }

    /// Reads a registry from an explicit path. A missing or unreadable file is
    /// an empty registry: losing update metadata must never break the app.
    pub fn load_from(path: &Path) -> Self {
        let Ok(text) = std::fs::read_to_string(path) else {
            return Self::default();
        };
        serde_json::from_str(&text).unwrap_or_default()
    }

    pub fn save(&self) -> Result<(), String> {
        self.save_to(&registry_path())
    }

    /// Writes through a temporary file, so a crash cannot leave half a JSON
    /// document behind.
    pub fn save_to(&self, path: &Path) -> Result<(), String> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|error| format!("could not create {}: {error}", parent.display()))?;
        }
        let json = serde_json::to_string_pretty(self)
            .map_err(|error| format!("could not encode the source registry: {error}"))?;
        let temporary = path.with_extension("json.tmp");
        std::fs::write(&temporary, json)
            .map_err(|error| format!("could not write {}: {error}", temporary.display()))?;
        std::fs::rename(&temporary, path)
            .map_err(|error| format!("could not replace {}: {error}", path.display()))
    }

    pub fn get(&self, id: &str) -> Option<&ManagedApp> {
        self.apps.get(id)
    }

    pub fn insert(&mut self, id: String, app: ManagedApp) {
        self.apps.insert(id, app);
    }

    pub fn remove(&mut self, id: &str) -> Option<ManagedApp> {
        self.apps.remove(id)
    }
}

/// `$XDG_DATA_HOME/app-manager`, or `~/.local/share/app-manager`.
pub fn data_dir() -> PathBuf {
    if let Some(dir) = std::env::var_os("XDG_DATA_HOME") {
        return PathBuf::from(dir).join("app-manager");
    }
    let home = std::env::var_os("HOME").map(PathBuf::from).unwrap_or_default();
    home.join(".local/share/app-manager")
}

pub fn registry_path() -> PathBuf {
    data_dir().join("installed.json")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_dir() -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!(
            "appmgr-registry-{}-{unique}",
            std::process::id()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn managed() -> ManagedApp {
        ManagedApp {
            repo: "owner/sonora".to_owned(),
            format: "appimage".to_owned(),
            asset: "sonora-v0.45.0-x86_64.AppImage".to_owned(),
            installed_version: "v0.45.0".to_owned(),
            package: None,
            path: Some("/home/ed/Applications/Sonora.AppImage".to_owned()),
        }
    }

    #[test]
    fn saves_and_loads_the_source_of_an_app() {
        let dir = temp_dir();
        let path = dir.join("installed.json");
        let mut registry = Registry::default();
        registry.insert("appimage:/home/ed/Applications/Sonora.AppImage".to_owned(), managed());

        registry.save_to(&path).unwrap();
        let loaded = Registry::load_from(&path);

        assert_eq!(registry.apps, loaded.apps);
        assert_eq!(loaded.get("appimage:/home/ed/Applications/Sonora.AppImage"), Some(&managed()));
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn a_missing_file_is_an_empty_registry() {
        let dir = temp_dir();
        let loaded = Registry::load_from(&dir.join("missing.json"));
        assert!(loaded.apps.is_empty());
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn invalid_json_is_an_empty_registry() {
        let dir = temp_dir();
        let path = dir.join("installed.json");
        std::fs::write(&path, "{not json").unwrap();
        assert!(Registry::load_from(&path).apps.is_empty());
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn removing_an_app_drops_its_source() {
        let mut registry = Registry::default();
        registry.insert("apt:sonora".to_owned(), managed());
        assert!(registry.remove("apt:sonora").is_some());
        assert!(registry.get("apt:sonora").is_none());
    }

    #[test]
    fn old_files_without_optional_fields_still_load() {
        let dir = temp_dir();
        let path = dir.join("installed.json");
        std::fs::write(
            &path,
            r#"{"apps":{"apt:sonora":{"repo":"owner/sonora","format":"deb"}}}"#,
        )
        .unwrap();
        let loaded = Registry::load_from(&path);
        let app = loaded.get("apt:sonora").unwrap();
        assert_eq!(app.asset, "");
        assert_eq!(app.installed_version, "");
        std::fs::remove_dir_all(dir).unwrap();
    }
}
