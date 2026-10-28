//! Optional capabilities: no WASI sockets, files or device handles reach guests.
use super::{Output, State};
use std::{
    collections::{BTreeSet, HashMap, VecDeque},
    net::{SocketAddr, UdpSocket},
    time::{Duration, Instant},
};

wasmtime::component::bindgen!({
    path: "../horizon-mod-sdk/wit",
    world: "realtime-host",
});
pub use horizon::mods::{audio, microphone, networking, udp};
mod pcm;
pub use pcm::Pcm;

const DATAGRAM: usize = 1200;
const PACKET_RATE: usize = 256;
const PAYLOAD: usize = 32767;
const INBOX_BYTES: usize = 256 << 10;

pub(super) struct Realtime {
    pub pcm: Pcm,
    sockets: HashMap<u32, UdpSocket>,
    next: u32,
    window: Instant,
    sent: usize,
    received: usize,
    opened: usize,
    pub channels: BTreeSet<String>,
    pub connections: VecDeque<networking::Event>,
    messages: VecDeque<networking::Message>,
    bytes: usize,
    pub budget: usize,
    pub packets: usize,
    pub entropy: usize,
    resolver: Option<std::sync::mpsc::Receiver<Result<Vec<String>, String>>>,
}
impl Default for Realtime {
    fn default() -> Self {
        Self {
            pcm: Pcm::default(),
            sockets: HashMap::new(),
            next: 0,
            window: Instant::now(),
            sent: 0,
            received: 0,
            opened: 0,
            channels: BTreeSet::new(),
            connections: VecDeque::new(),
            messages: VecDeque::new(),
            bytes: 0,
            budget: 64 << 10,
            packets: 32,
            entropy: 4096,
            resolver: None,
        }
    }
}
impl Realtime {
    fn random_bytes(&mut self, length: u32) -> Result<Vec<u8>, String> {
        let length = length as usize;
        if length > 1024 || length > self.entropy {
            return Err("random byte budget exceeded".into());
        }
        self.entropy -= length;
        let mut bytes = vec![0; length];
        getrandom::getrandom(&mut bytes).map_err(|e| e.to_string())?;
        Ok(bytes)
    }
    fn window(&mut self) {
        if self.window.elapsed() >= Duration::from_secs(1) {
            self.window = Instant::now();
            self.sent = 0;
            self.received = 0;
            self.opened = 0;
        }
    }
    pub fn reset_connection(&mut self) {
        self.resolver = None;
        self.sockets.clear();
        self.messages.clear();
        self.bytes = 0;
        self.pcm.stop();
        self.pcm.clear_sources();
    }
    pub fn deliver(&mut self, player: Option<&str>, channel: &str, data: &[u8]) {
        if !self.channels.contains(channel) || data.len() > PAYLOAD {
            return;
        }
        while self.messages.len() >= 64 || self.bytes + data.len() > INBOX_BYTES {
            if let Some(old) = self.messages.pop_front() {
                self.bytes -= old.data.len();
            } else {
                break;
            }
        }
        self.bytes += data.len();
        self.messages.push_back(networking::Message {
            player: player.map(str::to_owned),
            channel: channel.into(),
            data: data.to_vec(),
        });
    }
}

fn endpoint(address: &str) -> Result<SocketAddr, String> {
    let address: SocketAddr = address
        .parse()
        .map_err(|_| "expected a numeric IP address and port")?;
    let ip = address.ip();
    // Mapped IPv4 must obey the same policy as an ordinary IPv4 address.
    let ip = match ip {
        std::net::IpAddr::V6(v) => v.to_ipv4_mapped().map(std::net::IpAddr::V4).unwrap_or(ip),
        _ => ip,
    };
    let forbidden = match ip {
        std::net::IpAddr::V4(ip) => {
            ip.is_broadcast() || ip.is_link_local() || ip.octets()[0] == 0 || ip.octets()[0] >= 224
        }
        std::net::IpAddr::V6(ip) => {
            ip.is_unicast_link_local() || matches!(address, SocketAddr::V6(v) if v.scope_id() != 0)
        }
    };
    if address.port() == 0 || ip.is_unspecified() || ip.is_multicast() || forbidden {
        return Err("UDP requires a unicast endpoint with a nonzero port".into());
    }
    Ok(address)
}

static RESOLVERS: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
struct ResolverSlot;
impl Drop for ResolverSlot {
    fn drop(&mut self) {
        RESOLVERS.fetch_sub(1, std::sync::atomic::Ordering::Relaxed);
    }
}
impl Realtime {
    fn resolve(&mut self, host: String) -> Result<(), String> {
        use std::sync::atomic::Ordering;
        if self.resolver.is_some() {
            return Err("DNS lookup already pending; consume resolved first".into());
        }
        if host.is_empty()
            || host.len() > 253
            || !host
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b".-:".contains(&b))
        {
            return Err("invalid DNS hostname or numeric IP".into());
        }
        self.window();
        if self.opened >= 16 {
            return Err("UDP connect/resolve rate exceeded".into());
        }
        // Keep the Rust 1.95 baseline without using fetch_update, which is
        // deprecated on newer toolchains in favor of try_update.
        let mut active = RESOLVERS.load(Ordering::Relaxed);
        loop {
            if active >= 4 {
                return Err("DNS worker limit reached".into());
            }
            match RESOLVERS.compare_exchange_weak(
                active,
                active + 1,
                Ordering::Relaxed,
                Ordering::Relaxed,
            ) {
                Ok(_) => break,
                Err(current) => active = current,
            }
        }
        let slot = ResolverSlot;
        let (tx, rx) = std::sync::mpsc::sync_channel(1);
        std::thread::Builder::new()
            .name("mod DNS".into())
            .spawn(move || {
                use std::net::ToSocketAddrs;
                let _slot = slot;
                let result = (host.as_str(), 1)
                    .to_socket_addrs()
                    .map_err(|e| e.to_string())
                    .and_then(|addresses| {
                        let ips: BTreeSet<_> = addresses
                            .take(64)
                            .filter(|a| endpoint(&a.to_string()).is_ok())
                            .map(|a| a.ip().to_string())
                            .collect();
                        if ips.is_empty() {
                            Err("DNS returned no permitted unicast addresses".into())
                        } else {
                            Ok(ips.into_iter().collect())
                        }
                    });
                let _ = tx.send(result);
            })
            .map_err(|e| e.to_string())?;
        self.opened += 1;
        self.resolver = Some(rx);
        Ok(())
    }
    fn resolved(&mut self) -> Result<Option<Vec<String>>, String> {
        use std::sync::mpsc::TryRecvError;
        let Some(rx) = &self.resolver else {
            return Ok(None);
        };
        match rx.try_recv() {
            Ok(result) => {
                self.resolver = None;
                result.map(Some)
            }
            Err(TryRecvError::Empty) => Ok(None),
            Err(TryRecvError::Disconnected) => {
                self.resolver = None;
                Err("DNS worker stopped".into())
            }
        }
    }
}
impl udp::Host for State {
    fn resolve(&mut self, host: String) -> Result<(), String> {
        if !self.snapshot.connected {
            return Err("not connected to a Minecraft server".into());
        }
        self.realtime.resolve(host)
    }
    fn resolved(&mut self) -> Result<Option<Vec<String>>, String> {
        self.realtime.resolved()
    }

    fn connect(&mut self, address: String) -> Result<u32, String> {
        if !self.snapshot.connected {
            return Err("not connected to a Minecraft server".into());
        }
        let r = &mut self.realtime;
        r.window();
        if r.opened >= 16 {
            return Err("UDP connect rate exceeded".into());
        }
        r.opened += 1;
        if r.sockets.len() >= 4 {
            return Err("UDP socket limit reached".into());
        }
        let address = endpoint(&address)?;
        let socket = UdpSocket::bind(if address.is_ipv4() {
            "0.0.0.0:0"
        } else {
            "[::]:0"
        })
        .map_err(|e| e.to_string())?;
        socket.set_nonblocking(true).map_err(|e| e.to_string())?;
        socket.connect(address).map_err(|e| e.to_string())?;
        r.next = r.next.checked_add(1).ok_or("UDP handle limit reached")?;
        r.sockets.insert(r.next, socket);
        Ok(r.next)
    }
    fn send(&mut self, socket: u32, data: Vec<u8>) -> Result<(), String> {
        let r = &mut self.realtime;
        r.window();
        if data.len() > DATAGRAM {
            return Err("UDP datagram exceeds 1200 bytes".into());
        }
        if r.sent >= PACKET_RATE {
            return Err("UDP send rate exceeded".into());
        }
        let socket = r.sockets.get(&socket).ok_or("unknown UDP socket")?;
        r.sent += 1;
        socket.send(&data).map(|_| ()).map_err(|e| e.to_string())
    }
    fn receive(&mut self, socket: u32) -> Result<Vec<Vec<u8>>, String> {
        let r = &mut self.realtime;
        r.window();
        let socket = r.sockets.get(&socket).ok_or("unknown UDP socket")?;
        let mut packets = Vec::new();
        // Extra byte distinguishes a truncated oversized packet: discard it.
        let mut data = [0; DATAGRAM + 1];
        for _ in 0..32.min(PACKET_RATE.saturating_sub(r.received)) {
            match socket.recv(&mut data) {
                Ok(n) => {
                    r.received += 1;
                    if n <= DATAGRAM {
                        packets.push(data[..n].to_vec());
                    }
                }
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

fn channel_ok(channel: &str) -> bool {
    let Some((namespace, path)) = channel.split_once(':') else {
        return false;
    };
    channel.len() <= 128
        && !namespace.is_empty()
        && !path.is_empty()
        && !matches!(namespace, "minecraft" | "horizon")
        && namespace
            .bytes()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || b"_.-".contains(&c))
        && path
            .bytes()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || b"_.-/".contains(&c))
}
impl networking::Host for State {
    fn register(&mut self, channel: String) -> Result<(), String> {
        if !channel_ok(&channel) {
            return Err("expected a non-reserved namespaced channel (max 128 bytes)".into());
        }
        let r = &mut self.realtime;
        if r.channels.len() >= 32 && !r.channels.contains(&channel) {
            return Err("channel limit reached".into());
        }
        r.channels.insert(channel);
        Ok(())
    }
    fn unregister(&mut self, channel: String) {
        let r = &mut self.realtime;
        r.channels.remove(&channel);
        r.messages.retain(|m| m.channel != channel);
        r.bytes = r.messages.iter().map(|m| m.data.len()).sum();
    }
    fn send(
        &mut self,
        player: Option<String>,
        channel: String,
        data: Vec<u8>,
    ) -> Result<(), String> {
        let r = &mut self.realtime;
        if !r.channels.contains(&channel) {
            return Err("channel is not registered".into());
        }
        if self.server {
            if !player
                .as_ref()
                .is_some_and(|id| self.snapshot.players.iter().any(|p| p.id == *id))
            {
                return Err("expected an online player's UUID".into());
            }
        } else if player.is_some() || !self.snapshot.connected {
            return Err("client messages require a connection and no player target".into());
        }
        if data.len() > PAYLOAD || data.len() > r.budget || r.packets == 0 {
            return Err("custom payload budget exceeded".into());
        }
        r.budget -= data.len();
        r.packets -= 1;
        self.out.push(Output::Payload {
            player,
            channel,
            data,
        });
        Ok(())
    }
    fn poll(&mut self) -> Vec<networking::Event> {
        self.realtime.bytes = 0;
        self.realtime
            .connections
            .drain(..)
            .chain(
                self.realtime
                    .messages
                    .drain(..)
                    .map(networking::Event::Packet),
            )
            .collect()
    }
    fn local_player(&mut self) -> Option<String> {
        self.snapshot.local_player.clone()
    }
    fn server_address(&mut self) -> Option<String> {
        self.snapshot.server_address.clone()
    }
    fn random_bytes(&mut self, length: u32) -> Result<Vec<u8>, String> {
        self.realtime.random_bytes(length)
    }
}

impl microphone::Host for State {
    fn devices(&mut self) -> Vec<microphone::Device> {
        if !self.microphone_allowed {
            return Vec::new();
        }
        self.realtime.pcm.input_devices()
    }
    fn start(&mut self, device: String, gain: f32) -> Result<(), String> {
        if !self.microphone_allowed {
            return Err("microphone permission denied: allow recording in Options > Mods".into());
        }
        self.realtime.pcm.start(device, gain)
    }
    fn stop(&mut self) {
        self.realtime.pcm.stop();
    }
    fn set_gain(&mut self, gain: f32) -> Result<(), String> {
        self.realtime.pcm.set_gain(gain)
    }
    fn read(&mut self) -> Result<Vec<microphone::Frame>, String> {
        self.realtime.pcm.read()
    }
}
impl audio::Host for State {
    fn devices(&mut self) -> Vec<audio::Device> {
        self.realtime.pcm.output_devices()
    }
    fn select_device(&mut self, device: String) -> Result<(), String> {
        self.realtime.pcm.select_device(device)
    }
    fn create(&mut self, sample_rate: u32) -> Result<u32, String> {
        self.realtime.pcm.create(sample_rate)
    }
    fn write(&mut self, source: u32, samples: Vec<f32>) -> Result<(), String> {
        self.realtime.pcm.write(source, samples)
    }
    fn configure(&mut self, source: u32, settings: audio::Settings) -> Result<(), String> {
        self.realtime.pcm.configure(source, settings)
    }
    fn close(&mut self, source: u32) {
        self.realtime.pcm.close(source);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn cryptographic_randomness_is_bounded_per_hook() {
        let mut r = Realtime::default();
        assert!(r.random_bytes(1025).is_err());
        assert_eq!(r.entropy, 4096);
        for _ in 0..4 {
            assert_eq!(r.random_bytes(1024).unwrap().len(), 1024);
        }
        assert!(r.random_bytes(1).is_err());
        assert_eq!(r.entropy, 0);
    }
    #[test]
    fn asynchronous_resolution_filters_targets_and_consumes_results() {
        let mut r = Realtime::default();
        assert!(r.resolve("bad host/path".into()).is_err());
        for (host, permitted) in [("127.0.0.1", true), ("0.0.0.0", false)] {
            r.resolve(host.into()).unwrap();
            assert!(r.resolve(host.into()).is_err());
            let deadline = Instant::now() + Duration::from_secs(2);
            loop {
                match r.resolved() {
                    Ok(None) => {
                        assert!(Instant::now() < deadline);
                        std::thread::sleep(Duration::from_millis(1));
                    }
                    Ok(Some(ips)) => {
                        assert!(permitted);
                        assert_eq!(ips, [host]);
                        break;
                    }
                    Err(_) => {
                        assert!(!permitted);
                        break;
                    }
                }
            }
            assert!(r.resolved().unwrap().is_none());
        }
    }
    #[test]
    fn endpoints_require_unicast_and_no_dns() {
        for bad in [
            "example.com:24454",
            "0.0.0.0:1",
            "127.0.0.1:0",
            "255.255.255.255:1",
            "224.0.0.1:1",
            "[ff02::1]:1",
            "169.254.169.254:80",
            "[::ffff:224.0.0.1]:1",
            "[fe80::1]:1",
        ] {
            assert!(endpoint(bad).is_err(), "{bad}");
        }
        for good in ["127.0.0.1:24454", "192.168.1.2:24454", "[::1]:24454"] {
            assert!(endpoint(good).is_ok(), "{good}");
        }
    }
    #[test]
    fn channels_and_inbox_are_bounded() {
        assert!(channel_ok("voicechat:request_secret"));
        for bad in [
            "minecraft:brand",
            "horizon:quiet",
            "X:y",
            "x:",
            ":y",
            "x:y:z",
        ] {
            assert!(!channel_ok(bad));
        }
        let mut r = Realtime::default();
        r.channels.insert("test:voice".into());
        r.deliver(None, "test:other", &[1]);
        assert!(r.messages.is_empty());
        for n in 0..100 {
            r.deliver(Some("sender"), "test:voice", &[n; PAYLOAD]);
        }
        assert!(r.bytes <= INBOX_BYTES);
        assert_eq!(r.messages.back().unwrap().data[0], 99);
        r.reset_connection();
        assert!(r.messages.is_empty());
        assert_eq!(r.bytes, 0);
    }
}
