//! Bounded connected UDP and DNS workers, shared by native and WASM hosts.
use std::{
    collections::{BTreeSet, HashMap},
    net::{SocketAddr, UdpSocket},
    time::{Duration, Instant},
};
const DATAGRAM: usize = 1200;
const PACKET_RATE: usize = 256;
pub(crate) struct Datagrams {
    sockets: HashMap<u32, UdpSocket>,
    next: u32,
    window: Instant,
    sent: usize,
    received: usize,
    opened: usize,
    resolver: Option<std::sync::mpsc::Receiver<Result<Vec<String>, String>>>,
}
impl Default for Datagrams {
    fn default() -> Self {
        Self {
            sockets: HashMap::new(),
            next: 0,
            window: Instant::now(),
            sent: 0,
            received: 0,
            opened: 0,
            resolver: None,
        }
    }
}
impl Datagrams {
    fn window(&mut self) {
        if self.window.elapsed() >= Duration::from_secs(1) {
            self.window = Instant::now();
            self.sent = 0;
            self.received = 0;
            self.opened = 0;
        }
    }
    pub fn cancel_resolution(&mut self) {
        self.resolver = None;
    }
    pub fn reset(&mut self) {
        self.sockets.clear();
        self.resolver = None;
    }
}
pub(crate) fn endpoint(address: &str) -> Result<SocketAddr, String> {
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
impl Datagrams {
    pub fn resolve(&mut self, host: String) -> Result<(), String> {
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
    pub fn resolved(&mut self) -> Result<Option<Vec<String>>, String> {
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
impl Datagrams {
    pub fn connect(&mut self, address: String) -> Result<u32, String> {
        let r = self;
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
    pub fn send(&mut self, socket: u32, data: Vec<u8>) -> Result<(), String> {
        let r = self;
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
    pub fn receive(&mut self, socket: u32) -> Result<Vec<Vec<u8>>, String> {
        let r = self;
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
    pub fn close(&mut self, socket: u32) {
        self.sockets.remove(&socket);
    }
}
