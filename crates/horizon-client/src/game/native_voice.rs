//! Game-thread integration for native voice services, independent of WASM.
use super::Game;
use horizon_mods::native::{Declaration, Session};
use horizon_protocol::packets::{
    configuration::CustomPayload,
    play::{Clientbound, Serverbound},
};
use std::collections::BTreeSet;

pub(super) fn uuid(value: u128) -> String {
    let h = format!("{value:032x}");
    format!(
        "{}-{}-{}-{}-{}",
        &h[..8],
        &h[8..12],
        &h[12..16],
        &h[16..20],
        &h[20..]
    )
}
impl Game<'_> {
    pub(super) fn draw_native_microphone(&self, batch: &mut horizon_client::hud::HudBatch) {
        let capturing = self.mods.native_voice.capturing();
        if !capturing.is_empty() {
            self.native_text(
                batch,
                &format!(
                    "Microphone: {} (Options > Mods to stop)",
                    capturing.join(", ")
                ),
                [4.0, 4.0],
                0xff5555,
                true,
            );
        }
    }
    pub(super) fn native_voice_player_snapshot(&self) -> Vec<horizon_mods::native::Player> {
        let mut players = self.native_remote_voice_players();
        if self.remote.is_none() {
            players.extend(
                self.lan
                    .as_ref()
                    .into_iter()
                    .flat_map(|h| &h.hub.guests)
                    .filter(|g| g.placed)
                    .map(|g| horizon_mods::native::Player {
                        id: uuid(g.uuid),
                        name: g.name.clone(),
                        position: Some(g.position),
                        dimension: Some(g.dimension.clone()),
                    }),
            );
        }
        if let Some(player) = &self.player {
            let id = self
                .remote
                .as_ref()
                .map_or_else(|| self.player_uuid_string(), |r| uuid(r.profile));
            players.retain(|p| p.id != id);
            players.push(horizon_mods::native::Player {
                id,
                name: self.remote.as_ref().map_or_else(
                    || super::chat::PLAYER_NAME.to_owned(),
                    |r| r.session.name.clone(),
                ),
                position: Some([player.position.x, player.position.y, player.position.z]),
                dimension: Some(self.dim.id().to_owned()),
            });
        }
        players.sort_by(|a, b| a.id.cmp(&b.id));
        players
    }
    pub(super) fn refresh_native_voice(&mut self) {
        let mut declarations = Vec::<Declaration>::new();
        self.native_voice_mods(&mut declarations);
        let mut ids = BTreeSet::new();
        for declaration in declarations {
            if !self.native_on(&declaration.id) {
                continue;
            }
            let id = declaration.id.clone();
            if self.mods.native_voice.register(declaration).is_ok() {
                ids.insert(id);
            }
        }
        self.mods.native_voice.retain(&ids);
    }
    pub(super) fn sync_native_voice(&mut self) {
        self.refresh_native_voice();
        #[allow(unused_mut)]
        let mut active = !self.mods.native_voice.is_empty();
        #[cfg(feature = "mods")]
        {
            active |= !self.mods.mods.registered_channels().is_empty();
        }
        if !active {
            self.flush_native_voice();
            return;
        }
        let server_side = !self.options.execution_role.is_client();
        let local_player = self.player.as_ref().map(|_| {
            self.remote
                .as_ref()
                .map_or_else(|| self.player_uuid_string(), |r| uuid(r.profile))
        });
        let mut players = self.native_remote_voice_ids();
        players.extend(
            self.lan
                .as_ref()
                .into_iter()
                .flat_map(|h| &h.hub.guests)
                .filter(|g| g.placed)
                .map(|g| uuid(g.uuid)),
        );
        if let Some(id) = &local_player
            && !players.contains(id)
        {
            players.push(id.clone());
        }
        let session = Session {
            connected: server_side || self.player.is_some(),
            connection: self.remote.as_ref().map_or(0, |r| r.session.connection_id),
            server: self.remote.as_ref().map(|r| r.session.address.clone()),
            local_player,
            server_side,
            hosting: server_side || (self.remote.is_none() && self.player.is_some()),
            focused: self.focused,
            players,
        };
        let position = self
            .player
            .as_ref()
            .map_or([0.0; 3], |p| [p.position.x, p.position.y, p.position.z]);
        self.mods
            .native_voice
            .sync(session, position, self.yaw, self.settings.master_volume);
        if !self.focused && self.native_window() {
            self.mods.native_voice.stop_capture();
        }
        // Dynamic endpoint requests must be visible even when Options was already open.
        let ids: Vec<_> = self
            .settings
            .mods
            .installed
            .iter()
            .filter(|m| m.native.is_some())
            .map(|m| m.id.clone())
            .collect();
        for id in ids {
            let mut buttons = Vec::new();
            self.native_settings(&id, &mut buttons);
            buttons.extend(self.mods.native_voice.permission_buttons(&id));
            if let Some(page) = self
                .settings
                .mods
                .installed
                .iter_mut()
                .find(|m| m.id == id)
                .and_then(|m| m.native.as_mut())
            {
                page.buttons = buttons;
            }
        }
        for row in &mut self.settings.mods.installed {
            let description = self.mods.native_voice.permission_description(&row.id);
            if !description.is_empty() {
                row.description = description;
            }
        }
        self.flush_native_voice();
    }
    pub(super) fn flush_native_voice(&mut self) {
        #[allow(unused_mut)]
        let mut extra = BTreeSet::new();
        #[cfg(feature = "mods")]
        extra.extend(self.mods.mods.registered_channels());
        let mut messages = self.mods.native_voice.announcements(extra);
        messages.extend(self.mods.native_voice.drain_out());
        for message in messages {
            let payload = CustomPayload {
                channel: message.channel,
                data: message.data,
            };
            if let Some(player) = message.player {
                if self.remote.is_none()
                    && self.player.is_some()
                    && self.player_uuid_string() == player
                {
                    self.mods
                        .native_voice
                        .deliver(None, &payload.channel, &payload.data);
                    #[cfg(feature = "mods")]
                    self.mods
                        .mods
                        .custom_payload(None, &payload.channel, &payload.data);
                } else if let Some(host) = self.lan.as_mut()
                    && let Some(conn) = host
                        .hub
                        .guests
                        .iter()
                        .find(|g| g.placed && uuid(g.uuid) == player)
                        .map(|g| g.conn)
                {
                    host.hub.send_to(conn, &Clientbound::CustomPayload(payload));
                }
            } else if self.remote.is_some() {
                self.send(Serverbound::CustomPayload(payload));
            } else if self.player.is_some() {
                let player = self.player_uuid_string();
                self.mods
                    .native_voice
                    .deliver(Some(&player), &payload.channel, &payload.data);
                #[cfg(feature = "mods")]
                self.mods
                    .mods
                    .custom_payload(Some(&player), &payload.channel, &payload.data);
            }
        }
    }
}

// Compile the actual mixin helpers against Game when checking the binary's tests.
#[cfg(test)]
#[allow(dead_code)]
mod example {
    mod list {
        include!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../examples/native-voice-lab/list.rs"
        ));
    }
    mod services {
        include!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../examples/native-voice-lab/services.rs"
        ));
    }
    mod key {
        include!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../examples/native-voice-lab/key.rs"
        ));
    }
    mod frame {
        include!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../examples/native-voice-lab/frame.rs"
        ));
    }
    mod command {
        include!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../examples/native-voice-lab/command.rs"
        ));
    }
}
