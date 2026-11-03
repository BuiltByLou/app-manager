use appmgr_core::inventory::{App, InstallKind};

/// A set of apps with its own heading inside a page. Rows are indices into the
/// inventory, so the view borrows instead of cloning the apps on every redraw.
pub struct Group {
    pub kind: GroupKind,
    pub rows: Vec<usize>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GroupKind {
    Protected,
    Removable,
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

fn normalise(query: &str) -> String {
    query.trim().to_lowercase()
}

/// Indices of the apps of `kind` that match `query`, sorted by name.
pub fn matching(apps: &[App], kind: InstallKind, query: &str) -> Vec<usize> {
    let query = normalise(query);
    let mut rows: Vec<usize> = apps
        .iter()
        .enumerate()
        .filter(|(_, app)| app.kind == kind && matches(app, &query))
        .map(|(index, _)| index)
        .collect();
    rows.sort_by_cached_key(|&index| apps[index].name.to_lowercase());
    rows
}

/// The system apps, split between removable and protected. Removable ones come
/// first; protected ones are locked, so they sit at the bottom and can be
/// hidden. `show_protected` keeps them out of the list entirely when false.
pub fn system_groups(apps: &[App], query: &str, show_protected: bool) -> Vec<Group> {
    let (protected, removable): (Vec<usize>, Vec<usize>) = apps
        .iter()
        .enumerate()
        .filter(|(_, app)| app.kind != InstallKind::User && matches(app, query))
        .map(|(index, _)| index)
        .partition(|&index| apps[index].protected);

    let mut groups = Vec::new();
    if !removable.is_empty() {
        groups.push(Group {
            kind: GroupKind::Removable,
            rows: removable,
        });
    }
    if show_protected && !protected.is_empty() {
        groups.push(Group {
            kind: GroupKind::Protected,
            rows: protected,
        });
    }
    groups
}

/// How many locked system apps match the current search, shown or not.
pub fn protected_count(apps: &[App], query: &str) -> usize {
    apps
        .iter()
        .enumerate()
        .filter(|(_, app)| app.kind != InstallKind::User && app.protected && matches(app, query))
        .count()
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
    use appmgr_core::inventory::Origin;

    fn app(name: &str, kind: InstallKind) -> App {
        app_with(name, kind, false)
    }

    fn app_with(name: &str, kind: InstallKind, protected: bool) -> App {
        App {
            id: format!("apt:{name}"),
            name: name.to_owned(),
            version: Some("1.0".to_owned()),
            icon: None,
            origin: Origin::Apt,
            kind,
            protected,
            size_bytes: 0,
            package: Some(name.to_owned()),
        }
    }

    fn sample() -> Vec<App> {
        vec![
            app("zsh", InstallKind::User),
            app_with("Firefox", InstallKind::System, false),
            app("brave", InstallKind::User),
            app_with("COSMIC Store", InstallKind::System, true),
        ]
    }

    fn names<'a>(apps: &'a [App], rows: &[usize]) -> Vec<&'a str> {
        rows.iter().map(|&index| apps[index].name.as_str()).collect()
    }

    #[test]
    fn filters_by_name_ignoring_case() {
        let apps = sample();
        let found = matching(&apps, InstallKind::System, "FIREF");
        assert_eq!(names(&apps, &found), vec!["Firefox"]);
    }

    #[test]
    fn filters_by_package_name() {
        let apps = sample();
        assert_eq!(names(&apps, &matching(&apps, InstallKind::System, "cosmic")), vec!["COSMIC Store"]);
    }

    #[test]
    fn empty_query_keeps_everything_of_that_kind() {
        let apps = sample();
        assert_eq!(matching(&apps, InstallKind::User, "  ").len(), 2);
    }

    #[test]
    fn shows_only_the_requested_group() {
        let apps = sample();
        let user = matching(&apps, InstallKind::User, "");
        assert_eq!(user.len(), 2);
        assert!(user.iter().all(|&index| apps[index].kind == InstallKind::User));
    }

    #[test]
    fn sorts_alphabetically_ignoring_case() {
        let apps = sample();
        let ordered = matching(&apps, InstallKind::System, "");
        assert_eq!(names(&apps, &ordered), vec!["COSMIC Store", "Firefox"]);
    }

    #[test]
    fn combines_search_and_group() {
        let apps = sample();
        assert_eq!(names(&apps, &matching(&apps, InstallKind::User, "bra")), vec!["brave"]);
    }

    #[test]
    fn no_match_means_empty() {
        assert!(matching(&sample(), InstallKind::User, "firefox").is_empty());
    }

    #[test]
    fn separates_protected_system_apps_from_the_rest() {
        let apps = sample();
        let groups = system_groups(&apps, "", true);
        assert_eq!(groups.len(), 2);
        assert_eq!(groups[0].kind, GroupKind::Removable);
        assert_eq!(names(&apps, &groups[0].rows), vec!["Firefox"]);
        assert_eq!(groups[1].kind, GroupKind::Protected);
        assert_eq!(names(&apps, &groups[1].rows), vec!["COSMIC Store"]);
    }

    #[test]
    fn hides_a_system_group_when_it_is_empty() {
        let apps = vec![app_with("COSMIC Store", InstallKind::System, true)];
        let groups = system_groups(&apps, "", true);
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].kind, GroupKind::Protected);
    }

    #[test]
    fn hides_protected_apps_when_asked_to() {
        let apps = sample();
        let groups = system_groups(&apps, "", false);
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].kind, GroupKind::Removable);
        assert_eq!(names(&apps, &groups[0].rows), vec!["Firefox"]);
    }

    #[test]
    fn search_applies_to_both_system_groups() {
        let apps = sample();
        let groups = system_groups(&apps, "firefox", true);
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].kind, GroupKind::Removable);
    }

    #[test]
    fn counts_protected_matches_even_when_hidden() {
        let apps = sample();
        assert_eq!(protected_count(&apps, ""), 1);
        assert_eq!(protected_count(&apps, "cosmic"), 1);
        assert_eq!(protected_count(&apps, "firefox"), 0);
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
