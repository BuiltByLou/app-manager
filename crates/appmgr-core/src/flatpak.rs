#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FlatpakApp {
    pub id: String,
    pub name: String,
    pub version: String,
    pub origin: String,
    pub user_installation: bool,
    pub size_bytes: u64,
}

/// Lê a saída de:
/// `flatpak list --app --columns=application,name,version,origin,installation,size`
pub fn parse_flatpak_list(text: &str) -> Vec<FlatpakApp> {
    text.lines()
        .filter_map(|line| {
            let mut fields = line.split('\t');
            let id = fields.next()?.trim();
            let name = fields.next()?.trim();
            let version = fields.next()?.trim();
            let origin = fields.next()?.trim();
            let installation = fields.next()?.trim();
            let size = fields.next().unwrap_or("").trim();

            if id.is_empty() {
                return None;
            }

            Some(FlatpakApp {
                id: id.to_owned(),
                name: name.to_owned(),
                version: version.to_owned(),
                origin: origin.to_owned(),
                user_installation: installation.eq_ignore_ascii_case("user"),
                size_bytes: parse_flatpak_size(size),
            })
        })
        .collect()
}

/// Converte tamanhos do Flatpak ("97.2?MB") em bytes.
fn parse_flatpak_size(size: &str) -> u64 {
    let number: String = size
        .chars()
        .take_while(|c| c.is_ascii_digit() || *c == '.')
        .collect();
    let value: f64 = number.parse().unwrap_or(0.0);

    let unit: String = size
        .chars()
        .filter(char::is_ascii_alphabetic)
        .collect::<String>();
    let multiplier: u64 = match unit.as_str() {
        "kB" => 1_000,
        "MB" => 1_000_000,
        "GB" => 1_000_000_000,
        "TB" => 1_000_000_000_000,
        _ => 1,
    };

    (value * multiplier as f64) as u64
}

#[cfg(test)]
mod tests {
    use super::*;

    const LIST: &str = "\
io.github.DrakRosmann.CosmicExtAppletQuickSettings	Quick Settings	0.1.0	cosmic	user	97.2?MB
org.localsend.localsend_app	LocalSend	1.18.2	flathub	user	58.2?MB
org.gnome.Builder	Builder	46.1	flathub	system	1.4?GB
";

    #[test]
    fn reads_flatpak_apps() {
        let apps = parse_flatpak_list(LIST);
        assert_eq!(apps.len(), 3);
        let localsend = &apps[1];
        assert_eq!(localsend.id, "org.localsend.localsend_app");
        assert_eq!(localsend.name, "LocalSend");
        assert_eq!(localsend.version, "1.18.2");
        assert_eq!(localsend.origin, "flathub");
        assert!(localsend.user_installation);
        assert_eq!(localsend.size_bytes, 58_200_000);
    }

    #[test]
    fn marks_system_installations() {
        let apps = parse_flatpak_list(LIST);
        let builder = apps.iter().find(|a| a.id == "org.gnome.Builder").unwrap();
        assert!(!builder.user_installation);
        assert_eq!(builder.size_bytes, 1_400_000_000);
    }

    #[test]
    fn parses_sizes_with_and_without_units() {
        assert_eq!(parse_flatpak_size("512?kB"), 512_000);
        assert_eq!(parse_flatpak_size("2?MB"), 2_000_000);
        assert_eq!(parse_flatpak_size("0?B"), 0);
        assert_eq!(parse_flatpak_size(""), 0);
    }
}
