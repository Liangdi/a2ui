//! Simple pub/sub event stream for A2UI state models.
//!
//! Supports multiple listeners and provides disposable subscriptions.

use std::sync::Arc;

/// A subscription handle — unsubscribes on drop.
pub struct EventSubscription {
    slot: usize,
    drop_fn: Box<dyn Fn(usize) + Send + Sync>,
}

impl Drop for EventSubscription {
    fn drop(&mut self) {
        (self.drop_fn)(self.slot);
    }
}

type Listener<T> = Box<dyn Fn(&T) + Send + Sync>;

/// A simple multi-cast event stream.
///
/// Listeners are called synchronously when `emit()` is invoked.
/// Clone-safe — cloning shares the underlying listener list.
pub struct EventStream<T: 'static> {
    listeners: Arc<std::sync::Mutex<Vec<Option<Listener<T>>>>>,
    next_id: Arc<std::sync::Mutex<usize>>,
}

impl<T: 'static> Default for EventStream<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T: 'static> EventStream<T> {
    pub fn new() -> Self {
        Self {
            listeners: Arc::new(std::sync::Mutex::new(Vec::new())),
            next_id: Arc::new(std::sync::Mutex::new(0)),
        }
    }

    /// Subscribe to events. Returns an `EventSubscription` that unsubscribes on drop.
    pub fn on<F>(&self, listener: F) -> EventSubscription
    where
        F: Fn(&T) + Send + Sync + 'static,
    {
        let id = {
            let mut next = self.next_id.lock().unwrap();
            let id = *next;
            *next += 1;
            id
        };

        {
            let mut guard = self.listeners.lock().unwrap();
            if id >= guard.len() {
                guard.resize_with(id + 1, || None);
            }
            guard[id] = Some(Box::new(listener));
        }

        let listeners = Arc::clone(&self.listeners);
        EventSubscription {
            slot: id,
            drop_fn: Box::new(move |slot: usize| {
                let mut guard = listeners.lock().unwrap();
                if slot < guard.len() {
                    guard[slot] = None;
                }
            }),
        }
    }

    /// Emit an event to all active listeners.
    pub fn emit(&self, event: &T) {
        let guard = self.listeners.lock().unwrap();
        for listener in guard.iter().flatten() {
            listener(event);
        }
    }

    /// Returns the number of active listeners.
    #[allow(dead_code)]
    pub fn listener_count(&self) -> usize {
        self.listeners.lock().unwrap().iter().flatten().count()
    }
}

impl<T: 'static> Clone for EventStream<T> {
    fn clone(&self) -> Self {
        Self {
            listeners: Arc::clone(&self.listeners),
            next_id: Arc::clone(&self.next_id),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[test]
    fn test_subscribe_and_emit() {
        let stream: EventStream<i32> = EventStream::new();
        let count = Arc::new(AtomicUsize::new(0));
        let c = Arc::clone(&count);

        let _sub = stream.on(move |val: &i32| {
            if *val == 42 {
                c.fetch_add(1, Ordering::SeqCst);
            }
        });

        stream.emit(&42);
        stream.emit(&10);
        stream.emit(&42);
        assert_eq!(count.load(Ordering::SeqCst), 2);
    }

    #[test]
    fn test_unsubscribe_on_drop() {
        let stream: EventStream<i32> = EventStream::new();
        let count = Arc::new(AtomicUsize::new(0));

        {
            let c = Arc::clone(&count);
            let sub = stream.on(move |_: &i32| {
                c.fetch_add(1, Ordering::SeqCst);
            });
            stream.emit(&1);
            assert_eq!(count.load(Ordering::SeqCst), 1);
            drop(sub);
        }

        stream.emit(&1);
        assert_eq!(count.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn test_multiple_listeners() {
        let stream: EventStream<i32> = EventStream::new();
        let a = Arc::new(AtomicUsize::new(0));
        let b = Arc::new(AtomicUsize::new(0));

        let ac = Arc::clone(&a);
        let _sa = stream.on(move |_: &i32| {
            ac.fetch_add(1, Ordering::SeqCst);
        });
        let bc = Arc::clone(&b);
        let _sb = stream.on(move |_: &i32| {
            bc.fetch_add(1, Ordering::SeqCst);
        });

        stream.emit(&1);
        assert_eq!(a.load(Ordering::SeqCst), 1);
        assert_eq!(b.load(Ordering::SeqCst), 1);
    }
}
