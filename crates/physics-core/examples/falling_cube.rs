//! Headless, real Rapier simulation. No Minecraft resources or renderer required.
use physics_core::{BodyKind, Material, PhysicsWorld, Vec3};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut world = PhysicsWorld::new();
    let material = Material {
        restitution: 0.8,
        ..Material::default()
    };
    world.insert_cuboid(
        BodyKind::Fixed,
        Vec3::new(0.0, -0.5, 0.0),
        Vec3::new(5.0, 0.5, 5.0),
        material,
    )?;
    let cube = world.insert_cuboid(
        BodyKind::Dynamic,
        Vec3::new(0.0, 4.0, 0.0),
        Vec3::new(0.5, 0.5, 0.5),
        material,
    )?;
    let mut fell = false;
    let mut bounced = false;
    println!("step,y,vertical_velocity");
    for step in 1..=180 {
        world.step();
        let state = world.state(cube)?;
        fell |= state.linear_velocity.y < -1.0;
        bounced |= fell && state.linear_velocity.y > 1.0;
        if step % 10 == 0 {
            println!(
                "{step},{:.6},{:.6}",
                state.position.y, state.linear_velocity.y
            );
        }
    }
    if !bounced {
        return Err("cube did not bounce on the platform".into());
    }
    Ok(())
}
