//! Mod loading context - provides access to mod container, event bus, etc.

use crate::{container::ModContainer, event_bus::EventBus};
use std::sync::Arc;

/// Context passed to mods during loading (NeoForge-style ModLoadingContext)
#[derive(Clone)]
pub struct ModLoadingContext {
    container: Arc<ModContainer>,
    event_bus: Arc<EventBus>,
}

impl ModLoadingContext {
    pub fn new(container: Arc<ModContainer>, event_bus: Arc<EventBus>) -> Self {
        Self {
            container,
            event_bus,
        }
    }

    /// Get the mod container
    pub fn get_container(&self) -> &Arc<ModContainer> {
        &self.container
    }

    /// Get the event bus
    pub fn get_event_bus(&self) -> &Arc<EventBus> {
        &self.event_bus
    }

    /// Get mod id
    pub fn get_mod_id(&self) -> &str {
        &self.container.id
    }

    /// Get mod name
    pub fn get_mod_name(&self) -> &str {
        &self.container.name
    }
}

impl std::fmt::Debug for ModLoadingContext {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ModLoadingContext")
            .field("mod_id", &self.container.id)
            .finish()
    }
}
