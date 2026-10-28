//! Minecraft server DNS redirects (`ServerRedirectHandler` / `ServerNameResolver`).
//! Port 25565 asks for `_minecraft._tcp.<host>`'s SRV record; no record or a
//! failed query leaves the original address in use. The protocol handshake
//! still uses that original address.
use crate::{Error, Result};
use std::net::{IpAddr, SocketAddr, ToSocketAddrs};

mod dns;

pub(crate) fn resolve(host: &str, port: u16) -> Result<Vec<SocketAddr>> {
    let key = (host.to_ascii_lowercase(), port);
    if let Some(addrs) = cache::get(&key) {
        return Ok(addrs);
    }
    // The SRV question and the original host's own lookup go out together:
    // without a record (most servers) the addresses are ready when the SRV
    // answer is, instead of one lookup after the other.
    let early =
        (port == 25565 && host.parse::<IpAddr>().is_err() && host != "localhost").then(|| {
            let (host, (send, receive)) = (host.to_owned(), std::sync::mpsc::channel());
            let spawned = std::thread::Builder::new()
                .name("horizon-resolve".into())
                .spawn(move || {
                    let _ = send.send(lookup(&host, port));
                });
            spawned.ok().map(|_| receive)
        });
    let mut early = early.flatten();
    let addrs = resolve_with(host, port, srv_redirect, |name, at| match early.take() {
        Some(receive) if name == host && at == port => {
            receive.recv().unwrap_or_else(|_| lookup(name, at))
        }
        _ => lookup(name, at),
    })?;
    cache::put(key, &addrs);
    Ok(addrs)
}

fn lookup(host: &str, port: u16) -> Result<Vec<SocketAddr>> {
    (host, port)
        .to_socket_addrs()
        .map(|addrs| addrs.collect())
        .map_err(|e| Error::new(format!("Unknown host {host}: {e}")))
}

/// The addresses a server name resolved to, kept for a minute: the server
/// list's ping resolves the name, and joining a moment later connects at
/// once instead of asking DNS again. The address that last took a
/// connection goes first.
pub(crate) mod cache {
    use std::{
        collections::HashMap,
        net::SocketAddr,
        sync::Mutex,
        time::{Duration, Instant},
    };

    const TTL: Duration = Duration::from_secs(60);
    type Key = (String, u16);
    static CACHE: Mutex<Option<HashMap<Key, (Instant, Vec<SocketAddr>)>>> = Mutex::new(None);

    pub(crate) fn get(key: &Key) -> Option<Vec<SocketAddr>> {
        let mut cache = CACHE.lock().ok()?;
        let map = cache.get_or_insert_with(HashMap::new);
        match map.get(key) {
            Some((at, addrs)) if at.elapsed() < TTL => Some(addrs.clone()),
            Some(_) => {
                map.remove(key);
                None
            }
            None => None,
        }
    }
    pub(crate) fn put(key: Key, addrs: &[SocketAddr]) {
        if addrs.is_empty() {
            return;
        }
        if let Ok(mut cache) = CACHE.lock() {
            let map = cache.get_or_insert_with(HashMap::new);
            // Keep it small: a server list is tens of names.
            if map.len() > 256 {
                map.retain(|_, (at, _)| at.elapsed() < TTL);
            }
            map.insert(key, (Instant::now(), addrs.to_vec()));
        }
    }
    /// `address` took a connection: try it first next time.
    pub(crate) fn connected(host: &str, port: u16, address: SocketAddr) {
        if let Ok(mut cache) = CACHE.lock()
            && let Some((_, addrs)) = cache
                .get_or_insert_with(HashMap::new)
                .get_mut(&(host.to_ascii_lowercase(), port))
            && let Some(i) = addrs.iter().position(|a| *a == address)
        {
            let address = addrs.remove(i);
            addrs.insert(0, address);
        }
    }
    /// A connection to every address failed: resolve again next time.
    pub(crate) fn forget(host: &str, port: u16) {
        if let Ok(mut cache) = CACHE.lock() {
            cache
                .get_or_insert_with(HashMap::new)
                .remove(&(host.to_ascii_lowercase(), port));
        }
    }
}

fn resolve_with(
    host: &str,
    port: u16,
    srv: impl FnOnce(&str) -> Option<(String, u16)>,
    addresses: impl FnOnce(&str, u16) -> Result<Vec<SocketAddr>>,
) -> Result<Vec<SocketAddr>> {
    // Vanilla also asks for `_minecraft._tcp.127.0.0.1`, which never has a
    // record: IP literals and localhost skip the query.
    if port == 25565
        && host.parse::<IpAddr>().is_err()
        && host != "localhost"
        && let Some((target, target_port)) =
            srv(&format!("_minecraft._tcp.{}.", host.trim_end_matches('.')))
    {
        // No second SRV lookup at the target, and no fallback to the original
        // host when the target is unknown.
        return addresses(&target, target_port);
    }
    addresses(host, port)
}

fn srv_redirect(name: &str) -> Option<(String, u16)> {
    dns::query_srv(name, &dns::system_name_servers(), dns::SERVER_TIMEOUT)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn redirects_default_port_even_when_the_original_host_has_no_addresses() {
        let result = resolve_with(
            "play.example.org",
            25565,
            |name| {
                assert_eq!(name, "_minecraft._tcp.play.example.org.");
                Some(("backend.example.org.".into(), 25570))
            },
            |host, port| {
                assert_eq!((host, port), ("backend.example.org.", 25570));
                Ok(vec!["127.0.0.1:25570".parse().unwrap()])
            },
        )
        .unwrap();
        assert_eq!(
            result,
            vec!["127.0.0.1:25570".parse::<SocketAddr>().unwrap()]
        );
    }

    #[test]
    fn no_record_or_lookup_failure_keeps_the_original_address() {
        resolve_with(
            "play.example.org",
            25565,
            |_| None,
            |host, port| {
                assert_eq!((host, port), ("play.example.org", 25565));
                Ok(vec![])
            },
        )
        .unwrap();
    }

    #[test]
    fn explicit_nondefault_ports_and_ip_literals_do_not_query_srv() {
        for (host, port) in [
            ("play.example.org", 25570),
            ("127.0.0.1", 25565),
            ("::1", 25565),
            ("localhost", 25565),
        ] {
            resolve_with(
                host,
                port,
                |_| panic!("unexpected SRV query"),
                |actual, actual_port| {
                    assert_eq!((actual, actual_port), (host, port));
                    Ok(vec![])
                },
            )
            .unwrap();
        }
    }

    #[test]
    fn fully_qualified_host_does_not_get_a_second_trailing_dot() {
        resolve_with(
            "play.example.org.",
            25565,
            |name| {
                assert_eq!(name, "_minecraft._tcp.play.example.org.");
                None
            },
            |_, _| Ok(vec![]),
        )
        .unwrap();
    }

    #[test]
    fn unknown_redirect_target_does_not_fall_back_to_the_original() {
        let result = resolve_with(
            "play.example.org",
            25565,
            |_| Some(("missing.example.org.".into(), 25570)),
            |host, _| {
                assert_eq!(host, "missing.example.org.");
                Err(Error::new("unknown redirected host"))
            },
        );
        assert_eq!(result.unwrap_err().to_string(), "unknown redirected host");
    }

    #[test]
    fn resolved_names_are_reused_with_the_connected_address_first() {
        let (a, b): (SocketAddr, SocketAddr) = (
            "10.0.0.1:25565".parse().unwrap(),
            "10.0.0.2:25565".parse().unwrap(),
        );
        let key = ("cache-test.example.org".to_owned(), 25565);
        cache::put(key.clone(), &[a, b]);
        assert_eq!(resolve("Cache-Test.example.org", 25565).unwrap(), [a, b]);
        cache::connected("cache-test.example.org", 25565, b);
        assert_eq!(cache::get(&key).unwrap(), [b, a]);
        cache::forget("cache-test.example.org", 25565);
        assert!(cache::get(&key).is_none());
    }

    #[test]
    fn literal_address_can_connect_without_dns() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let connection = crate::connection::Connection::connect_server(
            "127.0.0.1",
            address.port(),
            std::time::Duration::from_secs(1),
        )
        .unwrap();
        let (accepted, _) = listener.accept().unwrap();
        drop((connection, accepted));
    }
}
