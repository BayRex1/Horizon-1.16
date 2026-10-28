# Horizon Hammer Core - Modding Core Development Log

> **FORK NOTICE:** В этом форке мы работаем **ТОЛЬКО** над модовым ядром.  
> Зона работы: `crates/horizon-mods/`, `crates/horizon-mod-sdk/`, `crates/cargo-horizon-mod/`, `mods/test_core/`.  
> Всё остальное (рендер, мир, геймплей, сеть, лаунчер и т.п.) трогать не нужно, если это не требуется исключительно для интеграции модового ядра.

This file tracks the development of the NeoForge-like modding core for Horizon.

## Current Version
**Core Version:** `0.1.0-dev`  
**Last Updated:** 2025-10-28

## Project Context
Horizon is a Minecraft: Java Edition 26.3 rewrite in Rust (wgpu). We are building a NeoForge-like modding API on top of the existing WASM-based mod system.

## Current State
The project has a working WASM mod system in `crates/horizon-mods/` with:
- WASM sandbox using wasmtime component model
- Capability-based permissions
- Client/Server mod separation
- Content mods (assets/data)
- Basic hooks (tick, frame, chat, command)

## Changelog

### 2025-10-28 - NeoForge-like Core Infrastructure

#### Added
- **`crates/horizon-mods/src/container.rs`** - `ModContainer` structure
  - Centralized mod representation (id, name, version, side, capabilities, dependencies)
  - `ModState` enum (Discovered, Validated, Constructing, Setup, Loaded, Errored, Disabled)
  - `ModDependency` with ordering support
  - NeoForge-like API surface
  
- **`crates/horizon-mods/src/event_bus.rs`** - Synchronous event bus
  - Type-safe event posting/subscribing using `TypeId`
  - `Event` trait for all engine events
  - Built-in events: `TickEvent`, `ModLoadEvent`
  - Thread-safe with RwLock
  
- **`crates/horizon-mods/src/context.rs`** - `ModLoadingContext`
  - Provides access to mod container and event bus during loading
  - NeoForge-style context pattern
  
- **`crates/horizon-mods/src/lifecycle.rs`** - Loading lifecycle
  - `LoadingStage` enum (Construct, CommonSetup, SidedSetup, LoadComplete, PostLoad)
  - `LifecycleManager` for stage progression
  
- **`crates/horizon-mods/src/dependency.rs`** - Dependency resolution
  - Topological sorting for load order
  - Circular dependency detection
  - Missing dependency validation
  
- **`crates/horizon-mods/src/dist.rs`** - Distribution side
  - `Dist` enum (Client, DedicatedServer)
  - Helper methods for side checking

#### Modified
- **`crates/horizon-mods/src/lib.rs`** - Exported all new modules and types
- **`crates/horizon-mods/src/runtime.rs`** 
  - Integrated `ModContainer` creation into mod loading
  - Added `EventBus` to `Mods` struct
  - Post `TickEvent` and `ModLoadEvent` automatically
  - Added `event_bus()`, `post_event()` methods to `Mods`

#### Test Mod
- **`mods/test_core/`** - Test mod for validating new infrastructure
  - Uses `chat` and `hud` capabilities
  - Tests tick hooks
  - Will be updated as new features are added

## Architecture Overview

```
crates/horizon-mods/src/
├── container.rs      # ModContainer, ModState, ModDependency
├── event_bus.rs      # EventBus, Event trait
├── context.rs        # ModLoadingContext
├── lifecycle.rs      # LoadingStage, LifecycleManager
├── dependency.rs     # DependencyResolver
├── dist.rs           # Dist enum
├── runtime.rs        # WASM runtime, Mods manager (integrated)
├── manifest.rs       # Manifest parsing
└── lib.rs            # Public API
```

## Roadmap

### Phase 1: Core Infrastructure (In Progress)
- [x] ModContainer
- [x] EventBus
- [x] ModLoadingContext
- [x] Lifecycle stages
- [x] Dependency resolution
- [x] Dist enum
- [ ] Integration with SDK (expose to mod authors)
- [ ] More engine events

### Phase 2: Registries (Next)
- [ ] Registry system (blocks, items, entities)
- [ ] DeferredRegister API
- [ ] Registry events
- [ ] Link to horizon-world/horizon-items

### Phase 3: Advanced Features
- [ ] Menu/Container replacement API
- [ ] Custom block entities
- [ ] Fluid system API
- [ ] Network channels for mods
- [ ] Capability expansion

## Testing
Run tests: `cargo test -p horizon-mods --features runtime,pack`

All tests passing as of last update.

## Notes
- We're building on top of existing WASM system, not replacing it
- Focus on adding NeoForge-like patterns while maintaining compatibility
- Use `mods/test_core/` to test new features
