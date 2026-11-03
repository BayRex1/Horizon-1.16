//! Immutable version facts for the isolated Minecraft Java Edition 1.16.5 port.
//!
//! These facts do not switch the active client away from 26.3. A real 1.16.5
//! build must select matching protocol, registry, world-generation, asset and
//! save adapters together; mixing this profile with the active 26.3 pipeline
//! is unsupported.

/// Minecraft Java Edition version.
pub const GAME_VERSION: &str = "1.16.5";
/// Java protocol number used by multiplayer servers.
pub const PROTOCOL_VERSION: i32 = 754;
/// DataVersion stored in 1.16.5 worlds.
pub const DATA_VERSION: i32 = 2586;
/// Java major version required by the original game.
pub const JAVA_VERSION: u32 = 8;
/// Resource pack format declared by Minecraft 1.16.5.
pub const RESOURCE_PACK_FORMAT: i32 = 6;
/// Data pack format declared by Minecraft 1.16.5.
pub const DATA_PACK_FORMAT: i32 = 6;

/// Version facts used as one indivisible compatibility profile.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct VersionProfile {
    pub game_version: &'static str,
    pub protocol_version: i32,
    pub data_version: i32,
    pub java_version: u32,
    pub resource_pack_format: i32,
    pub data_pack_format: i32,
}

/// The frozen Minecraft 1.16.5 compatibility profile.
pub const PROFILE: VersionProfile = VersionProfile {
    game_version: GAME_VERSION,
    protocol_version: PROTOCOL_VERSION,
    data_version: DATA_VERSION,
    java_version: JAVA_VERSION,
    resource_pack_format: RESOURCE_PACK_FORMAT,
    data_pack_format: DATA_PACK_FORMAT,
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn profile_is_internally_consistent() {
        assert_eq!(PROFILE.game_version, "1.16.5");
        assert_eq!(PROFILE.protocol_version, 754);
        assert_eq!(PROFILE.data_version, 2586);
        assert_eq!(PROFILE.java_version, 8);
        assert_eq!(PROFILE.resource_pack_format, 6);
        assert_eq!(PROFILE.data_pack_format, 6);
    }
}
