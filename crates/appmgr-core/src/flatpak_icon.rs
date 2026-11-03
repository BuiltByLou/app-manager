//! Resolves an icon file for a Flatpak app id, if one exists.
pub fn find_icon(id: &str) -> Option<String> {
    let home = std::env::var_os("HOME").map(std::path::PathBuf::from)?;
    let candidates = [
        home.join(".local/share/icons/hicolor/256x256/apps")
            .join(format!("{id}.png")),
        home.join(".local/share/icons/hicolor/128x128/apps")
            .join(format!("{id}.png")),
        home.join(".local/share/icons/hicolor/64x64/apps")
            .join(format!("{id}.png")),
        std::path::PathBuf::from("/var/lib/flatpak/exports/share/icons/hicolor/256x256/apps")
            .join(format!("{id}.png")),
        std::path::PathBuf::from("/var/lib/flatpak/exports/share/icons/hicolor/128x128/apps")
            .join(format!("{id}.png")),
        std::path::PathBuf::from("/var/lib/flatpak/exports/share/icons/hicolor/64x64/apps")
            .join(format!("{id}.png")),
        home.join(".local/share/flatpak/exports/share/icons/hicolor/256x256/apps")
            .join(format!("{id}.png")),
        home.join(".local/share/flatpak/exports/share/icons/hicolor/128x128/apps")
            .join(format!("{id}.png")),
        home.join(".local/share/flatpak/exports/share/icons/hicolor/64x64/apps")
            .join(format!("{id}.png")),
    ];
    for candidate in &candidates {
        if candidate.is_file() {
            return Some(candidate.to_string_lossy().into_owned());
        }
    }
    None
}
