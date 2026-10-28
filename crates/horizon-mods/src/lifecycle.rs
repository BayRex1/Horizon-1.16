//! Mod loading lifecycle stages (NeoForge-like)

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LoadingStage {
    /// Mod instance being constructed
    Construct,
    /// Common setup - registries, etc.
    CommonSetup,
    /// Sided setup (client/server specific)
    SidedSetup,
    /// Load complete
    LoadComplete,
    /// Post load
    PostLoad,
}

impl LoadingStage {
    pub fn name(&self) -> &'static str {
        match self {
            LoadingStage::Construct => "CONSTRUCT",
            LoadingStage::CommonSetup => "COMMON_SETUP",
            LoadingStage::SidedSetup => "SIDED_SETUP",
            LoadingStage::LoadComplete => "LOAD_COMPLETE",
            LoadingStage::PostLoad => "POST_LOAD",
        }
    }
}

#[derive(Debug)]
pub struct LifecycleManager {
    current_stage: LoadingStage,
}

impl Default for LifecycleManager {
    fn default() -> Self {
        Self {
            current_stage: LoadingStage::Construct,
        }
    }
}

impl LifecycleManager {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn current_stage(&self) -> LoadingStage {
        self.current_stage
    }

    pub fn advance(&mut self, stage: LoadingStage) {
        log::debug!("Advancing lifecycle to stage: {}", stage.name());
        self.current_stage = stage;
    }
}
