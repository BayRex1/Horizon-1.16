//! Distribution side (like NeoForge's Dist)

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Dist {
    /// Client side (single player window or multiplayer client)
    Client,
    /// Dedicated server side
    DedicatedServer,
}

impl Dist {
    pub fn is_client(&self) -> bool {
        matches!(self, Dist::Client)
    }

    pub fn is_server(&self) -> bool {
        matches!(self, Dist::DedicatedServer)
    }

    pub fn is_dedicated_server(&self) -> bool {
        matches!(self, Dist::DedicatedServer)
    }
}

impl Default for Dist {
    fn default() -> Self {
        // Most mods default to client
        Dist::Client
    }
}

impl std::fmt::Display for Dist {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Dist::Client => write!(f, "CLIENT"),
            Dist::DedicatedServer => write!(f, "DEDICATED_SERVER"),
        }
    }
}
