//! NeoForge-like mod container infrastructure.
//! Provides a centralized representation of a mod with its metadata,
//! dependencies, and loading state.

use crate::manifest::{Capability, Manifest, Side};
use std::collections::BTreeMap;
use std::path::PathBuf;

/// The state of a mod in the loading lifecycle
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ModState {
    /// Mod discovered but not yet validated
    Discovered,
    /// Mod passed validation
    Validated,
    /// Mod is being constructed/initialized
    Constructing,
    /// Mod is in setup phase
    Setup,
    /// Mod is fully loaded and running
    Loaded,
    /// Mod failed to load
    Errored(String),
    /// Mod is disabled
    Disabled,
}

/// Dependency specification
#[derive(Clone, Debug)]
pub struct ModDependency {
    pub mod_id: String,
    pub version_range: String,
    pub required: bool,
    pub ordering: DependencyOrdering,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DependencyOrdering {
    /// This mod must load after the dependency
    After,
    /// This mod must load before the dependency  
    Before,
    /// No ordering constraint
    None,
}

/// Central representation of a mod (NeoForge-style ModContainer)
#[derive(Clone, Debug)]
pub struct ModContainer {
    /// Unique mod identifier
    pub id: String,
    /// Display name
    pub name: String,
    /// Mod version
    pub version: String,
    /// Description
    pub description: String,
    /// Authors
    pub authors: Vec<String>,
    /// Path to mod file/folder
    pub mod_path: PathBuf,
    /// Side this mod runs on
    pub side: Side,
    /// API version
    pub api_version: String,
    /// Capabilities requested
    pub capabilities: Vec<Capability>,
    /// Dependencies
    pub dependencies: Vec<ModDependency>,
    /// Current state
    pub state: ModState,
    /// Custom properties
    pub properties: BTreeMap<String, String>,
}

impl ModContainer {
    /// Create a new ModContainer from a Manifest
    pub fn from_manifest(manifest: Manifest, mod_path: PathBuf) -> Self {
        Self {
            id: manifest.id.clone(),
            name: manifest.name.clone(),
            version: manifest.version.clone(),
            description: manifest.description.clone(),
            authors: manifest.authors.clone(),
            mod_path,
            side: manifest.side,
            api_version: manifest.api.clone(),
            capabilities: manifest.capabilities.clone(),
            dependencies: Vec::new(),
            state: ModState::Discovered,
            properties: BTreeMap::new(),
        }
    }

    /// Get the mod's display string
    pub fn display_name(&self) -> String {
        format!("{} ({}) v{}", self.name, self.id, self.version)
    }

    /// Check if mod has a specific capability
    pub fn has_capability(&self) -> bool {
        // This is a placeholder for future API
        true
    }

    /// Check if this mod can run on the given side
    pub fn runs_on_side(&self, is_server: bool) -> bool {
        match self.side {
            Side::Client => !is_server,
            Side::Server => is_server,
            Side::Both => true,
        }
    }

    /// Set a custom property
    pub fn set_property(&mut self, key: impl Into<String>, value: impl Into<String>) {
        self.properties.insert(key.into(), value.into());
    }

    /// Get a custom property
    pub fn get_property(&self, key: &str) -> Option<&String> {
        self.properties.get(key)
    }

    /// Transition to a new state
    pub fn set_state(&mut self, state: ModState) {
        self.state = state;
    }

    /// Check if mod is in a running state
    pub fn is_running(&self) -> bool {
        matches!(self.state, ModState::Loaded)
    }

    /// Check if mod has errored
    pub fn has_error(&self) -> bool {
        matches!(self.state, ModState::Errored(_))
    }
}

impl std::fmt::Display for ModContainer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} [{}] v{} ({:?})", self.name, self.id, self.version, self.state)
    }
}
