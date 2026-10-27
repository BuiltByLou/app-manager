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
    Ok(())
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
