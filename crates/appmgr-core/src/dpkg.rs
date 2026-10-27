use std::collections::HashSet;
use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstalledPackage {
    pub name: String,
    pub version: String,
    pub installed_size_kib: u64,
    pub essential: bool,
    pub priority: String,
}

/// Lê a saída de:
/// `dpkg-query -W -f='${binary:Package}\t${Version}\t${Installed-Size}\t${Status}\t${Essential}\t${Priority}\n'`
///
/// Só interessam pacotes instalados; o resto do sistema não conta.
pub fn parse_dpkg_query(text: &str) -> Vec<InstalledPackage> {
    text.lines()
        .filter_map(|line| {
            let mut fields = line.split('\t');
            let name = fields.next()?.trim();
            let version = fields.next()?.trim();
            let size = fields.next()?.trim();
            let status = fields.next()?.trim();
            let essential = fields.next()?.trim();
            let priority = fields.next()?.trim();

            if name.is_empty() || !status.ends_with("installed") {
                return None;
            }

            Some(InstalledPackage {
                name: name.to_owned(),
                version: version.to_owned(),
                installed_size_kib: size.parse().unwrap_or(0),
                essential: essential.eq_ignore_ascii_case("yes"),
                priority: priority.to_owned(),
            })
        })
        .collect()
}

/// Lê a saída de `dpkg-query -S <caminho>...`: uma linha por caminho,
/// no formato `pacote[, pacote2]: /caminho`.
pub fn parse_dpkg_search(text: &str) -> Vec<(String, PathBuf)> {
    text.lines()
        .filter_map(|line| {
            let (packages, path) = line.split_once(": ")?;
            if !path.starts_with('/') {
                return None;
            }
            let package = packages.split(',').next()?.trim();
            if package.is_empty() {
                return None;
            }
            Some((package.to_owned(), PathBuf::from(path.trim())))
        })
        .collect()
}

/// Nome do pacote sem a arquitetura (`libc6:amd64` → `libc6`).
pub fn base_name(name: &str) -> &str {
    name.split(':').next().unwrap_or(name)
}

/// Lê `/var/lib/apt/extended_states` e devolve os pacotes marcados como
/// instalados automaticamente.
pub fn parse_auto_installed(text: &str) -> HashSet<String> {
    let mut auto = HashSet::new();
    let mut current: Option<String> = None;

    for line in text.lines() {
        let line = line.trim();
        if let Some(name) = line.strip_prefix("Package: ") {
            current = Some(name.trim().to_owned());
        } else if line == "Auto-Installed: 1" {
            if let Some(name) = current.take() {
                auto.insert(name);
            }
        } else if line.is_empty() {
            current = None;
        }
    }

    auto
}

/// Lê relações do dpkg (`Depends`, `Recommends`): `a | b, c (>= 1)`.
pub fn parse_relations(text: &str) -> HashSet<String> {
    text.split([',', '|'])
        .filter_map(|part| part.split_whitespace().next())
        .filter(|name| !name.is_empty() && !name.starts_with('('))
        .map(str::to_owned)
        .collect()
}

/// Lê a saída de `apt-cache depends <metapacote>...`: devolve os pacotes
/// reais de que ele depende ou recomenda, diretos.
pub fn parse_apt_depends(text: &str) -> HashSet<String> {
    let mut packages = HashSet::new();

    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let indent = line.len() - line.trim_start().len();

        // Linhas indentadas a fundo são os fornecedores reais de um virtual.
        if indent >= 4 {
            if !trimmed.contains(':')
                && let Some(name) = trimmed.split_whitespace().next()
            {
                packages.insert(name.to_owned());
            }
            continue;
        }
        if indent != 2 {
            continue;
        }

        let Some((key, value)) = trimmed.split_once(':') else {
            continue;
        };
        if !matches!(
            key.trim_start_matches('|'),
            "Depends" | "PreDepends" | "Recommends"
        ) {
            continue;
        }

        let value = value.trim();
        // Pacotes virtuais ficam de fora: os fornecedores aparecem nas linhas
        // indentadas abaixo.
        if value.starts_with('<') {
            continue;
        }
        if let Some(name) = value.split_whitespace().next() {
            packages.insert(name.to_owned());
        }
    }

    packages
}

/// Lê a saída de `apt-mark showmanual`: um pacote por linha.
pub fn parse_apt_mark_manual(text: &str) -> HashSet<String> {
    text.lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(str::to_owned)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const QUERY: &str = "\
firefox	1:1snap1-0ubuntu5	42	install ok installed	no	optional
libc6:amd64	2.39-0ubuntu8.6	13000	install ok installed	no	optional
bash	5.2.21-2ubuntu4	2000	install ok installed	yes	required
vim	2:9.1.0016-1ubuntu7	4000	deinstall ok config-files	no	optional
";

    const MANUAL: &str = "\
firefox
vim
code
";

    #[test]
    fn keeps_only_installed_packages() {
        let packages = parse_dpkg_query(QUERY);
        let names: Vec<&str> = packages.iter().map(|p| p.name.as_str()).collect();
        assert_eq!(names, vec!["firefox", "libc6:amd64", "bash"]);
    }

    #[test]
    fn reads_package_metadata() {
        let packages = parse_dpkg_query(QUERY);
        let bash = packages.iter().find(|p| p.name == "bash").unwrap();
        assert_eq!(bash.version, "5.2.21-2ubuntu4");
        assert_eq!(bash.installed_size_kib, 2000);
        assert!(bash.essential);
        assert_eq!(bash.priority, "required");
    }

    #[test]
    fn parses_search_results() {
        let text = "\
dpkg: /usr/bin/dpkg
libc6:amd64, libc6: /lib/x86_64-linux-gnu/libc.so.6
dpkg-query: no path found matching pattern /nope
";
        let found = parse_dpkg_search(text);
        assert_eq!(found.len(), 2);
        assert_eq!(found[0].0, "dpkg");
        assert_eq!(found[0].1, PathBuf::from("/usr/bin/dpkg"));
        assert_eq!(found[1].0, "libc6:amd64");
    }

    #[test]
    fn strips_architecture_suffix() {
        assert_eq!(base_name("libc6:amd64"), "libc6");
        assert_eq!(base_name("firefox"), "firefox");
    }

    #[test]
    fn parses_auto_installed_from_extended_states() {
        let text = "\
Package: python3-blinker
Architecture: amd64
Auto-Installed: 1

Package: firefox
Architecture: amd64
Auto-Installed: 1
";
        let auto = parse_auto_installed(text);
        assert_eq!(auto.len(), 2);
        assert!(auto.contains("python3-blinker"));
        assert!(auto.contains("firefox"));
    }

    #[test]
    fn parses_relations_with_versions_and_alternatives() {
        let text = "dconf-gsettings-backend | gsettings-backend, libc6 (>= 2.39), cosmic-term,\n cosmic-edit";
        let names = parse_relations(text);
        assert!(names.contains("dconf-gsettings-backend"));
        assert!(names.contains("gsettings-backend"));
        assert!(names.contains("libc6"));
        assert!(names.contains("cosmic-term"));
        assert!(names.contains("cosmic-edit"));
        assert_eq!(names.len(), 5);
    }

    #[test]
    fn parses_direct_dependencies_of_a_metapackage() {
        let text = "\
pop-desktop
  PreDepends: pop-de-cosmic
  Depends: cosmic-session (= 1.0)
  Depends: <virtual-package>
    real-provider
  Recommends: cosmic-term
  Suggests: extra
  Breaks: broken
";
        let packages = parse_apt_depends(text);
        assert!(packages.contains("pop-de-cosmic"));
        assert!(packages.contains("cosmic-session"));
        assert!(packages.contains("cosmic-term"));
        assert!(packages.contains("real-provider"));
        assert!(!packages.contains("extra"));
        assert!(!packages.contains("broken"));
        assert!(!packages.contains("virtual-package"));
    }
    #[test]
    fn reads_manual_packages() {
        let manual = parse_apt_mark_manual(MANUAL);
        assert_eq!(manual.len(), 3);
        assert!(manual.contains("firefox"));
        assert!(!manual.contains("bash"));
    }
}
