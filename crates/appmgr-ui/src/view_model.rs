use appmgr_core::inventory::{App, InstallKind};

pub struct Section {
    pub title: &'static str,
    pub apps: Vec<App>,
}

pub const USER_TITLE: &str = "Instaladas por mim";
pub const SYSTEM_TITLE: &str = "Sistema";

/// Filtra por nome ou pacote, sem distinguir maiúsculas de minúsculas.
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

/// Agrupa em "instaladas por mim" e "sistema", cada uma por ordem alfabética.
/// Secções vazias não aparecem.
pub fn sections(apps: &[App], query: &str) -> Vec<Section> {
    let mut user: Vec<App> = Vec::new();
    let mut system: Vec<App> = Vec::new();

    for app in filter(apps, query) {
        match app.kind {
            InstallKind::User => user.push(app.clone()),
            InstallKind::System => system.push(app.clone()),
        }
    }

    for group in [&mut user, &mut system] {
        group.sort_by_key(|app| app.name.to_lowercase());
    }

    let mut result = Vec::new();
    if !user.is_empty() {
        result.push(Section {
            title: USER_TITLE,
            apps: user,
        });
    }
    if !system.is_empty() {
        result.push(Section {
            title: SYSTEM_TITLE,
            apps: system,
        });
    }
    result
}

/// Tamanho legível: "512 B", "12 kB", "58.2 MB", "1.4 GB".
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
        App {
            id: format!("apt:{name}"),
            name: name.to_owned(),
            version: Some("1.0".to_owned()),
            icon: None,
            origin: Origin::Apt,
            kind,
            size_bytes: 0,
            package: Some(name.to_owned()),
        }
    }

    fn sample() -> Vec<App> {
        vec![
            app("zsh", InstallKind::User),
            app("Firefox", InstallKind::System),
            app("brave", InstallKind::User),
            app("COSMIC Store", InstallKind::System),
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
    fn puts_user_section_before_system() {
        let sections = sections(&sample(), "");
        assert_eq!(sections.len(), 2);
        assert_eq!(sections[0].title, USER_TITLE);
        assert_eq!(sections[1].title, SYSTEM_TITLE);
    }

    #[test]
    fn sorts_sections_alphabetically_ignoring_case() {
        let sections = sections(&sample(), "");
        let user: Vec<&str> = sections[0].apps.iter().map(|a| a.name.as_str()).collect();
        assert_eq!(user, vec!["brave", "zsh"]);
        let system: Vec<&str> = sections[1].apps.iter().map(|a| a.name.as_str()).collect();
        assert_eq!(system, vec!["COSMIC Store", "Firefox"]);
    }

    #[test]
    fn hides_empty_sections() {
        let apps = vec![app("brave", InstallKind::User)];
        let sections = sections(&apps, "");
        assert_eq!(sections.len(), 1);
        assert_eq!(sections[0].title, USER_TITLE);
    }

    #[test]
    fn no_match_means_no_sections() {
        let sections = sections(&sample(), "nao-existe");
        assert!(sections.is_empty());
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
