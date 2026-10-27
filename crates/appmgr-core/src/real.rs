use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::dpkg::{InstalledPackage, parse_apt_mark_manual, parse_dpkg_query};
use crate::flatpak::{FlatpakApp, parse_flatpak_list};
use crate::inventory::{DesktopFile, SystemProbe};

/// Acesso real ao sistema. Sem lógica de decisão: só recolhe dados.
pub struct RealProbe {
    home: PathBuf,
}

impl RealProbe {
    pub fn new() -> Self {
        let home = std::env::var_os("HOME")
            .map(PathBuf::from)
            .unwrap_or_default();
        Self { home }
    }
}

impl SystemProbe for RealProbe {
    fn desktop_files(&self) -> Vec<DesktopFile> {
        let mut dirs = vec![
            PathBuf::from("/usr/share/applications"),
            PathBuf::from("/usr/local/share/applications"),
            PathBuf::from("/var/lib/flatpak/exports/share/applications"),
        ];
        dirs.push(self.home.join(".local/share/applications"));
        dirs.push(self.home.join(".local/share/flatpak/exports/share/applications"));

        let mut files = Vec::new();
        for dir in dirs {
            let Ok(entries) = std::fs::read_dir(&dir) else {
                continue;
            };
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().is_some_and(|ext| ext == "desktop")
                    && let Ok(contents) = std::fs::read_to_string(&path)
                {
                    files.push(DesktopFile { path, contents });
                }
            }
        }
        files
    }

    fn packages(&self) -> Vec<InstalledPackage> {
        run(
            Command::new("dpkg-query").args([
                "-W",
                "-f=${binary:Package}\t${Version}\t${Installed-Size}\t${Status}\t${Essential}\t${Priority}\n",
            ]),
        )
        .map(|out| parse_dpkg_query(&out))
        .unwrap_or_default()
    }

    fn manual_packages(&self) -> HashSet<String> {
        run(Command::new("apt-mark").arg("showmanual"))
            .map(|out| parse_apt_mark_manual(&out))
            .unwrap_or_default()
    }

    fn package_owners(&self, paths: &[PathBuf]) -> HashMap<PathBuf, String> {
        let mut seen = HashSet::new();
        let mut queried = Vec::new();
        for path in paths {
            let normalized = normalize_path(path);
            if seen.insert(normalized.clone()) {
                queried.push(normalized);
            }
        }

        let mut found: HashMap<PathBuf, String> = HashMap::new();
        for chunk in queried.chunks(400) {
            let Some(out) = run_stdout(Command::new("dpkg-query").arg("-S").args(chunk)) else {
                continue;
            };
            for (package, path) in crate::dpkg::parse_dpkg_search(&out) {
                found.entry(path).or_insert(package);
            }
        }

        paths
            .iter()
            .filter_map(|path| {
                let package = found.get(&normalize_path(path))?.clone();
                Some((path.clone(), package))
            })
            .collect()
    }

    fn which(&self, program: &str) -> Option<PathBuf> {
        if let Some(path) = std::env::var_os("PATH") {
            for dir in std::env::split_paths(&path) {
                let candidate = dir.join(program);
                if candidate.is_file() {
                    return Some(candidate);
                }
            }
        }
        ["/usr/local/bin", "/usr/bin", "/bin", "/snap/bin"]
            .iter()
            .map(|dir| Path::new(dir).join(program))
            .find(|candidate| candidate.is_file())
    }

    fn flatpak_apps(&self) -> Vec<FlatpakApp> {
        run(Command::new("flatpak").args([
            "list",
            "--app",
            "--columns=application,name,version,origin,installation,size",
        ]))
        .map(|out| parse_flatpak_list(&out))
        .unwrap_or_default()
    }

    fn appimages(&self) -> Vec<PathBuf> {
        let mut found = Vec::new();
        for dir in [
            self.home.join("Applications"),
            self.home.join(".local/bin"),
            self.home.join("bin"),
        ] {
            collect_appimages(&dir, 1, &mut found);
        }
        collect_appimages(Path::new("/opt"), 2, &mut found);
        found
    }
}

/// Executa um comando com locale fixo, para a saída ser sempre analisável.
fn run_stdout(command: &mut Command) -> Option<String> {
    let output = command.env("LC_ALL", "C").env("LANG", "C").output().ok()?;
    String::from_utf8(output.stdout).ok()
}

/// `/bin` e `/sbin` são symlinks para `/usr`; o dpkg só conhece o caminho real.
fn normalize_path(path: &Path) -> PathBuf {
    let text = path.to_string_lossy();
    for (from, to) in [
        ("/bin/", "/usr/bin/"),
        ("/sbin/", "/usr/sbin/"),
        ("/lib/", "/usr/lib/"),
    ] {
        if let Some(rest) = text.strip_prefix(from) {
            return PathBuf::from(format!("{to}{rest}"));
        }
    }
    path.to_path_buf()
}
fn run(command: &mut Command) -> Option<String> {
    let output = command.env("LC_ALL", "C").env("LANG", "C").output().ok()?;
    if !output.status.success() {
        return None;
    }
    String::from_utf8(output.stdout).ok()
}

fn collect_appimages(dir: &Path, depth: u8, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            if depth > 1 {
                collect_appimages(&path, depth - 1, out);
            }
            continue;
        }
        let is_appimage = path
            .extension()
            .is_some_and(|ext| ext.eq_ignore_ascii_case("appimage"));
        if is_appimage {
            out.push(path);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::inventory::Origin;
    use crate::inventory::build_inventory;

    #[test]
    fn real_inventory_is_consistent() {
        let apps = build_inventory(&RealProbe::new());

        assert!(
            !apps.is_empty(),
            "um desktop a sério tem de produzir pelo menos uma app"
        );

        let ids: HashSet<&str> = apps.iter().map(|a| a.id.as_str()).collect();
        assert_eq!(ids.len(), apps.len(), "ids não podem repetir-se");

        for app in &apps {
            assert!(!app.name.trim().is_empty(), "toda a app precisa de nome");
            if app.origin == Origin::Apt {
                assert!(
                    app.package.is_some(),
                    "app apt sem pacote associado: {}",
                    app.name
                );
                assert!(
                    app.version.is_some(),
                    "app apt sem versão: {}",
                    app.name
                );
            }
        }
    }
}
