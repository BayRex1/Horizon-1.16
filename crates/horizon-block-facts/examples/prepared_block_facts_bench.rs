//! Lightweight benchmark of BlockFacts, without linking the rendering client.
#[path = "../../horizon-client/examples/block_facts_bench.rs"]
mod benchmark;

fn main() -> horizon_block_facts::Result<()> {
    benchmark::main()
}
