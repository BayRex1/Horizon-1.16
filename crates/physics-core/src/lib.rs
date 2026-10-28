//! Opt-in rigid-body simulation, separate from Horizon's vanilla movement.
//! Units: one block = one metre, seconds, kilograms, Newtons. Y points up.
//! Bodies belong to one world/dimension; handles must only be used in that world.
use rapier3d_f64::prelude::*;
use std::time::Duration;

pub use horizon_core::{
    geometry::{Aabb, Vec3, VoxelShape},
    position::BlockPos,
};
pub use rapier3d_f64::dynamics::RigidBodyHandle as BodyHandle;

pub const PHYSICS_HZ: u32 = 60;
pub const FIXED_DT: f64 = 1.0 / PHYSICS_HZ as f64;
/// Bound work per `advance` call; excess time remains queued, never stretched.
pub const MAX_SUBSTEPS: u32 = 8;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BodyKind {
    Dynamic,
    Fixed,
}

#[derive(Clone, Copy, Debug)]
pub struct Material {
    /// Mass per cubic metre. Compound parts must not overlap for accurate mass.
    pub density: f64,
    pub friction: f64,
    pub restitution: f64,
}
impl Default for Material {
    fn default() -> Self {
        Self {
            density: 1.0,
            friction: 0.5,
            restitution: 0.0,
        }
    }
}

/// Already selected collision geometry of one block in a construction.
/// `offset` is relative to the body's origin; `shape` is block-local geometry,
/// e.g. `BlockCollisionWorld::context_shape`, NOT an already translated scan hit.
pub struct BlockCollider<'a> {
    pub offset: BlockPos,
    pub shape: &'a VoxelShape,
}

#[derive(Clone, Copy, Debug)]
pub struct BodyState {
    /// Transform of the original body origin (not its centre of mass).
    pub position: Vec3,
    /// Unit quaternion in [x, y, z, w] order.
    pub rotation: [f64; 4],
    pub linear_velocity: Vec3,
    pub angular_velocity: Vec3,
    pub mass: f64,
    pub sleeping: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PhysicsError {
    InvalidInput,
    MissingBody,
    NotDynamic,
}
impl std::fmt::Display for PhysicsError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::InvalidInput => "physics needs finite geometry, positive density/extents and valid material coefficients",
            Self::MissingBody => "body handle no longer exists in this world",
            Self::NotDynamic => "forces and impulses require a dynamic body",
        })
    }
}
impl std::error::Error for PhysicsError {}

type Result<T> = std::result::Result<T, PhysicsError>;
type Part = (Isometry<f64>, SharedShape);
fn vector(v: Vec3) -> Vector<f64> {
    Vector::new(v.x, v.y, v.z)
}
fn vec3(v: &Vector<f64>) -> Vec3 {
    Vec3::new(v.x, v.y, v.z)
}
fn finite(v: Vec3) -> Result<()> {
    if v.is_finite() {
        Ok(())
    } else {
        Err(PhysicsError::InvalidInput)
    }
}

pub struct PhysicsWorld {
    gravity: Vector<f64>,
    parameters: IntegrationParameters,
    pipeline: PhysicsPipeline,
    islands: IslandManager,
    broad_phase: BroadPhaseMultiSap,
    narrow_phase: NarrowPhase,
    bodies: RigidBodySet,
    colliders: ColliderSet,
    impulse_joints: ImpulseJointSet,
    multibody_joints: MultibodyJointSet,
    ccd: CCDSolver,
    /// Elapsed nanoseconds multiplied by 60, so a 50ms tick is exactly 3 steps.
    pending: u128,
}
impl Default for PhysicsWorld {
    fn default() -> Self {
        Self::new()
    }
}
impl PhysicsWorld {
    pub fn new() -> Self {
        Self {
            gravity: Vector::new(0.0, -9.81, 0.0),
            parameters: IntegrationParameters {
                dt: FIXED_DT,
                max_ccd_substeps: 4,
                ..Default::default()
            },
            pipeline: PhysicsPipeline::new(),
            islands: IslandManager::new(),
            broad_phase: BroadPhaseMultiSap::new(),
            narrow_phase: NarrowPhase::new(),
            bodies: RigidBodySet::new(),
            colliders: ColliderSet::new(),
            impulse_joints: ImpulseJointSet::new(),
            multibody_joints: MultibodyJointSet::new(),
            ccd: CCDSolver::new(),
            pending: 0,
        }
    }

    pub fn gravity(&self) -> Vec3 {
        vec3(&self.gravity)
    }

    /// Changing gravity wakes sleepers, including when switching to zero gravity.
    pub fn set_gravity(&mut self, gravity: Vec3) -> Result<()> {
        finite(gravity)?;
        self.gravity = vector(gravity);
        for (_, body) in self.bodies.iter_mut() {
            if body.is_dynamic() {
                body.wake_up(true);
            }
        }
        Ok(())
    }

    /// Simulate exactly 1/60s. Does not consume the `advance` accumulator.
    pub fn step(&mut self) {
        self.pipeline.step(
            &self.gravity,
            &self.parameters,
            &mut self.islands,
            &mut self.broad_phase,
            &mut self.narrow_phase,
            &mut self.bodies,
            &mut self.colliders,
            &mut self.impulse_joints,
            &mut self.multibody_joints,
            &mut self.ccd,
            None,
            &(),
            &(),
        );
    }

    /// Feed elapsed simulation time, not render time while paused. At normal
    /// Horizon speed pass 50ms per world tick. Drain backlog with Duration::ZERO.
    pub fn advance(&mut self, elapsed: Duration) -> u32 {
        self.pending = self
            .pending
            .saturating_add(elapsed.as_nanos() * PHYSICS_HZ as u128);
        let steps = (self.pending / 1_000_000_000).min(MAX_SUBSTEPS as u128) as u32;
        for _ in 0..steps {
            self.step();
        }
        self.pending -= steps as u128 * 1_000_000_000;
        steps
    }

    pub fn pending_steps(&self) -> u128 {
        self.pending / 1_000_000_000
    }

    pub fn insert_cuboid(
        &mut self,
        kind: BodyKind,
        centre: Vec3,
        half_extents: Vec3,
        material: Material,
    ) -> Result<BodyHandle> {
        finite(half_extents)?;
        if half_extents.components().iter().any(|v| *v <= 0.0) {
            return Err(PhysicsError::InvalidInput);
        }
        self.insert(
            kind,
            centre,
            vec![(
                Isometry::identity(),
                SharedShape::cuboid(half_extents.x, half_extents.y, half_extents.z),
            )],
            material,
        )
    }

    /// Each block's VoxelShape is decomposed into cuboid colliders attached to
    /// one body, so adjacent blocks move and rotate as one construction.
    /// Empty block shapes are skipped; an entirely empty construction is invalid.
    pub fn insert_blocks(
        &mut self,
        kind: BodyKind,
        origin: Vec3,
        blocks: &[BlockCollider<'_>],
        material: Material,
    ) -> Result<BodyHandle> {
        let mut parts = Vec::new();
        for block in blocks {
            for bounds in block.shape.boxes() {
                let lo = bounds.min_position();
                let hi = bounds.max_position();
                let half = (vector(hi) - vector(lo)) * 0.5;
                let centre = vector(lo)
                    + half
                    + Vector::new(
                        block.offset.x as f64,
                        block.offset.y as f64,
                        block.offset.z as f64,
                    );
                if !half.iter().all(|v| v.is_finite() && *v > 0.0)
                    || !centre.iter().all(|v| v.is_finite())
                {
                    return Err(PhysicsError::InvalidInput);
                }
                parts.push((
                    Isometry::translation(centre.x, centre.y, centre.z),
                    SharedShape::cuboid(half.x, half.y, half.z),
                ));
            }
        }
        self.insert(kind, origin, parts, material)
    }

    fn insert(
        &mut self,
        kind: BodyKind,
        origin: Vec3,
        parts: Vec<Part>,
        material: Material,
    ) -> Result<BodyHandle> {
        finite(origin)?;
        if parts.is_empty()
            || !material.density.is_finite()
            || material.density <= 0.0
            || !material.friction.is_finite()
            || material.friction < 0.0
            || !(0.0..=1.0).contains(&material.restitution)
        {
            return Err(PhysicsError::InvalidInput);
        }
        // Keep convex parts as separate colliders on the SAME rigid body.
        // This also keeps each block part available for future contact attribution.
        let colliders: Vec<_> = parts
            .into_iter()
            .map(|(pose, shape)| {
                ColliderBuilder::new(shape)
                    .position(pose)
                    .density(material.density)
                    .friction(material.friction)
                    .restitution(material.restitution)
                    .build()
            })
            .collect();
        let mass: f64 = colliders.iter().map(Collider::mass).sum();
        if !mass.is_finite() || mass <= 0.0 {
            return Err(PhysicsError::InvalidInput);
        }
        let builder = match kind {
            BodyKind::Dynamic => RigidBodyBuilder::dynamic().ccd_enabled(true),
            BodyKind::Fixed => RigidBodyBuilder::fixed(),
        };
        let handle = self
            .bodies
            .insert(builder.translation(vector(origin)).build());
        for collider in colliders {
            self.colliders
                .insert_with_parent(collider, handle, &mut self.bodies);
        }
        Ok(handle)
    }

    pub fn state(&self, handle: BodyHandle) -> Result<BodyState> {
        let body = self.bodies.get(handle).ok_or(PhysicsError::MissingBody)?;
        let q = body.rotation().quaternion();
        Ok(BodyState {
            position: vec3(body.translation()),
            rotation: [q.i, q.j, q.k, q.w],
            linear_velocity: vec3(body.linvel()),
            angular_velocity: vec3(body.angvel()),
            mass: body.mass(),
            sleeping: body.is_sleeping(),
        })
    }

    /// World-space bounds of every collider attached to this body.
    pub fn bounds(&self, handle: BodyHandle) -> Result<Aabb> {
        let body = self.bodies.get(handle).ok_or(PhysicsError::MissingBody)?;
        let mut bounds = None;
        for handle in body.colliders() {
            let collider = &self.colliders[*handle];
            // Compute from the body's current transform, including before the
            // first pipeline step or immediately after insertion.
            let pose = body.position() * collider.position_wrt_parent().unwrap();
            let aabb = collider.shape().compute_aabb(&pose);
            let next = Aabb::new(vec3(&aabb.mins.coords), vec3(&aabb.maxs.coords));
            bounds = Some(bounds.map_or(next, |b: Aabb| b.union_bounds(next)));
        }
        bounds.ok_or(PhysicsError::MissingBody)
    }

    fn dynamic(&mut self, handle: BodyHandle) -> Result<&mut RigidBody> {
        let body = self
            .bodies
            .get_mut(handle)
            .ok_or(PhysicsError::MissingBody)?;
        if !body.is_dynamic() {
            return Err(PhysicsError::NotDynamic);
        }
        Ok(body)
    }

    pub fn apply_impulse(&mut self, handle: BodyHandle, impulse: Vec3) -> Result<()> {
        finite(impulse)?;
        self.dynamic(handle)?.apply_impulse(vector(impulse), true);
        Ok(())
    }

    /// World-space point; an off-centre impulse also produces angular motion.
    pub fn apply_impulse_at_point(
        &mut self,
        handle: BodyHandle,
        impulse: Vec3,
        point: Vec3,
    ) -> Result<()> {
        finite(impulse)?;
        finite(point)?;
        self.dynamic(handle)?.apply_impulse_at_point(
            vector(impulse),
            Point::new(point.x, point.y, point.z),
            true,
        );
        Ok(())
    }

    /// Replace the persistent external force at the centre of mass. It acts on
    /// every substep until replaced; pass Vec3::ZERO to clear it. Does not stack.
    pub fn set_force(&mut self, handle: BodyHandle, force: Vec3) -> Result<()> {
        finite(force)?;
        let body = self.dynamic(handle)?;
        body.reset_forces(true);
        body.add_force(vector(force), true);
        Ok(())
    }

    /// Removes the body, its colliders and attached joints. Old handles go stale.
    pub fn remove(&mut self, handle: BodyHandle) -> Result<()> {
        self.bodies
            .remove(
                handle,
                &mut self.islands,
                &mut self.colliders,
                &mut self.impulse_joints,
                &mut self.multibody_joints,
                true,
            )
            .ok_or(PhysicsError::MissingBody)?;
        Ok(())
    }
}
