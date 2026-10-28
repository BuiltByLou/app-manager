//! Installing assets from a GitHub release. Downloads happen through curl,
//! never through a shell, and privileged steps go through sudo -n.

use std::path::{Path, PathBuf};

pub fn download(url: &str, destination: &Path) -> Result<(), String> {
    if let Some(parent) = destination.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|error| format!("could not create {}: {error}", parent.display()))?;
    }
    let output = std::process::Command::new("curl")
        .args(["-fsSL", "-o", &destination.to_string_lossy(), url])
        .output()
        .map_err(|error| format!("could not run curl: {error}"))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_owned();
        return Err(if stderr.is_empty() {
            format!("curl exited with {}", output.status.code().unwrap_or(-1))
        } else {
            stderr
        });
    }
    Ok(())
}

pub fn install_deb(path: &Path) -> Result<(), String> {
    run_sudo(&["apt-get", "install", "-y", &path.to_string_lossy()])
}

pub fn install_flatpak(path: &Path) -> Result<(), String> {
    run_sudo(&["flatpak", "install", "-y", &path.to_string_lossy()])
}

pub fn install_appimage(source: &Path, destination: &Path) -> Result<(), String> {
    if let Some(parent) = destination.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|error| format!("could not create {}: {error}", parent.display()))?;
    }
    std::fs::copy(source, destination)
        .map_err(|error| format!("could not copy the AppImage: {error}"))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let permissions = std::fs::Permissions::from_mode(0o755);
        std::fs::set_permissions(destination, permissions)
            .map_err(|error| format!("could not make the AppImage executable: {error}"))?;
    }

    let stem = destination
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("app")
        .to_owned();

    // Extract an icon from the AppImage so it shows up in the launcher.
    let tmp = std::env::temp_dir().join(format!("appmgr-extract-{}", stem));
    let _ = std::fs::remove_dir_all(&tmp);
    let _ = std::fs::create_dir_all(&tmp);
    let extract_ok = std::process::Command::new(destination)
        .arg("--appimage-extract")
        .current_dir(&tmp)
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false);

    let icons_dir = std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_default()
        .join(".local/share/icons");
    let _ = std::fs::create_dir_all(&icons_dir);

    let mut icon_ref = "application-x-executable".to_owned();
    if extract_ok {
        let root = tmp.join("squashfs-root");
        // Prefer .DirIcon
        let diricon = root.join(".DirIcon");
        let mut icon_src: Option<PathBuf> = if diricon.exists() {
            Some(diricon)
        } else {
            // Find the largest png/svg/xpm icon.
            let mut best: Option<(u64, PathBuf)> = None;
            for entry in walkdir(&root) {
                let ext = entry
                    .extension()
                    .and_then(|e| e.to_str())
                    .unwrap_or("")
                    .to_lowercase();
                if ext != "png" && ext != "svg" && ext != "xpm" {
                    continue;
                }
                if let Ok(meta) = std::fs::metadata(&entry) {
                    if best.as_ref().map(|(s, _)| meta.len() > *s).unwrap_or(true) {
                        best = Some((meta.len(), entry));
                    }
                }
            }
            best.map(|(_, p)| p)
        };
        if let Some(src) = icon_src.take() {
            let ext = src.extension().and_then(|e| e.to_str()).unwrap_or("png");
            let dest_icon = icons_dir.join(format!("{}.{}", stem, ext));
            if std::fs::copy(&src, &dest_icon).is_ok() {
                icon_ref = stem.clone();
            }
        }
    }
    let _ = std::fs::remove_dir_all(&tmp);

    // Create a .desktop entry so the app shows up in the launcher.
    let desktop_dir = std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_default()
        .join(".local/share/applications");
    let _ = std::fs::create_dir_all(&desktop_dir);
    let desktop_path = desktop_dir.join(format!("{}.desktop", stem));
    let desktop_content = format!(
        "[Desktop Entry]\nType=Application\nName={}\nExec={}\nIcon={}\nCategories=Utility;\nTerminal=false\n",
        stem, destination.display(), icon_ref
    );
    let _ = std::fs::write(&desktop_path, desktop_content);
    // Refresh the desktop database if available.
    let _ = std::process::Command::new("update-desktop-database")
        .arg(&desktop_dir)
        .output();
    let _ = std::process::Command::new("gtk-update-icon-cache")
        .arg(&icons_dir)
        .output();
    Ok(())
}

fn walkdir(root: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        if let Ok(entries) = std::fs::read_dir(&dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    stack.push(path);
                } else {
                    out.push(path);
                }
            }
        }
    }
    out
}

fn run_sudo(args: &[&str]) -> Result<(), String> {
    let mut full: Vec<&str> = Vec::with_capacity(args.len() + 1);
    full.push("sudo");
    full.push("-n");
    full.extend_from_slice(args);
    let output = std::process::Command::new(full[0])
        .args(&full[1..])
        .output()
        .map_err(|error| format!("could not run sudo: {error}"))?;
    if output.status.success() {
        Ok(())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_owned();
        Err(if stderr.is_empty() {
            format!("sudo exited with {}", output.status.code().unwrap_or(-1))
        } else {
            stderr
        })
    }
}

pub fn downloads_dir() -> PathBuf {
    let home = std::env::var_os("HOME").map(PathBuf::from).unwrap_or_default();
    home.join("Downloads")
}

pub fn applications_dir() -> PathBuf {
    let home = std::env::var_os("HOME").map(PathBuf::from).unwrap_or_default();
    home.join("Applications")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn downloads_dir_is_under_home() {
        let dir = downloads_dir();
        assert!(dir.ends_with("Downloads"));
    }

    #[test]
    fn applications_dir_is_under_home() {
        let dir = applications_dir();
        assert!(dir.ends_with("Applications"));
    }
}
