// Authored tiny registry shared by CPU physics integration tests.
use horizon_client::{
    chunk::Chunk, collisions::SceneCollisions, physics::PhysicsDimension, world::World,
};
use horizon_world::{
    DATA_VERSION,
    block_collision::BlockCollisionRegistry,
    collision::CachedCollisionRegistry,
    nbt::Tag,
    registry::{BlockRegistry, BlockStateId},
};
use std::{sync::Arc, time::Duration};

pub fn fixture() -> (BlockRegistry, Tag) {
    let states: Vec<_> = [
        "minecraft:air",
        "minecraft:stone",
        "physics:slab",
        "physics:unsupported",
    ]
    .into_iter()
    .map(|s| Tag::String(s.into()))
    .collect();
    let registry = Tag::compound([
        ("DataVersion", Tag::Int(DATA_VERSION)),
        ("states", Tag::List(states.clone())),
        (
            "defaults",
            Tag::compound([
                ("minecraft:air", Tag::Int(0)),
                ("minecraft:stone", Tag::Int(1)),
                ("physics:slab", Tag::Int(2)),
                ("physics:unsupported", Tag::Int(3)),
            ]),
        ),
    ]);
    let empty = Tag::compound([
        ("x", Tag::LongArray(vec![0])),
        ("y", Tag::LongArray(vec![0])),
        ("z", Tag::LongArray(vec![0])),
        ("occupied", Tag::ByteArray(vec![])),
        ("cube", Tag::Byte(0)),
        ("cube_axes", Tag::Byte(0)),
        ("discrete_min", Tag::IntArray(vec![0; 3])),
        ("discrete_max", Tag::IntArray(vec![0; 3])),
    ]);
    let shape = |height: f64| {
        Tag::compound([
            ("x", Tag::LongArray(vec![0, 1.0f64.to_bits() as i64])),
            ("y", Tag::LongArray(vec![0, height.to_bits() as i64])),
            ("z", Tag::LongArray(vec![0, 1.0f64.to_bits() as i64])),
            ("occupied", Tag::ByteArray(vec![1])),
            ("cube", Tag::Byte(i8::from(height == 1.0))),
            ("cube_axes", Tag::Byte(if height == 1.0 { 7 } else { 0 })),
            ("discrete_min", Tag::IntArray(vec![0; 3])),
            ("discrete_max", Tag::IntArray(vec![1; 3])),
        ])
    };
    let collision = Tag::compound([
        ("DataVersion", Tag::Int(DATA_VERSION)),
        ("states", Tag::List(states)),
        ("shapes", Tag::List(vec![empty, shape(1.0), shape(0.5)])),
        ("cached_shapes", Tag::IntArray(vec![0, 1, 2, -1])),
    ]);
    (BlockRegistry::from_nbt(&registry).unwrap(), collision)
}
pub fn binding<'a>(registry: &'a BlockRegistry, data: &Tag) -> BlockCollisionRegistry<'a> {
    BlockCollisionRegistry::new(Arc::new(
        CachedCollisionRegistry::from_nbt(data, registry).unwrap(),
    ))
    .unwrap()
}
pub fn world() -> World {
    let mut world = World::new(BlockStateId(0), 0);
    world.chunks.insert((0, 0), Arc::new(Chunk::empty(0, 0)));
    world
}
pub fn scene<'w, 'r>(
    world: &'w World,
    binding: &'w BlockCollisionRegistry<'r>,
) -> SceneCollisions<'w, 'r> {
    SceneCollisions {
        world,
        binding,
        subject: None,
    }
}
pub fn tick(
    physics: &mut PhysicsDimension,
    world: &World,
    binding: &BlockCollisionRegistry<'_>,
    n: usize,
) {
    for _ in 0..n {
        assert_eq!(
            physics
                .tick(&mut scene(world, binding), Duration::from_millis(50))
                .unwrap()
                .steps,
            3
        );
    }
}
