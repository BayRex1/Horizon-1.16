#![cfg(feature = "physics-unleashed")]
#[path = "support/physics_fixture.rs"]
mod fixture;
use fixture::{binding, fixture, scene, tick, world};
use glam::{DQuat, DVec3, Vec3};
use horizon_client::physics_view::{PhysicsView, block_transform};
use horizon_protocol::{
    buf::Reader,
    packets::{configuration::CustomPayload, play::Clientbound},
    physics::{CubeState, Snapshot},
};
use std::{
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    time::{Duration, Instant},
};

const DIM: &str = "minecraft:overworld";
fn frame(sequence: u64, step: u64, x: f64) -> Snapshot {
    Snapshot {
        dimension: DIM.into(),
        epoch: 1,
        sequence,
        step,
        bodies: vec![CubeState {
            id: 100,
            position: [x, 80.0, 8.5],
            rotation: [0.0, 0.0, 0.0, 1.0],
        }],
        removed: vec![],
    }
}

#[test]
fn interpolation_is_time_based_at_30_60_and_144_fps_and_rotates() {
    let now = Instant::now();
    for fps in [30, 60, 144] {
        let mut view = PhysicsView::default();
        view.receive(frame(1, 0, 0.0), DIM, now).unwrap();
        let mut next = frame(2, 12, 12.0);
        next.bodies[0].rotation = DQuat::from_rotation_y(std::f64::consts::PI).to_array();
        view.receive(next, DIM, now).unwrap();
        for n in 0..fps {
            let dt = n as f64 / fps as f64;
            if dt > 0.025 {
                break;
            }
            view.sample(now + Duration::from_secs_f64(dt));
        }
        let sample = view.sample(now + Duration::from_millis(25));
        assert!((sample[0].position[0] - 7.5).abs() < 1e-9);
        let rotated = DQuat::from_array(sample[0].rotation) * DVec3::X;
        let expected = DQuat::from_rotation_y(std::f64::consts::PI * 0.625) * DVec3::X;
        assert!(rotated.distance(expected) < 1e-9);
    }
}

#[test]
fn removal_full_membership_and_new_epoch_do_not_leave_ghosts() {
    let now = Instant::now();
    let mut view = PhysicsView::default();
    view.receive(frame(1, 0, 0.0), DIM, now).unwrap();
    assert_eq!(view.sample(now).len(), 1);
    let mut clear = frame(2, 0, 0.0);
    clear.bodies.clear();
    clear.removed = vec![100];
    view.receive(clear, DIM, now).unwrap();
    assert!(view.sample(now).is_empty());
    assert!(!view.receive(frame(1, 0, 0.0), DIM, now).unwrap());
    let mut restart = frame(1, 0, 99.0);
    restart.epoch = 2;
    restart.bodies[0].id = 200;
    view.receive(restart, DIM, now).unwrap();
    assert_eq!(view.sample(now)[0].position[0], 99.0);
    assert!(!view.receive(frame(3, 0, 0.0), DIM, now).unwrap());
    let mut wrong = frame(9, 0, 500.0);
    wrong.epoch = 10;
    wrong.dimension = "minecraft:the_nether".into();
    assert!(!view.receive(wrong, DIM, now).unwrap());
    assert_eq!(view.sample(now)[0].id, 200);
}

#[test]
fn frozen_updates_and_packet_loss_hold_real_latest_pose_without_extrapolation() {
    let now = Instant::now();
    let mut view = PhysicsView::default();
    view.receive(frame(1, 0, 0.0), DIM, now).unwrap();
    view.receive(frame(2, 12, 12.0), DIM, now).unwrap();
    let mut frozen = frame(3, 12, 12.0);
    // q and -q must represent exactly the same orientation, not a full spin.
    frozen.bodies[0].rotation = [0.0, 0.0, 0.0, -1.0];
    view.receive(frozen, DIM, now + Duration::from_millis(50))
        .unwrap();
    let pose = view.sample(now + Duration::from_millis(100));
    assert_eq!(pose[0].position[0], 12.0);
    assert_eq!(
        view.sample(now + Duration::from_secs(10))[0].position[0],
        12.0
    );
    assert!(view.receive(frame(4, 1, 1.0), DIM, now).is_err());
}

#[test]
fn render_matrix_centres_unit_block_and_preserves_large_coordinate_precision() {
    let mut state = frame(1, 0, 30_000_000.25).bodies.remove(0);
    state.rotation = DQuat::from_rotation_y(std::f64::consts::FRAC_PI_2).to_array();
    let pose = block_transform(&state, DVec3::new(30_000_000.0, 80.0, 8.5));
    assert!(
        pose.transform_point3(Vec3::splat(0.5))
            .distance(Vec3::new(0.25, 0.0, 0.0))
            < 1e-6
    );
    let a = pose.transform_point3(Vec3::ZERO);
    let b = pose.transform_point3(Vec3::X);
    assert!((a.distance(b) - 1.0).abs() < 1e-6);
    assert!((b - a).distance(Vec3::NEG_Z) < 1e-6);
}

#[test]
fn stable_ids_updates_clear_and_unload_come_from_the_real_simulation() {
    let (registry, data) = fixture();
    let binding = binding(&registry, &data);
    let mut world = world();
    let mut physics = world.physics.take().unwrap();
    let body = physics
        .spawn_cube(
            &mut scene(&world, &binding),
            physics_core::Vec3::new(8.5, 4.0, 8.5),
        )
        .unwrap();
    let first = physics.snapshot(DIM).unwrap();
    let id = first.bodies[0].id;
    tick(&mut physics, &world, &binding, 2);
    let moved = physics.snapshot(DIM).unwrap();
    assert_eq!(moved.bodies[0].id, id);
    assert_eq!(moved.step, 6);
    assert_eq!(
        moved.bodies[0].position,
        physics.state(body).unwrap().position.components()
    );
    assert!(moved.bodies[0].position[1] < first.bodies[0].position[1]);
    physics.clear().unwrap();
    let clear = physics.snapshot(DIM).unwrap();
    assert_eq!(clear.removed, vec![id]);
    assert!(clear.bodies.is_empty());
    let new = physics
        .spawn_cube(
            &mut scene(&world, &binding),
            physics_core::Vec3::new(8.5, 4.0, 8.5),
        )
        .unwrap();
    let current = physics.snapshot(DIM).unwrap();
    assert_ne!(id, current.bodies[0].id);
    world.chunks.clear();
    physics
        .tick(&mut scene(&world, &binding), Duration::ZERO)
        .unwrap();
    let unload = physics.snapshot(DIM).unwrap();
    assert_eq!(unload.removed, vec![current.bodies[0].id]);
    assert!(physics.state(new).is_err());
}

#[test]
fn real_rapier_angular_motion_reaches_the_render_matrix() {
    use physics_core::{BodyKind, Material, PhysicsWorld, Vec3 as PhysicsVec};
    let mut engine = PhysicsWorld::new();
    engine.set_gravity(PhysicsVec::ZERO).unwrap();
    let body = engine
        .insert_cuboid(
            BodyKind::Dynamic,
            PhysicsVec::new(8.5, 80.0, 8.5),
            PhysicsVec::new(0.5, 0.5, 0.5),
            Material::default(),
        )
        .unwrap();
    engine
        .apply_impulse_at_point(
            body,
            PhysicsVec::new(2.0, 0.0, 0.0),
            PhysicsVec::new(8.5, 80.5, 8.5),
        )
        .unwrap();
    engine.step();
    let state = engine.state(body).unwrap();
    assert!(state.rotation[2].abs() > 0.01);
    let mut frame = frame(1, 1, 0.0);
    frame.bodies[0].position = state.position.components();
    frame.bodies[0].rotation = state.rotation;
    let received = Snapshot::decode(&frame.encode().unwrap()).unwrap();
    let mut view = PhysicsView::default();
    let now = Instant::now();
    view.receive(received, DIM, now).unwrap();
    let sample = view.sample(now).remove(0);
    let matrix = block_transform(&sample, DVec3::from_array(state.position.components()));
    let expected = (DQuat::from_array(state.rotation) * DVec3::X).as_vec3();
    assert!(matrix.transform_vector3(Vec3::X).distance(expected) < 1e-6);
}

#[test]
fn snapshot_crosses_real_tcp_and_normal_horizon_custom_payload_codec() {
    let (registry, data) = fixture();
    let binding = binding(&registry, &data);
    let mut world = world();
    let mut physics = world.physics.take().unwrap();
    physics
        .spawn_cube(
            &mut scene(&world, &binding),
            physics_core::Vec3::new(8.5, 4.0, 8.5),
        )
        .unwrap();
    tick(&mut physics, &world, &binding, 3);
    let expected = physics.snapshot(DIM).unwrap();
    let packet = Clientbound::CustomPayload(CustomPayload {
        channel: horizon_protocol::physics::CHANNEL.into(),
        data: expected.encode().unwrap(),
    });
    let wire = horizon_protocol::frame::encode(&packet.encode(), Some(16)).unwrap();
    let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let address = listener.local_addr().unwrap();
    let sender = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        stream.write_all(&wire).unwrap();
    });
    let mut stream = TcpStream::connect(address).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .unwrap();
    let mut bytes = Vec::new();
    stream.read_to_end(&mut bytes).unwrap();
    sender.join().unwrap();
    let framed = horizon_protocol::frame::split(&mut bytes).unwrap().unwrap();
    assert!(bytes.is_empty());
    let body = horizon_protocol::frame::decompress(framed, Some(16)).unwrap();
    let mut reader = Reader::new(&body);
    let id = reader.varint().unwrap();
    let Clientbound::CustomPayload(payload) = Clientbound::decode(id, reader.rest()).unwrap()
    else {
        panic!("wrong packet");
    };
    assert_eq!(payload.channel, horizon_protocol::physics::CHANNEL);
    let received = Snapshot::decode(&payload.data).unwrap();
    assert_eq!(received, expected);
    let now = Instant::now();
    let mut view = PhysicsView::default();
    view.receive(received, DIM, now).unwrap();
    assert_eq!(view.sample(now), expected.bodies);
}
