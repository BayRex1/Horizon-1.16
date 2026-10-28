#!/usr/bin/env bash
set -e

ROOT="/home/artem/CLionProjects/horizon-hammer_core"
cd "$ROOT"

echo "=== Horizon Test Mod Runner ==="
echo ""

# Check if wasm32 target is installed
if ! rustup target list --installed | grep -q wasm32-unknown-unknown; then
    echo "[!] Installing wasm32-unknown-unknown target..."
    rustup target add wasm32-unknown-unknown
fi

# Build test_core mod
echo "[1/3] Building test_core mod..."
cd "$ROOT/mods/test_core"
cargo build --target wasm32-unknown-unknown --release 2>&1 | tail -15

# Copy to horizon-mods dir for the game
echo "[2/3] Installing test_core to mods directory..."
MODS_DIR="$ROOT/.local/horizon-client/26.3/horizon-mods"
mkdir -p "$MODS_DIR"
cp target/wasm32-unknown-unknown/release/test_core.wasm "$MODS_DIR/test_core.wasm" 2>/dev/null || cp target/wasm32-unknown-unknown/release/deps/*.wasm "$MODS_DIR/test_core.wasm" 2>/dev/null || true
cp horizon-mod.toml "$MODS_DIR/horizon-mod.toml" 2>/dev/null || true

# Also copy as folder structure
rm -rf "$MODS_DIR/test_core"
mkdir -p "$MODS_DIR/test_core"
cp target/wasm32-unknown-unknown/release/test_core.wasm "$MODS_DIR/test_core/mod.wasm" 2>/dev/null || true
cp horizon-mod.toml "$MODS_DIR/test_core/horizon-mod.toml"

echo "[3/3] Building and launching client..."
cd "$ROOT"
cargo run --profile fast -p horizon-client --features mods -- --resources "$ROOT/.local/horizon-client/26.3" 2>&1 | tail -40
