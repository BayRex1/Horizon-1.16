#![cfg(feature = "physics-unleashed")]
//! Real resident World/SceneCollisions CPU tests; no game assets or GUI session.
#[path = "support/physics_fixture.rs"]
mod fixture;
use fixture::{binding, fixture, scene, tick, world};
use horizon_client::{chunk::Chunk, physics::PhysicsDimension, world::World};
use horizon_core::geometry::Vec3;
use horizon_world::registry::BlockStateId;
use std::{sync::Arc, time::Duration};

#[test]
fn falls_and_bounces_on_real_resident_block_without_changing_it() {
    let (registry, data) = fixture();
    let binding = binding(&registry, &data);
    let mut world = world();
    world.set(8, 0, 8, BlockStateId(1)).unwrap();
    world.block_log = Some(Vec::new());
    let mut physics = world.physics.take().unwrap();
    let body = physics
        .spawn_cube(&mut scene(&world, &binding), Vec3::new(8.5, 4.0, 8.5))
        .unwrap();
    let mut fell = false;
    let mut bounced = false;
    for _ in 0..60 {
        tick(&mut physics, &world, &binding, 1);
        let state = physics.state(body).unwrap();
        fell |= state.linear_velocity.y < -1.0;
        bounced |= fell && state.linear_velocity.y > 1.0;
        assert!(state.position.y > 1.25);
        // The block is cached only while the body is near enough to need it.
        assert!(physics.terrain_count() <= 1); // No duplicates after rescan.
    }
    assert!(fell && bounced);
    assert_eq!(world.block(8, 0, 8), BlockStateId(1));
    assert!(world.block_log.as_ref().unwrap().is_empty());
}

#[test]
fn edits_replace_shape_and_removal_clears_collider_without_draining_network_log() {
    let (registry, data) = fixture();
    let binding = binding(&registry, &data);
    let mut world = world();
    world.set(8, 0, 8, BlockStateId(1)).unwrap();
    let mut physics = world.physics.take().unwrap();
    let body = physics
        .spawn_cube(&mut scene(&world, &binding), Vec3::new(8.5, 3.0, 8.5))
        .unwrap();
    tick(&mut physics, &world, &binding, 140);
    assert!((physics.state(body).unwrap().position.y - 1.5).abs() < 0.02);
    world.block_log = Some(Vec::new());
    world.set(8, 0, 8, BlockStateId(2)).unwrap();
    tick(&mut physics, &world, &binding, 120);
    assert!((physics.state(body).unwrap().position.y - 1.0).abs() < 0.02);
    assert_eq!(physics.terrain_count(), 1);
    world.set(8, 0, 8, BlockStateId(0)).unwrap();
    tick(&mut physics, &world, &binding, 20);
    assert_eq!(physics.terrain_count(), 0);
    assert!(physics.state(body).unwrap().position.y < 0.0);
    assert_eq!(
        world.block_log.as_ref().unwrap(),
        &vec![(8, 0, 8), (8, 0, 8)]
    );
}

#[test]
fn unload_removes_bodies_and_terrain_and_reload_creates_fresh_colliders() {
    let (registry, data) = fixture();
    let binding = binding(&registry, &data);
    let mut world = world();
    world.set(8, 0, 8, BlockStateId(1)).unwrap();
    let mut physics = world.physics.take().unwrap();
    let body = physics
        .spawn_cube(&mut scene(&world, &binding), Vec3::new(8.5, 3.0, 8.5))
        .unwrap();
    let chunk = world.chunks.remove(&(0, 0)).unwrap();
    // Frozen tick still cleans up; no body is simulated in a missing column.
    physics
        .tick(&mut scene(&world, &binding), Duration::ZERO)
        .unwrap();
    assert!(physics.state(body).is_err());
    assert!(physics.bodies().is_empty());
    assert_eq!(physics.terrain_count(), 0);
    assert!(
        physics
            .spawn_cube(&mut scene(&world, &binding), Vec3::new(8.5, 3.0, 8.5))
            .is_err()
    );
    world.chunks.insert((0, 0), chunk);
    let new = physics
        .spawn_cube(&mut scene(&world, &binding), Vec3::new(8.5, 3.0, 8.5))
        .unwrap();
    assert_ne!(body, new);
    assert_eq!(physics.terrain_count(), 1);
}

#[test]
fn gravity_and_simulation_state_are_owned_by_world_and_survive_parking() {
    let (registry, data) = fixture();
    let binding = binding(&registry, &data);
    let mut overworld = world();
    let mut nether = world();
    let mut physics = overworld.physics.take().unwrap();
    physics.set_gravity(Vec3::new(0.0, 9.81, 0.0)).unwrap();
    let body = physics
        .spawn_cube(&mut scene(&overworld, &binding), Vec3::new(8.5, 3.0, 8.5))
        .unwrap();
    tick(&mut physics, &overworld, &binding, 20);
    assert!(physics.state(body).unwrap().position.y > 7.0);
    overworld.physics = Some(physics);
    // Parked stores/swaps the complete World, not a shared global physics scene.
    std::mem::swap(&mut overworld, &mut nether);
    assert!(overworld.physics.as_ref().unwrap().bodies().is_empty());
    assert_eq!(
        overworld.physics.as_ref().unwrap().gravity(),
        Vec3::new(0.0, -9.81, 0.0)
    );
    assert_eq!(nether.physics.as_ref().unwrap().bodies(), &[body]);
    assert_eq!(
        nether.physics.as_ref().unwrap().gravity(),
        Vec3::new(0.0, 9.81, 0.0)
    );
}

#[test]
fn lag_budget_drops_debt_and_freeze_does_not_step() {
    let (registry, data) = fixture();
    let binding = binding(&registry, &data);
    let world = world();
    let mut physics = PhysicsDimension::default();
    let body = physics
        .spawn_cube(&mut scene(&world, &binding), Vec3::new(8.5, 3.0, 8.5))
        .unwrap();
    let report = physics
        .tick(&mut scene(&world, &binding), Duration::from_secs(3600))
        .unwrap();
    assert_eq!(report.steps, 8);
    assert_eq!(report.dropped_steps, 216_000 - 8);
    let before = physics.state(body).unwrap().position;
    assert_eq!(
        physics
            .tick(&mut scene(&world, &binding), Duration::ZERO)
            .unwrap()
            .steps,
        0
    );
    assert_eq!(before, physics.state(body).unwrap().position);
    assert_eq!(
        physics
            .tick(&mut scene(&world, &binding), Duration::from_millis(50))
            .unwrap()
            .steps,
        3
    );
}

#[test]
fn unknown_shapes_stop_simulation_instead_of_becoming_air_or_cubes() {
    let (registry, data) = fixture();
    let binding = binding(&registry, &data);
    let mut world = world();
    let mut physics = PhysicsDimension::default();
    let body = physics
        .spawn_cube(&mut scene(&world, &binding), Vec3::new(8.5, 3.0, 8.5))
        .unwrap();
    let before = physics.state(body).unwrap().position;
    world.set(8, 1, 8, BlockStateId(3)).unwrap();
    assert!(
        physics
            .tick(&mut scene(&world, &binding), Duration::from_millis(50))
            .is_err()
    );
    assert_eq!(before, physics.state(body).unwrap().position);
}

#[test]
fn neighbour_chunk_unload_despawns_border_body() {
    let (registry, data) = fixture();
    let binding = binding(&registry, &data);
    let mut world = world();
    world.chunks.insert((1, 0), Arc::new(Chunk::empty(1, 0)));
    let mut physics = PhysicsDimension::default();
    let body = physics
        .spawn_cube(&mut scene(&world, &binding), Vec3::new(15.5, 3.0, 8.5))
        .unwrap();
    world.chunks.remove(&(1, 0));
    physics
        .tick(&mut scene(&world, &binding), Duration::from_millis(50))
        .unwrap();
    assert!(physics.state(body).is_err());
    assert!(physics.bodies().is_empty());
}

#[test]
fn negative_block_and_chunk_coordinates_are_not_translated_twice() {
    let (registry, data) = fixture();
    let binding = binding(&registry, &data);
    let mut world = World::new(BlockStateId(0), 0);
    world
        .chunks
        .insert((-1, -1), Arc::new(Chunk::empty(-1, -1)));
    world.set(-9, 0, -9, BlockStateId(1)).unwrap();
    let mut physics = world.physics.take().unwrap();
    let body = physics
        .spawn_cube(&mut scene(&world, &binding), Vec3::new(-8.5, 3.0, -8.5))
        .unwrap();
    tick(&mut physics, &world, &binding, 140);
    let state = physics.state(body).unwrap();
    assert!((state.position.y - 1.5).abs() < 0.02);
    assert!((state.position.x + 8.5).abs() < 0.02);
    assert_eq!(physics.terrain_count(), 1);
}

#[test]
fn limits_and_repeated_teardown_keep_state_bounded() {
    let (registry, data) = fixture();
    let binding = binding(&registry, &data);
    let world = world();
    let mut physics = PhysicsDimension::default();
    assert!(
        physics
            .set_gravity(Vec3::new(f64::INFINITY, 0.0, 0.0))
            .is_err()
    );
    assert!(physics.set_gravity(Vec3::new(101.0, 0.0, 0.0)).is_err());
    assert!(
        physics
            .spawn_cube(&mut scene(&world, &binding), Vec3::new(8.5, -100.0, 8.5))
            .is_err()
    );
    for _ in 0..5 {
        let mut old = Vec::new();
        for _ in 0..16 {
            old.push(
                physics
                    .spawn_cube(&mut scene(&world, &binding), Vec3::new(8.5, 3.0, 8.5))
                    .unwrap(),
            );
        }
        assert!(
            physics
                .spawn_cube(&mut scene(&world, &binding), Vec3::new(8.5, 3.0, 8.5))
                .is_err()
        );
        assert_eq!(physics.bodies().len(), 16);
        physics.clear().unwrap();
        assert!(physics.bodies().is_empty());
        assert_eq!(physics.terrain_count(), 0);
        let new = physics
            .spawn_cube(&mut scene(&world, &binding), Vec3::new(8.5, 3.0, 8.5))
            .unwrap();
        assert!(!old.contains(&new));
        assert!(old.iter().all(|&body| physics.state(body).is_err()));
        physics.clear().unwrap();
    }
}
