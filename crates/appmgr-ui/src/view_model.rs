use appmgr_core::inventory::{App, InstallKind, Origin};

/// Where an app came from, as the second tag on a row. GitHub wins over the
/// package format, because that is what makes the app updatable.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SourceTag {
    Deb,
    Flatpak,
    AppImage,
    GitHub,
}

/// Which source tag an app carries. `managed` says whether the app has a
/// GitHub source recorded, which the inventory alone cannot know.
pub fn source_tag(app: &App, managed: bool) -> SourceTag {
    if managed {
        return SourceTag::GitHub;
    }
    match app.origin {
        Origin::Apt => SourceTag::Deb,
        Origin::Flatpak => SourceTag::Flatpak,
        Origin::AppImage => SourceTag::AppImage,
    }
}

/// Whether `app` matches an already-normalised (trimmed, lowercase) query.
pub fn matches(app: &App, query: &str) -> bool {
    if query.is_empty() {
        return true;
    }
    app.name.to_lowercase().contains(query)
        || app
            .package
            .as_deref()
            .is_some_and(|package| package.to_lowercase().contains(query))
}

/// The rows the list shows: search plus the two tag filters, sorted by name.
/// `managed` reports whether an app has a GitHub source recorded; it is only
/// called when a source filter is active.
pub fn visible_rows(
    apps: &[App],
    query: &str,
    kind: Option<InstallKind>,
    source: Option<SourceTag>,
    managed: impl Fn(&App) -> bool,
) -> Vec<usize> {
    let query = query.trim().to_lowercase();
    let mut rows: Vec<usize> = apps
        .iter()
        .enumerate()
        .filter(|(_, app)| {
            if kind.is_some_and(|kind| app.kind != kind) {
                return false;
            }
            if let Some(wanted) = source
                && source_tag(app, managed(app)) != wanted
            {
                return false;
            }
            matches(app, &query)
        })
        .map(|(index, _)| index)
        .collect();
    rows.sort_by_cached_key(|&index| apps[index].name.to_lowercase());
    rows
}

/// Human-readable size: "512 B", "12 kB", "58.2 MB", "1.4 GB".
pub fn format_size(bytes: u64) -> String {
    match bytes {
        0..1_000 => format!("{bytes} B"),
        1_000..1_000_000 => format!("{:.0} kB", bytes as f64 / 1_000.0),
        1_000_000..1_000_000_000 => format!("{:.1} MB", bytes as f64 / 1_000_000.0),
        _ => format!("{:.1} GB", bytes as f64 / 1_000_000_000.0),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn app(name: &str, kind: InstallKind, origin: Origin) -> App {
        App {
            id: format!("{}:{name}", match origin {
                Origin::Apt => "apt",
                Origin::Flatpak => "flatpak",
                Origin::AppImage => "appimage",
            }),
            name: name.to_owned(),
            version: Some("1.0".to_owned()),
            icon: None,
            origin,
            kind,
            protected: false,
            size_bytes: 0,
            package: Some(name.to_owned()),
        }
    }

    fn sample() -> Vec<App> {
        vec![
            app("zsh", InstallKind::User, Origin::Apt),
            app("Firefox", InstallKind::PreInstalled, Origin::Apt),
            app("brave", InstallKind::User, Origin::Flatpak),
            app("Tool", InstallKind::User, Origin::AppImage),
            app("Driver", InstallKind::System, Origin::Apt),
        ]
    }

    fn names<'a>(apps: &'a [App], rows: &[usize]) -> Vec<&'a str> {
        rows.iter().map(|&index| apps[index].name.as_str()).collect()
    }

    fn none_managed(_: &App) -> bool {
        false
    }

    #[test]
    fn filters_by_name_ignoring_case() {
        let apps = sample();
        let found = visible_rows(&apps, "FIREF", None, None, none_managed);
        assert_eq!(names(&apps, &found), vec!["Firefox"]);
    }

    #[test]
    fn filters_by_package_name() {
        let apps = sample();
        let found = visible_rows(&apps, "driver", None, None, none_managed);
        assert_eq!(names(&apps, &found), vec!["Driver"]);
    }

    #[test]
    fn sorts_alphabetically_ignoring_case() {
        let apps = sample();
        let rows = visible_rows(&apps, "", None, None, none_managed);
        assert_eq!(
            names(&apps, &rows),
            vec!["brave", "Driver", "Firefox", "Tool", "zsh"]
        );
    }

    #[test]
    fn filters_by_classification() {
        let apps = sample();
        let rows = visible_rows(&apps, "", Some(InstallKind::User), None, none_managed);
        assert_eq!(names(&apps, &rows), vec!["brave", "Tool", "zsh"]);
    }

    #[test]
    fn filters_by_source() {
        let apps = sample();
        let rows = visible_rows(&apps, "", None, Some(SourceTag::Flatpak), none_managed);
        assert_eq!(names(&apps, &rows), vec!["brave"]);
    }

    #[test]
    fn github_wins_over_the_package_format() {
        let apps = sample();
        let managed = |app: &App| app.name == "zsh";
        let rows = visible_rows(&apps, "", None, Some(SourceTag::GitHub), managed);
        assert_eq!(names(&apps, &rows), vec!["zsh"]);
        let rows = visible_rows(&apps, "", None, Some(SourceTag::Deb), managed);
        assert_eq!(names(&apps, &rows), vec!["Driver", "Firefox"]);
    }

    #[test]
    fn combines_search_and_both_filters() {
        let apps = sample();
        let rows = visible_rows(
            &apps,
            "o",
            Some(InstallKind::User),
            Some(SourceTag::AppImage),
            none_managed,
        );
        assert_eq!(names(&apps, &rows), vec!["Tool"]);
    }

    #[test]
    fn no_match_means_empty() {
        let apps = sample();
        assert!(visible_rows(&apps, "firefox", Some(InstallKind::User), None, none_managed).is_empty());
    }

    #[test]
    fn formats_sizes() {
        assert_eq!(format_size(0), "0 B");
        assert_eq!(format_size(512), "512 B");
        assert_eq!(format_size(1_000), "1 kB");
        assert_eq!(format_size(58_200_000), "58.2 MB");
        assert_eq!(format_size(1_400_000_000), "1.4 GB");
    }
}
