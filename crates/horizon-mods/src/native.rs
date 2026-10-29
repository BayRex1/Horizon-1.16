//! Cooperative native-mod services. Native code has OS rights; these are not a sandbox.
//! No WebAssembly runtime is needed. The game owns one registry per instance/Game.
use crate::transport::{Datagrams, endpoint};
use std::{
    collections::{BTreeMap, BTreeSet, VecDeque},
    net::{IpAddr, SocketAddr},
    time::{Duration, Instant},
};

pub mod microphone {
    #[derive(Clone, Debug)]
    pub struct Device {
        pub id: String,
        pub name: String,
    }
    #[derive(Clone, Debug)]
    pub struct Frame {
        pub sample_rate: u32,
        pub samples: Vec<f32>,
    }
}
pub mod audio {
    #[derive(Clone, Debug)]
    pub struct Device {
        pub id: String,
        pub name: String,
    }
    #[derive(Clone, Copy, Debug)]
    pub struct Position {
        pub x: f64,
        pub y: f64,
        pub z: f64,
    }
    #[derive(Clone, Copy, Debug)]
    pub struct Settings {
        pub gain: f32,
        pub at: Option<Position>,
        pub distance: f32,
    }
}
pub(crate) mod pcm;

/// Identity supplied by the reviewed bundle. It is attribution, not authentication of native code.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Declaration {
    pub id: String,
    pub title: String,
    pub author: String,
    pub version: String,
    pub purpose: String,
    pub microphone: bool,
    pub audio: bool,
}
/// Player identity, with position only while the game tracks that player in this dimension.
#[derive(Clone, Debug)]
pub struct Player {
    pub id: String,
    pub name: String,
    pub position: Option<[f64; 3]>,
    pub dimension: Option<String>,
}
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Session {
    pub connected: bool,
    /// Opaque connection generation; a reconnect to the same server still revokes grants.
    pub connection: u64,
    pub server: Option<String>,
    pub local_player: Option<String>,
    pub server_side: bool,
    pub hosting: bool,
    pub focused: bool,
    pub players: Vec<String>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Message {
    pub player: Option<String>,
    pub channel: String,
    pub data: Vec<u8>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Event {
    Connected(Session),
    Disconnected,
    PlayerJoined(String),
    PlayerLeft(String),
    Packet(Message),
    PermissionsChanged,
}
#[derive(Clone, Debug)]
struct Destination {
    host: String,
    port: u16,
    allowed: bool,
}

pub struct Voice {
    declaration: Declaration,
    session: Session,
    pcm: pcm::Pcm,
    transport: Datagrams,
    microphone_allowed: bool,
    endpoints: Vec<Destination>,
    resolving: Option<(String, u16, Instant)>,
    events: VecDeque<Event>,
    event_bytes: usize,
    channels: BTreeSet<String>,
    out: Vec<Message>,
    bytes_left: usize,
    packets_left: usize,
    entropy: usize,
}
impl Voice {
    fn new(declaration: Declaration) -> Self {
        Self {
            declaration,
            session: Session::default(),
            pcm: pcm::Pcm::default(),
            transport: Datagrams::default(),
            microphone_allowed: false,
            endpoints: Vec::new(),
            resolving: None,
            events: VecDeque::new(),
            event_bytes: 0,
            channels: BTreeSet::new(),
            out: Vec::new(),
            bytes_left: 65536,
            packets_left: 32,
            entropy: 4096,
        }
    }
    fn event(&mut self, event: Event) {
        let size = |e: &Event| match e {
            Event::Packet(m) => m.data.len(),
            _ => 0,
        };
        let bytes = size(&event);
        while self.events.len() >= 64 || self.event_bytes + bytes > 256 * 1024 {
            if let Some(old) = self.events.pop_front() {
                self.event_bytes -= size(&old);
            } else {
                break;
            }
        }
        self.event_bytes += bytes;
        self.events.push_back(event);
    }
    fn revoke(&mut self) {
        self.microphone_allowed = false;
        self.endpoints.clear();
        self.resolving = None;
        self.transport.reset();
        self.pcm = pcm::Pcm::default();
        self.out.clear();
        self.events.clear();
        self.event_bytes = 0;
    }
    pub fn session(&self) -> &Session {
        &self.session
    }
    pub fn poll(&mut self) -> Vec<Event> {
        self.event_bytes = 0;
        self.events.drain(..).collect()
    }
    pub fn input_devices(&mut self) -> Vec<microphone::Device> {
        if self.microphone_allowed {
            self.pcm.input_devices()
        } else {
            Vec::new()
        }
    }
    pub fn start_capture(&mut self, device: String, gain: f32) -> Result<(), String> {
        if !self.microphone_allowed
            || !self.session.connected
            || self.session.server_side
            || !self.session.focused
        {
            return Err(
                "microphone permission denied, window unfocused or no client session".into(),
            );
        }
        self.pcm.start(device, gain)
    }
    pub fn stop_capture(&mut self) {
        self.pcm.stop();
    }
    pub fn input_gain(&mut self, gain: f32) -> Result<(), String> {
        self.pcm.set_gain(gain)
    }
    pub fn read_capture(&mut self) -> Result<Vec<microphone::Frame>, String> {
        if !self.microphone_allowed {
            return Err("microphone permission denied".into());
        }
        self.pcm.read()
    }
    fn audio_allowed(&self) -> Result<(), String> {
        if self.declaration.audio && !self.session.server_side {
            Ok(())
        } else {
            Err("audio not declared or unavailable on server".into())
        }
    }
    pub fn output_devices(&mut self) -> Vec<audio::Device> {
        if self.audio_allowed().is_ok() {
            self.pcm.output_devices()
        } else {
            Vec::new()
        }
    }
    pub fn select_output(&mut self, device: String) -> Result<(), String> {
        self.audio_allowed()?;
        self.pcm.select_device(device)
    }
    pub fn create_source(&mut self, rate: u32) -> Result<u32, String> {
        self.audio_allowed()?;
        self.pcm.create(rate)
    }
    pub fn write_audio(&mut self, source: u32, samples: Vec<f32>) -> Result<(), String> {
        self.audio_allowed()?;
        self.pcm.write(source, samples)
    }
    pub fn configure_source(
        &mut self,
        source: u32,
        settings: audio::Settings,
    ) -> Result<(), String> {
        self.audio_allowed()?;
        self.pcm.configure(source, settings)
    }
    pub fn close_source(&mut self, source: u32) {
        self.pcm.close(source);
    }
    /// Request consent in Options > Mods > Settings. This performs no DNS or IO.
    pub fn request_endpoint(&mut self, host: &str, port: u16) -> Result<(), String> {
        if !self.session.connected || self.session.server_side {
            return Err("a connected client session is required".into());
        }
        if port == 0
            || host.is_empty()
            || host.len() > 253
            || !host
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b".-:".contains(&b))
        {
            return Err("invalid endpoint".into());
        }
        let host = host.to_ascii_lowercase();
        if let Ok(ip) = host.parse::<IpAddr>() {
            endpoint(&SocketAddr::new(ip, port).to_string())?;
        }
        if self
            .endpoints
            .iter()
            .any(|e| e.host == host && e.port == port)
        {
            return Ok(());
        }
        if self.endpoints.len() >= 16 {
            return Err("endpoint request limit reached".into());
        }
        self.endpoints.push(Destination {
            host,
            port,
            allowed: false,
        });
        Ok(())
    }
    /// Only an explicitly approved exact hostname/port can be resolved.
    pub fn resolve(&mut self, host: &str, port: u16) -> Result<(), String> {
        let host = host.to_ascii_lowercase();
        if !self
            .endpoints
            .iter()
            .any(|e| e.allowed && e.host == host && e.port == port)
        {
            return Err("DNS permission denied".into());
        }
        if self.resolving.is_some() {
            return Err("DNS lookup already pending".into());
        }
        self.transport.resolve(host.clone())?;
        self.resolving = Some((host, port, Instant::now()));
        Ok(())
    }
    /// Results create numeric endpoint requests. Each needs separate approval before connect.
    pub fn resolved(&mut self) -> Result<Option<Vec<String>>, String> {
        let Some((_, port, started)) = self.resolving.clone() else {
            return Ok(None);
        };
        if started.elapsed() > Duration::from_secs(10) {
            self.resolving = None;
            self.transport.cancel_resolution();
            return Err("DNS request timed out".into());
        }
        match self.transport.resolved() {
            Ok(Some(ips)) => {
                self.resolving = None;
                for ip in &ips {
                    self.request_endpoint(ip, port)?;
                }
                Ok(Some(ips))
            }
            Err(e) => {
                self.resolving = None;
                Err(e)
            }
            Ok(None) => Ok(None),
        }
    }
    pub fn connect_udp(&mut self, address: &str) -> Result<u32, String> {
        let address = endpoint(address)?;
        if !self.session.connected
            || !self.endpoints.iter().any(|e| {
                e.allowed
                    && e.port == address.port()
                    && e.host.parse::<IpAddr>().is_ok_and(|ip| ip == address.ip())
            })
        {
            return Err("UDP destination permission denied".into());
        }
        self.transport.connect(address.to_string())
    }
    pub fn send_udp(&mut self, socket: u32, data: Vec<u8>) -> Result<(), String> {
        self.transport.send(socket, data)
    }
    pub fn receive_udp(&mut self, socket: u32) -> Result<Vec<Vec<u8>>, String> {
        self.transport.receive(socket)
    }
    pub fn close_udp(&mut self, socket: u32) {
        self.transport.close(socket);
    }
    pub fn register_channel(&mut self, channel: &str) -> Result<(), String> {
        if !channel_ok(channel) {
            return Err("expected a non-reserved namespaced channel".into());
        }
        if self.channels.len() >= 32 && !self.channels.contains(channel) {
            return Err("channel limit reached".into());
        }
        self.channels.insert(channel.into());
        Ok(())
    }
    pub fn unregister_channel(&mut self, channel: &str) {
        self.channels.remove(channel);
        self.events
            .retain(|e| !matches!(e, Event::Packet(m) if m.channel == channel));
        self.event_bytes = self
            .events
            .iter()
            .map(|e| {
                if let Event::Packet(m) = e {
                    m.data.len()
                } else {
                    0
                }
            })
            .sum();
    }
    pub fn send_payload(
        &mut self,
        player: Option<String>,
        channel: &str,
        data: Vec<u8>,
    ) -> Result<(), String> {
        if !self.session.connected || !self.channels.contains(channel) {
            return Err("no session or channel is not registered".into());
        }
        if player.is_some() && (self.session.server_side || self.session.hosting) {
            if !player
                .as_ref()
                .is_some_and(|id| self.session.players.contains(id))
            {
                return Err("expected an online player's UUID".into());
            }
        } else if player.is_some() || self.session.server_side {
            return Err("client payloads cannot specify a player".into());
        }
        if data.len() > 32767 || data.len() > self.bytes_left || self.packets_left == 0 {
            return Err("payload budget exceeded".into());
        }
        self.bytes_left -= data.len();
        self.packets_left -= 1;
        self.out.push(Message {
            player,
            channel: channel.into(),
            data,
        });
        Ok(())
    }
    pub fn random_bytes(&mut self, length: usize) -> Result<Vec<u8>, String> {
        if length > 1024 || length > self.entropy {
            return Err("entropy budget exceeded".into());
        }
        self.entropy -= length;
        let mut bytes = vec![0; length];
        getrandom::getrandom(&mut bytes).map_err(|e| e.to_string())?;
        Ok(bytes)
    }
}
fn channel_ok(channel: &str) -> bool {
    channel.len() <= 128
        && channel.split_once(':').is_some_and(|(ns, path)| {
            !ns.is_empty()
                && !path.is_empty()
                && !matches!(ns, "minecraft" | "horizon")
                && ns
                    .bytes()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || b"_.-".contains(&c))
                && path
                    .bytes()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || b"_.-/".contains(&c))
        })
}
fn local(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v) => v.is_private() || v.is_loopback() || v.is_link_local(),
        IpAddr::V6(v) => v
            .to_ipv4_mapped()
            .map(|v| local(IpAddr::V4(v)))
            .unwrap_or_else(|| v.is_loopback() || v.is_unique_local() || v.is_unicast_link_local()),
    }
}

#[derive(Default)]
pub struct Services {
    mods: BTreeMap<String, Voice>,
    session: Session,
    announced: BTreeSet<String>,
}
impl Services {
    pub fn is_empty(&self) -> bool {
        self.mods.is_empty()
    }
    pub fn register(&mut self, declaration: Declaration) -> Result<(), String> {
        if [
            &declaration.title,
            &declaration.author,
            &declaration.version,
            &declaration.purpose,
        ]
        .iter()
        .any(|s| s.trim().is_empty() || s.chars().any(char::is_control))
            || declaration.id.is_empty()
            || declaration.id.len() > 64
            || !declaration
                .id
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_')
            || declaration.title.len() > 128
            || declaration.author.len() > 128
            || declaration.version.len() > 64
            || declaration.purpose.len() > 512
        {
            return Err("invalid native service declaration".into());
        }
        if self
            .mods
            .get(&declaration.id)
            .is_some_and(|v| v.declaration == declaration)
        {
            return Ok(());
        }
        if self.mods.len() >= 32 && !self.mods.contains_key(&declaration.id) {
            return Err("native voice mod limit reached".into());
        }
        let mut voice = Voice::new(declaration.clone());
        voice.session = self.session.clone();
        if voice.session.connected {
            voice.event(Event::Connected(voice.session.clone()));
        }
        self.mods.insert(declaration.id.clone(), voice);
        Ok(())
    }
    pub fn retain(&mut self, ids: &BTreeSet<String>) {
        self.mods.retain(|id, _| ids.contains(id));
    }
    pub fn voice(&mut self, id: &str) -> Result<&mut Voice, String> {
        self.mods
            .get_mut(id)
            .ok_or_else(|| "native voice services are not registered or mod is disabled".into())
    }
    pub fn sync(&mut self, session: Session, listener: [f64; 3], yaw: f32, master: f32) {
        let changed = self.session.connection != session.connection
            || (
                self.session.connected,
                &self.session.server,
                &self.session.local_player,
                self.session.server_side,
            ) != (
                session.connected,
                &session.server,
                &session.local_player,
                session.server_side,
            );
        for voice in self.mods.values_mut() {
            if changed {
                voice.revoke();
                if self.session.connected {
                    voice.event(Event::Disconnected);
                }
                if session.connected {
                    voice.event(Event::Connected(session.clone()));
                }
            }
            for id in session
                .players
                .iter()
                .filter(|id| !self.session.players.contains(id))
            {
                voice.event(Event::PlayerJoined(id.clone()));
            }
            for id in self
                .session
                .players
                .iter()
                .filter(|id| !session.players.contains(id))
            {
                voice.event(Event::PlayerLeft(id.clone()));
            }
            voice.session = session.clone();
            voice.bytes_left = 65536;
            voice.packets_left = 32;
            voice.entropy = 4096;
            voice.pcm.listener(listener, yaw);
            voice.pcm.master_gain(master);
        }
        if changed {
            self.announced.clear();
        }
        self.session = session;
    }
    pub fn deliver(&mut self, player: Option<&str>, channel: &str, data: &[u8]) {
        if data.len() > 32767 {
            return;
        }
        for voice in self
            .mods
            .values_mut()
            .filter(|v| v.channels.contains(channel))
        {
            voice.event(Event::Packet(Message {
                player: player.map(str::to_owned),
                channel: channel.into(),
                data: data.to_vec(),
            }));
        }
    }
    pub fn announcements(&mut self, mut extra: BTreeSet<String>) -> Vec<Message> {
        if !self.session.connected || self.session.server_side {
            return Vec::new();
        }
        extra.extend(self.mods.values().flat_map(|v| v.channels.iter().cloned()));
        let mut out = Vec::new();
        for (channel, changed) in [
            (
                "minecraft:register",
                extra
                    .difference(&self.announced)
                    .cloned()
                    .collect::<Vec<_>>(),
            ),
            (
                "minecraft:unregister",
                self.announced
                    .difference(&extra)
                    .cloned()
                    .collect::<Vec<_>>(),
            ),
        ] {
            for chunk in changed.chunks(128) {
                out.push(Message {
                    player: None,
                    channel: channel.into(),
                    data: chunk.join("\0").into_bytes(),
                });
            }
        }
        self.announced = extra;
        out
    }
    pub fn drain_out(&mut self) -> Vec<Message> {
        self.mods
            .values_mut()
            .flat_map(|v| std::mem::take(&mut v.out))
            .collect()
    }
    /// Labels identify the mod declaration and exact scope. Session only.
    pub fn permission_description(&self, id: &str) -> String {
        let Some(v) = self.mods.get(id) else {
            return String::new();
        };
        format!(
            "{} {} by {}. {}. Server: {}. Grants last only for this connection. Native code runs with OS rights; these controls govern Horizon helpers only.",
            v.declaration.title,
            v.declaration.version,
            v.declaration.author,
            v.declaration.purpose,
            self.session.server.as_deref().unwrap_or("local world")
        )
    }
    pub fn capturing(&self) -> Vec<String> {
        self.mods
            .values()
            .filter(|v| v.pcm.is_capturing())
            .map(|v| v.declaration.title.clone())
            .collect()
    }
    pub fn permission_buttons(&self, id: &str) -> Vec<String> {
        let Some(v) = self.mods.get(id) else {
            return Vec::new();
        };
        let d = &v.declaration;
        let mut buttons = vec![format!(
            "Microphone: {} (session)",
            if v.microphone_allowed {
                "Allowed — stop/revoke"
            } else {
                "Blocked — allow raw audio"
            }
        )];
        if !d.microphone {
            buttons[0] = "Microphone: not requested".into();
        }
        for e in &v.endpoints {
            let kind = e
                .host
                .parse::<IpAddr>()
                .map(|ip| {
                    if local(ip) {
                        "LOCAL NETWORK UDP"
                    } else {
                        "PUBLIC UDP"
                    }
                })
                .unwrap_or("DNS (system resolver)");
            buttons.push(format!(
                "{} {}:{} — {}",
                kind,
                e.host,
                e.port,
                if e.allowed {
                    "Allowed — revoke"
                } else {
                    "Allow this session"
                }
            ));
        }
        buttons
    }
    /// Called only from the game's own Options button handler, never automatically by a packet.
    pub fn permission_click(&mut self, id: &str, button: usize) {
        let Some(v) = self.mods.get_mut(id) else {
            return;
        };
        if !v.session.connected {
            return;
        }
        if button == 0 {
            if v.declaration.microphone && !v.session.server_side {
                v.microphone_allowed = !v.microphone_allowed;
                if !v.microphone_allowed {
                    v.pcm.stop();
                }
            }
        } else if let Some(e) = v.endpoints.get_mut(button - 1) {
            e.allowed = !e.allowed;
            if !e.allowed {
                v.transport.reset();
                v.resolving = None;
            }
        } else {
            return;
        }
        v.event(Event::PermissionsChanged);
    }
    /// Focus-loss barrier: stop now, before another key event can start capture.
    pub fn stop_capture(&mut self) {
        self.session.focused = false;
        for v in self.mods.values_mut() {
            v.session.focused = false;
            v.pcm.stop();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn declaration(id: &str) -> Declaration {
        Declaration {
            id: id.into(),
            title: id.into(),
            author: "test".into(),
            version: "1".into(),
            purpose: "voice test".into(),
            microphone: true,
            audio: true,
        }
    }
    fn services() -> Services {
        let mut s = Services::default();
        s.register(declaration("one")).unwrap();
        s.register(declaration("two")).unwrap();
        s.sync(
            Session {
                connected: true,
                server: Some("example.test:25565".into()),
                focused: true,
                ..Session::default()
            },
            [0.0; 3],
            0.0,
            1.0,
        );
        s
    }
    #[test]
    fn denials_do_not_open_devices_or_resolve_and_grants_are_per_mod() {
        let mut s = services();
        let v = s.voice("one").unwrap();
        assert!(v.input_devices().is_empty());
        assert!(v.start_capture("".into(), 1.0).is_err());
        assert!(v.resolve("example.test", 24454).is_err());
        assert!(v.resolving.is_none());
        assert!(v.connect_udp("127.0.0.1:24454").is_err());
        v.request_endpoint("example.test", 24454).unwrap();
        v.request_endpoint("example.test", 24454).unwrap();
        assert_eq!(v.endpoints.len(), 1);
        s.permission_click("one", 0);
        assert!(s.voice("one").unwrap().microphone_allowed);
        assert!(!s.voice("two").unwrap().microphone_allowed);
    }
    #[test]
    fn reconnect_to_the_same_server_revokes_session_consent() {
        let mut s = services();
        s.permission_click("one", 0);
        let mut next = s.session.clone();
        next.connection += 1;
        s.sync(next, [0.0; 3], 0.0, 1.0);
        assert!(!s.voice("one").unwrap().microphone_allowed);
        assert!(
            s.voice("one")
                .unwrap()
                .poll()
                .iter()
                .any(|e| matches!(e, Event::Disconnected))
        );
    }
    #[test]
    fn exact_endpoints_do_not_inherit_ports_or_numeric_grants_from_dns() {
        let mut s = services();
        s.voice("one")
            .unwrap()
            .request_endpoint("example.test", 24454)
            .unwrap();
        s.permission_click("one", 1);
        let v = s.voice("one").unwrap();
        assert!(v.resolve("example.test", 24455).is_err());
        assert!(v.connect_udp("127.0.0.1:24454").is_err());
        assert!(local("::ffff:127.0.0.1".parse().unwrap()));
        assert!(local("fd00::1".parse().unwrap()));
        assert!(!local("2001:4860:4860::8888".parse().unwrap()));
        assert!(v.request_endpoint("224.0.0.1", 24454).is_err());
        assert!(v.request_endpoint("169.254.169.254", 80).is_err());
    }
    #[test]
    fn disconnect_and_new_bundle_discard_grants_queues_and_pending_work() {
        let mut s = services();
        s.permission_click("one", 0);
        let v = s.voice("one").unwrap();
        v.request_endpoint("127.0.0.1", 24454).unwrap();
        v.register_channel("test:voice").unwrap();
        v.send_payload(None, "test:voice", vec![1]).unwrap();
        v.resolving = Some(("test".into(), 1, Instant::now()));
        s.sync(Session::default(), [0.0; 3], 0.0, 1.0);
        let v = s.voice("one").unwrap();
        assert!(!v.microphone_allowed);
        assert!(v.endpoints.is_empty());
        assert!(v.resolving.is_none());
        assert!(v.out.is_empty());
        assert!(v.poll().contains(&Event::Disconnected));
        s.sync(
            Session {
                connected: true,
                focused: true,
                ..Session::default()
            },
            [0.0; 3],
            0.0,
            1.0,
        );
        s.permission_click("one", 0);
        let mut d = declaration("one");
        d.version = "2".into();
        s.register(d).unwrap();
        assert!(!s.voice("one").unwrap().microphone_allowed);
        assert!(!Services::default().mods.contains_key("one"));
    }
    #[test]
    fn payload_identity_budgets_channels_and_announcements() {
        let mut s = services();
        s.voice("one")
            .unwrap()
            .register_channel("test:voice")
            .unwrap();
        assert!(
            s.voice("one")
                .unwrap()
                .register_channel("minecraft:brand")
                .is_err()
        );
        let first = s.announcements(BTreeSet::from(["wasm:voice".into()]));
        assert_eq!(first[0].data, b"test:voice\0wasm:voice");
        assert!(
            s.announcements(BTreeSet::from(["wasm:voice".into()]))
                .is_empty()
        );
        let payload = vec![1; 32767];
        for _ in 0..100 {
            s.deliver(Some("sender"), "test:voice", &payload);
        }
        let v = s.voice("one").unwrap();
        assert!(v.event_bytes <= 256 * 1024);
        assert!(v.events.len() <= 64);
        assert!(
            s.voice("two")
                .unwrap()
                .poll()
                .iter()
                .all(|e| !matches!(e, Event::Packet(_)))
        );
        s.sync(
            Session {
                connected: true,
                server_side: true,
                players: vec!["uuid".into()],
                ..Session::default()
            },
            [0.0; 3],
            0.0,
            1.0,
        );
        let v = s.voice("one").unwrap();
        assert!(
            v.send_payload(Some("other".into()), "test:voice", vec![])
                .is_err()
        );
        v.send_payload(Some("uuid".into()), "test:voice", vec![1])
            .unwrap();
        assert!(v.send_payload(None, "test:voice", vec![]).is_err());
        for _ in 0..4 {
            assert_eq!(v.random_bytes(1024).unwrap().len(), 1024);
        }
        assert!(v.random_bytes(1).is_err());
    }
    #[test]
    fn revoked_dns_completion_cannot_survive_and_focus_blocks_capture() {
        let mut s = services();
        let v = s.voice("one").unwrap();
        v.request_endpoint("example.test", 24454).unwrap();
        s.permission_click("one", 1);
        s.voice("one").unwrap().resolving = Some(("example.test".into(), 24454, Instant::now()));
        s.permission_click("one", 1);
        assert!(s.voice("one").unwrap().resolving.is_none());
        assert!(s.voice("one").unwrap().resolved().unwrap().is_none());
        s.permission_click("one", 0);
        s.stop_capture();
        assert!(!s.voice("one").unwrap().session.focused);
        assert!(
            s.voice("one")
                .unwrap()
                .start_capture("".into(), 1.0)
                .is_err()
        );
    }
}
