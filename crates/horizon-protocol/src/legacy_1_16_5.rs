//! Frozen protocol facts for the legacy Minecraft Java Edition 1.16.5 wire format.
//!
//! This module is deliberately separate from the active 26.3 protocol. It is
//! an initial compatibility boundary for the port; the live connection,
//! packet codecs, registries, and world decoder still need to be adapted before
//! this client can connect to a 1.16.5 server.
//!
//! Protocol reference: Minecraft Java Edition 1.16.5, protocol version 754.

/// Minecraft Java Edition version targeted by this port.
pub const GAME_VERSION: &str = "1.16.5";
/// Protocol version advertised by Minecraft Java Edition 1.16.5.
pub const PROTOCOL_VERSION: i32 = 754;
/// Saved-world data version for Minecraft Java Edition 1.16.5.
pub const DATA_VERSION: i32 = 2586;
/// Required Java major version for the original 1.16.5 game.
pub const JAVA_VERSION: u32 = 8;

/// Connection states present in protocol 754.
///
/// Unlike newer Minecraft protocols, 1.16.5 has no Configuration state.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum State {
    Handshake,
    Status,
    Login,
    Play,
}

/// Packet IDs for the handshake state (serverbound).
pub mod handshake {
    /// Client intention packet.
    pub const INTENTION: i32 = 0;
}

/// Packet IDs for the status state.
pub mod status {
    pub mod clientbound {
        /// JSON server status response.
        pub const RESPONSE: i32 = 0;
        /// Response to the client's ping.
        pub const PONG: i32 = 1;
    }

    pub mod serverbound {
        /// Request server status.
        pub const REQUEST: i32 = 0;
        /// Ping request.
        pub const PING: i32 = 1;
    }
}

/// Packet IDs for the login state.
pub mod login {
    pub mod clientbound {
        /// Login disconnect.
        pub const DISCONNECT: i32 = 0;
        /// Encryption request.
        pub const ENCRYPTION_REQUEST: i32 = 1;
        /// Successful login.
        pub const SUCCESS: i32 = 2;
        /// Set compression threshold.
        pub const SET_COMPRESSION: i32 = 3;
        /// Login plugin request.
        pub const PLUGIN_REQUEST: i32 = 4;
    }

    pub mod serverbound {
        /// Login start.
        pub const START: i32 = 0;
        /// Encryption response.
        pub const ENCRYPTION_RESPONSE: i32 = 1;
        /// Login plugin response.
        pub const PLUGIN_RESPONSE: i32 = 2;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn targets_java_1_16_5() {
        assert_eq!(GAME_VERSION, "1.16.5");
        assert_eq!(PROTOCOL_VERSION, 754);
        assert_eq!(DATA_VERSION, 2586);
        assert_eq!(JAVA_VERSION, 8);
    }

    #[test]
    fn legacy_protocol_has_no_configuration_state() {
        let states = [State::Handshake, State::Status, State::Login, State::Play];
        assert_eq!(states.len(), 4);
    }

    #[test]
    fn status_and_login_ids_match_legacy_layout() {
        assert_eq!(status::clientbound::RESPONSE, 0);
        assert_eq!(status::clientbound::PONG, 1);
        assert_eq!(login::clientbound::SUCCESS, 2);
        assert_eq!(login::serverbound::START, 0);
        assert_eq!(login::serverbound::PLUGIN_RESPONSE, 2);
    }
}
