use horizon_mod_sdk::server::{Registration, ServerMod, networking};

struct LabServer;
impl ServerMod for LabServer {
    fn init() -> Result<(Self, Registration), String> {
        networking::register("voice_lab:hello")?;
        Ok((Self, Registration::new().on_tick()))
    }
    fn tick(&mut self, _tick: u64) {
        for event in networking::poll() {
            if let networking::Event::Packet(message) = event {
                // An explicit protocol acknowledgement: [version, status].
                // Unknown versions get status 1, and no application traffic.
                let reply: &[u8] = if message.data.starts_with(&[1, 0]) {
                    &[1, 0]
                } else {
                    &[1, 1]
                };
                if let Err(e) =
                    networking::send(message.player.as_deref(), "voice_lab:hello", reply)
                {
                    horizon_mod_sdk::warn(&e);
                }
            }
        }
    }
}
horizon_mod_sdk::export_server_mod!(LabServer);
