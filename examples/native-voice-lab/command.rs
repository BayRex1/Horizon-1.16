fn command(game: &mut crate::game::Game<'_>, command: &str, ran: &mut bool) {
    if *ran || !game.native_window() || !game.native_on("native_voice_lab") {
        return;
    }
    let args: Vec<_> = command.split_whitespace().collect();
    if !args.first().is_some_and(|name| {
        matches!(
            *name,
            "voice_request"
                | "voice_resolve"
                | "voice_connect"
                | "voice_ping"
                | "voice_receive"
                | "voice_hello"
        )
    }) {
        return;
    }
    *ran = true;
    let result = (|| -> Result<String, String> {
        let voice = game.native_voice("native_voice_lab")?;
        match args.as_slice() {
            ["voice_request", host, port] => {
                voice.request_endpoint(host, port.parse().map_err(|_| "invalid port")?)?;
                Ok("Approve the exact destination in Options > Mods > Native Voice Lab > Settings".into())
            }
            ["voice_resolve", host, port] => {
                voice.resolve(host, port.parse().map_err(|_| "invalid port")?)?;
                Ok("DNS started; run /voice_receive dns to poll".into())
            }
            ["voice_connect", address] => Ok(format!("UDP socket {}",voice.connect_udp(address)?)),
            ["voice_ping", socket] => {
                voice.send_udp(socket.parse().map_err(|_| "invalid socket")?, b"native-voice-lab".to_vec())?;
                Ok("Sent diagnostic bytes (no microphone audio)".into())
            }
            ["voice_receive", "dns"] => Ok(format!("DNS: {:?}; approve numeric endpoints before connecting",voice.resolved()?)),
            ["voice_receive", socket] => Ok(format!("UDP: {:?}",voice.receive_udp(socket.parse().map_err(|_| "invalid socket")?)?)),
            ["voice_hello"] => {
                voice.register_channel("voice_lab:hello")?;
                voice.send_payload(None,"voice_lab:hello",vec![1])?;
                Ok("Sent diagnostic protocol version 1".into())
            }
            _ => Err("Usage: /voice_request HOST PORT, /voice_resolve HOST PORT, /voice_connect IP:PORT, /voice_ping SOCKET, /voice_receive dns|SOCKET, /voice_hello".into()),
        }
    })();
    game.native_chat(&result.unwrap_or_else(|error| error));
}
