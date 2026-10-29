//! A local PTT monitor and binary transport diagnostic, not a voice protocol.
//! Do not send unencrypted microphone PCM over UDP. A real mod encodes Opus
//! and authenticates/encrypts packets according to its server's protocol.
use horizon_mod_sdk::{Mod, Registration, audio, input, microphone, networking, udp};

struct VoiceLab {
    output: Option<(u32, u32)>,
    socket: Option<u32>,
}
impl Mod for VoiceLab {
    fn init() -> Result<(Self, Registration), String> {
        input::register_key("ptt", "Microphone monitor (hold)", "key.keyboard.v")?;
        networking::register("voice_lab:hello")?;
        Ok((
            Self {
                output: None,
                socket: None,
            },
            Registration::new()
                .on_frame()
                .command("voice_udp")
                .command("voice_ping")
                .command("voice_hello")
                .command("voice_capture"),
        ))
    }
    fn frame(&mut self, _seconds: f32) {
        for event in input::poll() {
            match event {
                input::Event::KeyDown(key) if key == "ptt" => {
                    if let Err(e) = microphone::start("", 1.0) {
                        horizon_mod_sdk::warn(&e);
                    }
                }
                input::Event::KeyUp(key) if key == "ptt" => {
                    microphone::stop();
                    if let Some((id, _)) = self.output.take() {
                        audio::close(id);
                    }
                }
                _ => {}
            }
        }
        match microphone::read() {
            Ok(frames) => {
                for frame in frames {
                    if self
                        .output
                        .is_none_or(|(_, rate)| rate != frame.sample_rate)
                    {
                        if let Some((id, _)) = self.output.take() {
                            audio::close(id);
                        }
                        if let Ok(id) = audio::create(frame.sample_rate) {
                            self.output = Some((id, frame.sample_rate));
                        }
                    }
                    if let Some((id, _)) = self.output {
                        let _ = audio::write(id, &frame.samples);
                    }
                }
            }
            Err(e) => horizon_mod_sdk::warn(&e),
        }
        for event in networking::poll() {
            match event {
                networking::Event::Connected(id) => {
                    horizon_mod_sdk::info(&format!("connected: {id:?}"))
                }
                networking::Event::Disconnected => {
                    self.socket = None;
                    self.output = None;
                }
                networking::Event::Packet(p) => {
                    horizon_mod_sdk::info(&format!("{}: {:?}", p.channel, p.data))
                }
            }
        }
        if let Some(id) = self.socket {
            match udp::receive(id) {
                Ok(packets) => {
                    for packet in packets {
                        horizon_mod_sdk::info(&format!("UDP: {packet:?}"));
                    }
                }
                Err(e) => horizon_mod_sdk::warn(&e),
            }
        }
    }
    fn command(&mut self, name: &str, args: &str) -> Result<Option<String>, String> {
        match name {
            "voice_capture" => {
                if args == "stop" {
                    microphone::stop();
                } else if args == "start" {
                    microphone::start("", 1.0)?;
                } else {
                    return Err("Usage: /voice_capture start|stop".into());
                }
                Ok(None)
            }
            "voice_udp" => {
                if let Some(id) = self.socket.take() {
                    udp::close(id);
                }
                if args != "close" {
                    self.socket = Some(udp::connect(args)?);
                }
                Ok(Some("UDP endpoint updated".into()))
            }
            "voice_ping" => {
                udp::send(
                    self.socket
                        .ok_or("Use /voice_udp <numeric IP:port> first")?,
                    args.as_bytes(),
                )?;
                Ok(None)
            }
            "voice_hello" => {
                // A protocol version followed by arbitrary diagnostic binary data.
                let mut bytes = vec![1, 0];
                bytes.extend_from_slice(args.as_bytes());
                networking::send(None, "voice_lab:hello", &bytes)?;
                Ok(None)
            }
            _ => Err("unknown voice-lab command".into()),
        }
    }
}
horizon_mod_sdk::export_mod!(VoiceLab);
