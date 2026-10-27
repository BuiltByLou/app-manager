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
    fn reads_manual_packages() {
        let manual = parse_apt_mark_manual(MANUAL);
        assert_eq!(manual.len(), 3);
        assert!(manual.contains("firefox"));
        assert!(!manual.contains("bash"));
    }
}
