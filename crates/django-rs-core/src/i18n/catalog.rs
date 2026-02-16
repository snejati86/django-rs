//! Translation catalog for loading and looking up translations.
//!
//! The catalog stores translations in a global, thread-safe registry organized
//! by language code. Translations can be loaded from JSON files or registered
//! programmatically.
//!
//! ## JSON Format
//!
//! ```json
//! {
//!   "messages": {
//!     "Hello": "Hola",
//!     "Goodbye": "Adiós"
//!   },
//!   "plurals": {
//!     "item": { "singular": "elemento", "plural": "elementos" }
//!   },
//!   "contexts": {
//!     "month\u0004May": "Mayo",
//!     "verb\u0004May": "Puede"
//!   }
//! }
//! ```

use std::collections::HashMap;
use std::sync::{OnceLock, RwLock};

/// A translation catalog for a single language.
#[derive(Debug, Clone, Default)]
pub struct TranslationCatalog {
    /// Simple message translations: msgid -> translated string.
    messages: HashMap<String, String>,
    /// Plural translations: singular msgid -> (translated singular, translated plural).
    plurals: HashMap<String, (String, String)>,
    /// Context translations: "context\x04msgid" -> translated string.
    contexts: HashMap<String, String>,
}

/// The global translation catalog registry, keyed by language code.
fn global_catalogs() -> &'static RwLock<HashMap<String, TranslationCatalog>> {
    static CATALOGS: OnceLock<RwLock<HashMap<String, TranslationCatalog>>> = OnceLock::new();
    CATALOGS.get_or_init(|| RwLock::new(HashMap::new()))
}

/// Returns a reference to the catalog for the given language, if one exists.
fn with_catalog<F, R>(language: &str, f: F) -> Option<R>
where
    F: FnOnce(&TranslationCatalog) -> Option<R>,
{
    let catalogs = global_catalogs().read().expect("catalog lock poisoned");
    catalogs.get(language).and_then(f)
}

/// Ensures a catalog entry exists for the given language and mutates it.
#[allow(clippy::significant_drop_tightening)]
fn with_catalog_mut<F>(language: &str, f: F)
where
    F: FnOnce(&mut TranslationCatalog),
{
    let mut catalogs = global_catalogs().write().expect("catalog lock poisoned");
    let catalog = catalogs.entry(language.to_string()).or_default();
    f(catalog);
}

// ── Registration API ─────────────────────────────────────────────────────

/// Registers simple message translations for a language.
///
/// Each entry is a `(msgid, translated)` pair. If translations already exist
/// for the language, the new entries are merged (overwriting duplicates).
///
/// # Examples
///
/// ```
/// use django_rs_core::i18n::catalog;
///
/// catalog::register_translations("fr", vec![
///     ("Hello", "Bonjour"),
///     ("Goodbye", "Au revoir"),
/// ]);
/// ```
pub fn register_translations(language: &str, entries: Vec<(&str, &str)>) {
    with_catalog_mut(language, |catalog| {
        for (msgid, translated) in entries {
            catalog
                .messages
                .insert(msgid.to_string(), translated.to_string());
        }
    });
}

/// Registers plural translations for a language.
///
/// Each entry is `(singular_msgid, plural_msgid, translated_singular, translated_plural)`.
///
/// # Examples
///
/// ```
/// use django_rs_core::i18n::catalog;
///
/// catalog::register_plural_translations("de", vec![
///     ("apple", "apples", "Apfel", "Äpfel"),
/// ]);
/// ```
pub fn register_plural_translations(language: &str, entries: Vec<(&str, &str, &str, &str)>) {
    with_catalog_mut(language, |catalog| {
        for (singular, _plural, trans_singular, trans_plural) in entries {
            catalog.plurals.insert(
                singular.to_string(),
                (trans_singular.to_string(), trans_plural.to_string()),
            );
        }
    });
}

/// Registers context-specific translations for a language.
///
/// Each entry is `(context, msgid, translated)`.
///
/// # Examples
///
/// ```
/// use django_rs_core::i18n::catalog;
///
/// catalog::register_context_translations("de", vec![
///     ("month", "May", "Mai"),
///     ("verb", "May", "Darf"),
/// ]);
/// ```
pub fn register_context_translations(language: &str, entries: Vec<(&str, &str, &str)>) {
    with_catalog_mut(language, |catalog| {
        for (context, msgid, translated) in entries {
            let key = format!("{context}\x04{msgid}");
            catalog.contexts.insert(key, translated.to_string());
        }
    });
}

/// Loads translations from a JSON string.
///
/// The JSON format should be:
/// ```json
/// {
///   "messages": { "msgid": "translated", ... },
///   "plurals": { "singular_msgid": { "singular": "...", "plural": "..." }, ... },
///   "contexts": { "context\u0004msgid": "translated", ... }
/// }
/// ```
///
/// All top-level keys are optional.
///
/// # Errors
///
/// Returns `Err` if the JSON is invalid.
pub fn load_from_json(language: &str, json_str: &str) -> Result<(), String> {
    let value: serde_json::Value =
        serde_json::from_str(json_str).map_err(|e| format!("Invalid JSON: {e}"))?;

    with_catalog_mut(language, |catalog| {
        // Load messages
        if let Some(messages) = value.get("messages").and_then(|v| v.as_object()) {
            for (msgid, translated) in messages {
                if let Some(t) = translated.as_str() {
                    catalog.messages.insert(msgid.clone(), t.to_string());
                }
            }
        }

        // Load plurals
        if let Some(plurals) = value.get("plurals").and_then(|v| v.as_object()) {
            for (singular_msgid, forms) in plurals {
                if let (Some(singular), Some(plural)) = (
                    forms.get("singular").and_then(|v| v.as_str()),
                    forms.get("plural").and_then(|v| v.as_str()),
                ) {
                    catalog.plurals.insert(
                        singular_msgid.clone(),
                        (singular.to_string(), plural.to_string()),
                    );
                }
            }
        }

        // Load contexts
        if let Some(contexts) = value.get("contexts").and_then(|v| v.as_object()) {
            for (key, translated) in contexts {
                if let Some(t) = translated.as_str() {
                    catalog.contexts.insert(key.clone(), t.to_string());
                }
            }
        }
    });

    Ok(())
}

// ── Lookup API ───────────────────────────────────────────────────────────

/// Looks up a simple translation in the catalog.
pub fn translate(language: &str, msgid: &str) -> Option<String> {
    with_catalog(language, |catalog| catalog.messages.get(msgid).cloned())
}

/// Looks up a plural translation in the catalog.
///
/// Returns the singular form if `count == 1`, otherwise the plural form.
pub fn translate_plural(
    language: &str,
    singular: &str,
    _plural: &str,
    count: u64,
) -> Option<String> {
    with_catalog(language, |catalog| {
        catalog
            .plurals
            .get(singular)
            .map(|(s, p)| if count == 1 { s.clone() } else { p.clone() })
    })
}

/// Looks up a context-specific translation in the catalog.
pub fn translate_context(language: &str, context: &str, msgid: &str) -> Option<String> {
    let key = format!("{context}\x04{msgid}");
    with_catalog(language, |catalog| catalog.contexts.get(&key).cloned())
}

/// Returns `true` if translations are registered for the given language.
pub fn has_language(language: &str) -> bool {
    let catalogs = global_catalogs().read().expect("catalog lock poisoned");
    catalogs.contains_key(language)
}

/// Returns a list of all languages that have translations registered.
pub fn available_languages() -> Vec<String> {
    let catalogs = global_catalogs().read().expect("catalog lock poisoned");
    catalogs.keys().cloned().collect()
}

/// Clears all translations for a given language.
pub fn clear_language(language: &str) {
    let mut catalogs = global_catalogs().write().expect("catalog lock poisoned");
    catalogs.remove(language);
}

/// Clears all translation catalogs.
pub fn clear_all() {
    let mut catalogs = global_catalogs().write().expect("catalog lock poisoned");
    catalogs.clear();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_register_and_translate() {
        register_translations("test_lang1", vec![("foo", "bar")]);
        assert_eq!(translate("test_lang1", "foo"), Some("bar".to_string()));
        assert_eq!(translate("test_lang1", "baz"), None);
    }

    #[test]
    fn test_translate_missing_language() {
        assert_eq!(translate("nonexistent_lang", "hello"), None);
    }

    #[test]
    fn test_plural_translations() {
        register_plural_translations("test_lang2", vec![("cat", "cats", "gato", "gatos")]);
        assert_eq!(
            translate_plural("test_lang2", "cat", "cats", 1),
            Some("gato".to_string())
        );
        assert_eq!(
            translate_plural("test_lang2", "cat", "cats", 0),
            Some("gatos".to_string())
        );
        assert_eq!(
            translate_plural("test_lang2", "cat", "cats", 99),
            Some("gatos".to_string())
        );
    }

    #[test]
    fn test_context_translations() {
        register_context_translations(
            "test_lang3",
            vec![("month", "May", "Mai"), ("modal", "May", "Darf")],
        );
        assert_eq!(
            translate_context("test_lang3", "month", "May"),
            Some("Mai".to_string())
        );
        assert_eq!(
            translate_context("test_lang3", "modal", "May"),
            Some("Darf".to_string())
        );
        assert_eq!(translate_context("test_lang3", "unknown", "May"), None);
    }

    #[test]
    fn test_load_from_json() {
        let json = r#"{
            "messages": {
                "Hello": "Bonjour",
                "Yes": "Oui"
            },
            "plurals": {
                "item": { "singular": "élément", "plural": "éléments" }
            },
            "contexts": {
                "greeting\u0004Hi": "Salut"
            }
        }"#;

        let result = load_from_json("test_json_lang", json);
        assert!(result.is_ok());

        assert_eq!(
            translate("test_json_lang", "Hello"),
            Some("Bonjour".to_string())
        );
        assert_eq!(translate("test_json_lang", "Yes"), Some("Oui".to_string()));
        assert_eq!(
            translate_plural("test_json_lang", "item", "items", 1),
            Some("élément".to_string())
        );
        assert_eq!(
            translate_plural("test_json_lang", "item", "items", 5),
            Some("éléments".to_string())
        );
        assert_eq!(
            translate_context("test_json_lang", "greeting", "Hi"),
            Some("Salut".to_string())
        );
    }

    #[test]
    fn test_load_from_json_invalid() {
        let result = load_from_json("bad", "not json");
        assert!(result.is_err());
    }

    #[test]
    fn test_load_from_json_partial() {
        // Only messages, no plurals or contexts
        let json = r#"{"messages": {"A": "B"}}"#;
        let result = load_from_json("test_partial_lang", json);
        assert!(result.is_ok());
        assert_eq!(translate("test_partial_lang", "A"), Some("B".to_string()));
    }

    #[test]
    fn test_load_from_json_empty() {
        let json = "{}";
        let result = load_from_json("test_empty_lang", json);
        assert!(result.is_ok());
    }

    #[test]
    fn test_has_language() {
        register_translations("test_has_lang", vec![("x", "y")]);
        assert!(has_language("test_has_lang"));
        assert!(!has_language("never_registered_lang"));
    }

    #[test]
    fn test_merge_translations() {
        register_translations("test_merge_lang", vec![("A", "1"), ("B", "2")]);
        register_translations("test_merge_lang", vec![("B", "3"), ("C", "4")]);

        assert_eq!(translate("test_merge_lang", "A"), Some("1".to_string()));
        assert_eq!(translate("test_merge_lang", "B"), Some("3".to_string())); // overwritten
        assert_eq!(translate("test_merge_lang", "C"), Some("4".to_string()));
    }

    #[test]
    fn test_clear_language() {
        register_translations("test_clear_lang", vec![("x", "y")]);
        assert!(has_language("test_clear_lang"));
        clear_language("test_clear_lang");
        assert!(!has_language("test_clear_lang"));
    }
}
