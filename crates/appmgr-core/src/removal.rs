//! Removal preview. It asks apt what it would do and reports it, without
//! changing anything: shared libraries used by other apps stay installed.

use crate::dpkg::base_name;
use crate::inventory::{App, InstallKind, Origin};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemovalPlan {
    pub origin: Origin,
    /// Package, Flatpak id or AppImage path that would go away.
    pub target: String,
    /// Packages removed besides the app itself, usually because they depend
    /// on it.
    pub extra_packages: Vec<String>,
    /// Auto-installed packages that are no longer needed but are kept. The
    /// app never runs `autoremove`, so shared libraries stay.
    pub kept_orphans: Vec<String>,
    /// Why the system refused the removal, if it did.
    pub error: Option<String>,
}

impl RemovalPlan {
    pub fn blocked(origin: Origin, target: String, error: String) -> Self {
        Self {
            origin,
            target,
            extra_packages: Vec::new(),
            kept_orphans: Vec::new(),
            error: Some(error),
        }
    }
}

/// Builds the preview. `simulate` receives the package name and returns the
/// output of `apt-get -s remove <package>`; it is only called for apt apps.
pub fn plan_for(app: &App, simulate: impl FnOnce(&str) -> String) -> RemovalPlan {
    match app.origin {
        Origin::Apt => {
            let target = app.package.clone().unwrap_or_default();
            if target.is_empty() {
                return RemovalPlan::blocked(
                    Origin::Apt,
                    target,
                    "no package is associated with this app".to_owned(),
                );
            }
            let simulation = parse_apt_simulation(&simulate(&target));
            let own = base_name(&target);
            let extra_packages = simulation
                .removed
                .into_iter()
                .filter(|name| base_name(name) != own)
                .collect();
            RemovalPlan {
                origin: Origin::Apt,
                target,
                extra_packages,
                kept_orphans: simulation.kept_orphans,
                error: simulation.error,
            }
        }
        Origin::Flatpak => RemovalPlan {
            origin: Origin::Flatpak,
            target: app.package.clone().unwrap_or_default(),
            extra_packages: Vec::new(),
            kept_orphans: Vec::new(),
            error: None,
        },
        Origin::AppImage => RemovalPlan {
            origin: Origin::AppImage,
            target: app
                .id
                .strip_prefix("appimage:")
                .unwrap_or(&app.id)
                .to_owned(),
            extra_packages: Vec::new(),
            kept_orphans: Vec::new(),
            error: None,
        },
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AptSimulation {
    pub removed: Vec<String>,
    pub kept_orphans: Vec<String>,
    pub error: Option<String>,
}

/// Reads the parts of `apt-get -s remove` that matter: which packages would
/// be removed, which orphaned packages are left alone, and any refusal.
pub fn parse_apt_simulation(text: &str) -> AptSimulation {
    #[derive(Clone, Copy, PartialEq)]
    enum Section {
        Removed,
        Orphans,
    }

    let mut simulation = AptSimulation::default();
    let mut section: Option<Section> = None;

    for line in text.lines() {
        let trimmed = line.trim();

        if let Some(error) = trimmed.strip_prefix("E: ") {
            simulation.error = Some(error.to_owned());
        }
        if trimmed.starts_with("WARNING: The following essential packages") {
            simulation.error = Some("an essential package would be removed".to_owned());
        }

        if trimmed.starts_with("The following packages will be REMOVED") {
            section = Some(Section::Removed);
            continue;
        }
        if trimmed.starts_with(
            "The following packages were automatically installed and are no longer required",
        ) {
            section = Some(Section::Orphans);
            continue;
        }

        if let Some(name) = trimmed.strip_prefix("Remv ") {
            if let Some(name) = name.split_whitespace().next() {
                push_unique(&mut simulation.removed, base_name(name));
            }
            continue;
        }

        if trimmed.is_empty() {
            section = None;
            continue;
        }

        if line.starts_with([' ', '\t']) {
            if let Some(section) = section {
                for name in trimmed.split_whitespace() {
                    let name = name.trim_end_matches(',');
                    if name.is_empty() || name.ends_with(':') || name.starts_with('(') {
                        continue;
                    }
                    let list = match section {
                        Section::Removed => &mut simulation.removed,
                        Section::Orphans => &mut simulation.kept_orphans,
                    };
                    push_unique(list, base_name(name));
                }
            }
        } else {
            section = None;
        }
    }

    if simulation.error.is_some() {
        simulation.removed.clear();
        simulation.kept_orphans.clear();
    }
    simulation
}

fn push_unique(list: &mut Vec<String>, name: &str) {
    if !list.iter().any(|existing| existing == name) {
        list.push(name.to_owned());
    }
}

/// The exact command that performs a removal. It is built as an argument
/// list, never through a shell, and every value is checked first.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RemovalCommand {
    Apt {
        package: String,
    },
    Flatpak {
        id: String,
        user: bool,
    },
    AppImage {
        path: String,
    },
}

impl RemovalCommand {
    pub fn argv(&self) -> Vec<String> {
        match self {
            RemovalCommand::Apt { package } => vec![
                "sudo".to_owned(),
                "-n".to_owned(),
                "env".to_owned(),
                "DEBIAN_FRONTEND=noninteractive".to_owned(),
                "apt-get".to_owned(),
                "-y".to_owned(),
                "remove".to_owned(),
                package.clone(),
            ],
            RemovalCommand::Flatpak { id, user: true } => vec![
                "flatpak".to_owned(),
                "uninstall".to_owned(),
                "--assumeyes".to_owned(),
                "--user".to_owned(),
                id.clone(),
            ],
            RemovalCommand::Flatpak { id, user: false } => vec![
                "sudo".to_owned(),
                "-n".to_owned(),
                "flatpak".to_owned(),
                "uninstall".to_owned(),
                "--assumeyes".to_owned(),
                "--system".to_owned(),
                id.clone(),
            ],
            RemovalCommand::AppImage { path } => {
                vec!["gio".to_owned(), "trash".to_owned(), path.clone()]
            }
        }
    }

    /// Whether the command needs the administrator password.
    pub fn needs_permission(&self) -> bool {
        match self {
            RemovalCommand::Apt { .. } | RemovalCommand::Flatpak { user: false, .. } => true,
            RemovalCommand::Flatpak { user: true, .. } | RemovalCommand::AppImage { .. } => false,
        }
    }
}

/// Builds the command for an app, refusing names that could be read as
/// options or that do not belong to the expected shape.
pub fn removal_command(app: &App) -> Result<RemovalCommand, String> {
    match app.origin {
        Origin::Apt => {
            let package = app
                .package
                .as_deref()
                .ok_or_else(|| "no package is associated with this app".to_owned())?;
            if !is_safe_name(package, "+-.:") {
                return Err(format!("refusing unsafe package name: {package}"));
            }
            Ok(RemovalCommand::Apt {
                package: package.to_owned(),
            })
        }
        Origin::Flatpak => {
            let id = app
                .package
                .as_deref()
                .ok_or_else(|| "no Flatpak id is associated with this app".to_owned())?;
            if !is_safe_name(id, "._-") {
                return Err(format!("refusing unsafe Flatpak id: {id}"));
            }
            Ok(RemovalCommand::Flatpak {
                id: id.to_owned(),
                user: app.kind == InstallKind::User,
            })
        }
        Origin::AppImage => {
            let path = app
                .id
                .strip_prefix("appimage:")
                .ok_or_else(|| "the AppImage has no file path".to_owned())?;
            if !path.starts_with('/') || path.contains('\n') {
                return Err(format!("refusing unsafe AppImage path: {path}"));
            }
            Ok(RemovalCommand::AppImage {
                path: path.to_owned(),
            })
        }
    }
}

fn is_safe_name(value: &str, extra: &str) -> bool {
    !value.is_empty()
        && !value.starts_with('-')
        && value
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || extra.contains(c))
}

/// Whether sudo already holds a valid authorization for this user, without
/// prompting for anything.
pub fn is_authorized() -> bool {
    std::process::Command::new("sudo")
        .args(["-n", "-v"])
        .env("LC_ALL", "C")
        .env("LANG", "C")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}

/// Authorizes sudo for a while using the user's password. The password is
/// written to sudo's stdin and is never part of the command line. After this
/// succeeds, removals run with `sudo -n` and do not ask again.
pub fn authorize(password: &str) -> Result<(), String> {
    use std::io::Write;
    use std::process::Stdio;

    if password.is_empty() {
        return Err("the password is empty".to_owned());
    }

    let mut child = std::process::Command::new("sudo")
        .args(["-S", "-p", "", "-v"])
        .env("LC_ALL", "C")
        .env("LANG", "C")
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| format!("could not run sudo: {error}"))?;

    if let Some(mut stdin) = child.stdin.take() {
        stdin
            .write_all(password.as_bytes())
            .and_then(|_| stdin.write_all(b"\n"))
            .map_err(|error| format!("could not send the password: {error}"))?;
    }

    let output = child
        .wait_with_output()
        .map_err(|error| format!("could not wait for sudo: {error}"))?;

    if output.status.success() {
        return Ok(());
    }

    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_owned();
    if stderr.is_empty() {
        Err("sudo refused the authorization".to_owned())
    } else {
        Err(stderr)
    }
}

/// Keeps the sudo authorization alive by refreshing its timestamp. It runs
/// silently and stops when the process exits.
pub fn keep_sudo_alive() {
    std::thread::spawn(|| {
        loop {
            std::thread::sleep(std::time::Duration::from_secs(50));
            let _ = std::process::Command::new("sudo")
                .args(["-n", "-v"])
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .status();
        }
    });
}

/// Runs the removal. It blocks until the command finishes, so the caller must
/// keep it off the UI thread.
pub fn run_removal(app: &App) -> Result<(), String> {
    let command = removal_command(app)?;
    let argv = command.argv();
    let (program, args) = argv
        .split_first()
        .ok_or_else(|| "empty removal command".to_owned())?;

    let output = std::process::Command::new(program)
        .args(args)
        .output()
        .map_err(|error| format!("could not run {program}: {error}"))?;

    if output.status.success() {
        return Ok(());
    }

    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_owned();
    if stderr.is_empty() {
        Err(format!(
            "{program} stopped with exit code {}",
            output.status.code().unwrap_or(-1)
        ))
    } else {
        Err(stderr)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIREFOX: &str = "\
NOTE: This is only a simulation!
Reading package lists...
Building dependency tree...
Reading state information...
The following packages were automatically installed and are no longer required:
  eog gettext gir1.2-peas-1.0 libexempi8 libpeas-1.0-0 libpeas-common
  webp-pixbuf-loader
Use 'apt autoremove' to remove them.
The following packages will be REMOVED:
  firefox
0 upgraded, 0 newly installed, 1 to remove and 0 not upgraded.
Remv firefox [1:158.0.1]
";

    const LIBPEAS: &str = "\
The following packages will be REMOVED:
  eog gir1.2-peas-1.0 libpeas-1.0-0
0 upgraded, 0 newly installed, 3 to remove and 0 not upgraded.
Remv eog [45.3-1ubuntu2]
Remv gir1.2-peas-1.0 [1.36.0-3build4]
Remv libpeas-1.0-0 [1.36.0-3build4]
";

    fn app(name: &str, package: &str, origin: Origin) -> App {
        App {
            id: format!("{origin:?}:{name}"),
            name: name.to_owned(),
            version: None,
            icon: None,
            origin,
            kind: crate::inventory::InstallKind::User,
            hub: true,
            protected: false,
            size_bytes: 0,
            package: Some(package.to_owned()),
        }
    }

    #[test]
    fn reads_removed_packages() {
        let simulation = parse_apt_simulation(FIREFOX);
        assert_eq!(simulation.removed, vec!["firefox"]);
        assert!(simulation.error.is_none());
    }

    #[test]
    fn reads_orphans_that_are_left_alone() {
        let simulation = parse_apt_simulation(FIREFOX);
        assert!(simulation.kept_orphans.contains(&"eog".to_owned()));
        assert!(simulation.kept_orphans.contains(&"libpeas-common".to_owned()));
        assert_eq!(simulation.kept_orphans.len(), 7);
    }

    #[test]
    fn reads_several_removed_packages() {
        let simulation = parse_apt_simulation(LIBPEAS);
        assert_eq!(
            simulation.removed,
            vec!["eog", "gir1.2-peas-1.0", "libpeas-1.0-0"]
        );
    }

    #[test]
    fn reports_refusals() {
        let text = "E: Unable to locate package nope\n";
        let simulation = parse_apt_simulation(text);
        assert_eq!(
            simulation.error.as_deref(),
            Some("Unable to locate package nope")
        );
        assert!(simulation.removed.is_empty());
    }

    #[test]
    fn treats_essential_removal_as_refused() {
        let text = "WARNING: The following essential packages will be removed.\n";
        let simulation = parse_apt_simulation(text);
        assert!(simulation.error.is_some());
    }

    #[test]
    fn plan_keeps_only_the_extra_packages() {
        let plan = plan_for(&app("Firefox", "firefox", Origin::Apt), |_| FIREFOX.to_owned());
        assert_eq!(plan.origin, Origin::Apt);
        assert_eq!(plan.target, "firefox");
        assert!(plan.extra_packages.is_empty());
        assert_eq!(plan.kept_orphans.len(), 7);
        assert!(plan.error.is_none());
    }

    #[test]
    fn plan_lists_dependents_as_extra_packages() {
        let plan = plan_for(
            &app("Eye of GNOME", "libpeas-1.0-0", Origin::Apt),
            |_| LIBPEAS.to_owned(),
        );
        assert_eq!(plan.extra_packages, vec!["eog", "gir1.2-peas-1.0"]);
    }

    #[test]
    fn flatpak_plan_has_no_apt_work() {
        let plan = plan_for(
            &app("Day", "com.github.day", Origin::Flatpak),
            |_| panic!("apt must not run for Flatpak"),
        );
        assert_eq!(plan.origin, Origin::Flatpak);
        assert_eq!(plan.target, "com.github.day");
        assert!(plan.error.is_none());
    }

    #[test]
    fn appimage_plan_uses_the_file_path() {
        let mut app = app("Tool", "", Origin::AppImage);
        app.id = "appimage:/home/ed/Applications/tool.AppImage".to_owned();
        let plan = plan_for(&app, |_| panic!("apt must not run for AppImage"));
        assert_eq!(plan.target, "/home/ed/Applications/tool.AppImage");
    }

    #[test]
    fn apt_command_runs_through_sudo_without_a_shell() {
        let command = removal_command(&app("Firefox", "firefox", Origin::Apt)).unwrap();
        assert_eq!(
            command.argv(),
            vec![
                "sudo",
                "-n",
                "env",
                "DEBIAN_FRONTEND=noninteractive",
                "apt-get",
                "-y",
                "remove",
                "firefox",
            ]
        );
        assert!(command.needs_permission());
    }

    #[test]
    fn user_flatpak_does_not_ask_for_a_password() {
        let command = removal_command(&app("Day", "com.github.day", Origin::Flatpak)).unwrap();
        assert_eq!(
            command.argv(),
            vec![
                "flatpak",
                "uninstall",
                "--assumeyes",
                "--user",
                "com.github.day",
            ]
        );
        assert!(!command.needs_permission());
    }

    #[test]
    fn system_flatpak_asks_for_a_password() {
        let mut flatpak = app("Day", "com.github.day", Origin::Flatpak);
        flatpak.kind = crate::inventory::InstallKind::System;
        let command = removal_command(&flatpak).unwrap();
        assert_eq!(
            command.argv(),
            vec![
                "sudo",
                "-n",
                "flatpak",
                "uninstall",
                "--assumeyes",
                "--system",
                "com.github.day",
            ]
        );
        assert!(command.needs_permission());
    }

    #[test]
    fn appimage_is_moved_to_the_trash() {
        let mut file = app("Tool", "", Origin::AppImage);
        file.id = "appimage:/home/ed/Applications/tool.AppImage".to_owned();
        let command = removal_command(&file).unwrap();
        assert_eq!(
            command.argv(),
            vec!["gio", "trash", "/home/ed/Applications/tool.AppImage"]
        );
        assert!(!command.needs_permission());
    }

    #[test]
    fn refuses_names_that_look_like_options() {
        let command = removal_command(&app("Evil", "--force-yes", Origin::Apt));
        assert!(command.is_err());

        let mut flatpak = app("Evil", "com.example.app,other", Origin::Flatpak);
        flatpak.kind = crate::inventory::InstallKind::User;
        assert!(removal_command(&flatpak).is_err());
    }

    #[test]
    fn refuses_a_relative_appimage_path() {
        let mut file = app("Tool", "", Origin::AppImage);
        file.id = "appimage:tool.AppImage".to_owned();
        assert!(removal_command(&file).is_err());
    }

    #[test]
    fn refuses_an_empty_password_without_running_sudo() {
        assert!(authorize("").is_err());
    }
}
