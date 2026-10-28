use physics_core::{
    Aabb, BlockCollider, BlockPos, BodyHandle, BodyKind, Material, PhysicsError, PhysicsWorld,
    Vec3, VoxelShape,
};
use std::time::Duration;

fn cube(world: &mut PhysicsWorld, at: Vec3, material: Material) -> BodyHandle {
    world
        .insert_cuboid(BodyKind::Dynamic, at, Vec3::new(0.5, 0.5, 0.5), material)
        .unwrap()
}
fn near(actual: f64, expected: f64, tolerance: f64) {
    assert!(
        (actual - expected).abs() < tolerance,
        "{actual} != {expected}"
    );
}

#[test]
fn falling_cube_collides_and_rebounds_above_platform() {
    let mut world = PhysicsWorld::new();
    let material = Material {
        restitution: 0.8,
        ..Material::default()
    };
    let platform = world
        .insert_cuboid(
            BodyKind::Fixed,
            Vec3::new(0.0, -0.5, 0.0),
            Vec3::new(5.0, 0.5, 5.0),
            material,
        )
        .unwrap();
    let body = cube(&mut world, Vec3::new(0.0, 4.0, 0.0), material);
    let mut falling = false;
    let mut rebound = false;
    let mut rose_after_contact = false;
    for _ in 0..240 {
        world.step();
        let state = world.state(body).unwrap();
        // Rapier permits sub-tick overlap below its hard-CCD speed threshold.
        // Bound that error by one step's travel, and reject passing the floor.
        let travel = (state.linear_velocity.y.abs() + 9.81 * physics_core::FIXED_DT)
            * physics_core::FIXED_DT;
        assert!(
            state.position.y >= 0.5 - travel - 0.005,
            "platform penetration: {state:?}"
        );
        assert!(state.position.y > 0.25, "passed through the platform");
        falling |= state.linear_velocity.y < -1.0;
        rebound |= falling && state.linear_velocity.y > 1.0;
        rose_after_contact |= rebound && state.position.y > 1.0;
    }
    assert!(falling && rebound && rose_after_contact);
    assert_eq!(
        world.state(platform).unwrap().position,
        Vec3::new(0.0, -0.5, 0.0)
    );
}

#[test]
fn dynamic_bodies_exchange_momentum_on_collision() {
    let mut world = PhysicsWorld::new();
    world.set_gravity(Vec3::ZERO).unwrap();
    let material = Material {
        restitution: 1.0,
        friction: 0.0,
        ..Material::default()
    };
    let a = cube(&mut world, Vec3::new(-2.0, 0.0, 0.0), material);
    let b = cube(&mut world, Vec3::new(2.0, 0.0, 0.0), material);
    world.apply_impulse(a, Vec3::new(2.0, 0.0, 0.0)).unwrap();
    world.apply_impulse(b, Vec3::new(-2.0, 0.0, 0.0)).unwrap();
    for _ in 0..90 {
        world.step();
    }
    let va = world.state(a).unwrap().linear_velocity.x;
    let vb = world.state(b).unwrap().linear_velocity.x;
    assert!(va < -1.5 && vb > 1.5, "bodies did not rebound: {va}, {vb}");
    near(va + vb, 0.0, 1e-6);
}

#[test]
fn force_is_persistent_replaced_and_cleared_and_impulse_uses_mass() {
    let mut world = PhysicsWorld::new();
    world.set_gravity(Vec3::ZERO).unwrap();
    let body = cube(
        &mut world,
        Vec3::ZERO,
        Material {
            density: 2.0,
            ..Material::default()
        },
    );
    near(world.state(body).unwrap().mass, 2.0, 1e-12);
    world.set_force(body, Vec3::new(4.0, 0.0, 0.0)).unwrap();
    world.set_force(body, Vec3::new(4.0, 0.0, 0.0)).unwrap();
    for _ in 0..60 {
        world.step();
    }
    near(world.state(body).unwrap().linear_velocity.x, 2.0, 1e-6);
    world.set_force(body, Vec3::ZERO).unwrap();
    world.apply_impulse(body, Vec3::new(4.0, 0.0, 0.0)).unwrap();
    for _ in 0..60 {
        world.step();
    }
    near(world.state(body).unwrap().linear_velocity.x, 4.0, 1e-6);
}

#[test]
fn changing_gravity_wakes_sleeping_bodies_and_supports_antigravity() {
    let mut world = PhysicsWorld::new();
    world.set_gravity(Vec3::ZERO).unwrap();
    let body = cube(&mut world, Vec3::ZERO, Material::default());
    for _ in 0..240 {
        world.step();
    }
    assert!(world.state(body).unwrap().sleeping);
    world.set_gravity(Vec3::new(6.0, 0.0, 0.0)).unwrap();
    for _ in 0..60 {
        world.step();
    }
    let state = world.state(body).unwrap();
    assert!(!state.sleeping && state.position.x > 2.5);
    near(state.linear_velocity.x, 6.0, 1e-6);
    near(state.position.y, 0.0, 1e-12);
    world.set_gravity(Vec3::new(0.0, 9.81, 0.0)).unwrap();
    for _ in 0..60 {
        world.step();
    }
    assert!(world.state(body).unwrap().linear_velocity.y > 9.0);
}

#[test]
fn fixed_steps_preserve_fractional_time_and_catch_up_without_dropping_it() {
    let mut ticked = PhysicsWorld::new();
    let mut framed = PhysicsWorld::new();
    let a = cube(&mut ticked, Vec3::ZERO, Material::default());
    let b = cube(&mut framed, Vec3::ZERO, Material::default());
    for _ in 0..20 {
        assert_eq!(ticked.advance(Duration::from_millis(50)), 3);
    }
    for _ in 0..100 {
        framed.advance(Duration::from_millis(10));
    }
    assert_eq!(
        ticked.state(a).unwrap().position,
        framed.state(b).unwrap().position
    );
    let mut delayed = PhysicsWorld::new();
    let c = cube(&mut delayed, Vec3::ZERO, Material::default());
    assert_eq!(
        delayed.advance(Duration::from_secs(1)),
        physics_core::MAX_SUBSTEPS
    );
    assert_eq!(
        delayed.pending_steps(),
        60 - physics_core::MAX_SUBSTEPS as u128
    );
    while delayed.pending_steps() > 0 {
        delayed.advance(Duration::ZERO);
    }
    assert_eq!(
        ticked.state(a).unwrap().position,
        delayed.state(c).unwrap().position
    );
    assert_eq!(delayed.advance(Duration::from_nanos(16_666_666)), 0);
    assert_eq!(delayed.advance(Duration::from_nanos(1)), 1);
}

#[test]
fn horizon_shapes_form_one_body_with_mass_and_angular_response() {
    let shape = VoxelShape::from_box(Aabb::new(Vec3::ZERO, Vec3::new(1.0, 1.0, 1.0))).unwrap();
    let blocks = [
        BlockCollider {
            offset: BlockPos::new(0, 0, 0),
            shape: &shape,
        },
        BlockCollider {
            offset: BlockPos::new(1, 0, 0),
            shape: &shape,
        },
    ];
    let mut world = PhysicsWorld::new();
    world.set_gravity(Vec3::ZERO).unwrap();
    let body = world
        .insert_blocks(
            BodyKind::Dynamic,
            Vec3::new(-10.0, 0.0, -20.0),
            &blocks,
            Material::default(),
        )
        .unwrap();
    near(world.state(body).unwrap().mass, 2.0, 1e-12);
    world
        .apply_impulse_at_point(body, Vec3::new(2.0, 0.0, 0.0), Vec3::new(-9.0, 1.5, -19.5))
        .unwrap();
    world.step();
    let state = world.state(body).unwrap();
    near(state.linear_velocity.x, 1.0, 1e-6);
    assert!(state.angular_velocity.z < -0.5 && state.rotation[2] < 0.0);
}

#[test]
fn invalid_input_and_fixed_body_forces_are_rejected() {
    let mut world = PhysicsWorld::new();
    let fixed = world
        .insert_cuboid(
            BodyKind::Fixed,
            Vec3::ZERO,
            Vec3::new(1.0, 1.0, 1.0),
            Material::default(),
        )
        .unwrap();
    assert_eq!(
        world.apply_impulse(fixed, Vec3::ZERO),
        Err(PhysicsError::NotDynamic)
    );
    assert_eq!(
        world.set_force(fixed, Vec3::ZERO),
        Err(PhysicsError::NotDynamic)
    );
    assert_eq!(
        world.set_gravity(Vec3::new(f64::NAN, 0.0, 0.0)),
        Err(PhysicsError::InvalidInput)
    );
    assert_eq!(world.gravity(), Vec3::new(0.0, -9.81, 0.0));
    for half in [
        Vec3::ZERO,
        Vec3::new(-1.0, 1.0, 1.0),
        Vec3::new(f64::INFINITY, 1.0, 1.0),
    ] {
        assert_eq!(
            world.insert_cuboid(BodyKind::Dynamic, Vec3::ZERO, half, Material::default()),
            Err(PhysicsError::InvalidInput)
        );
    }
    assert_eq!(
        world.insert_blocks(BodyKind::Dynamic, Vec3::ZERO, &[], Material::default()),
        Err(PhysicsError::InvalidInput)
    );
    for material in [
        Material {
            density: 0.0,
            ..Material::default()
        },
        Material {
            friction: -1.0,
            ..Material::default()
        },
        Material {
            restitution: f64::NAN,
            ..Material::default()
        },
    ] {
        assert_eq!(
            world.insert_cuboid(
                BodyKind::Dynamic,
                Vec3::ZERO,
                Vec3::new(1.0, 1.0, 1.0),
                material
            ),
            Err(PhysicsError::InvalidInput)
        );
    }
}

#[test]
fn fast_cube_cannot_tunnel_through_platform() {
    let mut world = PhysicsWorld::new();
    world
        .insert_cuboid(
            BodyKind::Fixed,
            Vec3::new(0.0, -0.5, 0.0),
            Vec3::new(5.0, 0.5, 5.0),
            Material::default(),
        )
        .unwrap();
    let body = cube(&mut world, Vec3::new(0.0, 4.0, 0.0), Material::default());
    world
        .apply_impulse(body, Vec3::new(0.0, -1000.0, 0.0))
        .unwrap();
    world.step();
    assert!(world.state(body).unwrap().position.y > 0.45);
    for _ in 0..10 {
        world.step();
    }
    assert!(world.state(body).unwrap().position.y > 0.45);
}

#[test]
fn block_local_slab_collides_at_its_real_height_in_negative_world_coordinates() {
    let slab = VoxelShape::from_box(Aabb::new(Vec3::ZERO, Vec3::new(1.0, 0.5, 1.0))).unwrap();
    let mut world = PhysicsWorld::new();
    world
        .insert_blocks(
            BodyKind::Fixed,
            Vec3::new(-10.0, 0.0, -20.0),
            &[BlockCollider {
                offset: BlockPos::new(0, 0, 0),
                shape: &slab,
            }],
            Material::default(),
        )
        .unwrap();
    let body = cube(&mut world, Vec3::new(-9.5, 2.0, -19.5), Material::default());
    for _ in 0..240 {
        world.step();
    }
    let state = world.state(body).unwrap();
    near(state.position.y, 1.0, 0.01);
    near(state.position.x, -9.5, 0.01);
    near(state.position.z, -19.5, 0.01);
}

#[test]
fn removing_body_removes_collisions_and_invalidates_handle() {
    let mut world = PhysicsWorld::new();
    let platform = world
        .insert_cuboid(
            BodyKind::Fixed,
            Vec3::new(0.0, -0.5, 0.0),
            Vec3::new(5.0, 0.5, 5.0),
            Material::default(),
        )
        .unwrap();
    let body = cube(&mut world, Vec3::new(0.0, 1.0, 0.0), Material::default());
    for _ in 0..120 {
        world.step();
    }
    assert!(world.state(body).unwrap().position.y > 0.45);
    world.remove(platform).unwrap();
    assert_eq!(world.remove(platform), Err(PhysicsError::MissingBody));
    assert!(matches!(
        world.state(platform),
        Err(PhysicsError::MissingBody)
    ));
    for _ in 0..60 {
        world.step();
    }
    assert!(world.state(body).unwrap().position.y < -3.0);
    world.remove(body).unwrap();
    let replacement = cube(&mut world, Vec3::ZERO, Material::default());
    assert_ne!(body, replacement);
    assert_eq!(
        world.apply_impulse(body, Vec3::ZERO),
        Err(PhysicsError::MissingBody)
    );
}
