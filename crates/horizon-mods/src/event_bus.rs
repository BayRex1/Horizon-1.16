//! Simple synchronous event bus for NeoForge-like modding.
//! Allows posting engine events and having systems/mods subscribe to them.

use std::any::{Any, TypeId};
use std::collections::HashMap;
use std::sync::{Arc, RwLock};

/// Base trait for all events
pub trait Event: Any + Send + Sync {
    /// Whether this event can be cancelled (for future use)
    fn is_cancellable(&self) -> bool {
        false
    }
}

/// Type-erased event handler
type EventHandler = Arc<dyn Fn(&dyn Any) + Send + Sync>;

/// Central event bus
#[derive(Default)]
pub struct EventBus {
    handlers: RwLock<HashMap<TypeId, Vec<EventHandler>>>,
}

impl EventBus {
    pub fn new() -> Self {
        Self {
            handlers: RwLock::new(HashMap::new()),
        }
    }

    /// Register a handler for a specific event type
    pub fn register<E, F>(&self, handler: F)
    where
        E: Event,
        F: Fn(&E) + Send + Sync + 'static,
    {
        let handler: EventHandler = Arc::new(move |event: &dyn Any| {
            if let Some(e) = event.downcast_ref::<E>() {
                handler(e);
            }
        });

        let mut handlers = self.handlers.write().unwrap();
        handlers.entry(TypeId::of::<E>()).or_default().push(handler);
    }

    /// Post an event to all registered handlers
    pub fn post<E: Event>(&self, event: &E) {
        let handlers = self.handlers.read().unwrap();
        if let Some(list) = handlers.get(&TypeId::of::<E>()) {
            for handler in list {
                handler(event);
            }
        }
    }

    /// Get number of handlers for an event type (for debugging)
    pub fn handler_count<E: Event>(&self) -> usize {
        let handlers = self.handlers.read().unwrap();
        handlers.get(&TypeId::of::<E>()).map_or(0, |v| v.len())
    }
}

impl std::fmt::Debug for EventBus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let handlers = self.handlers.read().unwrap();
        f.debug_struct("EventBus")
            .field("event_types", &handlers.len())
            .finish()
    }
}

// Some basic engine events to test the system
#[derive(Debug, Clone)]
pub struct TickEvent {
    pub tick: u64,
}

impl Event for TickEvent {}

#[derive(Debug, Clone)]
pub struct ModLoadEvent {
    pub mod_id: String,
}

impl Event for ModLoadEvent {}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};

    #[derive(Debug)]
    struct TestEvent {
        value: i32,
    }
    impl Event for TestEvent {}

    #[test]
    fn test_event_bus() {
        let bus = EventBus::new();
        let counter = Arc::new(AtomicU32::new(0));
        let c = counter.clone();

        bus.register::<TestEvent, _>(move |e| {
            c.fetch_add(e.value as u32, Ordering::SeqCst);
        });

        bus.post(&TestEvent { value: 5 });
        bus.post(&TestEvent { value: 3 });
        assert_eq!(counter.load(Ordering::SeqCst), 8);
    }
}
