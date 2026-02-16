//! Lazy initialization primitive.
//!
//! [`LazyObject`] defers the creation of a value until it is first accessed,
//! similar to Django's `LazyObject`. The value is computed once and then cached.

use std::ops::Deref;
use std::sync::OnceLock;

/// A lazily-initialized wrapper that creates its value on first access.
///
/// The factory function is called at most once. Subsequent accesses return
/// a reference to the cached value. `LazyObject` is `Send + Sync` as long as
/// the contained value is.
///
/// # Examples
///
/// ```
/// use django_rs_core::utils::LazyObject;
///
/// let lazy = LazyObject::new(|| {
///     // expensive computation
///     42
/// });
///
/// assert_eq!(*lazy, 42);
/// ```
pub struct LazyObject<T> {
    init: OnceLock<T>,
    factory: Box<dyn Fn() -> T + Send + Sync>,
}

impl<T: std::fmt::Debug> std::fmt::Debug for LazyObject<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.init.get() {
            Some(value) => f.debug_tuple("LazyObject").field(value).finish(),
            None => f
                .debug_tuple("LazyObject")
                .field(&"<uninitialized>")
                .finish(),
        }
    }
}

impl<T> LazyObject<T> {
    /// Creates a new `LazyObject` with the given factory function.
    ///
    /// The factory is not called until the value is first accessed.
    pub fn new(factory: impl Fn() -> T + Send + Sync + 'static) -> Self {
        Self {
            init: OnceLock::new(),
            factory: Box::new(factory),
        }
    }

    /// Returns a reference to the initialized value, calling the factory if necessary.
    pub fn get(&self) -> &T {
        self.init.get_or_init(&self.factory)
    }

    /// Returns `true` if the value has been initialized.
    pub fn is_initialized(&self) -> bool {
        self.init.get().is_some()
    }
}

impl<T> Deref for LazyObject<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        self.get()
    }
}

// SAFETY: LazyObject is Send + Sync because:
// - OnceLock<T> is Send + Sync when T: Send + Sync
// - The factory is Send + Sync (enforced by the trait bound)
// - The Box<dyn Fn() -> T + Send + Sync> is Send + Sync
// These bounds are already enforced by the struct fields, so these impls
// are automatically derived. We add explicit assertions here for clarity.
const _: () = {
    const fn assert_send_sync<T: Send + Sync>() {}
    // This will fail to compile if LazyObject<i32> is not Send + Sync
    #[allow(dead_code)]
    const fn check() {
        assert_send_sync::<LazyObject<i32>>();
    }
};

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;

    #[test]
    fn test_lazy_initialization() {
        let call_count = Arc::new(AtomicUsize::new(0));
        let count = call_count.clone();

        let lazy = LazyObject::new(move || {
            count.fetch_add(1, Ordering::SeqCst);
            "hello"
        });

        assert!(!lazy.is_initialized());
        assert_eq!(call_count.load(Ordering::SeqCst), 0);

        assert_eq!(*lazy, "hello");
        assert!(lazy.is_initialized());
        assert_eq!(call_count.load(Ordering::SeqCst), 1);

        // Second access does not call factory again
        assert_eq!(*lazy, "hello");
        assert_eq!(call_count.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn test_deref() {
        let lazy = LazyObject::new(|| String::from("world"));
        // Deref allows calling String methods
        assert_eq!(lazy.len(), 5);
        assert!(lazy.starts_with("wor"));
    }

    #[test]
    fn test_debug_uninitialized() {
        let lazy = LazyObject::new(|| 42);
        let debug = format!("{lazy:?}");
        assert!(debug.contains("uninitialized"));
    }

    #[test]
    fn test_debug_initialized() {
        let lazy = LazyObject::new(|| 42);
        let _ = *lazy; // force init
        let debug = format!("{lazy:?}");
        assert!(debug.contains("42"));
    }

    #[test]
    fn test_send_sync() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<LazyObject<String>>();
    }
}
