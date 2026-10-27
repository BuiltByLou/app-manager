//! Leftover files and integration artifacts that an app may leave behind after
//! removal. They are only ever sent to the trash, never deleted outright.

use std::path::Path;

use crate::inventory::App;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LeftoverKind {
    Config,
    Data,
    Cache,
    SystemdUnit,
    DesktopEntry,
    Icon,
}

impl LeftoverKind {
    pub fn label(self) -> &'static str {
        match self {
            LeftoverKind::Config => "config",
            LeftoverKind::Data => "data",
            LeftoverKind::Cache => "cache",
            LeftoverKind::SystemdUnit => "systemd unit",
            LeftoverKind::DesktopEntry => "desktop entry",
            LeftoverKind::Icon => "icon",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Leftover {
    pub path: String,
    pub kind: LeftoverKind,
}

/// Collects every leftover that exists for `app` under `home`. Matching is
/// conservative: a directory or file name must equal a known needle (the app
/// name, its package, its Flatpak id, or the AppImage basename) or start with
/// that needle followed by a separator.
pub fn find_leftovers(app: &App, home: &Path) -> Vec<Leftover> {
    let mut needles: Vec<String> = Vec::new();
    push_needle(&mut needles, &app.name);
    if let Some(package) = &app.package {
        push_needle(&mut needles, package);
    }
    if let Some(stripped) = app.id.strip_prefix("appimage:") {
        if let Some(name) = Path::new(stripped).file_name() {
            push_needle(&mut needles, &name.to_string_lossy());
        }
    }
    // Flatpak id is already in `package` for Flatpak apps, but the id also
    // contains dots that map to a directory name in some setups.
    if app.origin == crate::inventory::Origin::Flatpak {
        push_needle(&mut needles, &app.id.trim_start_matches("flatpak:").to_owned());
    }

    let mut leftovers = Vec::new();

    // ~/.config, ~/.local/share, ~/.cache
    for (subdir, kind) in [
        (".config", LeftoverKind::Config),
        (".local/share", LeftoverKind::Data),
        (".cache", LeftoverKind::Cache),
    ] {
        let base = home.join(subdir);
        if let Ok(entries) = std::fs::read_dir(&base) {
            for entry in entries.flatten() {
                let name = entry.file_name().to_string_lossy().to_string();
                if matches_needle(&name, &needles) {
                    let path = entry.path();
                    if path.exists() {
                        leftovers.push(Leftover {
                            path: path.to_string_lossy().to_string(),
                            kind,
                        });
                    }
                }
            }
        }
    }

    // systemd user units
    for subdir in [".config/systemd/user", ".local/share/systemd/user"] {
        let base = home.join(subdir);
        if let Ok(entries) = std::fs::read_dir(&base) {
            for entry in entries.flatten() {
                let name = entry.file_name().to_string_lossy().to_string();
                if name.ends_with(".service")
                    || name.ends_with(".timer")
                    || name.ends_with(".socket")
                {
                    let stem = name
                        .trim_end_matches(".service")
                        .trim_end_matches(".timer")
                        .trim_end_matches(".socket");
                    if matches_needle(stem, &needles) {
                        let path = entry.path();
                        if path.exists() {
                            leftovers.push(Leftover {
                                path: path.to_string_lossy().to_string(),
                                kind: LeftoverKind::SystemdUnit,
                            });
                        }
                    }
                }
            }
        }
    }

    // AppImage integration: .desktop entries and icons pointing at the file
    if app.origin == crate::inventory::Origin::AppImage {
        if let Some(image_path) = app.id.strip_prefix("appimage:") {
            leftovers.extend(find_appimage_integration(image_path, home));
        }
    }

    // Flatpak data directory in ~/.var/app/<id>
    if app.origin == crate::inventory::Origin::Flatpak {
        let flatpak_id = app.package.as_deref().unwrap_or("");
        if !flatpak_id.is_empty() {
            let data_dir = home.join(".var").join("app").join(flatpak_id);
            if data_dir.exists() {
                leftovers.push(Leftover {
                    path: data_dir.to_string_lossy().to_string(),
                    kind: LeftoverKind::Data,
                });
            }
        }
    }

    leftovers.sort_by(|a, b| a.path.cmp(&b.path));
    leftovers.dedup_by(|a, b| a.path == b.path);
    leftovers
}

fn push_needle(needles: &mut Vec<String>, value: &str) {
    let needle = value.trim().to_lowercase();
    if needle.is_empty() {
        return;
    }
    // For names with spaces, also try the first word and a slug.
    if !needles.contains(&needle) {
        needles.push(needle.clone());
    }
    let slug: String = needle.chars().map(|c| if c.is_alphanumeric() { c } else { '-' }).collect();
    let slug = slug.trim_matches('-').to_owned();
    if !slug.is_empty() && slug != needle && !needles.contains(&slug) {
        needles.push(slug);
    }
    // First word only, for multi-word names like "COSMIC Files".
    if let Some(first) = needle.split_whitespace().next() {
        if first.len() >= 3 && !needles.contains(&first.to_owned()) {
            needles.push(first.to_owned());
        }
    }
}

fn matches_needle(name: &str, needles: &[String]) -> bool {
    let lower = name.to_lowercase();
    for needle in needles {
        if lower == *needle {
            return true;
        }
        // Prefix followed by a separator.
        for separator in ['-', '_', '.', ' '] {
            if lower.starts_with(&format!("{needle}{separator}")) {
                return true;
            }
        }
    }
    false
}

fn find_appimage_integration(image_path: &str, home: &Path) -> Vec<Leftover> {
    let mut leftovers = Vec::new();

    // .desktop entries whose Exec line references the AppImage.
    let apps_dir = home.join(".local/share/applications");
    if let Ok(entries) = std::fs::read_dir(&apps_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("desktop") {
                continue;
            }
            if let Ok(content) = std::fs::read_to_string(&path) {
                if content.contains(image_path) {
                    leftovers.push(Leftover {
                        path: path.to_string_lossy().to_string(),
                        kind: LeftoverKind::DesktopEntry,
                    });
                }
            }
        }
    }

    // Icons: search ~/.local/share/icons for files whose name contains the
    // AppImage basename stem.
    let stem = Path::new(image_path)
        .file_stem()
        .map(|s| s.to_string_lossy().to_lowercase())
        .unwrap_or_default();
    if !stem.is_empty() {
        let icons_root = home.join(".local/share/icons");
        collect_icons(&icons_root, &stem, &mut leftovers);
    }

    leftovers
}

fn collect_icons(root: &Path, stem: &str, leftovers: &mut Vec<Leftover>) {
    let Ok(entries) = std::fs::read_dir(root) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_icons(&path, stem, leftovers);
        } else {
            let name = entry.file_name().to_string_lossy().to_lowercase();
            if name.contains(stem) {
                leftovers.push(Leftover {
                    path: path.to_string_lossy().to_string(),
                    kind: LeftoverKind::Icon,
                });
            }
        }
    }
}

/// Sends a single path to the trash. Returns an error if `gio` is missing
/// or refuses.
pub fn trash_path(path: &str) -> Result<(), String> {
    let output = std::process::Command::new("gio")
        .args(["trash", path])
        .output()
        .map_err(|error| format!("could not run gio: {error}"))?;
    if output.status.success() {
        Ok(())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_owned();
        if stderr.is_empty() {
            Err(format!("gio trash exited with {}", output.status.code().unwrap_or(-1)))
        } else {
            Err(stderr)
        }
    }
}

/// Appends one line to the removal history file.
pub fn append_history(home: &Path, app_name: &str, origin: &str, result: &str) {
    let dir = home.join(".local/share/app-manager");
    let _ = std::fs::create_dir_all(&dir);
    let path = dir.join("history.log");
    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let line = format!("{timestamp}\t{app_name}\t{origin}\t{result}\n");
    use std::io::Write;
    if let Ok(mut file) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
    {
        let _ = file.write_all(line.as_bytes());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::inventory::{App, InstallKind, Origin};
    use std::fs;

    fn app(name: &str, package: &str, origin: Origin) -> App {
        App {
            id: format!("{origin:?}:{name}"),
            name: name.to_owned(),
            version: None,
            icon: None,
            origin,
            kind: InstallKind::User,
            protected: false,
            size_bytes: 0,
            package: Some(package.to_owned()),
        }
    }

    fn temp_home(name: &str) -> std::path::PathBuf {
        let path = std::env::temp_dir().join(format!("appmgr-leftovers-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&path);
        fs::create_dir_all(&path).unwrap();
        path
    }

    #[test]
    fn finds_config_and_cache_dirs() {
        let home = temp_home("basic");
        fs::create_dir_all(home.join(".config").join("Firefox")).unwrap();
        fs::create_dir_all(home.join(".cache").join("firefox")).unwrap();
        fs::create_dir_all(home.join(".local/share").join("firefox")).unwrap();

        let leftovers = find_leftovers(&app("Firefox", "firefox", Origin::Apt), &home);
        assert_eq!(leftovers.len(), 3);
    }

    #[test]
    fn ignores_unrelated_dirs() {
        let home = temp_home("ignore");
        fs::create_dir_all(home.join(".config").join("gtk-4.0")).unwrap();
        fs::create_dir_all(home.join(".config").join("cosmic")).unwrap();

        let leftovers = find_leftovers(&app("Firefox", "firefox", Origin::Apt), &home);
        assert!(leftovers.is_empty());
    }

    #[test]
    fn finds_flatpak_data_dir() {
        let home = temp_home("flatpak");
        let data = home.join(".var").join("app").join("com.example.App");
        fs::create_dir_all(&data).unwrap();

        let mut a = app("App", "com.example.App", Origin::Flatpak);
        a.id = "flatpak:com.example.App".to_owned();
        let leftovers = find_leftovers(&a, &home);
        assert!(leftovers.iter().any(|l| l.path == data.to_string_lossy()));
    }

    #[test]
    fn finds_appimage_desktop_and_icon() {
        let home = temp_home("appimage");
        let apps_dir = home.join(".local/share/applications");
        fs::create_dir_all(&apps_dir).unwrap();
        fs::write(
            apps_dir.join("mytool.desktop"),
            "Exec=/home/ed/Applications/mytool.AppImage\n",
        )
        .unwrap();
        let icons_dir = home.join(".local/share/icons").join("hicolor").join("48x48").join("apps");
        fs::create_dir_all(&icons_dir).unwrap();
        fs::write(icons_dir.join("mytool.png"), b"").unwrap();

        let mut a = app("Tool", "", Origin::AppImage);
        a.id = "appimage:/home/ed/Applications/mytool.AppImage".to_owned();
        let leftovers = find_leftovers(&a, &home);
        assert!(leftovers.iter().any(|l| l.kind == LeftoverKind::DesktopEntry));
        assert!(leftovers.iter().any(|l| l.kind == LeftoverKind::Icon));
    }

    #[test]
    fn finds_systemd_units() {
        let home = temp_home("systemd");
        let units = home.join(".config/systemd/user");
        fs::create_dir_all(&units).unwrap();
        fs::write(units.join("firefox.service"), "[Unit]\n").unwrap();

        let leftovers = find_leftovers(&app("Firefox", "firefox", Origin::Apt), &home);
        assert!(leftovers.iter().any(|l| l.kind == LeftoverKind::SystemdUnit));
    }
}
