//! Stateful reactive value (signal / BehaviorSubject pattern).
//!
//! Holds a current value and notifies listeners when it changes.

use std::sync::Arc;

use super::event_stream::{EventSubscription, EventStream};

/// A reactive container that holds a value and notifies subscribers on change.
pub struct Signal<T: 'static> {
    value: Arc<std::sync::Mutex<T>>,
    stream: EventStream<T>,
}

impl<T: Clone + 'static> Signal<T> {
    pub fn new(initial: T) -> Self {
        Self {
            value: Arc::new(std::sync::Mutex::new(initial)),
            stream: EventStream::new(),
        }
    }

    /// Get the current value.
    pub fn get(&self) -> T {
        self.value.lock().unwrap().clone()
    }

    /// Set a new value and notify all subscribers.
    pub fn set(&self, new_value: T) {
        {
            let mut guard = self.value.lock().unwrap();
            *guard = new_value.clone();
        }
        self.stream.emit(&new_value);
    }

    /// Subscribe to value changes. Fires on every `set()`.
    pub fn subscribe<F>(&self, callback: F) -> EventSubscription
    where
        F: Fn(&T) + Send + Sync + 'static,
    {
        self.stream.on(callback)
    }

    /// Subscribe and immediately fire with the current value.
    #[allow(dead_code)]
    pub fn subscribe_with_initial<F>(&self, callback: F) -> EventSubscription
    where
        F: Fn(&T) + Send + Sync + 'static,
    {
        let current = self.get();
        callback(&current);
        self.stream.on(callback)
    }
}

impl<T: Clone + 'static> Clone for Signal<T> {
    fn clone(&self) -> Self {
        Self {
            value: Arc::clone(&self.value),
            stream: self.stream.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_set_and_get() {
        let signal = Signal::new(0i32);
        assert_eq!(signal.get(), 0);
        signal.set(42);
        assert_eq!(signal.get(), 42);
    }

    #[test]
    fn test_notifies_on_change() {
        let signal = Signal::new(0i32);
        let received = Arc::new(std::sync::Mutex::new(Vec::new()));
        let r = Arc::clone(&received);

        let _sub = signal.subscribe(move |v: &i32| {
            r.lock().unwrap().push(*v);
        });

        signal.set(1);
        signal.set(2);
        signal.set(3);
        assert_eq!(*received.lock().unwrap(), vec![1, 2, 3]);
    }
}
