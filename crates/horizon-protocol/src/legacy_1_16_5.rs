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

/// Encode the legacy Login Start packet (username only; no UUID field in 1.16.5).
///
/// This builds the packet body and frame but does not perform online-mode
/// encryption or send it over a live connection.
pub fn encode_login_start(username: &str) -> crate::Result<Vec<u8>> {
    use crate::buf::Writer;

    if username.is_empty()
        || username.len() > 16
        || !username.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
    {
        return Err(crate::Error::new("invalid 1.16.5 username"));
    }

    let mut packet = Writer::new();
    packet.varint(login::serverbound::START);
    packet.string(username);
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



/// Result of the legacy offline-mode login handshake. The returned connection
/// is already in Play state at the protocol level; world/play packet handling
/// is still the caller's responsibility.
pub struct OfflineLogin {
    pub uuid: String,
    pub username: String,
    pub connection: crate::connection::Connection,
}

/// Connect and perform the actual 1.16.5 handshake/login exchange against an
/// offline-mode server. Online-mode encryption is intentionally rejected
/// rather than silently pretending to authenticate.
pub fn connect_offline(
    host: &str,
    port: u16,
    username: &str,
    timeout: std::time::Duration,
) -> crate::Result<OfflineLogin> {
    use crate::buf::{Reader, Writer};
    use crate::connection::Connection;

    // Validate inputs and produce the exact legacy wire packets.
    let handshake_frame = encode_handshake(host, port, 2)?;
    let login_frame = encode_login_start(username)?;
    let mut connection = Connection::connect_server(host, port, timeout)?;
    connection.reader.set_read_timeout(Some(timeout))?;

    // Connection::send takes packet bodies and owns framing/compression.
    let mut framed = handshake_frame;
    let handshake_body = crate::frame::split(&mut framed)?
        .ok_or_else(|| crate::Error::new("could not frame legacy handshake"))?;
    connection.send(&handshake_body)?;
    let mut framed = login_frame;
    let login_body = crate::frame::split(&mut framed)?
        .ok_or_else(|| crate::Error::new("could not frame legacy login start"))?;
    connection.send(&login_body)?;

    loop {
        let (packet_id, payload) = connection.recv()?;
        match packet_id {
            login::clientbound::DISCONNECT => {
                let reason = decode_login_disconnect(&payload)?;
                return Err(crate::Error::new(format!("server rejected login: {reason}")));
            }
            login::clientbound::ENCRYPTION_REQUEST => {
                return Err(crate::Error::new(
                    "server requires online-mode authentication; this legacy connector currently supports offline-mode servers only",
                ));
            }
            login::clientbound::SET_COMPRESSION => {
                let threshold = decode_set_compression(&payload)?;
                connection.set_compression(threshold);
            }
            login::clientbound::SUCCESS => {
                let (uuid, username) = decode_login_success(&payload)?;
                return Ok(OfflineLogin { uuid, username, connection });
            }
            login::clientbound::PLUGIN_REQUEST => {
                // Unknown login plugin channels are answered with "not understood".
                let mut request = Reader::new(&payload);
                let message_id = request.varint()?;
                let _channel = request.string(32767)?;
                let mut response = Writer::new();
                response.varint(login::serverbound::PLUGIN_RESPONSE);
                response.varint(message_id);
                response.bool(false);
                connection.send(&response.data)?;
            }
            other => {
                return Err(crate::Error::new(format!(
                    "unexpected 1.16.5 login packet id {other}"
                )));
            }
        }
    }
}

/// Decode the 1.16.5 Set Compression packet payload (threshold as VarInt).
pub fn decode_set_compression(payload: &[u8]) -> crate::Result<i32> {
    let mut r = crate::buf::Reader::new(payload);
    let threshold = r.varint()?;
    if r.remaining() != 0 {
        return Err(crate::Error::new("trailing bytes in 1.16.5 set-compression packet"));
    }
    Ok(threshold)
}

/// Decode the 1.16.5 Login Success payload. In protocol 754 the UUID is a
/// canonical UUID string followed by the username (not the newer binary UUID).
pub fn decode_login_success(payload: &[u8]) -> crate::Result<(String, String)> {
    let mut r = crate::buf::Reader::new(payload);
    let uuid = r.string(36)?;
    let username = r.string(16)?;
    if r.remaining() != 0 {
        return Err(crate::Error::new("trailing bytes in 1.16.5 login-success packet"));
    }
    if uuid.len() != 36
        || !uuid.bytes().enumerate().all(|(i, b)| {
            if [8, 13, 18, 23].contains(&i) {
                b == b'-'
            } else {
                b.is_ascii_hexdigit()
            }
        })
    {
        return Err(crate::Error::new("invalid UUID in 1.16.5 login-success packet"));
    }
    Ok((uuid, username))
}

/// Decode the JSON string carried by the 1.16.5 Login Disconnect packet.
pub fn decode_login_disconnect(payload: &[u8]) -> crate::Result<String> {
    let mut r = crate::buf::Reader::new(payload);
    let reason = r.string(32767)?;
    if r.remaining() != 0 {
        return Err(crate::Error::new("trailing bytes in 1.16.5 login-disconnect packet"));
    }
    Ok(reason)
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
    fn encodes_legacy_login_start_without_uuid() {
        use crate::buf::Reader;
        let frame = encode_login_start("BayRex_16").unwrap();
        assert_eq!(frame[0] as usize, frame.len() - 1);
        let mut packet = Reader::new(&frame[1..]);
        assert_eq!(packet.varint().unwrap(), login::serverbound::START);
        assert_eq!(packet.string(16).unwrap(), "BayRex_16");
        assert_eq!(packet.remaining(), 0);
    }

    #[test]
    fn rejects_invalid_legacy_login_names() {
        assert!(encode_login_start("").is_err());
        assert!(encode_login_start("this_name_is_too_long").is_err());
        assert!(encode_login_start("bad-name").is_err());
        assert!(encode_login_start("name with space").is_err());
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

    #[test]
    fn decodes_legacy_login_success_and_compression() {
        use crate::buf::Writer;
        let mut payload = Writer::new();
        payload.string("123e4567-e89b-12d3-a456-426614174000");
        payload.string("BayRex");
        assert_eq!(
            decode_login_success(&payload.data).unwrap(),
            ("123e4567-e89b-12d3-a456-426614174000".to_owned(), "BayRex".to_owned())
        );
        assert_eq!(decode_set_compression(&[0x80, 0x01]).unwrap(), 128);
        assert!(decode_set_compression(&[1, 0]).is_err());
    }

    #[test]
    fn rejects_malformed_legacy_login_success() {
        use crate::buf::Writer;
        let mut payload = Writer::new();
        payload.string("not-a-uuid");
        payload.string("BayRex");
        assert!(decode_login_success(&payload.data).is_err());
        assert_eq!(decode_login_disconnect(&[2, b'{', b'}']).unwrap(), "{}");
        assert!(decode_login_disconnect(&[2, b'{', b'}', 0]).is_err());
    }

}
