//! Frozen protocol facts for the legacy Minecraft Java Edition 1.16.5 wire format.
//!
//! This module is deliberately separate from the active 26.3 protocol. It is
//! an initial compatibility boundary for the port; the live connection,
//! packet codecs, registries, and world decoder still need to be adapted before
//! this client can connect to a 1.16.5 server.
//!
//! Protocol reference: Minecraft Java Edition 1.16.5, protocol version 754.

/// Minecraft Java Edition version targeted by this port.
pub const GAME_VERSION: &str = horizon_core::legacy_1_16_5::GAME_VERSION;
/// Protocol version advertised by Minecraft Java Edition 1.16.5.
pub const PROTOCOL_VERSION: i32 = horizon_core::legacy_1_16_5::PROTOCOL_VERSION;
/// Saved-world data version for Minecraft Java Edition 1.16.5.
pub const DATA_VERSION: i32 = horizon_core::legacy_1_16_5::DATA_VERSION;
/// Required Java major version for the original 1.16.5 game.
pub const JAVA_VERSION: u32 = horizon_core::legacy_1_16_5::JAVA_VERSION;
