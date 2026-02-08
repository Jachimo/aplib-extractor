# AGENTS.md

This file provides technical documentation about the aplib-extractor codebase for AI agents.

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

General notes on the format of Aperture libraries (`.aplibrary` bundles) is provided in the `docs/format.md` file.
This file should be read to understand what is known about the format.

In brief: Aperture Library bundles have two possible directory layouts:

1. **Root-level structure** (possibly older libraries):
   - `Info.plist` - Bundle metadata
   - `Albums/` - Album definitions (`.apalbum` files)
   - `Folders/` - Folder/Project definitions (`.apfolder` files)
   - `Masters/` - Original image files
   - `Keywords.plist` - Keyword hierarchy

2. **Database subdirectory structure** (possibly newer libraries):
   - `Database/Albums/`
   - `Database/Folders/`
   - `Database/Keywords.plist`
   - `Database/Versions/` - **METADATA ONLY** (`.apversion` and `.apmaster` plist files)
   - `Database/apdb/Library.apdb` - SQLite database
   - `Masters/` - **Actual image files** for both masters AND versions

**CRITICAL**: Version image files are stored in `Masters/YYYY/MM/DD/YYYYMMDD-HHMMSS/`, 
NOT in the `Database/Versions/` tree. The Versions tree contains only metadata plist files.

The code in `library.rs` handles both layouts via `resolve_subdir()`.

### Actual Library Example Structure

An example of an actual `Aperture Library.aplibrary` bundle is provided (in the form
of output from the Linux `tree` command) in the file `docs/structure.txt`. 

NOTE: This file is quite large (40+ MB), use caution when reading/parsing it.

If this is available, it should be used to resolve ambiguities or bugs in the code.

### Key Relationships

- **Masters** are original images, stored in `Masters/YYYY/MM/DD/YYYYMMDD-HHMMSS/` with date-based paths
- **Versions** are edited variants:
  - **Metadata** (plist files): stored in `Database/Versions/YYYY/MM/DD/YYYYMMDD-HHMMSS/UUID/`
  - **Image files**: stored in `Masters/YYYY/MM/DD/YYYYMMDD-HHMMSS/` (same location as masters!)
- **Projects** are Folders with `folder_type = Project`
- **Albums** belong to Folders via `parent` UUID
- **Volumes** represent external storage locations (can be in plist files or SQLite database)

**Key Insight**: Version images and master images share the same directory structure in `Masters/`. 
Only the metadata plists are separated into `Database/Versions/` with UUID subdirectories.

### Export Functionality

The `export` command (`src/bin/dumper/exporter.rs`):
1. Loads all library objects (masters, versions, albums, folders, keywords)
2. Uses a local cache file (`/tmp/aplib_cache_*.bin`) to speed up repeated operations on network-mounted libraries
3. Generates `ExportJob` structs that group masters with their versions
4. **Transforms Version paths**: Converts `Database/Versions/.../UUID/` paths to `Masters/.../` paths to locate actual image files
5. Copies image files and generates XMP sidecars containing:
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
- **Keywords** are resolved from UUID references to human-readable names during export:
  - UUIDs are looked up in the keyword map
  - Non-UUID strings are treated as direct names (iPhoto imports)
  - Multi-space delimiters (2+ consecutive spaces) split hierarchical keywords
  - Example: "Wedding  Stock Category" becomes two keywords: "Wedding" and "Stock Category"
  - All keywords are sanitized to remove null bytes and illegal XML characters before XMP export
- **Version file paths**: The `Version.source_directory` field captures the plist location in `Database/Versions/`, 
  but must be transformed to `Masters/` tree to find actual image files (see `transform_versions_to_masters_path()` in exporter.rs)
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
- `src/xmp.rs` - XMP metadata generation and sanitization utilities
- `src/audit.rs` - Auditing/validation framework

## External Dependencies

- **exempi2** - XMP metadata manipulation (requires `libexempi-dev`)
- **rusqlite** - SQLite database access (requires `libsqlite3-dev`)
- **plist** - Apple property list parsing
- **clap** - CLI argument parsing
- **serde/serde_json** - Cache serialization
