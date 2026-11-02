//! Deciding whether an installed app is behind its repository's latest
//! release, and which asset would update it.

use crate::github::{Asset, AssetKind, Release, is_compatible};
use crate::version;
use std::cmp::Ordering;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UpdateStatus {
    /// The versions cannot be compared, so the app never claims "up to date".
    Unknown,
    UpToDate,
    Available,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UpdateCheck {
    pub installed: Option<String>,
    pub latest: Option<String>,
    pub status: UpdateStatus,
    /// The asset an update would install, when one exists.
    pub asset: Option<Asset>,
    pub error: Option<String>,
}

impl UpdateCheck {
    pub fn failed(installed: Option<&str>, error: String) -> Self {
        Self {
            installed: installed.map(str::to_owned),
            latest: None,
            status: UpdateStatus::Failed,
            asset: None,
            error: Some(error),
        }
    }
}

/// Picks the asset that would update an app.
///
/// Staying on the same format matters: a `.deb` install must not turn into an
/// AppImage. When the previous asset still exists, it wins, so a repository
/// publishing several architecture-compatible builds updates the same one.
pub fn pick_asset(release: &Release, kind: &AssetKind, previous: Option<&str>) -> Option<Asset> {
    let candidates: Vec<&Asset> = release
        .assets
        .iter()
        .filter(|asset| asset.kind == *kind && is_compatible(asset))
        .collect();

    if let Some(previous) = previous
        && let Some(found) = candidates.iter().find(|asset| asset.name == previous)
    {
        return Some((*found).clone());
    }

    candidates.first().map(|asset| (*asset).clone())
}

/// Compares the installed release with the latest one and selects the next
/// asset. `previous` is the asset name recorded at install time, if any.
pub fn evaluate(
    installed: Option<&str>,
    release: &Release,
    kind: &AssetKind,
    previous: Option<&str>,
) -> UpdateCheck {
    let latest = Some(release.tag.clone());
    let asset = pick_asset(release, kind, previous);

    let Some(asset) = asset else {
        return UpdateCheck {
            installed: installed.map(str::to_owned),
            latest,
            status: UpdateStatus::Failed,
            asset: None,
            error: Some("this release has no asset for this machine".to_owned()),
        };
    };

    let Some(installed) = installed else {
        return UpdateCheck {
            installed: None,
            latest,
            status: UpdateStatus::Unknown,
            asset: Some(asset),
            error: None,
        };
    };

    let status = match version::compare(installed, &release.tag) {
        Some(Ordering::Less) => UpdateStatus::Available,
        Some(Ordering::Equal | Ordering::Greater) => UpdateStatus::UpToDate,
        None => UpdateStatus::Unknown,
    };

    UpdateCheck {
        installed: Some(installed.to_owned()),
        latest,
        status,
        asset: Some(asset),
        error: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn release(tag: &str, assets: &str) -> Release {
        Release::parse_json(&format!(
            r#"{{"tag_name":"{tag}","assets":[{assets}]}}"#
        ))
        .unwrap()
    }

    fn appimage(name: &str) -> String {
        format!(r#"{{"name":"{name}","browser_download_url":"u","size":1}}"#)
    }

    #[test]
    fn reports_an_update_when_upstream_is_newer() {
        let release = release("v0.46.0", &appimage("sonora-v0.46.0-x86_64.AppImage"));
        let check = evaluate(
            Some("v0.45.0"),
            &release,
            &AssetKind::AppImage,
            Some("sonora-v0.45.0-x86_64.AppImage"),
        );
        assert_eq!(check.status, UpdateStatus::Available);
        assert_eq!(check.latest.as_deref(), Some("v0.46.0"));
        assert_eq!(
            check.asset.as_ref().map(|asset| asset.name.as_str()),
            Some("sonora-v0.46.0-x86_64.AppImage")
        );
    }

    #[test]
    fn equal_versions_are_up_to_date() {
        let release = release("v0.45.0", &appimage("sonora.AppImage"));
        assert_eq!(
            evaluate(Some("0.45.0"), &release, &AssetKind::AppImage, None).status,
            UpdateStatus::UpToDate
        );
    }

    #[test]
    fn an_unknown_installed_version_is_never_called_current() {
        let release = release("v0.45.0", &appimage("sonora.AppImage"));
        let check = evaluate(None, &release, &AssetKind::AppImage, None);
        assert_eq!(check.status, UpdateStatus::Unknown);
        assert!(check.asset.is_some(), "unknown still allows a manual update");
    }

    #[test]
    fn versions_that_cannot_be_compared_are_unknown() {
        let release = release("nightly", &appimage("sonora.AppImage"));
        let check = evaluate(Some("v0.45.0"), &release, &AssetKind::AppImage, None);
        assert_eq!(check.status, UpdateStatus::Unknown);
    }

    #[test]
    fn a_newer_installed_version_is_up_to_date() {
        let release = release("v0.45.0", &appimage("sonora.AppImage"));
        assert_eq!(
            evaluate(Some("v0.46.0"), &release, &AssetKind::AppImage, None).status,
            UpdateStatus::UpToDate
        );
    }

    #[test]
    fn keeps_the_same_format() {
        let assets = format!(
            "{},{{\"name\":\"app.deb\",\"browser_download_url\":\"u\",\"size\":1}}",
            appimage("app.AppImage")
        );
        let release = release("v2", &assets);
        let check = evaluate(Some("v1"), &release, &AssetKind::Deb, None);
        assert_eq!(check.asset.unwrap().kind, AssetKind::Deb);
    }

    #[test]
    fn prefers_the_asset_already_installed() {
        let assets = format!(
            "{},{}",
            appimage("app-x86_64.AppImage"),
            appimage("app-x86_64-custom.AppImage")
        );
        let release = release("v2", &assets);
        let check = evaluate(
            Some("v1"),
            &release,
            &AssetKind::AppImage,
            Some("app-x86_64-custom.AppImage"),
        );
        assert_eq!(check.asset.unwrap().name, "app-x86_64-custom.AppImage");
    }

    #[test]
    fn a_release_without_a_compatible_asset_fails_explicitly() {
        let release = release("v2", r#"{"name":"README.md","browser_download_url":"u","size":1}"#);
        let check = evaluate(Some("v1"), &release, &AssetKind::AppImage, None);
        assert_eq!(check.status, UpdateStatus::Failed);
        assert!(check.error.is_some());
    }
}
