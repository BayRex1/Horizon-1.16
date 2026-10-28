//! Authoritative, opt-in physics owned by one resident World/dimension.
//! Terrain is a bounded working set near bodies, selected by Horizon itself.
use horizon_core::position::ChunkPos;
use horizon_world::{
    block_collision_scan::{BlockCollisionScan, BlockCollisionWorld},
    collision_context::CollisionContext,
};
use physics_core::{
    Aabb, BlockCollider, BlockPos, BodyHandle, BodyKind, BodyState, FIXED_DT, MAX_SUBSTEPS,
    Material, PhysicsWorld, Vec3,
};
use std::{collections::BTreeMap, io, time::Duration};

const MAX_BODIES: usize = 16;
const SCAN_BUDGET: usize = 32_768;
const MAX_TERRAIN: usize = 16_384;
type Position = (i32, i32, i32);
struct Terrain {
    handle: BodyHandle,
    boxes: Vec<Aabb>,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct TickReport {
    pub steps: u32,
    pub dropped_steps: u128,
}

pub struct PhysicsDimension {
    engine: PhysicsWorld,
    bodies: Vec<BodyHandle>,
    terrain: BTreeMap<Position, Terrain>,
    remainder: u128,
}
impl Default for PhysicsDimension {
    fn default() -> Self {
        Self {
            engine: PhysicsWorld::new(),
            bodies: Vec::new(),
            terrain: BTreeMap::new(),
            remainder: 0,
        }
    }
}
impl PhysicsDimension {
    pub fn bodies(&self) -> &[BodyHandle] {
        &self.bodies
    }
    pub fn terrain_count(&self) -> usize {
        self.terrain.len()
    }
    pub fn state(&self, body: BodyHandle) -> io::Result<BodyState> {
        self.engine.state(body).map_err(io::Error::other)
    }
    pub fn gravity(&self) -> Vec3 {
        self.engine.gravity()
    }
    pub fn set_gravity(&mut self, gravity: Vec3) -> io::Result<()> {
        if !gravity.is_finite() || gravity.components().iter().any(|v| v.abs() > 100.0) {
            return Err(io::Error::other(
                "gravity components must be finite and within +/-100m/s²",
            ));
        }
        self.engine.set_gravity(gravity).map_err(io::Error::other)
    }
    pub fn clear(&mut self) -> io::Result<()> {
        // Keep the arena generations and dimension gravity. Replacing the whole
        // engine here would let a stale handle alias a cube spawned after clear.
        for body in self.bodies.drain(..) {
            self.engine.remove(body).map_err(io::Error::other)?;
        }
        for (_, terrain) in std::mem::take(&mut self.terrain) {
            self.engine
                .remove(terrain.handle)
                .map_err(io::Error::other)?;
        }
        self.remainder = 0;
        Ok(())
    }

    /// Unit cube, added only after its collision neighbourhood is resident.
    pub fn spawn_cube<W: BlockCollisionWorld<Error = io::Error>>(
        &mut self,
        world: &mut W,
        centre: Vec3,
    ) -> io::Result<BodyHandle> {
        if self.bodies.len() >= MAX_BODIES
            || !centre.is_finite()
            || centre.components().iter().any(|v| v.abs() > 30_000_000.0)
            || centre.y < crate::chunk::MIN_Y as f64 + 0.5
            || centre.y > (crate::chunk::MIN_Y + crate::chunk::HEIGHT) as f64 - 0.5
        {
            return Err(io::Error::other(
                "invalid position or physics body limit reached",
            ));
        }
        let bounds = Aabb::new(centre, centre).inflate(Vec3::new(2.0, 2.0, 2.0));
        if !loaded(world, bounds)? {
            return Err(io::Error::other(
                "physics spawn neighbourhood is not loaded",
            ));
        }
        let body = self
            .engine
            .insert_cuboid(
                BodyKind::Dynamic,
                centre,
                Vec3::new(0.5, 0.5, 0.5),
                Material {
                    restitution: 0.6,
                    ..Default::default()
                },
            )
            .map_err(io::Error::other)?;
        self.bodies.push(body);
        if let Err(error) = self.synchronize(world) {
            self.bodies.pop();
            self.engine.remove(body).map_err(io::Error::other)?;
            return Err(error);
        }
        Ok(body)
    }

    /// Feed simulation time only: the game passes 50ms per running vanilla tick,
    /// never wall-clock lag. Even arbitrary callers cannot accumulate catch-up
    /// debt: at most eight fixed steps run and excess whole steps are discarded.
    pub fn tick<W: BlockCollisionWorld<Error = io::Error>>(
        &mut self,
        world: &mut W,
        elapsed: Duration,
    ) -> io::Result<TickReport> {
        let total = self.remainder + elapsed.as_nanos() * 60;
        let requested = total / 1_000_000_000;
        self.remainder = total % 1_000_000_000;
        let steps = requested.min(MAX_SUBSTEPS as u128) as u32;
        self.synchronize(world)?; // Also prune unloads while frozen.
        for step in 0..steps {
            if step != 0 {
                self.synchronize(world)?;
            }
            self.engine.step();
        }
        // A body that crossed the world's vertical limit is removed immediately.
        self.synchronize(world)?;
        Ok(TickReport {
            steps,
            dropped_steps: requested - steps as u128,
        })
    }

    fn synchronize<W: BlockCollisionWorld<Error = io::Error>>(
        &mut self,
        world: &mut W,
    ) -> io::Result<()> {
        let mut selected = BTreeMap::new();
        let mut removed = Vec::new();
        for &body in &self.bodies {
            let state = self.state(body)?;
            // Cubes only in stage 2: padding covers rotation and neighbouring
            // context/overhanging block shapes. Sweep includes the next substep.
            let motion = Vec3::new(
                state.linear_velocity.x * FIXED_DT,
                state.linear_velocity.y * FIXED_DT,
                state.linear_velocity.z * FIXED_DT,
            );
            let bounds = self
                .engine
                .bounds(body)
                .map_err(io::Error::other)?
                .expand_towards(motion)
                .inflate(Vec3::new(2.0, 2.0, 2.0));
            if state.position.y < crate::chunk::MIN_Y as f64
                || state.position.y > (crate::chunk::MIN_Y + crate::chunk::HEIGHT) as f64
                || !loaded(world, bounds)?
            {
                removed.push(body);
                continue;
            }
            let scan = BlockCollisionScan::new(
                world,
                CollisionContext::empty(false),
                bounds,
                false,
                SCAN_BUDGET,
            )
            .map_err(io::Error::other)?;
            for hit in scan {
                let hit = hit.map_err(io::Error::other)?;
                selected.insert((hit.position.x, hit.position.y, hit.position.z), hit.shape);
                if selected.len() > MAX_TERRAIN {
                    return Err(io::Error::other("physics terrain budget exhausted"));
                }
            }
        }
        for body in removed {
            self.engine.remove(body).map_err(io::Error::other)?;
            self.bodies.retain(|b| *b != body);
        }
        let stale: Vec<_> = self
            .terrain
            .keys()
            .filter(|key| !selected.contains_key(key))
            .copied()
            .collect();
        for key in stale {
            let old = self.terrain.remove(&key).unwrap();
            self.engine.remove(old.handle).map_err(io::Error::other)?;
        }
        for (key, shape) in selected {
            if self
                .terrain
                .get(&key)
                .is_some_and(|old| old.boxes == shape.boxes())
            {
                continue;
            }
            let origin = Vec3::new(key.0 as f64, key.1 as f64, key.2 as f64);
            // Scan hits are WORLD translated. Convert once back to block-local.
            let local = shape.moved(Vec3::new(-origin.x, -origin.y, -origin.z));
            let handle = self
                .engine
                .insert_blocks(
                    BodyKind::Fixed,
                    origin,
                    &[BlockCollider {
                        offset: BlockPos::new(0, 0, 0),
                        shape: &local,
                    }],
                    Material::default(),
                )
                .map_err(io::Error::other)?;
            if let Some(old) = self.terrain.insert(
                key,
                Terrain {
                    handle,
                    boxes: shape.boxes().to_vec(),
                },
            ) {
                self.engine.remove(old.handle).map_err(io::Error::other)?;
            }
        }
        Ok(())
    }
}

/// Unlike vanilla's scan, missing chunks must NOT count as empty space for a
/// rigid body. Check the whole swept footprint plus the scan's outer cells.
fn loaded<W: BlockCollisionWorld<Error = io::Error>>(
    world: &mut W,
    bounds: Aabb,
) -> io::Result<bool> {
    let min = bounds.min_position();
    let max = bounds.max_position();
    if !min.is_finite() || !max.is_finite() {
        return Err(io::Error::other("non-finite physics bounds"));
    }
    let lo = BlockPos::containing(min).offset(-2, 0, -2).chunk();
    let hi = BlockPos::containing(max).offset(2, 0, 2).chunk();
    let count = (hi.x as i128 - lo.x as i128 + 1) * (hi.z as i128 - lo.z as i128 + 1);
    if !(1..=64).contains(&count) {
        return Err(io::Error::other(
            "physics sweep exceeds resident chunk budget",
        ));
    }
    for x in lo.x..=hi.x {
        for z in lo.z..=hi.z {
            if world.collision_chunk(ChunkPos::new(x, z))?.is_none() {
                return Ok(false);
            }
        }
    }
    Ok(true)
}
