# Signals

The signal system provides decoupled event dispatch, allowing components to send and receive notifications without direct dependencies. This mirrors Django's `django.dispatch.Signal`.

---

## Overview

The signal system lives in the `django-rs-signals` crate. A `Signal<T>` holds a list of receivers that are called when the signal is sent. Receivers are `Arc`-wrapped closures that are `Send + Sync`, so signals can be dispatched from any thread.

```rust
use django_rs_signals::Signal;
use std::sync::Arc;

let signal: Signal<String> = Signal::new();

signal.connect("my_handler", Arc::new(|msg: &String| {
    println!("Received: {msg}");
    None
}));

signal.send(&"hello".to_string());
```

---

## Signal API

### Creating a signal

```rust
use django_rs_signals::Signal;

// Typed signal -- receivers get a reference to the payload
let signal: Signal<MyEvent> = Signal::new();
```

### Connecting a receiver

```rust
use std::sync::Arc;

signal.connect("unique_id", Arc::new(|event: &MyEvent| {
    // Handle the event
    None // Return None or Some(Box<dyn Any + Send>)
}));
```

Each receiver has a string ID. If you connect a receiver with an ID that already exists, the old receiver is replaced.

### Disconnecting a receiver

```rust
let was_removed = signal.disconnect("unique_id");
assert!(was_removed);
```

### Sending a signal

```rust
let results = signal.send(&my_event);
// results: Vec<Option<Box<dyn Any + Send>>>
```

Receivers are called in the order they were connected. Each receiver can optionally return a value.

### Receiver count

```rust
let count = signal.receiver_count();
```

---

## Built-in signals

django-rs provides pre-defined signal types for common framework events:

| Signal | Type | When Fired |
|--------|------|------------|
| `pre_save` | `Signal<PreSave>` | Before a model instance is saved |
| `post_save` | `Signal<PostSave>` | After a model instance is saved |
| `pre_delete` | `Signal<PreDelete>` | Before a model instance is deleted |
| `post_delete` | `Signal<PostDelete>` | After a model instance is deleted |
| `pre_init` | `Signal<PreInit>` | Before a model instance is initialized |
| `post_init` | `Signal<PostInit>` | After a model instance is initialized |
| `request_started` | `Signal<RequestStarted>` | When an HTTP request begins processing |
| `request_finished` | `Signal<RequestFinished>` | When an HTTP request finishes processing |

### Global signal registry

All built-in signals are accessible through the global `SIGNALS` registry:

```rust
use django_rs_signals::{SIGNALS, PostSave, RequestStarted};
use std::sync::Arc;

// Log every request
SIGNALS.request_started.connect("request_logger", Arc::new(|_: &RequestStarted| {
    println!("Request started");
    None
}));

// React to model saves
SIGNALS.post_save.connect("cache_invalidator", Arc::new(|_: &PostSave| {
    println!("Model saved, invalidating cache");
    None
}));
```

### Custom signals

You can create custom named signals through the registry:

```rust
use django_rs_signals::SIGNALS;
use std::sync::Arc;
use std::any::Any;

// Get or create a custom signal
let payment_signal = SIGNALS.get_or_create_custom("payment_completed");

// Connect a receiver
payment_signal.connect("email_notifier", Arc::new(|payload: &Box<dyn Any + Send + Sync>| {
    println!("Payment completed!");
    None
}));

// Send the signal
let payload: Box<dyn Any + Send + Sync> = Box::new("order_123".to_string());
payment_signal.send(&payload);
```

---

## Practical examples

### Audit logging

```rust
use django_rs_signals::{SIGNALS, PostSave, PostDelete};
use std::sync::Arc;

SIGNALS.post_save.connect("audit_log_save", Arc::new(|_: &PostSave| {
    // Log the save event to an audit table
    println!("[AUDIT] Model instance saved");
    None
}));

SIGNALS.post_delete.connect("audit_log_delete", Arc::new(|_: &PostDelete| {
    println!("[AUDIT] Model instance deleted");
    None
}));
```

### Cache invalidation

```rust
use django_rs_signals::{SIGNALS, PostSave};
use std::sync::Arc;

SIGNALS.post_save.connect("cache_clear", Arc::new(|_: &PostSave| {
    // Clear the relevant cache entries
    println!("Clearing cache after model save");
    None
}));
```

### Request timing

```rust
use django_rs_signals::{SIGNALS, RequestStarted, RequestFinished};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

static REQUEST_COUNT: AtomicU64 = AtomicU64::new(0);

SIGNALS.request_started.connect("counter", Arc::new(|_: &RequestStarted| {
    REQUEST_COUNT.fetch_add(1, Ordering::Relaxed);
    None
}));
```

---

## Comparison with Django

| Django (Python) | django-rs (Rust) |
|-----------------|------------------|
| `from django.dispatch import Signal` | `use django_rs_signals::Signal;` |
| `my_signal = Signal()` | `let my_signal: Signal<MyType> = Signal::new();` |
| `my_signal.connect(receiver)` | `signal.connect("id", Arc::new(\|...\| ...))` |
| `my_signal.disconnect(receiver)` | `signal.disconnect("id")` |
| `my_signal.send(sender=self)` | `signal.send(&payload)` |
| `from django.db.models.signals import post_save` | `use django_rs_signals::{SIGNALS, PostSave};` |
| `post_save.connect(handler, sender=MyModel)` | `SIGNALS.post_save.connect("handler", ...)` |

Key differences:
- **Type safety** -- Signals carry a typed payload `T` rather than `**kwargs`
- **String IDs** -- Receivers are identified by string IDs rather than function references
- **Thread safety** -- All receivers must be `Send + Sync` (enforced at compile time)
- **Return values** -- Receivers can return `Option<Box<dyn Any + Send>>` rather than arbitrary tuples
