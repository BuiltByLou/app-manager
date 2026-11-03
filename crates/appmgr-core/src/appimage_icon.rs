//! Resolves the icon file created for an AppImage, if one exists.
pub fn find_icon(path: &std::path::Path) -> Option<String> {
    let stem = path.file_stem().and_then(|s| s.to_str())?;
    let home = std::env::var_os("HOME").map(std::path::PathBuf::from)?;
    for extension in ["png", "svg", "xpm"] {
        let candidate = home
            .join(".local/share/icons")
            .join(format!("{stem}.{extension}"));
        if candidate.is_file() {
            return Some(candidate.to_string_lossy().into_owned());
        }
    }
    None
}
