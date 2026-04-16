# src/core/

## Responsibility
Business logic layer — all non-UI functionality for the Minecraft launcher: authentication, game launching, version/asset management, mod loader integration, modpack installation, Java runtime management, and instance CRUD.

## Design
19 modules, each owning a specific domain. Key patterns:
- **Pipeline**: `launch.rs` orchestrates a multi-step launch sequence (version fetch → loader merge → Java selection → client JAR → Forge processors → libraries → assets → spawn)
- **Auth Chain**: `account.rs` implements Microsoft Device Code Flow (MS → XBL → XSTS → MC → Profile) with token refresh
- **Profile Merging**: `loader_profiles.rs` merges mod loader profiles (Fabric/Quilt/Forge) into base Mojang version info
- **Parallel Downloads**: `version.rs` uses 8-thread pool for asset downloads; `java.rs` uses parallel manifest downloads for Mojang JRE
- **SHA1 Verification**: All downloaded files verified via shared `sha1_hex()` in `mod.rs`
- **JSON Persistence**: Instances, accounts, config stored as JSON in platform directories

## Modules

| Module | Purpose | Key Types |
|--------|---------|-----------|
| `mod.rs` | Shared utilities: USER_AGENT, http_client(), sha1_hex(), maven_path(), extract_zip_overrides(), `ModpackModEntry` struct | — |
| `config.rs` | App configuration persistence | `AppConfig` |
| `instance.rs` | Instance model + CRUD | `Instance`, `ModLoader` enum (Vanilla/Forge/NeoForge/Fabric/Quilt) |
| `account.rs` | Microsoft OAuth + offline accounts | `MinecraftAccount`, `AccountStore` |
| `java.rs` | Java detection, download (Adoptium + Mojang JRE), version recommendation | `JavaInstallation`, `detect_java_installations()` |
| `launch.rs` | Game launch pipeline, process management | `LaunchContext`, `ProcessState` |
| `version.rs` | Mojang manifest, library/asset downloads | `VersionManifest`, `VersionInfo`, rule evaluation || `forge.rs` | Forge/NeoForge installer processing | Forge profile merging, processor execution |
| `modpack_manager.rs` | Modpack installation orchestrator | `ModpackManager`, background thread spawning |
| `launch_manager.rs` | Game launch orchestration | `LaunchManager`, `LaunchEvent`, `RunningProcess` |
| `modrinth_modpack.rs` | Modrinth .mrpack installation + updates, writes `.modpack_mods.json` | `install_modrinth_modpack()`, `update_modrinth_modpack()` |
| `curseforge_modpack.rs` | CurseForge modpack installation + updates, writes `.modpack_mods.json` | `install_curseforge_modpack()`, `update_curseforge_modpack()`, `wait_for_cf_manual_download()` |
| `curseforge.rs` | CurseForge API client | API search, file download || `modrinth.rs` | Modrinth API client | `search_mods()`, `get_project_versions()`, `download_mod_file()` |
| `local_mods.rs` | Local mod management | `InstalledMod`, `scan_installed_mods()`, toggle, remove |
| `loader_profiles.rs` | Mod loader version fetching + profile merging | `fetch_loader_versions()`, `fetch_and_merge_loader_profile()` |
| `servers.rs` | Server list management per instance | server.dat reading/writing |
| `shaders.rs` | Shader pack management | Shader file operations |
| `worlds.rs` | World/save management | World import/export |
| `import_export.rs` | Instance import/export | Archive creation/extraction |
| `mod_cache.rs` | Mod file caching: resolve_or_download(), resolve_from_cache(), cache_file() | Cross-instance mod dedup via SHA1-indexed cache |
| `update.rs` | Modpack update checking + metadata propagation | `ModpackUpdateInfo`, `UpdatedModpackMeta` (mc_version, loader, loader_version) |

## Flow
1. **Config** loads from `config_dir/config.json` at startup
2. **Instances** loaded from `instances_dir/*/instance.json` via `load_all_instances()`
3. **Accounts** loaded from `config_dir/accounts.json` via `AccountStore`
4. **Launch sequence**: UI triggers → `prepare_and_launch()` → version resolution → loader merge → Java auto-select → downloads → `LaunchContext::build_command()` → `ProcessState` (stdout/stderr capture, kill support)
5. **Modpack install**: Parse archive → create instance → download mods → write overrides → persist `.modpack_mods.json` manifest

## Integration
- **Consumed by**: `src/app.rs` (central orchestrator), UI modules via App state
- **Depends on**: `src/util/paths.rs` (directory resolution), external APIs (Mojang, Microsoft, Modrinth, CurseForge, Adoptium, Fabric/Quilt meta)
