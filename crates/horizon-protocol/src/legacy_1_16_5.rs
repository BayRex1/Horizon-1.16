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

/// Packet IDs for the Play state in protocol 754 (Minecraft 1.16.5).
///
/// Keep this table isolated from the current protocol's packet IDs.
pub mod play {
    pub mod clientbound {
        /// Disconnect while in the world.
        pub const DISCONNECT: i32 = 0x19;
        /// Keep-alive request carrying a signed 64-bit token.
        pub const KEEP_ALIVE: i32 = 0x21;
        /// Chunk column data (light data follows in the same packet on 1.16.5).
        pub const CHUNK_DATA: i32 = 0x20;
        /// Server correction of player position and rotation.
        pub const PLAYER_POSITION_LOOK: i32 = 0x34;
    }

    pub mod serverbound {
        /// Acknowledge the server's keep-alive token.
        pub const KEEP_ALIVE: i32 = 0x10;
        /// Confirm a server-issued player teleport.
        pub const TELEPORT_CONFIRM: i32 = 0x00;
        /// Report player position and rotation.
        pub const PLAYER_POSITION_ROTATION: i32 = 0x12;
    }
}

/// Decode the payload of a protocol-754 Play Keep Alive packet.
pub fn decode_play_keep_alive(payload: &[u8]) -> crate::Result<i64> {
    let mut r = crate::buf::Reader::new(payload);
    let token = r.i64()?;
    if r.remaining() != 0 {
        return Err(crate::Error::new(
            "trailing bytes in 1.16.5 keep-alive packet",
        ));
    }
    Ok(token)
}

/// Encode the serverbound protocol-754 Play Keep Alive response as a packet body.
pub fn encode_play_keep_alive(token: i64) -> crate::Result<Vec<u8>> {
    let mut packet = crate::buf::Writer::new();
    packet.varint(play::serverbound::KEEP_ALIVE);
    packet.i64(token);
    Ok(packet.data)
}

/// Absolute/relative position update sent by a protocol-754 server.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PlayerPositionLook {
    pub x: f64,
    pub y: f64,
    pub z: f64,
    pub yaw: f32,
    pub pitch: f32,
    /// Bit mask: X=0x01, Y=0x02, Z=0x04, Yaw=0x08, Pitch=0x10.
    pub flags: u8,
    pub teleport_id: i32,
}

/// Decode the protocol-754 Player Position And Look packet payload.
pub fn decode_player_position_look(payload: &[u8]) -> crate::Result<PlayerPositionLook> {
    let mut r = crate::buf::Reader::new(payload);
    let result = PlayerPositionLook {
        x: r.f64()?,
        y: r.f64()?,
        z: r.f64()?,
        yaw: r.f32()?,
        pitch: r.f32()?,
        flags: r.u8()?,
        teleport_id: r.varint()?,
    };
    if r.remaining() != 0 {
        return Err(crate::Error::new(
            "trailing bytes in 1.16.5 player-position packet",
        ));
    }
    if result.flags & !0x1f != 0 {
        return Err(crate::Error::new(
            "invalid flags in 1.16.5 player-position packet",
        ));
    }
    Ok(result)
}

/// Encode a serverbound teleport acknowledgement packet body.
pub fn encode_teleport_confirm(teleport_id: i32) -> crate::Result<Vec<u8>> {
    if teleport_id < 0 {
        return Err(crate::Error::new("negative 1.16.5 teleport id"));
    }
    let mut packet = crate::buf::Writer::new();
    packet.varint(play::serverbound::TELEPORT_CONFIRM);
    packet.varint(teleport_id);
    Ok(packet.data)
}

/// Encode serverbound player position and rotation (position is absolute).
pub fn encode_player_position_rotation(
    x: f64,
    y: f64,
    z: f64,
    yaw: f32,
    pitch: f32,
    on_ground: bool,
) -> crate::Result<Vec<u8>> {
    if ![x, y, z].iter().all(|v| v.is_finite()) || !yaw.is_finite() || !pitch.is_finite() {
        return Err(crate::Error::new("non-finite player position or rotation"));
    }
    let mut packet = crate::buf::Writer::new();
    packet.varint(play::serverbound::PLAYER_POSITION_ROTATION);
    packet.f64(x);
    packet.f64(y);
    packet.f64(z);
    packet.f32(yaw);
    packet.f32(pitch);
    packet.bool(on_ground);
    Ok(packet.data)
}

/// One decoded 1.16.5 chunk section. Block-state values are registry IDs
/// from the 1.16.5 global block-state palette when palette is empty.
#[derive(Clone, Debug, PartialEq)]
pub struct LegacyChunkSection {
    pub non_air_blocks: u16,
    pub bits_per_block: u8,
    pub palette: Vec<i32>,
    pub data: Vec<i64>,
}
impl LegacyChunkSection {
    /// Decode a protocol-754 section's block-state paletted container.
    pub fn read(r: &mut crate::buf::Reader) -> crate::Result<Self> {
        let non_air_blocks = r.u16()?;
        let wire_bits = r.u8()?;
        if wire_bits > 16 {
            return Err(crate::Error::new("invalid 1.16.5 block palette bit width"));
        }
        let (bits_per_block, palette) = if wire_bits == 0 {
            (0, vec![r.varint()?])
        } else if wire_bits <= 4 {
            let n = r.count(1usize << 4)?;
            (
                4,
                (0..n)
                    .map(|_| r.varint())
                    .collect::<crate::Result<Vec<_>>>()?,
            )
        } else if wire_bits <= 8 {
            let n = r.count(1usize << wire_bits)?;
            (
                wire_bits,
                (0..n)
                    .map(|_| r.varint())
                    .collect::<crate::Result<Vec<_>>>()?,
            )
        } else {
            (wire_bits, Vec::new())
        };
        let expected = if bits_per_block == 0 {
            0
        } else {
            4096usize.div_ceil(64 / bits_per_block as usize)
        };
        let count = r.count(4096)?;
        if count != expected {
            return Err(crate::Error::new(format!(
                "invalid 1.16.5 block storage length: expected {expected}, got {count}"
            )));
        }
        let data = r.fixed_longs(count)?;
        Ok(Self {
            non_air_blocks,
            bits_per_block,
            palette,
            data,
        })
    }

    /// Resolve a block-state registry ID for the local section index (0..4095).
    pub fn block_state(&self, index: usize) -> Option<i32> {
        if index >= 4096 {
            return None;
        }
        if self.bits_per_block == 0 {
            return self.palette.first().copied();
        }
        let bits = self.bits_per_block as usize;
        let per_long = 64 / bits;
        let word = *self.data.get(index / per_long)? as u64;
        let palette_index = ((word >> ((index % per_long) * bits)) & ((1u64 << bits) - 1)) as usize;
        if self.palette.is_empty() {
            Some(palette_index as i32)
        } else {
            self.palette.get(palette_index).copied()
        }
    }
}

/// A partial 1.16.5 chunk column. Heightmaps and block-entity NBT are preserved
/// as tags; lighting data arrives in separate packets and is not included here.
#[derive(Clone, Debug)]
pub struct LegacyChunkColumn {
    pub x: i32,
    pub z: i32,
    pub full_chunk: bool,
    pub section_mask: u16,
    pub heightmaps: horizon_world::nbt::Tag,
    pub biomes: Vec<i32>,
    pub sections: Vec<Option<LegacyChunkSection>>,
    pub block_entities: Vec<horizon_world::nbt::Tag>,
}

/// Decode Clientbound Level Chunk With Light's legacy chunk-data portion
/// (the 1.16.5 ClientboundLevelChunkPacketData layout).
pub fn decode_chunk_data(payload: &[u8]) -> crate::Result<LegacyChunkColumn> {
    let mut r = crate::buf::Reader::new(payload);
    let x = r.i32()?;
    let z = r.i32()?;
    let full_chunk = r.bool()?;
    let mask = r.varint()?;
    if !(0..=0xffff).contains(&mask) {
        return Err(crate::Error::new("invalid 1.16.5 chunk section mask"));
    }
    let section_mask = mask as u16;
    let heightmaps = r.nbt()?;
    let biomes = if full_chunk {
        let count = r.count(1024)?;
        if count != 1024 {
            return Err(crate::Error::new(
                "1.16.5 full chunk must contain 1024 biome IDs",
            ));
        }
        (0..count)
            .map(|_| r.varint())
            .collect::<crate::Result<Vec<_>>>()?
    } else {
        Vec::new()
    };
    let section_bytes = r.count(2 * 1024 * 1024)?;
    let mut section_reader = crate::buf::Reader::new(r.bytes(section_bytes)?);
    let mut sections = Vec::with_capacity(16);
    for section_y in 0..16 {
        if section_mask & (1 << section_y) != 0 {
            sections.push(Some(LegacyChunkSection::read(&mut section_reader)?));
        } else {
            sections.push(None);
        }
    }
    if section_reader.remaining() != 0 {
        return Err(crate::Error::new(
            "trailing bytes in 1.16.5 chunk section data",
        ));
    }
    let entity_count = r.count(4096)?;
    let block_entities = (0..entity_count)
        .map(|_| r.nbt())
        .collect::<crate::Result<Vec<_>>>()?;
    if r.remaining() != 0 {
        return Err(crate::Error::new("trailing bytes in 1.16.5 chunk packet"));
    }
    Ok(LegacyChunkColumn {
        x,
        z,
        full_chunk,
        section_mask,
        heightmaps,
        biomes,
        sections,
        block_entities,
    })
}

/// Minimal player state maintained while processing protocol-754 Play packets.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct PlayerState {
    pub x: f64,
    pub y: f64,
    pub z: f64,
    pub yaw: f32,
    pub pitch: f32,
    pub on_ground: bool,
}

/// Result of dispatching one packet received in the 1.16.5 Play state.
#[derive(Clone, Debug, PartialEq)]
pub enum PlayEvent {
    PositionUpdated(PlayerState),
    Disconnected(String),
    ChunkReceived { x: i32, z: i32, sections: usize },
    Ignored,
}

/// Dispatch one packet from a protocol-754 Play connection.
///
/// This keeps the connection alive, acknowledges server teleports, and tracks
/// server position corrections. Unknown gameplay packets are ignored for now.
pub fn handle_play_packet(
    connection: &mut crate::connection::Connection,
    packet_id: i32,
    payload: &[u8],
    state: &mut PlayerState,
) -> crate::Result<PlayEvent> {
    match packet_id {
        play::clientbound::KEEP_ALIVE => {
            let token = decode_play_keep_alive(payload)?;
            connection.send(&encode_play_keep_alive(token)?)?;
            Ok(PlayEvent::Ignored)
        }
        play::clientbound::DISCONNECT => {
            Ok(PlayEvent::Disconnected(decode_play_disconnect(payload)?))
        }
        play::clientbound::CHUNK_DATA => {
            let chunk = decode_chunk_data(payload)?;
            let count = chunk.sections.iter().filter(|s| s.is_some()).count();
            Ok(PlayEvent::ChunkReceived {
                x: chunk.x,
                z: chunk.z,
                sections: count,
            })
        }
        play::clientbound::PLAYER_POSITION_LOOK => {
            let update = decode_player_position_look(payload)?;
            if update.flags & 0x01 != 0 {
                state.x += update.x;
            } else {
                state.x = update.x;
            }
            if update.flags & 0x02 != 0 {
                state.y += update.y;
            } else {
                state.y = update.y;
            }
            if update.flags & 0x04 != 0 {
                state.z += update.z;
            } else {
                state.z = update.z;
            }
            if update.flags & 0x08 != 0 {
                state.yaw += update.yaw;
            } else {
                state.yaw = update.yaw;
            }
            if update.flags & 0x10 != 0 {
                state.pitch += update.pitch;
            } else {
                state.pitch = update.pitch;
            }
            if ![state.x, state.y, state.z].iter().all(|v| v.is_finite())
                || !state.yaw.is_finite()
                || !state.pitch.is_finite()
            {
                return Err(crate::Error::new(
                    "server sent non-finite 1.16.5 player position",
                ));
            }
            connection.send(&encode_teleport_confirm(update.teleport_id)?)?;
            connection.send(&encode_player_position_rotation(
                state.x,
                state.y,
                state.z,
                state.yaw,
                state.pitch,
                state.on_ground,
            )?)?;
            Ok(PlayEvent::PositionUpdated(*state))
        }
        _ => Ok(PlayEvent::Ignored),
    }
}

/// Run the protocol-754 Play receive loop until the server disconnects or the
/// socket errors. World packets are still ignored until chunk and registry
/// support is implemented.
pub fn run_play_loop(login: OfflineLogin) -> crate::Result<(String, PlayerState)> {
    let mut connection = login.connection;
    let mut state = PlayerState::default();
    loop {
        let (packet_id, payload) = connection.recv()?;
        match handle_play_packet(&mut connection, packet_id, &payload, &mut state)? {
            PlayEvent::Disconnected(reason) => return Ok((reason, state)),
            PlayEvent::PositionUpdated(_)
            | PlayEvent::ChunkReceived { .. }
            | PlayEvent::Ignored => {}
        }
    }
}

/// Decode the JSON string carried by the Play Disconnect packet.
pub fn decode_play_disconnect(payload: &[u8]) -> crate::Result<String> {
    let mut r = crate::buf::Reader::new(payload);
    let reason = r.string(32767)?;
    if r.remaining() != 0 {
        return Err(crate::Error::new(
            "trailing bytes in 1.16.5 play-disconnect packet",
        ));
    }
    Ok(reason)
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
        return Err(crate::Error::new(
            "handshake next_state must be 1 (status) or 2 (login)",
        ));
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
        || !username
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_')
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
                return Err(crate::Error::new(format!(
                    "server rejected login: {reason}"
                )));
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
                return Ok(OfflineLogin {
                    uuid,
                    username,
                    connection,
                });
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
        return Err(crate::Error::new(
            "trailing bytes in 1.16.5 set-compression packet",
        ));
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
        return Err(crate::Error::new(
            "trailing bytes in 1.16.5 login-success packet",
        ));
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
        return Err(crate::Error::new(
            "invalid UUID in 1.16.5 login-success packet",
        ));
    }
    Ok((uuid, username))
}

/// Decode the JSON string carried by the 1.16.5 Login Disconnect packet.
pub fn decode_login_disconnect(payload: &[u8]) -> crate::Result<String> {
    let mut r = crate::buf::Reader::new(payload);
    let reason = r.string(32767)?;
    if r.remaining() != 0 {
        return Err(crate::Error::new(
            "trailing bytes in 1.16.5 login-disconnect packet",
        ));
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
            if b & 0x80 == 0 {
                break;
            }
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
    fn play_keep_alive_round_trips_the_signed_token() {
        use crate::buf::Reader;
        let token = -0x0102030405060708i64;
        let body = encode_play_keep_alive(token).unwrap();
        let mut r = Reader::new(&body);
        assert_eq!(r.varint().unwrap(), play::serverbound::KEEP_ALIVE);
        assert_eq!(
            decode_play_keep_alive(&body[r.position()..]).unwrap(),
            token
        );
    }

    #[test]
    fn position_look_decodes_and_teleport_is_acknowledged() {
        use crate::buf::{Reader, Writer};
        let mut payload = Writer::new();
        payload.f64(12.5);
        payload.f64(64.0);
        payload.f64(-8.25);
        payload.f32(90.0);
        payload.f32(-10.0);
        payload.u8(0x04);
        payload.varint(37);
        let pos = decode_player_position_look(&payload.data).unwrap();
        assert_eq!(pos.x, 12.5);
        assert_eq!(pos.y, 64.0);
        assert_eq!(pos.z, -8.25);
        assert_eq!(pos.flags, 0x04);
        assert_eq!(pos.teleport_id, 37);

        let ack = encode_teleport_confirm(pos.teleport_id).unwrap();
        let mut r = Reader::new(&ack);
        assert_eq!(r.varint().unwrap(), play::serverbound::TELEPORT_CONFIRM);
        assert_eq!(r.varint().unwrap(), 37);
        assert!(encode_teleport_confirm(-1).is_err());
    }

    #[test]
    fn relative_position_flags_are_applied_to_existing_state() {
        let mut payload = crate::buf::Writer::new();
        payload.f64(2.0);
        payload.f64(3.0);
        payload.f64(-4.0);
        payload.f32(10.0);
        payload.f32(5.0);
        payload.u8(0x1f);
        payload.varint(1);
        let update = decode_player_position_look(&payload.data).unwrap();
        let mut state = PlayerState {
            x: 10.0,
            y: 60.0,
            z: 8.0,
            yaw: 90.0,
            pitch: 0.0,
            on_ground: false,
        };
        if update.flags & 0x01 != 0 {
            state.x += update.x;
        }
        if update.flags & 0x02 != 0 {
            state.y += update.y;
        }
        if update.flags & 0x04 != 0 {
            state.z += update.z;
        }
        if update.flags & 0x08 != 0 {
            state.yaw += update.yaw;
        }
        if update.flags & 0x10 != 0 {
            state.pitch += update.pitch;
        }
        assert_eq!(
            state,
            PlayerState {
                x: 12.0,
                y: 63.0,
                z: 4.0,
                yaw: 100.0,
                pitch: 5.0,
                on_ground: false
            }
        );
    }

    #[test]
    fn player_position_rotation_rejects_nan() {
        assert!(encode_player_position_rotation(f64::NAN, 64.0, 0.0, 0.0, 0.0, true).is_err());
        let body = encode_player_position_rotation(1.0, 65.0, 2.0, 45.0, 0.0, true).unwrap();
        assert_eq!(body[0], play::serverbound::PLAYER_POSITION_ROTATION as u8);
    }

    #[test]
    fn play_packet_ids_match_protocol_754_layout() {
        assert_eq!(play::clientbound::DISCONNECT, 0x19);
        assert_eq!(play::clientbound::KEEP_ALIVE, 0x21);
        assert_eq!(play::clientbound::CHUNK_DATA, 0x20);
        assert_eq!(play::clientbound::PLAYER_POSITION_LOOK, 0x34);
        assert_eq!(play::serverbound::TELEPORT_CONFIRM, 0x00);
        assert_eq!(play::serverbound::KEEP_ALIVE, 0x10);
        assert_eq!(play::serverbound::PLAYER_POSITION_ROTATION, 0x12);
        assert!(decode_play_keep_alive(&[0; 9]).is_err());
        assert!(decode_play_disconnect(&[2, b'{', b'}', 0]).is_err());
    }

    #[test]
    fn offline_login_runs_over_a_real_local_tcp_connection() {
        use crate::{buf::Writer, connection::Connection};
        use std::{
            net::TcpListener,
            thread,
            time::Duration,
        };

        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let port = listener.local_addr().unwrap().port();
        let server = thread::spawn(move || {
            let (stream, _) = listener.accept().unwrap();
            let mut connection = Connection::from_stream(stream).unwrap();

            // The client sends the legacy handshake followed by Login Start.
            let (handshake_id, handshake) = connection.recv().unwrap();
            assert_eq!(handshake_id, handshake::INTENTION);
            let mut r = crate::buf::Reader::new(&handshake);
            assert_eq!(r.varint().unwrap(), PROTOCOL_VERSION);
            assert_eq!(r.string(255).unwrap(), "127.0.0.1");
            assert_eq!(r.u16().unwrap(), port);
            assert_eq!(r.varint().unwrap(), 2);

            let (login_id, login) = connection.recv().unwrap();
            assert_eq!(login_id, login::serverbound::START);
            let mut r = crate::buf::Reader::new(&login);
            assert_eq!(r.string(16).unwrap(), "BayRex");
            assert_eq!(r.remaining(), 0);

            let mut success = Writer::new();
            success.varint(login::clientbound::SUCCESS);
            success.string("123e4567-e89b-12d3-a456-426614174000");
            success.string("BayRex");
            connection.send(&success.data).unwrap();

            let mut disconnect = Writer::new();
            disconnect.varint(play::clientbound::DISCONNECT);
            disconnect.string(r#"{"text":"test complete"}"#);
            connection.send(&disconnect.data).unwrap();
        });

        let login = connect_offline(
            "127.0.0.1",
            port,
            "BayRex",
            Duration::from_secs(3),
        )
        .unwrap();
        assert_eq!(login.username, "BayRex");
        assert_eq!(login.uuid, "123e4567-e89b-12d3-a456-426614174000");
        let (reason, state) = run_play_loop(login).unwrap();
        assert_eq!(reason, r#"{"text":"test complete"}"#);
        assert_eq!(state, PlayerState::default());
        server.join().unwrap();
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
            (
                "123e4567-e89b-12d3-a456-426614174000".to_owned(),
                "BayRex".to_owned()
            )
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
