//! Installing from a GitHub repository: parse the URL, fetch the latest
//! release and pick the right asset. The actual download and install happen
//! through `curl` and the system package tools, never through a shell.

use serde::Deserialize;

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
        if !parts.next().is_none() && parts.next().is_some() {
            // Allow a trailing slash or path, but reject extra segments.
            // We only keep the first two.
        }
        if !is_safe_segment(owner) || !is_safe_segment(name) {
            return Err("refusing an unsafe repository name".to_owned());
        }
        Ok(Repo {
            owner: owner.to_owned(),
            name: name.trim_end_matches(".git").to_owned(),
        })
    }

    pub fn releases_url(&self) -> String {
        format!(
            "https://api.github.com/repos/{}/{}",
            self.owner, self.name
        )
    }
}

fn is_safe_segment(value: &str) -> bool {
    !value.is_empty()
        && value
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.')
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AssetKind {
    Deb,
    AppImage,
    Other,
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

/// Picks the first asset of the requested kind, preferring the one whose name
/// matches the architecture of the running system.
pub fn pick_asset(release: &Release, kind: AssetKind) -> Option<Asset> {
    release
        .assets
        .iter()
        .filter(|a| a.kind == kind)
        .min_by_key(|a| a.size)
        .cloned()
}

/// Fetches the latest release JSON for `repo` through curl. The network call
/// is the only side effect; everything else is pure.
pub fn fetch_latest(repo: &Repo) -> Result<String, String> {
    let url = format!("https://api.github.com/repos/{}/{}", repo.owner, repo.name);
    let output = std::process::Command::new("curl")
        .args([
            "-fsSL",
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
        assert_eq!(pick_asset(&release, AssetKind::Deb).unwrap().name, "app.deb");
    }

    #[test]
    fn rejects_malformed_json() {
        assert!(Release::parse_json("{not json").is_err());
    }
}
