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


/// Encode the legacy handshake packet and its outer frame.
///
/// `next_state` is 1 for status or 2 for login. This function does not
/// open a socket or change the active Horizon connection implementation.
pub fn encode_handshake(host: &str, port: u16, next_state: i32) -> crate::Result<Vec<u8>> {
    use crate::buf::Writer;

    if host.is_empty() || host.len() > 255 || host.contains('\0') {
        return Err(crate::Error::new("invalid 1.16.5 server host"));
    }
    if next_state != 1 && next_state != 2 {
        return Err(crate::Error::new("handshake next_state must be 1 (status) or 2 (login)"));
    }

    let mut packet = Writer::new();
    packet.varint(handshake::INTENTION);
    packet.varint(PROTOCOL_VERSION);
    packet.string(host);
    packet.u16(port);
    packet.varint(next_state);
    crate::frame::encode(&packet.data, None)
}

/// Encode the server-list status request packet with its outer frame.
pub fn encode_status_request() -> crate::Result<Vec<u8>> {
    crate::frame::encode(&[status::serverbound::REQUEST as u8], None)
}

/// Encode a status ping packet with its outer frame.
pub fn encode_status_ping(payload: i64) -> crate::Result<Vec<u8>> {
    use crate::buf::Writer;
    let mut packet = Writer::new();
    packet.varint(status::serverbound::PING);
    packet.i64(payload);
    crate::frame::encode(&packet.data, None)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encodes_legacy_handshake_for_status_and_login() {
        use crate::buf::Reader;
        let frame = encode_handshake("localhost", 25565, 1).unwrap();
        let mut framed = frame.as_slice();
        let mut len = 0usize;
        let mut shift = 0;
        loop {
            let b = framed[0];
            framed = &framed[1..];
            len |= usize::from(b & 0x7f) << shift;
            if b & 0x80 == 0 { break; }
            shift += 7;
        }
        assert_eq!(framed.len(), len);
        let mut packet = Reader::new(framed);
        assert_eq!(packet.varint().unwrap(), handshake::INTENTION);
        assert_eq!(packet.varint().unwrap(), 754);
        assert_eq!(packet.string(255).unwrap(), "localhost");
        assert_eq!(packet.u16().unwrap(), 25565);
        assert_eq!(packet.varint().unwrap(), 1);
        assert_eq!(packet.remaining(), 0);

        let login = encode_handshake("127.0.0.1", 25565, 2).unwrap();
        assert!(!login.is_empty());
    }

    #[test]
    fn rejects_invalid_legacy_handshakes() {
        assert!(encode_handshake("", 25565, 1).is_err());
        assert!(encode_handshake("localhost", 25565, 3).is_err());
        assert!(encode_handshake("bad\0host", 25565, 1).is_err());
    }

    #[test]
    fn encodes_status_packets() {
        use crate::buf::Reader;
        assert_eq!(encode_status_request().unwrap(), vec![1, 0]);
        let ping = encode_status_ping(0x0102030405060708).unwrap();
        let mut framed = ping.as_slice();
        assert_eq!(framed[0], 9);
        framed = &framed[1..];
        let mut packet = Reader::new(framed);
        assert_eq!(packet.varint().unwrap(), 1);
        assert_eq!(packet.i64().unwrap(), 0x0102030405060708);
    }

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
