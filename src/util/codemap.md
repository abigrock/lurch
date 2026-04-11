# src/util/

## Responsibility
Shared utilities — platform directory resolution.

## Design
- **`paths.rs`**: Uses `directories` crate (`ProjectDirs`) to resolve platform-appropriate paths: `data_dir()`, `config_dir()`, `themes_dir()`, `instances_dir()`. Creates directories on first access.

## Flow
1. `paths.rs` functions called throughout app for all file I/O locations

## Integration
- **Consumed by**: All `src/core/` modules, `src/ui/` modules, `src/theme/` (themes dir)
- **Depends on**: `directories` crate
