//! Installing assets from a GitHub release. Downloads happen through curl,
//! never through a shell, and privileged steps go through sudo -n.

use std::path::{Path, PathBuf};

pub fn download(url: &str, destination: &Path) -> Result<(), String> {
    if let Some(parent) = destination.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|error| format!("could not create {}: {error}", parent.display()))?;
    }
    let output = std::process::Command::new("curl")
        .args([
            "-fsSL",
            "--connect-timeout",
            "10",
            "--max-time",
            "900",
            "-o",
            &destination.to_string_lossy(),
            url,
        ])
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

/// Installs or updates a Flatpak bundle and returns the installed app id.
pub fn install_flatpak(path: &Path) -> Result<String, String> {
    let output = run_sudo_output(&[
        "flatpak",
        "install",
        "--bundle",
        "--assumeyes",
        "--noninteractive",
        "--or-update",
        &path.to_string_lossy(),
    ])?;
    parse_flatpak_ref(&output)
        .ok_or_else(|| "flatpak finished without reporting which app it installed".to_owned())
}

/// Reads the `Package:` field from a `.deb` without installing it.
pub fn deb_package_name(path: &Path) -> Result<String, String> {
    let output = std::process::Command::new("dpkg-deb")
        .args(["-f", &path.to_string_lossy(), "Package"])
        .output()
        .map_err(|error| format!("could not run dpkg-deb: {error}"))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_owned();
        return Err(if stderr.is_empty() {
            format!(
                "dpkg-deb exited with {}",
                output.status.code().unwrap_or(-1)
            )
        } else {
            stderr
        });
    }
    parse_deb_package_field(&String::from_utf8_lossy(&output.stdout))
        .ok_or_else(|| "the package does not declare a name".to_owned())
}

fn parse_deb_package_field(text: &str) -> Option<String> {
    let name = text.lines().next()?.trim();
    if name.is_empty() {
        None
    } else {
        Some(name.to_owned())
    }
}

/// Extracts the app id from `flatpak install` output, which mentions a ref
/// such as `app/org.example.App/x86_64/stable`.
fn parse_flatpak_ref(output: &str) -> Option<String> {
    output.split_whitespace().find_map(|token| {
        token
            .strip_prefix("app/")
            .and_then(|rest| rest.split('/').next())
            .filter(|id| !id.is_empty())
            .map(str::to_owned)
    })
}

/// Stable file name for an AppImage installed from `repo`. Keeping the version
/// out of the name is what lets an update replace the file in place.
pub fn stable_appimage_path(repo_name: &str) -> PathBuf {
    let stem: String = repo_name
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.' {
                c
            } else {
                '-'
            }
        })
        .collect();
    applications_dir().join(format!("{stem}.AppImage"))
}

/// The files an AppImage install created for itself: the image, its desktop
/// entry and its icon. Used to clean up after an update moved to a new name.
pub fn appimage_integration_paths(image: &Path, home: &Path) -> Vec<PathBuf> {
    let Some(stem) = image.file_stem().and_then(|stem| stem.to_str()) else {
        return Vec::new();
    };

    let mut paths = vec![
        image.to_path_buf(),
        home.join(".local/share/applications")
            .join(format!("{stem}.desktop")),
    ];
    for extension in ["png", "svg", "xpm"] {
        paths.push(
            home.join(".local/share/icons")
                .join(format!("{stem}.{extension}")),
        );
    }
    paths
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
    let tmp = cache_dir().join(format!("extract-{stem}"));
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
            // Find the largest icon in the icon theme, or one sitting next to
            // AppRun. Walking the whole tree would also pick up splash screens.
            let theme_icons = root.join("usr/share/icons");
            let mut best: Option<(u64, PathBuf)> = None;
            for entry in walkdir(&root) {
                if !entry.starts_with(&theme_icons) && entry.parent() != Some(root.as_path()) {
                    continue;
                }
                let ext = entry
                    .extension()
                    .and_then(|e| e.to_str())
                    .unwrap_or("")
                    .to_ascii_lowercase();
                if ext != "png" && ext != "svg" && ext != "xpm" {
                    continue;
                }
                if let Ok(meta) = std::fs::metadata(&entry)
                    && best.as_ref().map(|(s, _)| meta.len() > *s).unwrap_or(true)
                {
                    best = Some((meta.len(), entry));
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
    run_sudo_output(args).map(|_| ())
}

fn run_sudo_output(args: &[&str]) -> Result<String, String> {
    let mut full: Vec<&str> = Vec::with_capacity(args.len() + 1);
    full.push("sudo");
    full.push("-n");
    full.extend_from_slice(args);
    let output = std::process::Command::new(full[0])
        .args(&full[1..])
        .output()
        .map_err(|error| format!("could not run sudo: {error}"))?;
    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).into_owned())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_owned();
        Err(if stderr.is_empty() {
            format!("sudo exited with {}", output.status.code().unwrap_or(-1))
        } else {
            stderr
        })
    }
}

/// Where downloads wait until they are installed.
pub fn cache_dir() -> PathBuf {
    let home = std::env::var_os("HOME").map(PathBuf::from).unwrap_or_default();
    home.join(".cache/app-manager/downloads")
}

pub fn applications_dir() -> PathBuf {
    let home = std::env::var_os("HOME").map(PathBuf::from).unwrap_or_default();
    home.join("Applications")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cache_dir_is_under_home() {
        let dir = cache_dir();
        assert!(dir.ends_with(".cache/app-manager/downloads"));
    }

    #[test]
    fn applications_dir_is_under_home() {
        let dir = applications_dir();
        assert!(dir.ends_with("Applications"));
    }

    #[test]
    fn reads_the_package_name_from_dpkg_deb_output() {
        assert_eq!(
            parse_deb_package_field("sonora\n").as_deref(),
            Some("sonora")
        );
        assert_eq!(parse_deb_package_field("  \n"), None);
    }

    #[test]
    fn reads_the_app_id_from_flatpak_output() {
        let output = "Installing app/io.github.example.Sonora/x86_64/stable\n";
        assert_eq!(
            parse_flatpak_ref(output).as_deref(),
            Some("io.github.example.Sonora")
        );
        assert_eq!(parse_flatpak_ref("nothing here"), None);
    }

    #[test]
    fn stable_appimage_name_has_no_version() {
        let path = stable_appimage_path("sonora");
        assert_eq!(path.file_name().and_then(|name| name.to_str()), Some("sonora.AppImage"));
    }

    #[test]
    fn lists_everything_an_appimage_install_created() {
        let home = Path::new("/home/ed");
        let paths = appimage_integration_paths(
            Path::new("/home/ed/Applications/sonora-v0.45.0-x86_64.AppImage"),
            home,
        );
        assert!(paths.contains(&PathBuf::from(
            "/home/ed/Applications/sonora-v0.45.0-x86_64.AppImage"
        )));
        assert!(paths.contains(&PathBuf::from(
            "/home/ed/.local/share/applications/sonora-v0.45.0-x86_64.desktop"
        )));
        assert!(paths.contains(&PathBuf::from(
            "/home/ed/.local/share/icons/sonora-v0.45.0-x86_64.png"
        )));
    }
}
