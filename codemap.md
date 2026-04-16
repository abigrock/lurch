# Repository Atlas: Lurch

## Project Responsibility
A Minecraft launcher written in Rust using eframe/egui, supporting multiple mod loaders (Vanilla, Forge, NeoForge, Fabric, Quilt), modpack sources (Modrinth, CurseForge), Microsoft authentication, Java runtime management, and a themeable GUI.

## System Entry Points
- `src/main.rs`: Application entry — configures eframe window, loads fonts, creates App
- `src/app.rs`: Central orchestrator (~1900 lines) — all state, background task polling, request dispatch
- `Cargo.toml`: Dependency manifest (eframe 0.34.1, egui 0.34.1, reqwest, serde, tokio, etc.)

## Architecture Overview
```
main.rs → App::new() → eframe event loop
                ↓
        App::update() each frame
        ├── poll_background_tasks()     — check Arc<Mutex> slots
        ├── render sidebar → get View   — ui/sidebar.rs
        ├── render active view          — ui/* modules
        └── handle_view_requests()      — dispatch to core/* logic
```

**Key patterns**: Immediate-mode GUI, background threads with Arc<Mutex<T>> polling, request flags for UI→logic communication, SHA1-verified downloads, JSON persistence.

## Directory Map

| Directory | Responsibility | Detailed Map |
|-----------|---------------|--------------|
| `src/` | Application root: entry point, App struct, module organization | [View Map](src/codemap.md) |
| `src/core/` | Business logic: auth, launch, versions, mods, Java, instances (21 modules) | [View Map](src/core/codemap.md) |
| `src/ui/` | GUI presentation: views, sidebar, helpers (egui immediate-mode) | [View Map](src/ui/codemap.md) |
| `src/ui/instances/` | Instance management UI: list, creation, modpack browser (~5400 LOC) | [View Map](src/ui/instances/codemap.md) |
| `src/ui/instances/detail/` | Per-instance detail: tabbed mods/worlds/shaders/servers management | [View Map](src/ui/instances/detail/codemap.md) |
| `src/theme/` | Theme engine: 33 bundled themes, user themes, styling helpers | [View Map](src/theme/codemap.md) |
| `src/util/` | Utilities: platform directory resolution | [View Map](src/util/codemap.md) |
| `.github/workflows/` | CI (`ci.yml`: check/clippy/test on 3 OS) and Release (`release.yml`: build 4 targets on tag push, create GitHub Release) | — |
