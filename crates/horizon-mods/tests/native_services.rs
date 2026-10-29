//! Real loopback transport with the public native API. Requires local UDP permission.
use horizon_mods::native::{Declaration, Services, Session};
#[test]
fn approved_peer_echoes_and_revocation_closes_the_socket() {
    use std::{net::UdpSocket, time::Duration};
    let peer = UdpSocket::bind("127.0.0.1:0").unwrap();
    peer.set_read_timeout(Some(Duration::from_secs(2))).unwrap();
    let address = peer.local_addr().unwrap();
    let mut services = Services::default();
    services
        .register(Declaration {
            id: "echo".into(),
            title: "Echo".into(),
            author: "test".into(),
            version: "1".into(),
            purpose: "UDP echo".into(),
            microphone: false,
            audio: false,
        })
        .unwrap();
    services.sync(
        Session {
            connected: true,
            focused: true,
            ..Session::default()
        },
        [0.0; 3],
        0.0,
        1.0,
    );
    services
        .voice("echo")
        .unwrap()
        .request_endpoint("127.0.0.1", address.port())
        .unwrap();
    assert!(
        services
            .voice("echo")
            .unwrap()
            .connect_udp(&address.to_string())
            .is_err()
    );
    services.permission_click("echo", 1);
    let socket = services
        .voice("echo")
        .unwrap()
        .connect_udp(&address.to_string())
        .unwrap();
    services
        .voice("echo")
        .unwrap()
        .send_udp(socket, vec![1, 0, 255])
        .unwrap();
    let mut buffer = [0; 8];
    let (n, from) = peer.recv_from(&mut buffer).unwrap();
    assert_eq!(&buffer[..n], &[1, 0, 255]);
    peer.send_to(&buffer[..n], from).unwrap();
    let deadline = std::time::Instant::now() + Duration::from_secs(2);
    loop {
        let packets = services.voice("echo").unwrap().receive_udp(socket).unwrap();
        if !packets.is_empty() {
            assert_eq!(packets, vec![vec![1, 0, 255]]);
            break;
        }
        assert!(std::time::Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(1));
    }
    services.permission_click("echo", 1);
    assert!(
        services
            .voice("echo")
            .unwrap()
            .send_udp(socket, vec![1])
            .is_err()
    );
    assert!(
        services
            .voice("echo")
            .unwrap()
            .connect_udp(&address.to_string())
            .is_err()
    );
}
