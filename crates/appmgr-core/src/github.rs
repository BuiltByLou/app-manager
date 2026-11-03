//! Installing from a GitHub repository: parse the URL, fetch the latest
//! release and pick the right asset. The actual download and install happen
//! through `curl` and the system package tools, never through a shell.

use serde::Deserialize;
use std::sync::OnceLock;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Repo {
    pub owner: String,
    pub name: String,
}

impl Repo {
    pub fn parse(input: &str) -> Result<Self, String> {
        let trimmed = input.trim();
        let trimmed = trimmed
            .trim_start_matches("https://")
            .trim_start_matches("http://")
            .trim_start_matches("github.com/")
            .trim_start_matches("www.github.com/");

        let mut parts = trimmed.split('/');
        let owner = parts
            .next()
            .filter(|s| !s.is_empty())
            .ok_or_else(|| "missing repository owner".to_owned())?;
        let name = parts
            .next()
            .filter(|s| !s.is_empty())
            .ok_or_else(|| "missing repository name".to_owned())?;
        if !is_safe_segment(owner) || !is_safe_segment(name) {
            return Err("refusing an unsafe repository name".to_owned());
        }
        Ok(Repo {
            owner: owner.to_owned(),
            name: name.trim_end_matches(".git").to_owned(),
        })
    }

    /// `owner/repo`, the form stored in the source registry and shown in the UI.
    pub fn slug(&self) -> String {
        format!("{}/{}", self.owner, self.name)
    }

    pub fn release_url(&self, tag: &str) -> String {
        format!(
            "https://github.com/{}/{}/releases/tag/{}",
            self.owner, self.name, tag
        )
    }
}

impl std::fmt::Display for Repo {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.slug())
    }
}

fn is_safe_segment(value: &str) -> bool {
    !value.is_empty()
        && value
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.')
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AssetKind {
    Deb,
    AppImage,
    Flatpak,
    Other,
}

impl AssetKind {
    /// Stable name used by the source registry.
    pub fn as_str(self) -> &'static str {
        match self {
            AssetKind::Deb => "deb",
            AssetKind::AppImage => "appimage",
            AssetKind::Flatpak => "flatpak",
            AssetKind::Other => "other",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "deb" => Some(AssetKind::Deb),
            "appimage" => Some(AssetKind::AppImage),
            "flatpak" => Some(AssetKind::Flatpak),
            "other" => Some(AssetKind::Other),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Asset {
    pub name: String,
    pub url: String,
    pub size: u64,
    pub kind: AssetKind,
}

impl Asset {
    pub fn from_name(name: &str, url: &str, size: u64) -> Self {
        let kind = if name.ends_with(".deb") {
            AssetKind::Deb
        } else if name.ends_with(".AppImage") || name.ends_with(".appimage") {
            AssetKind::AppImage
        } else if name.ends_with(".flatpak") {
            AssetKind::Flatpak
        } else {
            AssetKind::Other
        };
        Asset {
            name: name.to_owned(),
            url: url.to_owned(),
            size,
            kind,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Release {
    pub tag: String,
    pub assets: Vec<Asset>,
}

#[derive(Deserialize)]
struct RawRelease {
    tag_name: String,
    assets: Vec<RawAsset>,
}

#[derive(Deserialize)]
struct RawAsset {
    name: String,
    browser_download_url: String,
    size: u64,
}

impl Release {
    pub fn parse_json(text: &str) -> Result<Self, String> {
        let raw: RawRelease =
            serde_json::from_str(text).map_err(|error| format!("invalid release JSON: {error}"))?;
        Ok(Release {
            tag: raw.tag_name,
            assets: raw
                .assets
                .into_iter()
                .map(|a| Asset::from_name(&a.name, &a.browser_download_url, a.size))
                .collect(),
        })
    }
}

/// Returns the Debian architecture of the running system (e.g. `amd64`).
///
/// The answer cannot change while the app runs, and asking costs a process
/// spawn, so it is detected once and kept.
pub fn system_arch() -> &'static str {
    static ARCH: OnceLock<String> = OnceLock::new();
    ARCH.get_or_init(detect_arch)
}

fn detect_arch() -> String {
    if let Ok(output) = std::process::Command::new("dpkg")
        .args(["--print-architecture"])
        .output()
        && output.status.success()
    {
        let arch = String::from_utf8_lossy(&output.stdout).trim().to_owned();
        if !arch.is_empty() {
            return arch;
        }
    }
    // Fallback to uname -m, mapped to Debian-style names.
    if let Ok(output) = std::process::Command::new("uname").arg("-m").output()
        && output.status.success()
    {
        let machine = String::from_utf8_lossy(&output.stdout).trim().to_owned();
        return match machine.as_str() {
            "x86_64" => "amd64".to_owned(),
            "aarch64" => "arm64".to_owned(),
            "armv7l" => "armhf".to_owned(),
            other => other.to_owned(),
        };
    }
    "amd64".to_owned()
}

const KNOWN_ARCHES: &[&str] = &[
    "amd64", "arm64", "armhf", "i386", "riscv64", "ppc64el", "s390x",
    "x86_64", "aarch64", "armv7",
];

/// Whether a token found in an asset name is the architecture we run on.
fn arch_matches(arch: &str, token: &str) -> bool {
    arch == token
        || matches!(
            (arch, token),
            ("amd64", "x86_64") | ("arm64", "aarch64") | ("armhf", "armv7")
        )
}

/// Returns true when the asset name is compatible with the running system.
pub fn is_compatible(asset: &Asset) -> bool {
    let arch = system_arch();
    let name = asset.name.to_ascii_lowercase();
    // An asset that names an architecture must name ours; one that names none
    // is generic and assumed to run anywhere.
    let mut mentioned = false;
    for known in KNOWN_ARCHES {
        if name.contains(known) {
            mentioned = true;
            if arch_matches(arch, known) {
                return true;
            }
        }
    }
    !mentioned
}

/// The assets the user can install: only the package formats the app handles,
/// and only those built for this machine.
pub fn installable_assets(release: &Release) -> Vec<Asset> {
    release
        .assets
        .iter()
        .filter(|asset| asset.kind != AssetKind::Other && is_compatible(asset))
        .cloned()
        .collect()
}

/// Fetches the latest release JSON for `repo` through curl. The network call
/// is the only side effect; everything else is pure.
pub fn fetch_latest(repo: &Repo) -> Result<String, String> {
    let url = format!(
        "https://api.github.com/repos/{}/{}/releases/latest",
        repo.owner, repo.name
    );
    let output = std::process::Command::new("curl")
        .args([
            "-fsSL",
            "--connect-timeout",
            "10",
            "--max-time",
            "60",
            "-H",
            "Accept: application/vnd.github+json",
            "-H",
            "User-Agent: app-manager",
            &url,
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
    String::from_utf8(output.stdout).map_err(|error| format!("invalid UTF-8 from GitHub: {error}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_a_full_url() {
        let repo = Repo::parse("https://github.com/owner/repo").unwrap();
        assert_eq!(repo.owner, "owner");
        assert_eq!(repo.name, "repo");
    }

    #[test]
    fn parses_a_bare_slug() {
        let repo = Repo::parse("owner/repo").unwrap();
        assert_eq!(repo.owner, "owner");
        assert_eq!(repo.name, "repo");
    }

    #[test]
    fn strips_git_suffix() {
        let repo = Repo::parse("https://github.com/owner/repo.git").unwrap();
        assert_eq!(repo.name, "repo");
    }

    #[test]
    fn keeps_the_slug_and_builds_release_links() {
        let repo = Repo::parse("https://github.com/owner/repo").unwrap();
        assert_eq!(repo.slug(), "owner/repo");
        assert_eq!(repo.to_string(), "owner/repo");
        assert_eq!(
            repo.release_url("v1.2.3"),
            "https://github.com/owner/repo/releases/tag/v1.2.3"
        );
    }

    /// A real release mixes every platform. The picker must still find the
    /// Linux builds for this machine, or the install card ends up empty.
    #[test]
    fn keeps_the_linux_formats_of_a_mixed_release() {
        let json = r#"{"tag_name":"v0.47.0","assets":[
            {"name":"Sonora-Setup.exe","browser_download_url":"u","size":1},
            {"name":"sonora-v0.47.0-aarch64.AppImage","browser_download_url":"u","size":1},
            {"name":"sonora-v0.47.0-x86_64.AppImage","browser_download_url":"u","size":1},
            {"name":"sonora-v0.47.0-aarch64.flatpak","browser_download_url":"u","size":1},
            {"name":"sonora-v0.47.0-x86_64.flatpak","browser_download_url":"u","size":1},
            {"name":"sonora-v0.47.0-x86_64-unknown-linux-gnu","browser_download_url":"u","size":1},
            {"name":"sonora-v0.47.0-licenses.tar.gz","browser_download_url":"u","size":1}
        ]}"#;
        let release = Release::parse_json(json).unwrap();
        let assets = installable_assets(&release);

        assert!(!assets.is_empty(), "the card would have no button at all");
        assert!(assets.iter().all(is_compatible));
        assert!(assets.iter().all(|asset| asset.kind != AssetKind::Other));
        assert!(assets.iter().all(|asset| !asset.name.ends_with(".exe")));
    }

    #[test]
    fn asset_kind_names_round_trip() {
        for kind in [
            AssetKind::Deb,
            AssetKind::AppImage,
            AssetKind::Flatpak,
            AssetKind::Other,
        ] {
            assert_eq!(AssetKind::parse(kind.as_str()), Some(kind));
        }
        assert_eq!(AssetKind::parse("snap"), None);
    }

    #[test]
    fn rejects_unsafe_names() {
        assert!(Repo::parse("owner/repo;rm -rf /").is_err());
        assert!(Repo::parse("only-one").is_err());
        assert!(Repo::parse("").is_err());
    }

    #[test]
    fn classifies_assets_by_extension() {
        assert_eq!(Asset::from_name("app.deb", "u", 1).kind, AssetKind::Deb);
        assert_eq!(
            Asset::from_name("app.AppImage", "u", 1).kind,
            AssetKind::AppImage
        );
        assert_eq!(Asset::from_name("README.md", "u", 1).kind, AssetKind::Other);
    }

    #[test]
    fn parses_a_release() {
        let json = r#"{
            "tag_name": "v1.2.3",
            "assets": [
                {"name": "app.deb", "browser_download_url": "https://x/app.deb", "size": 100},
                {"name": "app.AppImage", "browser_download_url": "https://x/app.AppImage", "size": 200}
            ]
        }"#;
        let release = Release::parse_json(json).unwrap();
        assert_eq!(release.tag, "v1.2.3");
        assert_eq!(release.assets.len(), 2);
        assert_eq!(installable_assets(&release).len(), 2);
    }

    #[test]
    fn keeps_only_installable_compatible_assets() {
        let foreign = if system_arch() == "amd64" {
            "aarch64"
        } else {
            "amd64"
        };
        let json = format!(
            r#"{{
                "tag_name": "v1",
                "assets": [
                    {{"name": "app_{foreign}.deb", "browser_download_url": "u", "size": 1}},
                    {{"name": "app.deb", "browser_download_url": "u", "size": 1}},
                    {{"name": "SHA256SUMS", "browser_download_url": "u", "size": 1}}
                ]
            }}"#
        );
        let release = Release::parse_json(&json).unwrap();
        let assets = installable_assets(&release);
        assert_eq!(assets.len(), 1);
        assert_eq!(assets[0].name, "app.deb");
    }

    #[test]
    fn rejects_malformed_json() {
        assert!(Release::parse_json("{not json").is_err());
    }
}
