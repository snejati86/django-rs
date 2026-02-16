//! Lazy translation strings.
//!
//! [`LazyString`] defers the translation lookup until the string is actually
//! displayed or dereferenced. This is useful for module-level constants or
//! model field labels that should be translated at render time, not at
//! import/definition time.

use std::fmt;

/// A lazily-translated string.
///
/// The translation is evaluated each time `Display::fmt` is called, using
/// whatever language is active on the calling thread at that moment.
/// This mirrors Django's `gettext_lazy`.
///
/// # Examples
///
/// ```
/// use django_rs_core::i18n;
/// use django_rs_core::i18n::lazy::LazyString;
///
/// let lazy = LazyString::new("Hello".to_string());
/// // Translation is not evaluated until display
/// assert_eq!(lazy.to_string(), "Hello");
///
/// i18n::catalog::register_translations("pt", vec![("Hello", "Olá")]);
/// i18n::activate("pt");
/// assert_eq!(lazy.to_string(), "Olá");
/// i18n::deactivate();
/// ```
#[derive(Clone)]
pub struct LazyString {
    msgid: String,
}

impl LazyString {
    /// Creates a new `LazyString` with the given message ID.
    pub const fn new(msgid: String) -> Self {
        Self { msgid }
    }

    /// Returns the message ID (the untranslated string).
    pub fn msgid(&self) -> &str {
        &self.msgid
    }

    /// Evaluates the translation using the current thread's active language.
    pub fn evaluate(&self) -> String {
        super::gettext(&self.msgid)
    }
}

impl fmt::Display for LazyString {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.evaluate())
    }
}

impl fmt::Debug for LazyString {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("LazyString")
            .field("msgid", &self.msgid)
            .finish()
    }
}

impl PartialEq<str> for LazyString {
    fn eq(&self, other: &str) -> bool {
        self.evaluate() == other
    }
}

impl PartialEq<String> for LazyString {
    fn eq(&self, other: &String) -> bool {
        self.evaluate() == *other
    }
}

impl PartialEq for LazyString {
    fn eq(&self, other: &Self) -> bool {
        self.msgid == other.msgid
    }
}

impl Eq for LazyString {}

impl From<LazyString> for String {
    fn from(lazy: LazyString) -> Self {
        lazy.evaluate()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_lazy_string_no_translation() {
        crate::i18n::deactivate();
        let lazy = LazyString::new("Hello".to_string());
        assert_eq!(lazy.to_string(), "Hello");
    }

    #[test]
    fn test_lazy_string_with_translation() {
        crate::i18n::catalog::register_translations("lazy_test_lang", vec![("Greet", "Saludo")]);

        let lazy = LazyString::new("Greet".to_string());
        assert_eq!(lazy.to_string(), "Greet"); // default language

        crate::i18n::activate("lazy_test_lang");
        assert_eq!(lazy.to_string(), "Saludo");
        crate::i18n::deactivate();
    }

    #[test]
    fn test_lazy_string_msgid() {
        let lazy = LazyString::new("test_msg".to_string());
        assert_eq!(lazy.msgid(), "test_msg");
    }

    #[test]
    fn test_lazy_string_debug() {
        let lazy = LazyString::new("debug_test".to_string());
        let debug = format!("{lazy:?}");
        assert!(debug.contains("LazyString"));
        assert!(debug.contains("debug_test"));
    }

    #[test]
    fn test_lazy_string_equality() {
        let a = LazyString::new("same".to_string());
        let b = LazyString::new("same".to_string());
        let c = LazyString::new("different".to_string());
        assert_eq!(a, b);
        assert_ne!(a, c);
    }

    #[test]
    fn test_lazy_string_partial_eq_str() {
        crate::i18n::deactivate();
        let lazy = LazyString::new("hello".to_string());
        assert!(lazy == *"hello");
    }

    #[test]
    fn test_lazy_string_into_string() {
        crate::i18n::deactivate();
        let lazy = LazyString::new("convert".to_string());
        let s: String = lazy.into();
        assert_eq!(s, "convert");
    }

    #[test]
    fn test_lazy_string_clone() {
        let lazy = LazyString::new("clone_me".to_string());
        let cloned = lazy.clone();
        assert_eq!(lazy.msgid(), cloned.msgid());
    }
}
