//! Internationalization and Localization (i18n/l10n) framework.
//!
//! This module provides Django-compatible i18n support including:
//!
//! - **Translation catalog**: Load translations from JSON files. Supports `gettext`,
//!   `ngettext`, and `pgettext` for contextual translations.
//! - **Lazy translations**: `gettext_lazy()` defers translation until the string is used.
//! - **Language activation**: Thread-local `activate()`, `deactivate()`, `get_language()`.
//! - **Timezone support**: `activate_timezone()`, `localtime()`, `now()`.
//!
//! ## Quick Start
//!
//! ```
//! use django_rs_core::i18n;
//!
//! // Register translations
//! i18n::catalog::register_translations("es", vec![
//!     ("Hello", "Hola"),
//!     ("Goodbye", "Adiós"),
//! ]);
//!
//! // Activate a language for the current thread
//! i18n::activate("es");
//! assert_eq!(i18n::gettext("Hello"), "Hola");
//!
//! // Deactivate to return to default
//! i18n::deactivate();
//! assert_eq!(i18n::gettext("Hello"), "Hello");
//! ```

pub mod catalog;
pub mod lazy;
pub mod timezone;

use std::cell::RefCell;

// ── Thread-local language state ──────────────────────────────────────────

thread_local! {
    static CURRENT_LANGUAGE: RefCell<Option<String>> = const { RefCell::new(None) };
}

/// Activates the given language code for the current thread.
///
/// All subsequent calls to `gettext`, `ngettext`, and `pgettext` on this
/// thread will use the specified language.
///
/// # Examples
///
/// ```
/// use django_rs_core::i18n;
///
/// i18n::activate("fr");
/// assert_eq!(i18n::get_language(), "fr");
/// i18n::deactivate();
/// ```
pub fn activate(language_code: &str) {
    CURRENT_LANGUAGE.with(|cell| {
        *cell.borrow_mut() = Some(language_code.to_string());
    });
}

/// Deactivates the current thread's language setting, reverting to the default.
///
/// After deactivation, `get_language()` returns `"en"`.
pub fn deactivate() {
    CURRENT_LANGUAGE.with(|cell| {
        *cell.borrow_mut() = None;
    });
}

/// Returns the language code active on the current thread.
///
/// Returns the activated language if one is set, otherwise returns `"en"`.
pub fn get_language() -> String {
    CURRENT_LANGUAGE.with(|cell| cell.borrow().clone().unwrap_or_else(|| "en".to_string()))
}

/// Translates a message using the current thread's active language.
///
/// If no translation is found, returns the original `msgid`.
///
/// # Examples
///
/// ```
/// use django_rs_core::i18n;
///
/// i18n::catalog::register_translations("de", vec![("Yes", "Ja")]);
/// i18n::activate("de");
/// assert_eq!(i18n::gettext("Yes"), "Ja");
/// assert_eq!(i18n::gettext("Unknown"), "Unknown");
/// i18n::deactivate();
/// ```
pub fn gettext(msgid: &str) -> String {
    let lang = get_language();
    catalog::translate(&lang, msgid).unwrap_or_else(|| msgid.to_string())
}

/// Translates a message with plural support.
///
/// Returns the singular form if `count == 1`, otherwise the plural form.
/// If no translation is found, returns the appropriate English form.
///
/// # Examples
///
/// ```
/// use django_rs_core::i18n;
///
/// i18n::catalog::register_plural_translations("fr", vec![
///     ("item", "items", "élément", "éléments"),
/// ]);
/// i18n::activate("fr");
/// assert_eq!(i18n::ngettext("item", "items", 1), "élément");
/// assert_eq!(i18n::ngettext("item", "items", 5), "éléments");
/// i18n::deactivate();
/// ```
pub fn ngettext(singular: &str, plural: &str, count: u64) -> String {
    let lang = get_language();
    catalog::translate_plural(&lang, singular, plural, count).unwrap_or_else(|| {
        if count == 1 {
            singular.to_string()
        } else {
            plural.to_string()
        }
    })
}

/// Translates a message with a context disambiguator.
///
/// Context is used to distinguish identical source strings that have
/// different meanings (e.g., "May" as a month vs. a verb).
///
/// # Examples
///
/// ```
/// use django_rs_core::i18n;
///
/// i18n::catalog::register_context_translations("de", vec![
///     ("month", "May", "Mai"),
///     ("verb", "May", "Darf"),
/// ]);
/// i18n::activate("de");
/// assert_eq!(i18n::pgettext("month", "May"), "Mai");
/// assert_eq!(i18n::pgettext("verb", "May"), "Darf");
/// i18n::deactivate();
/// ```
pub fn pgettext(context: &str, msgid: &str) -> String {
    let lang = get_language();
    catalog::translate_context(&lang, context, msgid).unwrap_or_else(|| msgid.to_string())
}

/// Returns a lazy translation that defers `gettext` until the value is used.
///
/// The returned `LazyString` evaluates the translation at the time
/// `Display::fmt` or `Deref<Target=str>` is called, using whatever
/// language is active on the calling thread at that moment.
///
/// # Examples
///
/// ```
/// use django_rs_core::i18n;
///
/// let lazy = i18n::gettext_lazy("Hello");
/// // Translation is not evaluated yet
///
/// i18n::catalog::register_translations("ja", vec![("Hello", "こんにちは")]);
/// i18n::activate("ja");
/// assert_eq!(lazy.to_string(), "こんにちは");
/// i18n::deactivate();
/// ```
pub fn gettext_lazy(msgid: &str) -> lazy::LazyString {
    lazy::LazyString::new(msgid.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    // Reset state between tests to avoid inter-test interference.
    fn setup() {
        deactivate();
    }

    #[test]
    fn test_activate_and_get_language() {
        setup();
        assert_eq!(get_language(), "en");
        activate("fr");
        assert_eq!(get_language(), "fr");
        deactivate();
        assert_eq!(get_language(), "en");
    }

    #[test]
    fn test_gettext_no_translation() {
        setup();
        assert_eq!(gettext("untranslated"), "untranslated");
    }

    #[test]
    fn test_gettext_with_translation() {
        setup();
        catalog::register_translations("es", vec![("Hello", "Hola")]);
        activate("es");
        assert_eq!(gettext("Hello"), "Hola");
        deactivate();
        // Without activation, returns original
        assert_eq!(gettext("Hello"), "Hello");
    }

    #[test]
    fn test_ngettext_no_translation() {
        setup();
        assert_eq!(ngettext("apple", "apples", 1), "apple");
        assert_eq!(ngettext("apple", "apples", 0), "apples");
        assert_eq!(ngettext("apple", "apples", 5), "apples");
    }

    #[test]
    fn test_ngettext_with_translation() {
        setup();
        catalog::register_plural_translations("fr", vec![("apple", "apples", "pomme", "pommes")]);
        activate("fr");
        assert_eq!(ngettext("apple", "apples", 1), "pomme");
        assert_eq!(ngettext("apple", "apples", 3), "pommes");
        deactivate();
    }

    #[test]
    fn test_pgettext_no_translation() {
        setup();
        assert_eq!(pgettext("context", "message"), "message");
    }

    #[test]
    fn test_pgettext_with_translation() {
        setup();
        catalog::register_context_translations(
            "de",
            vec![("month", "May", "Mai"), ("verb", "May", "Darf")],
        );
        activate("de");
        assert_eq!(pgettext("month", "May"), "Mai");
        assert_eq!(pgettext("verb", "May"), "Darf");
        deactivate();
    }

    #[test]
    fn test_gettext_lazy() {
        setup();
        catalog::register_translations("it", vec![("Goodbye", "Arrivederci")]);
        let lazy = gettext_lazy("Goodbye");

        // Before activation, returns original
        assert_eq!(lazy.to_string(), "Goodbye");

        activate("it");
        assert_eq!(lazy.to_string(), "Arrivederci");
        deactivate();
    }

    #[test]
    fn test_multiple_languages() {
        setup();
        catalog::register_translations("fr", vec![("Yes", "Oui")]);
        catalog::register_translations("de", vec![("Yes", "Ja")]);
        catalog::register_translations("es", vec![("Yes", "Sí")]);

        activate("fr");
        assert_eq!(gettext("Yes"), "Oui");

        activate("de");
        assert_eq!(gettext("Yes"), "Ja");

        activate("es");
        assert_eq!(gettext("Yes"), "Sí");

        deactivate();
        assert_eq!(gettext("Yes"), "Yes");
    }

    #[test]
    fn test_activate_unknown_language() {
        setup();
        activate("zz");
        assert_eq!(get_language(), "zz");
        // No catalog for "zz", so gettext returns original
        assert_eq!(gettext("Hello"), "Hello");
        deactivate();
    }
}
