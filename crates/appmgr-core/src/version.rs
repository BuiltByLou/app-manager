//! Comparison of version strings, tolerant of the schemes found in GitHub
//! releases: `v1.2.3`, `1.2.3-beta.2`, `2026-10-27`.
//!
//! The rule that matters is never to claim "up to date" when the two versions
//! cannot be compared. Those cases return `None`, and the UI shows `unknown`.

use std::cmp::Ordering;
use std::path::Path;

/// Compares two versions. `None` means at least one of them is not a version
/// this module understands.
pub fn compare(left: &str, right: &str) -> Option<Ordering> {
    Some(Version::parse(left)?.cmp(&Version::parse(right)?))
}

/// Reads a version from an AppImage filename, as in
/// `sonora-v0.45.0-x86_64.AppImage` → `v0.45.0`.
///
/// Tokens that look like an architecture (`x86_64`) do not parse as versions,
/// and a single number is not enough: it would match too much.
pub fn version_from_filename(path: &Path) -> Option<String> {
    let stem = path.file_stem()?.to_str()?;
    stem.split(['-', '_'])
        .filter(|token| !token.is_empty())
        .find(|token| {
            token.chars().any(|c| c.is_ascii_digit())
                && Version::parse(token).is_some_and(|version| version.numbers.len() >= 2)
        })
        .map(str::to_owned)
}

/// Removes the leading `v` for display, keeping everything else.
pub fn display(version: &str) -> &str {
    version
        .strip_prefix('v')
        .or_else(|| version.strip_prefix('V'))
        .unwrap_or(version)
}

#[derive(Debug, PartialEq, Eq)]
struct Version {
    numbers: Vec<u64>,
    /// Empty means a final release, which sorts after any pre-release.
    prerelease: Vec<Identifier>,
}

#[derive(Debug, PartialEq, Eq)]
enum Identifier {
    Number(u64),
    Text(String),
}

impl Version {
    fn parse(input: &str) -> Option<Self> {
        let text = input.trim();
        if text.is_empty() {
            return None;
        }
        let text = display(text);
        let text = text.split('+').next()?;

        if let Some(version) = parse_date(text) {
            return Some(version);
        }

        let (core, prerelease) = match text.split_once('-') {
            Some((core, rest)) => (core, Some(rest)),
            None => (text, None),
        };

        let numbers = core
            .split('.')
            .map(|part| part.parse::<u64>().ok())
            .collect::<Option<Vec<u64>>>()?;
        if numbers.is_empty() {
            return None;
        }

        let prerelease = prerelease
            .map(|rest| {
                rest.split('.')
                    .filter(|part| !part.is_empty())
                    .map(Identifier::parse)
                    .collect()
            })
            .unwrap_or_default();

        Some(Version {
            numbers,
            prerelease,
        })
    }
}

fn parse_date(text: &str) -> Option<Version> {
    let bytes = text.as_bytes();
    if bytes.len() != 10 || bytes[4] != b'-' || bytes[7] != b'-' {
        return None;
    }
    if !bytes
        .iter()
        .enumerate()
        .all(|(index, byte)| index == 4 || index == 7 || byte.is_ascii_digit())
    {
        return None;
    }
    let year = text[0..4].parse().ok()?;
    let month = text[5..7].parse().ok()?;
    let day = text[8..10].parse().ok()?;
    Some(Version {
        numbers: vec![year, month, day],
        prerelease: Vec::new(),
    })
}

impl Identifier {
    fn parse(text: &str) -> Self {
        match text.parse::<u64>() {
            Ok(number) => Identifier::Number(number),
            Err(_) => Identifier::Text(text.to_ascii_lowercase()),
        }
    }
}

impl Ord for Identifier {
    fn cmp(&self, other: &Self) -> Ordering {
        match (self, other) {
            (Identifier::Number(left), Identifier::Number(right)) => left.cmp(right),
            // Semver: numeric identifiers sort before alphanumeric ones.
            (Identifier::Number(_), Identifier::Text(_)) => Ordering::Less,
            (Identifier::Text(_), Identifier::Number(_)) => Ordering::Greater,
            (Identifier::Text(left), Identifier::Text(right)) => left.cmp(right),
        }
    }
}

impl PartialOrd for Identifier {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Version {
    fn cmp(&self, other: &Self) -> Ordering {
        let length = self.numbers.len().max(other.numbers.len());
        for index in 0..length {
            let left = self.numbers.get(index).copied().unwrap_or(0);
            let right = other.numbers.get(index).copied().unwrap_or(0);
            match left.cmp(&right) {
                Ordering::Equal => {}
                other => return other,
            }
        }

        match (self.prerelease.is_empty(), other.prerelease.is_empty()) {
            (true, true) => Ordering::Equal,
            (true, false) => Ordering::Greater,
            (false, true) => Ordering::Less,
            (false, false) => compare_prerelease(&self.prerelease, &other.prerelease),
        }
    }
}

fn compare_prerelease(left: &[Identifier], right: &[Identifier]) -> Ordering {
    for (left, right) in left.iter().zip(right) {
        match left.cmp(right) {
            Ordering::Equal => {}
            other => return other,
        }
    }
    left.len().cmp(&right.len())
}

impl PartialOrd for Version {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ignores_a_leading_v() {
        assert_eq!(compare("v1.2.3", "1.2.3"), Some(Ordering::Equal));
        assert_eq!(compare("V2.0.0", "v1.9.9"), Some(Ordering::Greater));
    }

    #[test]
    fn compares_numeric_parts_in_order() {
        assert_eq!(compare("0.45.0", "v0.46.0"), Some(Ordering::Less));
        assert_eq!(compare("1.10.0", "1.9.0"), Some(Ordering::Greater));
        assert_eq!(compare("2", "2.0.0"), Some(Ordering::Equal));
    }

    #[test]
    fn a_prerelease_is_older_than_the_release() {
        assert_eq!(compare("1.2.3-beta", "1.2.3"), Some(Ordering::Less));
        assert_eq!(compare("1.2.3", "1.2.3-rc.1"), Some(Ordering::Greater));
    }

    #[test]
    fn prerelease_numbers_compare_as_numbers() {
        assert_eq!(
            compare("1.2.3-beta.2", "1.2.3-beta.10"),
            Some(Ordering::Less)
        );
    }

    #[test]
    fn compares_dates() {
        assert_eq!(compare("2026-10-27", "2026-11-02"), Some(Ordering::Less));
        assert_eq!(compare("2026-11-02", "2026-11-02"), Some(Ordering::Equal));
    }

    #[test]
    fn non_versions_are_unknown() {
        assert_eq!(compare("nightly", "2026-10-27"), None);
        assert_eq!(compare("", "1.0.0"), None);
        assert_eq!(compare("release", "release"), None);
    }

    #[test]
    fn reads_the_version_from_an_appimage_name() {
        let path = Path::new("/home/ed/Applications/sonora-v0.45.0-x86_64.AppImage");
        assert_eq!(version_from_filename(path).as_deref(), Some("v0.45.0"));
        assert_eq!(
            version_from_filename(Path::new("app_1.2.3_x86_64.AppImage")).as_deref(),
            Some("1.2.3")
        );
    }

    #[test]
    fn does_not_mistake_an_architecture_for_a_version() {
        assert_eq!(version_from_filename(Path::new("app-x86_64.AppImage")), None);
        assert_eq!(version_from_filename(Path::new("app-64.AppImage")), None);
    }

    #[test]
    fn display_drops_only_the_leading_v() {
        assert_eq!(display("v1.2.3"), "1.2.3");
        assert_eq!(display("1.2.3-beta"), "1.2.3-beta");
    }
}
