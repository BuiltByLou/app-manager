use appmgr_core::inventory::{App, InstallKind};

/// A set of apps with its own heading inside a page.
pub struct Group {
    pub kind: GroupKind,
    pub apps: Vec<App>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GroupKind {
    Protected,
    Removable,
}

/// Filters by name or package, ignoring case.
pub fn filter<'a>(apps: &'a [App], query: &str) -> Vec<&'a App> {
    let query = query.trim().to_lowercase();
    if query.is_empty() {
        return apps.iter().collect();
    }
    apps.iter()
        .filter(|app| {
            app.name.to_lowercase().contains(&query)
                || app
                    .package
                    .as_deref()
                    .is_some_and(|package| package.to_lowercase().contains(&query))
        })
        .collect()
}

/// Apps of a single kind, filtered and sorted alphabetically.
pub fn apps_for(apps: &[App], kind: InstallKind, query: &str) -> Vec<App> {
    let mut matching: Vec<App> = filter(apps, query)
        .into_iter()
        .filter(|app| app.kind == kind)
        .cloned()
        .collect();
    matching.sort_by_key(|app| app.name.to_lowercase());
    matching
}

/// The system apps, split between removable and protected. Removable ones come
/// first; protected ones are locked, so they sit at the bottom and can be
/// hidden. `show_protected` keeps them out of the list entirely when false.
pub fn system_groups(apps: &[App], query: &str, show_protected: bool) -> Vec<Group> {
    let (protected, removable): (Vec<App>, Vec<App>) = apps_for(apps, InstallKind::System, query)
        .into_iter()
        .partition(|app| app.protected);

    let mut groups = Vec::new();
    if !removable.is_empty() {
        groups.push(Group {
            kind: GroupKind::Removable,
            apps: removable,
        });
    }
    if show_protected && !protected.is_empty() {
        groups.push(Group {
            kind: GroupKind::Protected,
            apps: protected,
        });
    }
    groups
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

    #[test]
    fn filters_by_name_ignoring_case() {
        let apps = sample();
        let found = filter(&apps, "FIREF");
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].name, "Firefox");
    }

    #[test]
    fn filters_by_package_name() {
        let apps = sample();
        assert_eq!(filter(&apps, "cosmic").len(), 1);
    }

    #[test]
    fn empty_query_keeps_everything() {
        let apps = sample();
        assert_eq!(filter(&apps, "  ").len(), 4);
    }

    #[test]
    fn shows_only_the_requested_group() {
        let apps = sample();
        let user = apps_for(&apps, InstallKind::User, "");
        assert_eq!(user.len(), 2);
        assert!(user.iter().all(|a| a.kind == InstallKind::User));
    }

    #[test]
    fn sorts_alphabetically_ignoring_case() {
        let apps = sample();
        let ordered = apps_for(&apps, InstallKind::System, "");
        let system: Vec<&str> = ordered.iter().map(|a| a.name.as_str()).collect();
        assert_eq!(system, vec!["COSMIC Store", "Firefox"]);
    }

    #[test]
    fn combines_search_and_group() {
        let apps = sample();
        let found = apps_for(&apps, InstallKind::User, "bra");
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].name, "brave");
    }

    #[test]
    fn no_match_means_empty() {
        assert!(apps_for(&sample(), InstallKind::User, "firefox").is_empty());
    }

    #[test]
    fn separates_protected_system_apps_from_the_rest() {
        let groups = system_groups(&sample(), "", true);
        assert_eq!(groups.len(), 2);
        assert_eq!(groups[0].kind, GroupKind::Removable);
        assert_eq!(groups[0].apps[0].name, "Firefox");
        assert_eq!(groups[1].kind, GroupKind::Protected);
        assert_eq!(groups[1].apps[0].name, "COSMIC Store");
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
        let groups = system_groups(&sample(), "", false);
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].kind, GroupKind::Removable);
        assert_eq!(groups[0].apps[0].name, "Firefox");
    }

    #[test]
    fn search_applies_to_both_system_groups() {
        let apps = sample();
        let groups = system_groups(&apps, "firefox", true);
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].kind, GroupKind::Removable);
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
