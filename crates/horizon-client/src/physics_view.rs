//! Render-only snapshot buffer. Never steps Rapier or predicts collisions.
use glam::{DQuat, DVec3, Mat4, Quat, Vec3};
use horizon_protocol::{
    Error, Result,
    physics::{CubeState, Snapshot},
};
use std::{collections::VecDeque, time::Instant};

/// Two normal server ticks of jitter buffering. Server physics still runs 60Hz;
/// network snapshots are sent each vanilla tick (normally 20Hz).
const DELAY_STEPS: f64 = 6.0;
const HISTORY: usize = 8;

#[derive(Default, Clone)]
pub struct PhysicsView {
    /// Last frame's submitted block-model vertices; not a GPU visibility test.
    pub rendered_vertices: usize,
    frames: VecDeque<Snapshot>,
    received: Option<Instant>,
    rendered_step: f64,
}
impl PhysicsView {
    pub fn receive(&mut self, snapshot: Snapshot, dimension: &str, now: Instant) -> Result<bool> {
        snapshot.validate()?;
        if snapshot.dimension != dimension {
            return Ok(false);
        }
        if let Some(last) = self.frames.back() {
            if snapshot.epoch < last.epoch
                || (snapshot.epoch == last.epoch && snapshot.sequence <= last.sequence)
            {
                return Ok(false);
            }
            if snapshot.epoch == last.epoch && snapshot.step < last.step {
                return Err(Error::new("physics time went backwards"));
            }
            if snapshot.epoch != last.epoch {
                self.frames.clear();
                self.rendered_step = 0.0;
                self.received = None;
            }
        }
        let advances = self
            .frames
            .back()
            .is_none_or(|last| snapshot.step > last.step);
        if self
            .frames
            .back()
            .is_some_and(|last| last.step == snapshot.step)
        {
            self.frames.pop_back();
        }
        self.frames.push_back(snapshot);
        while self.frames.len() > HISTORY {
            self.frames.pop_front();
        }
        // Repeated frozen snapshots must not reset the interpolation clock.
        if advances {
            self.received = Some(now);
        }
        Ok(true)
    }

    pub fn sample(&mut self, now: Instant) -> Vec<CubeState> {
        let Some(latest) = self.frames.back() else {
            return Vec::new();
        };
        let elapsed = self
            .received
            .map_or(0.0, |t| now.saturating_duration_since(t).as_secs_f64());
        let target = (latest.step as f64 - DELAY_STEPS + elapsed * 60.0)
            .max(self.frames.front().unwrap().step as f64)
            .max(self.rendered_step)
            .min(latest.step as f64);
        self.rendered_step = target;
        let left = self
            .frames
            .iter()
            .rev()
            .find(|f| f.step as f64 <= target)
            .unwrap_or(self.frames.front().unwrap());
        let right = self
            .frames
            .iter()
            .find(|f| f.step as f64 >= target)
            .unwrap_or(latest);
        let alpha = if left.step == right.step {
            1.0
        } else {
            (target - left.step as f64) / (right.step - left.step) as f64
        };
        // Latest full membership wins immediately: removals never leave ghosts,
        // and newly spawned IDs do not interpolate from another body's position.
        latest
            .bodies
            .iter()
            .map(|body| {
                let a = left.bodies.iter().find(|b| b.id == body.id);
                let b = right.bodies.iter().find(|b| b.id == body.id);
                match (a, b) {
                    (Some(a), Some(b)) => interpolate(a, b, alpha),
                    _ => body.clone(),
                }
            })
            .collect()
    }
}

fn interpolate(a: &CubeState, b: &CubeState, alpha: f64) -> CubeState {
    let position = DVec3::from_array(a.position).lerp(DVec3::from_array(b.position), alpha);
    let rotation = DQuat::from_array(a.rotation)
        .normalize()
        .slerp(DQuat::from_array(b.rotation).normalize(), alpha)
        .normalize();
    CubeState {
        id: b.id,
        position: position.to_array(),
        rotation: rotation.to_array(),
    }
}

/// Baked Horizon block models occupy [0,1]^3. Rapier poses describe the centre.
/// Subtract the camera in f64 BEFORE converting to GPU f32 coordinates.
pub fn block_transform(body: &CubeState, camera: DVec3) -> Mat4 {
    let translation = (DVec3::from_array(body.position) - camera).as_vec3();
    let q = DQuat::from_array(body.rotation).normalize();
    Mat4::from_rotation_translation(
        Quat::from_xyzw(q.x as f32, q.y as f32, q.z as f32, q.w as f32),
        translation,
    ) * Mat4::from_translation(Vec3::splat(-0.5))
}
