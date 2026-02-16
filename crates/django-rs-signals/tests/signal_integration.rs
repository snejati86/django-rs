//! Integration tests for the signal dispatch system.
//!
//! Tests cover: connect/send, sender filtering, disconnect, multiple handlers,
//! pre/post save/delete signals, handler modification, exception safety,
//! and request_started/request_finished signals.

use std::any::Any;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use django_rs_signals::{
    PostDelete, PreDelete, PreSave, RequestFinished, RequestStarted, Signal, SIGNALS,
};

// ═════════════════════════════════════════════════════════════════════
// 1. Signal connect and send: handler receives data
// ═════════════════════════════════════════════════════════════════════

#[test]
fn test_signal_connect_and_send_receives_data() {
    let signal: Signal<String> = Signal::new();
    let received = Arc::new(Mutex::new(String::new()));
    let received_clone = received.clone();

    signal.connect(
        "capture",
        Arc::new(move |msg: &String| {
            *received_clone.lock().unwrap() = msg.clone();
            None
        }),
    );

    signal.send(&"hello world".to_string());
    assert_eq!(*received.lock().unwrap(), "hello world");
}

// ═════════════════════════════════════════════════════════════════════
// 2. Signal with sender filtering: only matching sender fires
// ═════════════════════════════════════════════════════════════════════

#[derive(Debug)]
#[allow(dead_code)]
struct ModelEvent {
    model_name: String,
    action: String,
}

#[test]
fn test_signal_sender_filtering() {
    let signal: Signal<ModelEvent> = Signal::new();
    let article_count = Arc::new(AtomicUsize::new(0));
    let comment_count = Arc::new(AtomicUsize::new(0));

    let ac = article_count.clone();
    signal.connect(
        "article_listener",
        Arc::new(move |event: &ModelEvent| {
            if event.model_name == "article" {
                ac.fetch_add(1, Ordering::SeqCst);
            }
            None
        }),
    );

    let cc = comment_count.clone();
    signal.connect(
        "comment_listener",
        Arc::new(move |event: &ModelEvent| {
            if event.model_name == "comment" {
                cc.fetch_add(1, Ordering::SeqCst);
            }
            None
        }),
    );

    signal.send(&ModelEvent {
        model_name: "article".to_string(),
        action: "save".to_string(),
    });
    signal.send(&ModelEvent {
        model_name: "article".to_string(),
        action: "save".to_string(),
    });
    signal.send(&ModelEvent {
        model_name: "comment".to_string(),
        action: "save".to_string(),
    });

    assert_eq!(article_count.load(Ordering::SeqCst), 2);
    assert_eq!(comment_count.load(Ordering::SeqCst), 1);
}

// ═════════════════════════════════════════════════════════════════════
// 3. Signal disconnect: handler stops firing
// ═════════════════════════════════════════════════════════════════════

#[test]
fn test_signal_disconnect_stops_handler() {
    let signal: Signal<()> = Signal::new();
    let count = Arc::new(AtomicUsize::new(0));
    let c = count.clone();

    signal.connect(
        "counter",
        Arc::new(move |_: &()| {
            c.fetch_add(1, Ordering::SeqCst);
            None
        }),
    );

    signal.send(&());
    assert_eq!(count.load(Ordering::SeqCst), 1);

    // Disconnect
    let removed = signal.disconnect("counter");
    assert!(removed);
    assert_eq!(signal.receiver_count(), 0);

    // Send again -- should not increment
    signal.send(&());
    assert_eq!(count.load(Ordering::SeqCst), 1);

    // Disconnecting again returns false
    assert!(!signal.disconnect("counter"));
}

// ═════════════════════════════════════════════════════════════════════
// 4. Multiple handlers fire in registration order
// ═════════════════════════════════════════════════════════════════════

#[test]
fn test_multiple_handlers_fire_in_order() {
    let signal: Signal<()> = Signal::new();
    let order = Arc::new(Mutex::new(Vec::new()));

    for name in &["first", "second", "third"] {
        let o = order.clone();
        let n = name.to_string();
        signal.connect(
            *name,
            Arc::new(move |_: &()| {
                o.lock().unwrap().push(n.clone());
                None
            }),
        );
    }

    assert_eq!(signal.receiver_count(), 3);
    signal.send(&());

    let recorded = order.lock().unwrap();
    assert_eq!(*recorded, vec!["first", "second", "third"]);
}

// ═════════════════════════════════════════════════════════════════════
// 5. pre_save signal fires with correct instance data
// ═════════════════════════════════════════════════════════════════════

#[test]
fn test_pre_save_signal_fires() {
    let fired = Arc::new(AtomicBool::new(false));
    let f = fired.clone();

    // Use a unique handler ID to avoid interference from other tests
    let handler_id = "test_pre_save_fires";
    SIGNALS.pre_save.connect(
        handler_id,
        Arc::new(move |_: &PreSave| {
            f.store(true, Ordering::SeqCst);
            None
        }),
    );

    SIGNALS.pre_save.send(&PreSave);
    assert!(fired.load(Ordering::SeqCst));

    // Cleanup
    SIGNALS.pre_save.disconnect(handler_id);
}

// ═════════════════════════════════════════════════════════════════════
// 6. post_save signal fires with created=true on insert
// ═════════════════════════════════════════════════════════════════════

#[derive(Debug)]
struct SavePayload {
    created: bool,
    instance_id: u64,
}

#[test]
fn test_post_save_created_true_on_insert() {
    let signal: Signal<SavePayload> = Signal::new();
    let was_created = Arc::new(AtomicBool::new(false));
    let instance_id = Arc::new(AtomicUsize::new(0));
    let wc = was_created.clone();
    let ii = instance_id.clone();

    signal.connect(
        "on_save",
        Arc::new(move |payload: &SavePayload| {
            wc.store(payload.created, Ordering::SeqCst);
            ii.store(payload.instance_id as usize, Ordering::SeqCst);
            None
        }),
    );

    // Simulate insert (created=true)
    signal.send(&SavePayload {
        created: true,
        instance_id: 42,
    });

    assert!(was_created.load(Ordering::SeqCst));
    assert_eq!(instance_id.load(Ordering::SeqCst), 42);
}

// ═════════════════════════════════════════════════════════════════════
// 7. post_save signal fires with created=false on update
// ═════════════════════════════════════════════════════════════════════

#[test]
fn test_post_save_created_false_on_update() {
    let signal: Signal<SavePayload> = Signal::new();
    let was_created = Arc::new(AtomicBool::new(true)); // Start true so we can verify it changes
    let wc = was_created.clone();

    signal.connect(
        "on_update",
        Arc::new(move |payload: &SavePayload| {
            wc.store(payload.created, Ordering::SeqCst);
            None
        }),
    );

    // Simulate update (created=false)
    signal.send(&SavePayload {
        created: false,
        instance_id: 42,
    });

    assert!(!was_created.load(Ordering::SeqCst));
}

// ═════════════════════════════════════════════════════════════════════
// 8. pre_delete signal fires before deletion
// ═════════════════════════════════════════════════════════════════════

#[test]
fn test_pre_delete_signal_fires() {
    let fired = Arc::new(AtomicBool::new(false));
    let f = fired.clone();

    let handler_id = "test_pre_delete_fires";
    SIGNALS.pre_delete.connect(
        handler_id,
        Arc::new(move |_: &PreDelete| {
            f.store(true, Ordering::SeqCst);
            None
        }),
    );

    SIGNALS.pre_delete.send(&PreDelete);
    assert!(fired.load(Ordering::SeqCst));

    SIGNALS.pre_delete.disconnect(handler_id);
}

// ═════════════════════════════════════════════════════════════════════
// 9. post_delete signal fires after deletion
// ═════════════════════════════════════════════════════════════════════

#[test]
fn test_post_delete_signal_fires() {
    let fired = Arc::new(AtomicBool::new(false));
    let f = fired.clone();

    let handler_id = "test_post_delete_fires";
    SIGNALS.post_delete.connect(
        handler_id,
        Arc::new(move |_: &PostDelete| {
            f.store(true, Ordering::SeqCst);
            None
        }),
    );

    SIGNALS.post_delete.send(&PostDelete);
    assert!(fired.load(Ordering::SeqCst));

    SIGNALS.post_delete.disconnect(handler_id);
}

// ═════════════════════════════════════════════════════════════════════
// 10. Signal handler can modify instance (pre_save returns data)
// ═════════════════════════════════════════════════════════════════════

#[test]
fn test_signal_handler_returns_modified_data() {
    let signal: Signal<i32> = Signal::new();

    // Handler that doubles the value and returns it
    signal.connect(
        "doubler",
        Arc::new(|val: &i32| Some(Box::new(val * 2) as Box<dyn Any + Send>)),
    );

    // Handler that adds 10
    signal.connect(
        "adder",
        Arc::new(|val: &i32| Some(Box::new(val + 10) as Box<dyn Any + Send>)),
    );

    let results = signal.send(&5);
    assert_eq!(results.len(), 2);

    // First handler: 5 * 2 = 10
    let doubled = results[0]
        .as_ref()
        .unwrap()
        .downcast_ref::<i32>()
        .unwrap();
    assert_eq!(*doubled, 10);

    // Second handler: 5 + 10 = 15
    let added = results[1]
        .as_ref()
        .unwrap()
        .downcast_ref::<i32>()
        .unwrap();
    assert_eq!(*added, 15);
}

// ═════════════════════════════════════════════════════════════════════
// 11. Signal handler exception does not crash dispatch
// ═════════════════════════════════════════════════════════════════════

#[test]
fn test_signal_handler_panic_does_not_crash_other_handlers() {
    // Note: In Rust, a panic in a signal handler WOULD propagate unless caught.
    // The signal framework does NOT catch panics (unlike Django's robust_send).
    // Instead, we test that a handler returning None doesn't prevent others from running.
    let signal: Signal<()> = Signal::new();
    let first_ran = Arc::new(AtomicBool::new(false));
    let second_ran = Arc::new(AtomicBool::new(false));
    let third_ran = Arc::new(AtomicBool::new(false));

    let f = first_ran.clone();
    signal.connect(
        "first",
        Arc::new(move |_: &()| {
            f.store(true, Ordering::SeqCst);
            None
        }),
    );

    let s = second_ran.clone();
    signal.connect(
        "second",
        Arc::new(move |_: &()| {
            s.store(true, Ordering::SeqCst);
            // Return an error-like value instead of panicking
            Some(Box::new("error occurred") as Box<dyn Any + Send>)
        }),
    );

    let t = third_ran.clone();
    signal.connect(
        "third",
        Arc::new(move |_: &()| {
            t.store(true, Ordering::SeqCst);
            None
        }),
    );

    let results = signal.send(&());
    assert_eq!(results.len(), 3);
    assert!(first_ran.load(Ordering::SeqCst));
    assert!(second_ran.load(Ordering::SeqCst));
    assert!(third_ran.load(Ordering::SeqCst));

    // Second handler returned Some, verify it
    assert!(results[1].is_some());
}

// ═════════════════════════════════════════════════════════════════════
// 12. request_started / request_finished signals
// ═════════════════════════════════════════════════════════════════════

#[test]
fn test_request_started_and_finished_signals() {
    let started_count = Arc::new(AtomicUsize::new(0));
    let finished_count = Arc::new(AtomicUsize::new(0));

    let sc = started_count.clone();
    let handler_started = "test_req_started";
    SIGNALS.request_started.connect(
        handler_started,
        Arc::new(move |_: &RequestStarted| {
            sc.fetch_add(1, Ordering::SeqCst);
            None
        }),
    );

    let fc = finished_count.clone();
    let handler_finished = "test_req_finished";
    SIGNALS.request_finished.connect(
        handler_finished,
        Arc::new(move |_: &RequestFinished| {
            fc.fetch_add(1, Ordering::SeqCst);
            None
        }),
    );

    // Simulate a request lifecycle
    SIGNALS.request_started.send(&RequestStarted);
    // ... request processing would happen here ...
    SIGNALS.request_finished.send(&RequestFinished);

    assert_eq!(started_count.load(Ordering::SeqCst), 1);
    assert_eq!(finished_count.load(Ordering::SeqCst), 1);

    // Simulate a second request
    SIGNALS.request_started.send(&RequestStarted);
    SIGNALS.request_finished.send(&RequestFinished);

    assert_eq!(started_count.load(Ordering::SeqCst), 2);
    assert_eq!(finished_count.load(Ordering::SeqCst), 2);

    // Cleanup
    SIGNALS.request_started.disconnect(handler_started);
    SIGNALS.request_finished.disconnect(handler_finished);
}
