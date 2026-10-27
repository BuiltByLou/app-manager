#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DesktopEntry {
    pub name: String,
    pub exec: String,
    pub icon: Option<String>,
}

pub fn parse_desktop_entry(text: &str) -> Option<DesktopEntry> {
    let mut in_entry = false;
    let mut name = None;
    let mut exec = None;
    let mut icon = None;
    let mut kind = None;
    let mut no_display = false;

    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some(section) = line.strip_prefix('[').and_then(|l| l.strip_suffix(']')) {
            in_entry = section == "Desktop Entry";
            continue;
        }
        if !in_entry {
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let value = value.trim();
        match key.trim() {
            "Name" => name = Some(value.to_owned()),
            "Exec" => exec = Some(value.to_owned()),
            "Icon" => icon = Some(value.to_owned()),
            "Type" => kind = Some(value.to_owned()),
            "NoDisplay" => no_display = value.eq_ignore_ascii_case("true"),
            _ => {}
        }
    }

    if no_display {
        return None;
    }
    if kind.as_deref().is_some_and(|k| k != "Application") {
        return None;
    }

    Some(DesktopEntry {
        name: name?,
        exec: strip_field_codes(&exec?),
        icon,
    })
}

/// Remove os códigos de campo (`%u`, `%F`, …) que não fazem sentido fora do
/// lançador. `%%` é um `%` literal.
fn strip_field_codes(exec: &str) -> String {
    let mut tokens = exec
        .split_whitespace()
        .filter(|token| !token.starts_with('%'))
        .peekable();

    let mut command = Vec::new();
    if tokens.peek() == Some(&"env") {
        tokens.next();
        for token in tokens.by_ref() {
            if !token.contains('=') {
                command.push(token);
                break;
            }
        }
    }
    command.extend(tokens);

    command.join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIREFOX: &str = "\
[Desktop Entry]
Type=Application
Name=Firefox
Exec=/usr/lib/firefox/firefox %u
Icon=firefox
";

    const WITH_ENV: &str = "\
[Desktop Entry]
Type=Application
Name=Snap App
Exec=env BAMF_DESKTOP_FILE_HINT=/var/lib/snapd/desktop/apps/foo.desktop /snap/bin/foo %U
Icon=foo
";

    #[test]
    fn drops_env_prefix_and_assignments_from_exec() {
        let entry = parse_desktop_entry(WITH_ENV).expect("deve ser uma entrada válida");
        assert_eq!(entry.exec, "/snap/bin/foo");
    }

    const HIDDEN: &str = "\
[Desktop Entry]
Type=Application
Name=Internal Helper
Exec=/usr/bin/helper
NoDisplay=true
";

    const LINK: &str = "\
[Desktop Entry]
Type=Link
Name=Site
Exec=/usr/bin/xdg-open %u
URL=https://example.com
";

    #[test]
    fn ignores_entries_marked_no_display() {
        assert_eq!(parse_desktop_entry(HIDDEN), None);
    }

    #[test]
    fn ignores_entries_that_are_not_applications() {
        assert_eq!(parse_desktop_entry(LINK), None);
    }

    #[test]
    fn ignores_entry_without_exec() {
        let text = "[Desktop Entry]\nType=Application\nName=Sem comando\n";
        assert_eq!(parse_desktop_entry(text), None);
    }

    #[test]
    fn parses_name_exec_and_icon() {
        let entry = parse_desktop_entry(FIREFOX).expect("deve ser uma entrada válida");
        assert_eq!(entry.name, "Firefox");
        assert_eq!(entry.exec, "/usr/lib/firefox/firefox");
        assert_eq!(entry.icon.as_deref(), Some("firefox"));
    }
}
