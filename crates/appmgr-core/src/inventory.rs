use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

use crate::desktop::DesktopEntry;
use crate::dpkg::InstalledPackage;
use crate::flatpak::FlatpakApp;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Origin {
    Apt,
    Flatpak,
    AppImage,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InstallKind {
    /// Instalada pelo utilizador; remover é seguro por princípio.
    User,
    /// Parte do sistema, mas removível sem partir a máquina.
    PreInstalled,
    /// Essencial ou parte do desktop: remover pode partir o sistema.
    System,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct App {
    pub id: String,
    pub name: String,
    pub version: Option<String>,
    pub icon: Option<String>,
    pub origin: Origin,
    pub kind: InstallKind,
    /// A public repository updates this app (apt with a remote origin, or a
    /// Flatpak from a network remote). False means the app came from a local
    /// file and nothing would update it unless a source is linked.
    pub hub: bool,
    /// Não deve ser removida: é essencial ou faz parte do desktop.
    pub protected: bool,
    pub size_bytes: u64,
    pub package: Option<String>,
}

pub struct DesktopFile {
    pub path: PathBuf,
    pub contents: String,
}

/// Tudo o que o inventário precisa de saber sobre a máquina. A implementação
/// real chama `dpkg`/`flatpak`; os testes fornecem um sistema falso.
pub trait SystemProbe {
    fn desktop_files(&self) -> Vec<DesktopFile>;
    fn packages(&self) -> Vec<InstalledPackage>;
    /// Packages the user installed on purpose. Receives the package list so a
    /// probe never has to ask `dpkg` twice.
    fn manual_packages(&self, packages: &[InstalledPackage]) -> HashSet<String>;
    /// Pacotes que compõem o desktop (dependências diretas dos metapacotes).
    fn desktop_packages(&self) -> HashSet<String>;
    /// Pacotes donos de caminhos (`dpkg -S`), resolvidos numa só chamada.
    fn package_owners(&self, paths: &[PathBuf]) -> HashMap<PathBuf, String>;
    /// Which of these packages were installed from a remote repository
    /// (`apt-cache policy`), as opposed to a local `.deb` file.
    fn packages_with_remote_origin(&self, packages: &[String]) -> HashSet<String>;
    /// Caminho absoluto de um programa, resolvendo nomes simples pelo `PATH`.
    fn which(&self, program: &str) -> Option<PathBuf>;
    fn flatpak_apps(&self) -> Vec<FlatpakApp>;
    fn appimages(&self) -> Vec<PathBuf>;
}

pub fn build_inventory(probe: &impl SystemProbe) -> Vec<App> {
    let packages = probe.packages();
    let by_name: HashMap<&str, &InstalledPackage> =
        packages.iter().map(|p| (p.name.as_str(), p)).collect();
    let manual = probe.manual_packages(&packages);
    let desktop = probe.desktop_packages();

    let mut apps = Vec::new();
    let mut seen_ids = HashSet::new();
    let mut seen_packages = HashSet::new();

    for flatpak in probe.flatpak_apps() {
        let id = format!("flatpak:{}", flatpak.id);
        if !seen_ids.insert(id.clone()) {
            continue;
        }
        apps.push(App {
            id,
            name: flatpak.name,
            version: Some(flatpak.version),
            icon: flatpak.icon.clone(),
            origin: Origin::Flatpak,
            kind: if flatpak.user_installation {
                InstallKind::User
            } else {
                InstallKind::PreInstalled
            },
            hub: flatpak.from_remote,
            protected: false,
            size_bytes: flatpak.size_bytes,
            package: Some(flatpak.id),
        });
    }

    for path in probe.appimages() {
        let id = format!("appimage:{}", path.display());
        if !seen_ids.insert(id.clone()) {
            continue;
        }
        let name = path
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| id.clone());
        let icon = crate::appimage_icon::find_icon(&path);
        apps.push(App {
            id,
            name,
            version: crate::version::version_from_filename(&path),
            icon,
            origin: Origin::AppImage,
            kind: InstallKind::User,
            hub: false,
            protected: false,
            size_bytes: 0,
            package: None,
        });
    }

    let mut candidates = Vec::new();
    for file in probe.desktop_files() {
        let Some(entry) = crate::desktop::parse_desktop_entry(&file.contents) else {
            continue;
        };
        if is_flatpak_entry(&entry) || is_appimage_entry(&entry) {
            continue;
        }
        let program = exec_program(&entry.exec);
        let Some(resolved) = resolve_program(probe, program) else {
            continue;
        };
        candidates.push((entry, resolved));
    }

    let paths: Vec<PathBuf> = candidates.iter().map(|(_, path)| path.clone()).collect();
    let owners = probe.package_owners(&paths);

    // Only the packages behind a desktop entry matter here, and one call
    // answers for all of them.
    let names: Vec<String> = owners
        .values()
        .map(|owner| crate::dpkg::base_name(owner).to_owned())
        .collect::<HashSet<_>>()
        .into_iter()
        .collect();
    let remote = probe.packages_with_remote_origin(&names);

    for (entry, path) in candidates {
        let Some(owner) = owners.get(&path) else {
            continue;
        };
        let Some(package) = by_name.get(owner.as_str()) else {
            continue;
        };
        if !seen_packages.insert(package.name.clone()) {
            continue;
        }

        let id = format!("apt:{}", package.name);
        if !seen_ids.insert(id.clone()) {
            continue;
        }

        apps.push(App {
            id,
            name: entry.name,
            version: Some(package.version.clone()),
            icon: entry.icon,
            origin: Origin::Apt,
            kind: if is_protected(package, &desktop) {
                InstallKind::System
            } else if is_system_package(package, &manual, &desktop) {
                InstallKind::PreInstalled
            } else {
                InstallKind::User
            },
            hub: remote.contains(crate::dpkg::base_name(&package.name)),
            protected: is_protected(package, &desktop),
            size_bytes: package.installed_size_kib * 1024,
            package: Some(package.name.clone()),
        });
    }

    apps
}

fn resolve_program(probe: &impl SystemProbe, program: &str) -> Option<PathBuf> {
    if program.contains('/') {
        Some(PathBuf::from(program))
    } else {
        probe.which(program)
    }
}

fn exec_program(exec: &str) -> &str {
    exec.split_whitespace()
        .next()
        .unwrap_or(exec)
        .trim_matches('"')
}

fn is_system_package(
    package: &InstalledPackage,
    manual: &HashSet<String>,
    desktop: &HashSet<String>,
) -> bool {
    is_protected(package, desktop) || !manual.contains(crate::dpkg::base_name(&package.name))
}

/// Essencial, de prioridade alta ou parte do desktop: nunca remover.
fn is_protected(package: &InstalledPackage, desktop: &HashSet<String>) -> bool {
    let protected_priority = matches!(package.priority.as_str(), "required" | "important");
    let name = crate::dpkg::base_name(&package.name);
    package.essential || protected_priority || desktop.contains(name)
}

fn is_flatpak_entry(entry: &DesktopEntry) -> bool {
    entry.exec.starts_with("/usr/bin/flatpak")
        || entry.exec.starts_with("flatpak ")
        || entry.exec.contains("/var/lib/flatpak/")
}

fn is_appimage_entry(entry: &DesktopEntry) -> bool {
    entry.exec.to_ascii_lowercase().ends_with(".appimage")
}

#[cfg(test)]
mod tests {
    use super::*;

    struct FakeSystem {
        desktops: Vec<DesktopFile>,
        packages: Vec<InstalledPackage>,
        manual: HashSet<String>,
        desktop: HashSet<String>,
        owners: HashMap<String, String>,
        remote: HashSet<String>,
        programs: HashMap<String, PathBuf>,
        flatpaks: Vec<FlatpakApp>,
        appimages: Vec<PathBuf>,
    }

    impl SystemProbe for FakeSystem {
        fn desktop_files(&self) -> Vec<DesktopFile> {
            self.desktops
                .iter()
                .map(|d| DesktopFile {
                    path: d.path.clone(),
                    contents: d.contents.clone(),
                })
                .collect()
        }

        fn packages(&self) -> Vec<InstalledPackage> {
            self.packages.clone()
        }

        fn manual_packages(&self, _packages: &[InstalledPackage]) -> HashSet<String> {
            self.manual.clone()
        }

        fn desktop_packages(&self) -> HashSet<String> {
            self.desktop.clone()
        }

        fn package_owners(&self, paths: &[PathBuf]) -> HashMap<PathBuf, String> {
            paths
                .iter()
                .filter_map(|path| {
                    let owner = self.owners.get(path.to_str()?)?;
                    Some((path.clone(), owner.clone()))
                })
                .collect()
        }

        fn packages_with_remote_origin(&self, packages: &[String]) -> HashSet<String> {
            packages
                .iter()
                .filter(|name| self.remote.contains(name.as_str()))
                .cloned()
                .collect()
        }

        fn which(&self, program: &str) -> Option<PathBuf> {
            self.programs.get(program).cloned()
        }

        fn flatpak_apps(&self) -> Vec<FlatpakApp> {
            self.flatpaks.clone()
        }

        fn appimages(&self) -> Vec<PathBuf> {
            self.appimages.clone()
        }
    }

    fn desktop(name: &str, exec: &str) -> DesktopFile {
        DesktopFile {
            path: PathBuf::from(format!("/usr/share/applications/{name}.desktop")),
            contents: format!(
                "[Desktop Entry]\nType=Application\nName={name}\nExec={exec}\nIcon={name}\n"
            ),
        }
    }

    fn package(name: &str, version: &str, priority: &str, essential: bool) -> InstalledPackage {
        InstalledPackage {
            name: name.to_owned(),
            version: version.to_owned(),
            installed_size_kib: 1000,
            essential,
            priority: priority.to_owned(),
        }
    }

    fn fake() -> FakeSystem {
        FakeSystem {
            desktops: vec![
                desktop("Firefox", "/usr/lib/firefox/firefox %u"),
                desktop("GIMP", "/usr/bin/gimp %U"),
                desktop("Terminal", "/usr/bin/gnome-terminal"),
                desktop("Software", "/usr/bin/gnome-software"),
                desktop("LocalSend", "/usr/bin/flatpak run org.localsend.localsend_app"),
                desktop("Tool", "/home/ed/Applications/tool.AppImage"),
            ],
            packages: vec![
                package("firefox", "1:158.0.1", "optional", false),
                package("gimp", "2.10.36", "optional", false),
                package("gnome-terminal", "3.52.0", "important", false),
                package("gnome-software", "46.0", "optional", false),
                package("libgtk-4-1", "4.14.2", "optional", false),
            ],
            manual: HashSet::from([
                "firefox".to_owned(),
                "gimp".to_owned(),
                "gnome-terminal".to_owned(),
            ]),
            desktop: HashSet::new(),
            owners: HashMap::from([
                ("/usr/lib/firefox/firefox".to_owned(), "firefox".to_owned()),
                ("/usr/bin/gimp".to_owned(), "gimp".to_owned()),
                ("/usr/bin/gnome-terminal".to_owned(), "gnome-terminal".to_owned()),
                ("/usr/bin/gnome-software".to_owned(), "gnome-software".to_owned()),
            ]),
            remote: HashSet::from([
                "firefox".to_owned(),
                "gimp".to_owned(),
                "gnome-terminal".to_owned(),
                "gnome-software".to_owned(),
            ]),
            flatpaks: vec![FlatpakApp {
                id: "org.localsend.localsend_app".to_owned(),
                name: "LocalSend".to_owned(),
                version: "1.18.2".to_owned(),
                origin: "flathub".to_owned(),
                user_installation: true,
                size_bytes: 58_200_000,
                icon: None,
                from_remote: true,
            }],
            appimages: vec![PathBuf::from("/home/ed/Applications/tool.AppImage")],
            programs: HashMap::new(),
        }
    }

    #[test]
    fn maps_desktop_entries_to_their_packages() {
        let apps = build_inventory(&fake());
        let firefox = apps.iter().find(|a| a.name == "Firefox").expect("firefox");
        assert_eq!(firefox.origin, Origin::Apt);
        assert_eq!(firefox.package.as_deref(), Some("firefox"));
        assert_eq!(firefox.version.as_deref(), Some("1:158.0.1"));
        assert_eq!(firefox.icon.as_deref(), Some("Firefox"));
        assert_eq!(firefox.size_bytes, 1000 * 1024);
    }

    #[test]
    fn marks_a_package_with_a_remote_origin_as_a_hub_app() {
        let apps = build_inventory(&fake());
        let firefox = apps.iter().find(|a| a.name == "Firefox").unwrap();
        assert!(firefox.hub);
    }

    #[test]
    fn a_deb_installed_from_a_file_is_not_a_hub_app() {
        let mut system = fake();
        system.remote.remove("gimp");
        let apps = build_inventory(&system);
        let gimp = apps.iter().find(|a| a.name == "GIMP").unwrap();
        assert!(!gimp.hub, "apt installed it, but no repository provides it");
        let firefox = apps.iter().find(|a| a.name == "Firefox").unwrap();
        assert!(firefox.hub);
    }

    #[test]
    fn a_sideloaded_flatpak_is_not_a_hub_app() {
        let mut system = fake();
        system.flatpaks[0].from_remote = false;
        let apps = build_inventory(&system);
        let localsend = apps.iter().find(|a| a.name == "LocalSend").unwrap();
        assert!(!localsend.hub);
    }

    #[test]
    fn marks_manual_packages_as_user_installed() {
        let apps = build_inventory(&fake());
        let firefox = apps.iter().find(|a| a.name == "Firefox").unwrap();
        assert_eq!(firefox.kind, InstallKind::User);

        let gimp = apps.iter().find(|a| a.name == "GIMP").unwrap();
        assert_eq!(gimp.kind, InstallKind::User);
    }

    #[test]
    fn marks_essential_and_important_packages_as_system() {
        let apps = build_inventory(&fake());
        let terminal = apps.iter().find(|a| a.name == "Terminal").unwrap();
        assert_eq!(terminal.kind, InstallKind::System);
        assert!(terminal.protected);
    }

    #[test]
    fn marks_auto_installed_packages_as_system() {
        let apps = build_inventory(&fake());
        let software = apps.iter().find(|a| a.name == "Software").unwrap();
        assert_eq!(software.kind, InstallKind::PreInstalled);
        assert!(!software.protected, "auto-instalada mas removível");
    }

    #[test]
    fn user_apps_are_not_protected() {
        let apps = build_inventory(&fake());
        let firefox = apps.iter().find(|a| a.name == "Firefox").unwrap();
        assert!(!firefox.protected);
    }

    #[test]
    fn ignores_desktop_entries_without_an_installed_package() {
        let mut system = fake();
        system.owners.remove("/usr/bin/gnome-software");
        let apps = build_inventory(&system);
        assert!(apps.iter().all(|a| a.name != "Software"));
    }

    #[test]
    fn includes_flatpak_apps_once_without_duplicating_their_desktop_entry() {
        let apps = build_inventory(&fake());
        let localsend: Vec<&App> = apps.iter().filter(|a| a.name == "LocalSend").collect();
        assert_eq!(localsend.len(), 1);
        assert_eq!(localsend[0].origin, Origin::Flatpak);
        assert_eq!(localsend[0].kind, InstallKind::User);
        assert_eq!(localsend[0].size_bytes, 58_200_000);
    }

    #[test]
    fn includes_appimages_without_an_apt_package() {
        let apps = build_inventory(&fake());
        let tool: Vec<&App> = apps.iter().filter(|a| a.name == "tool").collect();
        assert_eq!(tool.len(), 1);
        assert_eq!(tool[0].origin, Origin::AppImage);
        assert_eq!(tool[0].kind, InstallKind::User);
    }

    #[test]
    fn reads_an_appimage_version_from_its_filename() {
        let mut system = fake();
        system.appimages = vec![PathBuf::from(
            "/home/ed/Applications/sonora-v0.45.0-x86_64.AppImage",
        )];
        let apps = build_inventory(&system);
        let sonora = apps
            .iter()
            .find(|app| app.origin == Origin::AppImage)
            .expect("appimage");
        assert_eq!(sonora.version.as_deref(), Some("v0.45.0"));
    }

    #[test]
    fn resolves_bare_program_names_before_querying_packages() {
        let mut system = fake();
        system.desktops.push(desktop("Vim", "vim %F"));
        system
            .programs
            .insert("vim".to_owned(), PathBuf::from("/usr/bin/vim"));
        system
            .owners
            .insert("/usr/bin/vim".to_owned(), "vim".to_owned());
        system.packages.push(package("vim", "9.1.0", "optional", false));
        system.manual.insert("vim".to_owned());

        let apps = build_inventory(&system);
        let vim = apps.iter().find(|a| a.name == "Vim").expect("vim");
        assert_eq!(vim.package.as_deref(), Some("vim"));
    }

    #[test]
    fn skips_programs_that_cannot_be_resolved() {
        let mut system = fake();
        system.desktops.push(desktop("Fantasma", "fantasma %F"));
        let apps = build_inventory(&system);
        assert!(apps.iter().all(|a| a.name != "Fantasma"));
    }

    #[test]
    fn treats_desktop_meta_packages_as_system_even_when_manual() {
        let mut system = fake();
        system.desktops.push(desktop(
            "COSMIC Terminal",
            "/usr/bin/cosmic-term",
        ));
        system
            .owners
            .insert("/usr/bin/cosmic-term".to_owned(), "cosmic-term".to_owned());
        system
            .packages
            .push(package("cosmic-term", "1.11.0", "optional", false));
        system.manual.insert("cosmic-term".to_owned());
        system.desktop.insert("cosmic-term".to_owned());

        let apps = build_inventory(&system);
        let terminal = apps
            .iter()
            .find(|a| a.name == "COSMIC Terminal")
            .expect("terminal");
        assert_eq!(terminal.kind, InstallKind::System);
        assert!(terminal.protected, "recommend do desktop");
    }

    #[test]
    fn app_ids_are_unique() {
        let apps = build_inventory(&fake());
        let ids: HashSet<&str> = apps.iter().map(|a| a.id.as_str()).collect();
        assert_eq!(ids.len(), apps.len());
    }
}
