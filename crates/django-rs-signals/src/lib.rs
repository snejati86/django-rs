//! # django-rs-signals
//!
//! Signal dispatcher for the django-rs framework. Provides a decoupled event system
//! allowing components to send and receive notifications without direct dependencies.
//! Supports pre/post save, pre/post delete, request started/finished, and custom signals.
//!
//! ## Usage
//!
//! ```
//! use django_rs_signals::Signal;
//! use std::sync::Arc;
//!
//! struct UserCreated;
//!
//! let signal: Signal<UserCreated> = Signal::new();
//!
//! signal.connect("my_handler", Arc::new(|_sender: &UserCreated| {
//!     println!("A user was created!");
//!     None
//! }));
//!
//! let results = signal.send(&UserCreated);
//! assert_eq!(results.len(), 1);
//! ```

use std::any::Any;
use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use once_cell::sync::Lazy;

/// The type signature for a signal receiver callback.
///
/// Receivers accept a reference to the signal payload and may optionally
/// return a boxed value. Receivers must be `Send + Sync` so that signals
/// can be dispatched from any thread.
pub type SignalReceiver<T> = Arc<dyn Fn(&T) -> Option<Box<dyn Any + Send>> + Send + Sync>;

/// A signal that can be connected to and dispatched.
///
/// Each signal carries a payload type `T`. Receivers are called in the order
/// they were connected.
///
/// # Examples
///
/// ```
/// use django_rs_signals::Signal;
/// use std::sync::Arc;
///
/// let signal: Signal<String> = Signal::new();
///
/// signal.connect("logger", Arc::new(|msg: &String| {
///     println!("Received: {msg}");
///     None
/// }));
///
/// signal.send(&"hello".to_string());
/// ```
pub struct Signal<T: 'static> {
    receivers: RwLock<Vec<(String, SignalReceiver<T>)>>,
}

impl<T: 'static> Default for Signal<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T: 'static> Signal<T> {
    /// Creates a new signal with no connected receivers.
    pub fn new() -> Self {
        Self {
            receivers: RwLock::new(Vec::new()),
        }
    }

    /// Connects a receiver to this signal.
    ///
    /// The `receiver_id` is used to identify the receiver for later disconnection.
    /// If a receiver with the same ID is already connected, it is replaced.
    pub fn connect(&self, receiver_id: impl Into<String>, callback: SignalReceiver<T>) {
        let id = receiver_id.into();
        let mut receivers = self.receivers.write().expect("signal lock poisoned");

        // Replace if already connected with this ID
        if let Some(entry) = receivers.iter_mut().find(|(rid, _)| *rid == id) {
            entry.1 = callback;
        } else {
            receivers.push((id, callback));
        }
    }

    /// Disconnects the receiver with the given ID.
    ///
    /// Returns `true` if a receiver was found and removed.
    pub fn disconnect(&self, receiver_id: &str) -> bool {
        let mut receivers = self.receivers.write().expect("signal lock poisoned");
        let len_before = receivers.len();
        receivers.retain(|(id, _)| id != receiver_id);
        receivers.len() < len_before
    }

    /// Sends the signal to all connected receivers.
    ///
    /// Receivers are called in connection order. Returns a vector of the
    /// return values from each receiver.
    pub fn send(&self, sender: &T) -> Vec<Option<Box<dyn Any + Send>>> {
        let receivers = self.receivers.read().expect("signal lock poisoned");
        receivers
            .iter()
            .map(|(_, callback)| callback(sender))
            .collect()
    }

    /// Returns the number of connected receivers.
    pub fn receiver_count(&self) -> usize {
        self.receivers.read().expect("signal lock poisoned").len()
    }
}

// ── Pre-defined signal types ─────────────────────────────────────────

/// Signal sent before a model instance is saved.
pub struct PreSave;

/// Signal sent after a model instance is saved.
pub struct PostSave;

/// Signal sent before a model instance is deleted.
pub struct PreDelete;

/// Signal sent after a model instance is deleted.
pub struct PostDelete;

/// Signal sent before a model instance is initialized.
pub struct PreInit;

/// Signal sent after a model instance is initialized.
pub struct PostInit;

/// Signal sent when an HTTP request begins processing.
pub struct RequestStarted;

/// Signal sent when an HTTP request finishes processing.
pub struct RequestFinished;

// ── Global signal registry ───────────────────────────────────────────

/// A type-erased signal that can carry any payload.
pub type DynSignal = Signal<Box<dyn Any + Send + Sync>>;

/// Storage type for named custom signals.
type CustomSignalMap = RwLock<HashMap<String, Arc<DynSignal>>>;

/// A global registry holding well-known signals.
///
/// Access pre-defined signals through this struct's fields.
pub struct SignalRegistry {
    /// Fired before a model is saved.
    pub pre_save: Signal<PreSave>,
    /// Fired after a model is saved.
    pub post_save: Signal<PostSave>,
    /// Fired before a model is deleted.
    pub pre_delete: Signal<PreDelete>,
    /// Fired after a model is deleted.
    pub post_delete: Signal<PostDelete>,
    /// Fired before a model is initialized.
    pub pre_init: Signal<PreInit>,
    /// Fired after a model is initialized.
    pub post_init: Signal<PostInit>,
    /// Fired when a request starts.
    pub request_started: Signal<RequestStarted>,
    /// Fired when a request finishes.
    pub request_finished: Signal<RequestFinished>,
    /// Custom named signals.
    custom: CustomSignalMap,
}

impl SignalRegistry {
    /// Creates a new signal registry with all pre-defined signals.
    fn new() -> Self {
        Self {
            pre_save: Signal::new(),
            post_save: Signal::new(),
            pre_delete: Signal::new(),
            post_delete: Signal::new(),
            pre_init: Signal::new(),
            post_init: Signal::new(),
            request_started: Signal::new(),
            request_finished: Signal::new(),
            custom: RwLock::new(HashMap::new()),
        }
    }

    /// Returns a custom named signal, creating it if it does not exist.
    pub fn get_or_create_custom(&self, name: &str) -> Arc<DynSignal> {
        {
            let custom = self.custom.read().expect("signal registry lock poisoned");
            if let Some(signal) = custom.get(name) {
                return Arc::clone(signal);
            }
        }

        let mut custom = self.custom.write().expect("signal registry lock poisoned");
        Arc::clone(
            custom
                .entry(name.to_string())
                .or_insert_with(|| Arc::new(Signal::new())),
        )
    }
}

/// The global signal registry instance.
///
/// Use this to connect to and dispatch well-known framework signals.
///
/// # Examples
///
/// ```
/// use django_rs_signals::{SIGNALS, RequestStarted};
/// use std::sync::Arc;
///
/// SIGNALS.request_started.connect("my_handler", Arc::new(|_: &RequestStarted| {
///     println!("Request started!");
///     None
/// }));
/// ```
pub static SIGNALS: Lazy<SignalRegistry> = Lazy::new(SignalRegistry::new);

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[test]
    fn test_signal_connect_and_send() {
        let signal: Signal<String> = Signal::new();
        let count = Arc::new(AtomicUsize::new(0));
        let count_clone = count.clone();

        signal.connect(
            "counter",
            Arc::new(move |_: &String| {
                count_clone.fetch_add(1, Ordering::SeqCst);
                None
            }),
        );

        let results = signal.send(&"hello".to_string());
        assert_eq!(results.len(), 1);
        assert_eq!(count.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn test_signal_multiple_receivers() {
        let signal: Signal<i32> = Signal::new();
        let count = Arc::new(AtomicUsize::new(0));

        for i in 0..3 {
            let c = count.clone();
            signal.connect(
                format!("receiver_{i}"),
                Arc::new(move |_: &i32| {
                    c.fetch_add(1, Ordering::SeqCst);
                    None
                }),
            );
        }

        assert_eq!(signal.receiver_count(), 3);

        let results = signal.send(&42);
        assert_eq!(results.len(), 3);
        assert_eq!(count.load(Ordering::SeqCst), 3);
    }

    #[test]
    fn test_signal_disconnect() {
        let signal: Signal<()> = Signal::new();

        signal.connect("a", Arc::new(|(): &()| None));
        signal.connect("b", Arc::new(|(): &()| None));
        assert_eq!(signal.receiver_count(), 2);

        assert!(signal.disconnect("a"));
        assert_eq!(signal.receiver_count(), 1);

        assert!(!signal.disconnect("nonexistent"));
        assert_eq!(signal.receiver_count(), 1);
    }

    #[test]
    fn test_signal_replace_receiver() {
        let signal: Signal<()> = Signal::new();
        let count = Arc::new(AtomicUsize::new(0));
        let count_clone = count.clone();

        signal.connect("handler", Arc::new(|(): &()| None));
        signal.connect(
            "handler",
            Arc::new(move |(): &()| {
                count_clone.fetch_add(1, Ordering::SeqCst);
                None
            }),
        );

        assert_eq!(signal.receiver_count(), 1);
        signal.send(&());
        assert_eq!(count.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn test_signal_return_values() {
        let signal: Signal<i32> = Signal::new();

        signal.connect(
            "doubler",
            Arc::new(|val: &i32| Some(Box::new(val * 2) as Box<dyn Any + Send>)),
        );
        signal.connect("none", Arc::new(|_: &i32| None));

        let results = signal.send(&21);
        assert_eq!(results.len(), 2);

        let first = results[0].as_ref().unwrap();
        let doubled = first.downcast_ref::<i32>().unwrap();
        assert_eq!(*doubled, 42);

        assert!(results[1].is_none());
    }

    #[test]
    fn test_empty_signal_send() {
        let signal: Signal<()> = Signal::new();
        let results = signal.send(&());
        assert!(results.is_empty());
    }

    #[test]
    fn test_global_signals_registry() {
        let count = Arc::new(AtomicUsize::new(0));
        let c = count.clone();

        SIGNALS.request_started.connect(
            "test_global",
            Arc::new(move |_: &RequestStarted| {
                c.fetch_add(1, Ordering::SeqCst);
                None
            }),
        );

        SIGNALS.request_started.send(&RequestStarted);
        assert_eq!(count.load(Ordering::SeqCst), 1);

        // Clean up
        SIGNALS.request_started.disconnect("test_global");
    }

    #[test]
    fn test_custom_signal_registry() {
        let signal = SIGNALS.get_or_create_custom("my_custom_event");
        assert_eq!(signal.receiver_count(), 0);

        // Same name returns the same signal
        let signal2 = SIGNALS.get_or_create_custom("my_custom_event");
        let count = Arc::new(AtomicUsize::new(0));
        let c = count.clone();

        signal.connect(
            "handler",
            Arc::new(move |_: &Box<dyn Any + Send + Sync>| {
                c.fetch_add(1, Ordering::SeqCst);
                None
            }),
        );

        signal2.send(&(Box::new(()) as Box<dyn Any + Send + Sync>));
        assert_eq!(count.load(Ordering::SeqCst), 1);

        // Clean up
        signal.disconnect("handler");
    }

    #[test]
    fn test_signal_default() {
        let signal: Signal<i32> = Signal::default();
        assert_eq!(signal.receiver_count(), 0);
    }
}
