//! Profile the resource-dependent block-entity renderer loader in isolation.
//! Run with the prepared resource root as the single argument.
use horizon_client::{assets::Assets, block_entity_render::BlockEntityRenderer};
use std::{path::PathBuf, time::Instant};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let root = PathBuf::from(std::env::args_os().nth(1).ok_or("missing resource root")?);
    let start = Instant::now();
    let assets = Assets::load(&root)?;
    eprintln!("registry_ms={:.3}", start.elapsed().as_secs_f64() * 1000.0);
    let started = Instant::now();
    let renderer = BlockEntityRenderer::load(&root, assets.registry)?;
    std::hint::black_box(renderer);
    eprintln!(
        "block_entity_renderer_ms={:.3}",
        started.elapsed().as_secs_f64() * 1000.0
    );
    Ok(())
}
