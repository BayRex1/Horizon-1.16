//! Server-only experimental commands and ticking; no rendering or vanilla motion changes.
use super::Game;
use horizon_client::{collisions::SceneCollisions, text::Text};
use physics_core::Vec3;
use std::{io, time::Duration};

impl Game<'_> {
    pub(super) fn tick_physics(&mut self, running: bool) {
        if self.remote.is_some() {
            return;
        }
        let Some(mut physics) = self.world.physics.take() else {
            return;
        };
        let mut scene = SceneCollisions {
            world: &self.world,
            binding: self.collision,
            subject: None,
        };
        let elapsed = if running {
            Duration::from_millis(50)
        } else {
            Duration::ZERO
        };
        let result = physics.tick(&mut scene, elapsed);
        self.world.physics = Some(physics);
        match result {
            Err(error) => {
                log::warn!("PHYSICS UNLEASHED: simulation stopped for this tick: {error}")
            }
            Ok(report) if report.dropped_steps != 0 => log::warn!(
                "PHYSICS UNLEASHED: discarded {} catch-up steps",
                report.dropped_steps
            ),
            _ => {}
        }
    }

    /// Raw custom command, handled on the authority like server mod commands.
    /// No Brigadier client-tree registration/autocomplete is provided yet.
    pub(super) fn physics_command(&mut self, command: &str) -> Option<bool> {
        let words: Vec<_> = command.split_whitespace().collect();
        if words.first().copied() != Some("physics") {
            return None;
        }
        if self.remote.is_some() {
            self.send_chat(&format!("/{command}"));
            return Some(true);
        }
        if self.ui.registries.permission < 2 {
            self.command_failure(Text::literal("Physics commands require operator level 2"));
            return Some(false);
        }
        let Some(mut physics) = self.world.physics.take() else {
            return Some(false);
        };
        let result = (|| -> io::Result<String> {
            let vector = |args: &[&str]| -> io::Result<Vec3> {
                if args.len() != 3 {
                    return Err(io::Error::other("expected three absolute coordinates"));
                }
                let mut values = [0.0; 3];
                for (value, word) in values.iter_mut().zip(args) {
                    *value = word.parse().map_err(io::Error::other)?;
                }
                Ok(Vec3::new(values[0], values[1], values[2]))
            };
            match words.get(1).copied() {
                Some("spawn") => {
                    let centre = vector(&words[2..])?;
                    let mut scene = SceneCollisions {
                        world: &self.world,
                        binding: self.collision,
                        subject: None,
                    };
                    let body = physics.spawn_cube(&mut scene, centre)?;
                    Ok(format!(
                        "Created physics cube {body:?} at {centre:?} (not rendered)"
                    ))
                }
                Some("gravity") => {
                    physics.set_gravity(vector(&words[2..])?)?;
                    Ok(format!("Dimension gravity: {:?}", physics.gravity()))
                }
                Some("clear") if words.len() == 2 => {
                    physics.clear()?;
                    Ok("Removed this dimension's physics bodies and terrain cache".into())
                }
                Some("status") if words.len() == 2 => {
                    let mut lines = vec![format!(
                        "Physics: {} bodies, {} terrain blocks, gravity {:?}",
                        physics.bodies().len(),
                        physics.terrain_count(),
                        physics.gravity()
                    )];
                    for &body in physics.bodies() {
                        lines.push(format!("{body:?}: {:?}", physics.state(body)?));
                    }
                    Ok(lines.join("\n"))
                }
                _ => Err(io::Error::other(
                    "Usage: /physics spawn x y z | gravity x y z | status | clear",
                )),
            }
        })();
        self.world.physics = Some(physics);
        Some(match result {
            Ok(message) => {
                self.feedback(Text::literal(message));
                true
            }
            Err(error) => {
                self.command_failure(Text::literal(error.to_string()));
                false
            }
        })
    }
}
