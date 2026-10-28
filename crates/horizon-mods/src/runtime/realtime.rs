//! Permission checked, quota bounded real-time services owned by one sandbox.
use super::{HasSelf, Linker, Mods, Output, State};
use crate::Capability;
use std::{
    collections::{BTreeMap, BTreeSet, VecDeque},
    net::{SocketAddr, UdpSocket},
};
mod sound;
pub(super) mod bindings {
    wasmtime::component::bindgen!({
        path: "../horizon-mod-sdk/wit",
        world: "realtime-host",
    });
}
use bindings::horizon::mods::{audio, messaging, microphone, network};

pub(super) fn link(linker: &mut Linker<State>) -> Result<(), String> {
    bindings::RealtimeHost::add_to_linker::<State, HasSelf<State>>(linker, |s| s)
        .map_err(|e| e.to_string())
}

pub(super) struct Realtime {
    permissions: Vec<Capability>,
    server: bool,
    sockets: BTreeMap<u32, UdpSocket>,
    next: u32,
    channels: BTreeSet<String>,
    packets: VecDeque<messaging::Packet>,
    budget: usize,
    window: std::time::Instant,
    sound: sound::Sound,
}
impl Realtime {
    pub fn new(permissions: Vec<Capability>, server: bool) -> Self {
        Self {
            permissions,
            server,
            sockets: BTreeMap::new(),
            next: 0,
            channels: BTreeSet::new(),
            packets: VecDeque::new(),
            budget: 131072,
            window: std::time::Instant::now(),
            sound: sound::Sound::default(),
        }
    }
    pub(super) fn prune(&mut self, snapshot: &super::Snapshot) {
        if self.server {
            self.packets
                .retain(|packet| snapshot.players.iter().any(|p| p.id == packet.peer));
        } else if snapshot.player.is_none() {
            self.sound.suspend();
            self.packets.clear();
        }
    }
    fn require(&self, cap: Capability) -> Result<(), String> {
        self.permissions
            .contains(&cap)
            .then_some(())
            .ok_or_else(|| format!("permission {cap} denied"))
    }
    fn spend(&mut self, bytes: usize) -> Result<(), String> {
        if self.window.elapsed() >= std::time::Duration::from_millis(50) {
            self.window = std::time::Instant::now();
            self.budget = 131072;
        }
        self.budget = self
            .budget
            .checked_sub(bytes.max(256))
            .ok_or("real-time I/O budget exceeded")?;
        Ok(())
    }
    fn udp(&mut self) -> Result<(), String> {
        self.require(Capability::Network)?;
        self.require(Capability::Udp)?;
        self.spend(256)
    }
}
impl State {
    pub(super) fn update_listener(&mut self) {
        if let Some(p) = &self.snapshot.player {
            self.realtime.sound.listener([p.x, p.y, p.z], p.yaw);
        } else {
            self.realtime.sound.suspend();
        }
    }
}
impl microphone::Host for State {
    fn request_permission(&mut self) -> bool {
        self.realtime.require(Capability::Microphone).is_ok()
    }
    fn devices(&mut self) -> Result<Vec<microphone::Device>, String> {
        self.realtime.require(Capability::Microphone)?;
        self.realtime.spend(4096)?;
        self.realtime.sound.devices().map(|d| {
            d.into_iter()
                .map(|(id, name)| microphone::Device { id, name })
                .collect()
        })
    }
    fn start(&mut self, device: Option<String>) -> Result<(u32, u16), String> {
        self.realtime.require(Capability::Microphone)?;
        self.realtime.spend(4096)?;
        self.realtime.sound.start(device)
    }
    fn read(&mut self) -> Vec<f32> {
        if self.realtime.require(Capability::Microphone).is_err()
            || self.realtime.spend(32768).is_err()
        {
            return Vec::new();
        }
        self.realtime.sound.read()
    }
    fn stop(&mut self) {
        self.realtime.sound.stop();
    }
}
impl audio::Host for State {
    fn create(&mut self, rate: u32, channels: u16) -> Result<u32, String> {
        self.realtime.require(Capability::Audio)?;
        self.realtime.spend(4096)?;
        self.realtime.sound.create(rate, channels)
    }
    fn push(&mut self, source: u32, pcm: Vec<f32>) -> Result<(), String> {
        self.realtime.require(Capability::Audio)?;
        self.realtime.spend(pcm.len().saturating_mul(4))?;
        self.realtime.sound.push(source, pcm)
    }
    fn configure(
        &mut self,
        source: u32,
        gain: f32,
        at: Option<audio::Position>,
        radius: f32,
    ) -> Result<(), String> {
        self.realtime.require(Capability::Audio)?;
        self.realtime.spend(256)?;
        self.realtime
            .sound
            .configure(source, gain, at.map(|p| [p.x, p.y, p.z]), radius)
    }
    fn remove(&mut self, source: u32) {
        self.realtime.sound.remove(source);
    }
}
impl network::Host for State {
    fn connect(&mut self, address: String) -> Result<u32, String> {
        self.realtime.udp()?;
        if self.realtime.sockets.len() >= 4 {
            return Err("socket limit reached".into());
        }
        let peer: SocketAddr = address.parse().map_err(|_| "expected numeric IP:port")?;
        if peer.port() == 0
            || peer.ip().is_unspecified()
            || peer.ip().is_multicast()
            || peer.ip() == std::net::Ipv4Addr::BROADCAST
        {
            return Err("expected unicast peer with nonzero port".into());
        }
        let socket = UdpSocket::bind(if peer.is_ipv4() {
            "0.0.0.0:0"
        } else {
            "[::]:0"
        })
        .map_err(|e| e.to_string())?;
        socket.set_nonblocking(true).map_err(|e| e.to_string())?;
        socket.connect(peer).map_err(|e| e.to_string())?;
        self.realtime.next = self
            .realtime
            .next
            .checked_add(1)
            .ok_or("handle limit reached")?;
        self.realtime.sockets.insert(self.realtime.next, socket);
        Ok(self.realtime.next)
    }
    fn send(&mut self, socket: u32, data: Vec<u8>) -> Result<(), String> {
        self.realtime.udp()?;
        if data.len() > 1200 {
            return Err("datagram exceeds 1200 bytes".into());
        }
        self.realtime.spend(data.len())?;
        self.realtime
            .sockets
            .get(&socket)
            .ok_or("unknown socket")?
            .send(&data)
            .map(|_| ())
            .map_err(|e| e.to_string())
    }
    fn poll(&mut self, socket: u32) -> Result<Vec<Vec<u8>>, String> {
        self.realtime.udp()?;
        self.realtime.spend(38400)?;
        let socket = self.realtime.sockets.get(&socket).ok_or("unknown socket")?;
        let mut packets = Vec::new();
        for _ in 0..32 {
            let mut buf = [0; 1201];
            match socket.recv(&mut buf) {
                Ok(n) if n <= 1200 => packets.push(buf[..n].to_vec()),
                Ok(_) => {}
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => break,
                Err(e) => return Err(e.to_string()),
            }
        }
        Ok(packets)
    }
    fn close(&mut self, socket: u32) {
        self.realtime.sockets.remove(&socket);
    }
}
impl messaging::Host for State {
    fn register(&mut self, channel: String) -> Result<(), String> {
        self.realtime.require(Capability::Network)?;
        self.realtime.spend(256)?;
        if !valid_channel(&self.id, &channel) {
            return Err("channel must use the mod's namespace".into());
        }
        if self.realtime.channels.len() >= 16 && !self.realtime.channels.contains(&channel) {
            return Err("channel limit reached".into());
        }
        self.realtime.channels.insert(channel);
        Ok(())
    }
    fn send(&mut self, peer: String, channel: String, data: Vec<u8>) -> Result<(), String> {
        self.realtime.require(Capability::Network)?;
        self.realtime.spend(data.len())?;
        if !self.realtime.channels.contains(&channel) || data.len() > 8192 {
            return Err("unregistered channel or payload exceeds 8192 bytes".into());
        }
        let server = self.realtime.server;
        // Server mods without `players` can still address an authenticated online UUID.
        let online = self.snapshot.players.iter().any(|p| p.id == peer);
        if (server && !online) || (!server && !peer.is_empty()) {
            return Err("peer is not connected".into());
        }
        if peer.is_empty() && self.snapshot.player.is_none() {
            return Err("not connected".into());
        }
        self.out.push(Output::ModPacket {
            peer,
            channel,
            data,
        });
        Ok(())
    }
    fn poll(&mut self) -> Vec<messaging::Packet> {
        if self.realtime.require(Capability::Network).is_err() {
            return Vec::new();
        }
        let mut out = Vec::new();
        while !self.realtime.packets.is_empty() && out.len() < 32 {
            let size = self.realtime.packets.front().unwrap().data.len();
            if self.realtime.spend(size).is_err() {
                break;
            }
            out.push(self.realtime.packets.pop_front().unwrap());
        }
        out
    }
}
fn valid_channel(id: &str, channel: &str) -> bool {
    channel
        .strip_prefix(id)
        .and_then(|s| s.strip_prefix(':'))
        .is_some_and(|s| {
            (1..=64).contains(&s.len())
                && s.bytes()
                    .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_' || b == b'-')
        })
}
impl Mods {
    /// Applies the game's master volume to every live mod source.
    pub fn set_audio_gain(&mut self, gain: f32) {
        for run in self.entries.iter_mut().filter_map(|e| e.run.as_mut()) {
            run.store.data_mut().realtime.sound.master(gain);
        }
    }
    /// Called only with authenticated connection identities by the game.
    pub fn receive_mod_packet(&mut self, peer: &str, channel: &str, data: &[u8]) {
        if data.len() > 8192 {
            return;
        }
        for run in self.entries.iter_mut().filter_map(|e| e.run.as_mut()) {
            let rt = &mut run.store.data_mut().realtime;
            if rt.server == peer.is_empty() {
                continue;
            }
            if rt.channels.contains(channel) && rt.packets.len() < 64 {
                rt.packets.push_back(messaging::Packet {
                    peer: peer.into(),
                    channel: channel.into(),
                    data: data.into(),
                });
            }
        }
    }
    /// A connection boundary must not leak queued packets or audio into the next session.
    pub fn clear_realtime_session(&mut self) {
        for run in self.entries.iter_mut().filter_map(|e| e.run.as_mut()) {
            let rt = &mut run.store.data_mut().realtime;
            rt.packets.clear();
            rt.sockets.clear();
            rt.sound.suspend();
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn state(permissions: Vec<Capability>) -> State {
        use super::super::*;
        State {
            name: "Voice".into(),
            id: "voice".into(),
            snapshot: Arc::new(Snapshot::default()),
            out: Vec::new(),
            panel: None,
            ui: UiState::default(),
            shapes: Vec::new(),
            corner: None,
            zoom: None,
            minimap: None,
            far_terrain: None,
            input: InputState::default(),
            browser: BrowserState::default(),
            data_dir: PathBuf::new(),
            storage_budget: 0,
            limits: StoreLimitsBuilder::new().build(),
            world: Arc::new(Mutex::new(None)),
            realtime: Realtime::new(permissions, false),
        }
    }
    #[test]
    fn udp_binary_round_trip_limits_and_close() {
        let peer = UdpSocket::bind("127.0.0.1:0").unwrap();
        peer.set_read_timeout(Some(std::time::Duration::from_secs(1)))
            .unwrap();
        let mut s = state(vec![Capability::Network, Capability::Udp]);
        let handle =
            network::Host::connect(&mut s, peer.local_addr().unwrap().to_string()).unwrap();
        network::Host::send(&mut s, handle, vec![0, 255, 0, 128]).unwrap();
        let mut data = [0; 1201];
        let (n, addr) = peer.recv_from(&mut data).unwrap();
        assert_eq!(&data[..n], &[0, 255, 0, 128]);
        peer.send_to(&[9, 0, 255], addr).unwrap();
        assert_eq!(
            network::Host::poll(&mut s, handle).unwrap(),
            vec![vec![9, 0, 255]]
        );
        assert!(network::Host::send(&mut s, handle, vec![0; 1201]).is_err());
        network::Host::close(&mut s, handle);
        assert!(network::Host::send(&mut s, handle, vec![0]).is_err());
    }
    #[test]
    fn denied_devices_and_foreign_channels() {
        let mut s = state(Vec::new());
        assert!(!microphone::Host::request_permission(&mut s));
        assert!(microphone::Host::devices(&mut s).is_err());
        assert!(microphone::Host::start(&mut s, None).is_err());
        assert!(audio::Host::create(&mut s, 48000, 1).is_err());
        assert!(network::Host::connect(&mut s, "127.0.0.1:24454".into()).is_err());
        s.realtime.permissions.push(Capability::Network);
        assert!(messaging::Host::register(&mut s, "other:voice".into()).is_err());
        messaging::Host::register(&mut s, "voice:pcm".into()).unwrap();
        assert!(
            messaging::Host::send(&mut s, "spoofed-player".into(), "voice:pcm".into(), vec![0])
                .is_err()
        );
        assert!(
            messaging::Host::send(&mut s, String::new(), "voice:pcm".into(), vec![0; 8193])
                .is_err()
        );
    }
    #[test]
    fn namespaces_and_permissions() {
        assert!(valid_channel("voice", "voice:pcm"));
        for c in ["other:pcm", "voice:", "voiceevil:pcm", "voice:../pcm"] {
            assert!(!valid_channel("voice", c));
        }
        let mut rt = Realtime::new(vec![Capability::Udp], false);
        assert!(rt.udp().is_err());
        rt.permissions.push(Capability::Network);
        assert!(rt.udp().is_ok());
        assert!(rt.spend(131072).is_err());
    }
}
