//! Interface translations. The language follows the desktop locale, and any
//! message missing from that language falls back to English. To add a language,
//! create `i18n/<locale>/app_manager.ftl`.

use std::sync::LazyLock;

use i18n_embed::fluent::{FluentLanguageLoader, fluent_language_loader};
use i18n_embed::{DefaultLocalizer, LanguageLoader, Localizer};
use rust_embed::RustEmbed;

#[derive(RustEmbed)]
#[folder = "i18n/"]
struct Localizations;

pub static LOADER: LazyLock<FluentLanguageLoader> = LazyLock::new(|| {
    let loader: FluentLanguageLoader = fluent_language_loader!();
    loader
        .load_fallback_language(&Localizations)
        .expect("English translations are embedded");

    let requested = i18n_embed::DesktopLanguageRequester::requested_languages();
    let localizer = DefaultLocalizer::new(&loader, &Localizations);
    if let Err(error) = localizer.select(&requested) {
        eprintln!("appmgr: no translation loaded: {error}");
    }
    loader
});

#[macro_export]
macro_rules! fl {
    ($message_id:literal) => {
        i18n_embed_fl::fl!($crate::i18n::LOADER, $message_id)
    };
    ($message_id:literal, $($args:tt)+) => {
        i18n_embed_fl::fl!($crate::i18n::LOADER, $message_id, $($args)+)
    };
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;
    use std::path::Path;

    fn keys(ftl: &str) -> BTreeSet<String> {
        ftl.lines()
            .filter(|line| !line.starts_with(' ') && !line.starts_with('#'))
            .filter_map(|line| line.split_once(" = ").map(|(key, _)| key.to_string()))
            .collect()
    }

    #[test]
    fn english_defines_every_message_once() {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("i18n")
            .join("en")
            .join("app_manager.ftl");
        let text = std::fs::read_to_string(path).unwrap();
        let all: Vec<String> = text
            .lines()
            .filter(|line| !line.starts_with(' ') && !line.starts_with('#'))
            .filter_map(|line| line.split_once(" = ").map(|(key, _)| key.to_string()))
            .collect();
        let unique: BTreeSet<&String> = all.iter().collect();
        assert_eq!(all.len(), unique.len(), "duplicate message ids");
        assert!(!all.is_empty());
    }

    /// Every locale must translate every English message, so no screen falls
    /// back silently except on purpose.
    #[test]
    fn every_translation_defines_every_english_message() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("i18n");
        let english = keys(
            &std::fs::read_to_string(root.join("en").join("app_manager.ftl")).unwrap(),
        );
        for entry in std::fs::read_dir(&root).unwrap() {
            let locale = entry.unwrap().path();
            if locale.file_name().is_some_and(|name| name == "en") {
                continue;
            }
            let text = std::fs::read_to_string(locale.join("app_manager.ftl")).unwrap();
            let translated = keys(&text);
            let missing: Vec<_> = english.difference(&translated).collect();
            assert!(missing.is_empty(), "{} lacks {missing:?}", locale.display());
        }
    }
}
