# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Overview

This is a Rust tool for extracting data from Apple Aperture 3.x libraries. It parses the proprietary `.aplib` bundle format (plists and SQLite database) to enable migration to other photo management systems. This fork adds export functionality with XMP sidecar generation.

## Build and Run Commands

```bash
# Build the dumper binary
cargo build --release

# Run dumper (basic structure)
cargo run --bin dumper -- <COMMAND> [OPTIONS] <LIBRARY_PATH>

# Common commands
cargo run --bin dumper -- dump --all ~/Pictures/MyLibrary.aplibrary
cargo run --bin dumper -- list --masters ~/Pictures/MyLibrary.aplibrary
cargo run --bin dumper -- tree ~/Pictures/MyLibrary.aplibrary
cargo run --bin dumper -- export --out-dir ./output ~/Pictures/MyLibrary.aplibrary
cargo run --bin dumper -- export --dryrun ~/Pictures/MyLibrary.aplibrary

# Tests (if present)
cargo test
```

## Architecture

### Core Data Model

The library follows a trait-based architecture for loading Aperture objects:

- **`AplibObject`** - Base trait for all library objects (Albums, Folders, Masters, Versions, Keywords, Volumes)
- **`PlistLoadable`** - Trait for objects loaded from `.plist` files
- **`SqliteLoadable`** - Trait for objects loaded from SQLite database (fallback for Volumes)
- **`store::Wrapper`** - Enum wrapper for storing heterogeneous objects in a single HashMap

Objects are stored in the `Library` struct, keyed by UUID strings.

### Aperture Library Structure

Aperture libraries (`.aplibrary` bundles) have two possible directory layouts:

1. **Root-level structure** (older libraries):
   - `Info.plist` - Bundle metadata
   - `Albums/` - Album definitions (`.apalbum` files)
   - `Folders/` - Folder/Project definitions (`.apfolder` files)
   - `Masters/` - Original image files
   - `Keywords.plist` - Keyword hierarchy

2. **Database subdirectory structure** (newer libraries):
   - `Database/Albums/`
   - `Database/Folders/`
   - `Database/Masters/`
   - `Database/Keywords.plist`
   - `Database/Versions/` - Edited versions (`.apversion` and `.apmaster` files)
   - `Database/apdb/Library.apdb` - SQLite database

The code in `library.rs` handles both layouts via `resolve_subdir()`.

### Key Relationships

- **Masters** are original images, stored in `Masters/` with relative paths
- **Versions** are edited variants, stored in `Database/Versions/` in UUID-based subdirectories (first 2 chars of UUID)
- **Projects** are Folders with `folder_type = Project`
- **Albums** belong to Folders via `parent` UUID
- **Volumes** represent external storage locations (can be in plist files or SQLite database)

### Export Functionality

The `export` command (`src/bin/dumper/exporter.rs`):
1. Loads all library objects (masters, versions, albums, folders, keywords)
2. Uses a local cache file (`/tmp/aplib_cache_*.bin`) to speed up repeated operations on network-mounted libraries
3. Generates `ExportJob` structs that group masters with their versions
4. Copies image files and generates XMP sidecars containing:
   - Standard EXIF/IPTC metadata
   - Aperture-specific metadata in custom `aplib:` namespace
   - Resolved keyword names (converted from UUIDs)
   - Original library path information

### Caching System

The dumper implements a JSON-based caching system in `main.rs`:
- Cache file location: `/tmp/aplib_cache_<hash>.bin` (hash based on library path)
- Caches: Version, Master, Album, and Folder objects
- **Important**: Cache assumes library is read-only (not being actively modified)
- First run may take 40+ minutes on large network-mounted libraries; subsequent runs use cache

### XMP Metadata

The `xmp.rs` module provides the `ToXmp` trait for converting Aperture metadata to XMP format. Custom namespace `aplib:` is used for Aperture-specific fields like:
- Face detection regions
- Stack membership
- Adjustment settings
- Original Aperture UUIDs

## Important Implementation Details

- Progress bars use `pbr` crate and stderr
- Versions directory scanning is slow on large libraries - uses progress bar
- Keywords are resolved from UUID references to human-readable names during export
- The code handles missing/corrupted files gracefully with warnings
- Audit mode (`audit` command) tracks parsing issues and skipped files
- The SQLite database is only accessed as a fallback when plist files are missing

## File Organization

- `src/lib.rs` - Core traits and type definitions
- `src/library.rs` - Main Library struct with loading logic
- `src/bin/dumper/main.rs` - CLI entry point and cache management
- `src/bin/dumper/exporter.rs` - Export functionality
- `src/bin/dumper/tree.rs` - Tree view command
- `src/{album,folder,master,version,volume,keyword}.rs` - Individual object types
- `src/xmp.rs` - XMP metadata generation
- `src/audit.rs` - Auditing/validation framework

## External Dependencies

- **exempi2** - XMP metadata manipulation (requires `libexempi-dev`)
- **rusqlite** - SQLite database access (requires `libsqlite3-dev`)
- **plist** - Apple property list parsing
- **clap** - CLI argument parsing
- **serde/serde_json** - Cache serialization
