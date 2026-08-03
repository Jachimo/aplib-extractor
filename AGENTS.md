# AGENTS.md

This file documents the current behavior of the aplib-extractor repository for coding agents.
## Overview

This repository is now focused on Aperture-to-filesystem export for DigiKam migration.
Current moving pieces:

- Rust library code for parsing Aperture `.aplibrary` bundles.
- One supported CLI binary, `export`, implemented in `src/bin/dumper/`.
- Helper utilities under `extras/`, notably:
  - `extras/digikam-group-utility/` for grouping DigiKam images by shared `aplib:MasterUUID`.
  - `extras/fix-image-uuid/` for backfilling `digiKam:ImageUniqueID` into existing XMP sidecars.
The legacy multi-command CLI is gone. The supported invocation is:

```bash
cargo run --bin export -- [OPTIONS] <LIBRARY_PATH>
```
## Build And Test Commands

```bash
# Build the export binary
cargo build --release

# Run export
cargo run --bin export -- --out-dir ./output ~/Pictures/MyLibrary.aplibrary
cargo run --bin export -- --dryrun ~/Pictures/MyLibrary.aplibrary

# Export with throttled I/O for fragile NAS storage
cargo run --bin export -- --nas-safe --out-dir ./output ~/Pictures/MyLibrary.aplibrary

# Full test suite
cargo test

# Exporter-focused regression coverage
cargo test --bin export exporter::tests:: -- --nocapture
```
## Repository Scope

The live migration path is centered on a reduced set of Aperture object types:

- `Master`: original image records and source image paths.
- `Version`: edited/export variants and the plist locations that describe them.
- `Keyword`: keyword hierarchy used to resolve Aperture keyword UUIDs into DigiKam-facing tags.
- `Volume`: still exists in the shared library for path-resolution support, but is not part of the main export workflow.

The export cache currently stores only masters and versions.
## Aperture Library Layout

General format notes live in `docs/format.md`. The implementation supports two practical layouts:

1. Root-level layout:
  - `Info.plist`
  - `Masters/`
  - `Keywords.plist`

2. Database subdirectory layout:
  - `Database/Keywords.plist`
  - `Database/Versions/` containing metadata plists only (`.apversion`, `.apmaster`)
  - `Database/apdb/Library.apdb`
  - `Masters/` containing the real image files for both masters and versions

Critical distinction:

- Version metadata lives under `Database/Versions/.../UUID/`.
- Version image files do not live there. They live in `Masters/YYYY/MM/DD/YYYYMMDD-HHMMSS/`.

`Version.source_directory` captures the plist location in the Versions tree. The exporter transforms that path into the corresponding Masters tree path via `transform_versions_to_masters_path()` and then searches for the actual image file.
## Export Pipeline

The `export` binary is implemented by `src/bin/dumper/main.rs` and `src/bin/dumper/exporter.rs`.

High-level flow:

1. Parse CLI args.
2. Resolve keyword maps from the Aperture keyword hierarchy.
3. Load or rebuild a local cache of masters and versions.
4. Build one `ExportJob` per master.
5. Copy the master image.
6. Find, copy, or materialize each non-original version image.
7. Write XMP sidecars for the master and each version.
8. Append checkpoint log entries while exporting.

### Export Jobs

Each `ExportJob` groups:

- one master UUID,
- its source image path,
- its output-relative directory,
- zero or more non-original versions,
- stable exported filenames for those versions.

Version filenames are derived from:

- the master stem,
- the Aperture version name when available,
- a short UUID token as a deterministic dedupe fallback.

The exporter deliberately avoids basename collisions by appending UUID-derived suffixes when necessary.

### Metadata-Only Versions

Some Aperture versions are metadata-only and point at the same image file as the master.

For these cases, the exporter still materializes a separate version image filename in the output directory so that importers such as DigiKam see a matching image basename for the version sidecar. It attempts:

1. hard-linking the exported master file to the version filename,
2. copying the file if hard-linking fails.

### I/O Throttling And NAS Safety

The exporter supports throttled I/O for unstable network storage.

Relevant CLI flags:

- `--nas-safe`
- `--max-write-mib-per-sec`
- `--max-read-mib-per-sec`
- `--io-delay-ms`
- `--io-chunk-kib`

`--nas-safe` currently defaults to approximately:

- 4 MiB/s read limit,
- 4 MiB/s write limit,
- 20 ms delay between operations,
- 64 KiB chunk size.

### Checkpoint Logging

Non-dry-run exports write `export-checkpoint.log` in the output directory.

The log records:

- a `run` header,
- one line per exported master,
- one line per exported version sidecar,
- a final `done` summary with aggregate I/O stats.

This log is meant for resumability diagnostics and post-run auditing.

## Cache Behavior

The export binary maintains a cache file at:

```text
/tmp/aplib_cache_<hash>.bin
```

Important details:

- Despite the `.bin` suffix and the `bincode` dependency in `Cargo.toml`, the current cache implementation uses `serde_json` read/write in `src/bin/dumper/main.rs`.
- The hash includes the library path and the library bundle modification time when available.
- Cache contents currently include:
  - `HashMap<String, Version>`
  - `HashMap<String, Master>`
- The cache assumes the source library is effectively read-only during export.

If you hit path-resolution failures caused by stale cached objects, clearing `/tmp/aplib_cache_*.bin` is the intended recovery path.

## XMP Output Behavior

XMP helper code lives in `src/xmp.rs`. Aperture object serializers live primarily in `src/master.rs` and `src/version.rs`.

### Namespaces Written During Export

The exporter registers and uses at least these namespaces:

- `aplib:` for Aperture-specific provenance fields
- `digiKam:`
- `dc:`
- `xmp:`
- `photoshop:`
- `exif:`
- `tiff:`
- `MicrosoftPhoto:`
- `lr:`
- `mediapro:`

### Master Sidecars

Master sidecars include, among other fields:

- `aplib:MasterUUID`
- `aplib:OriginalVersionUUID` when available
- title/headline/date metadata
- resolved keyword metadata
- `ApertureLibraryPath` provenance
- `digiKam:ImageUniqueID`

Current `digiKam:ImageUniqueID` behavior for master sidecars:

1. use `original_version_uuid` when present,
2. otherwise fall back to the master UUID.

### Version Sidecars

Version sidecars include, among other fields:

- `xmp:VersionUUID`
- `xmp:VersionFileName`
- `tiff:FileName`
- `xmp:Rating`
- `xmp:CreateDate`
- `exif:DateTimeOriginal`
- `digiKam:PickLabel`
- `digiKam:ColorLabel`
- `aplib:MasterUUID`
- `aplib:MasterFilename`
- `ApertureLibraryPath`
- `digiKam:ImageUniqueID`

Current `digiKam:ImageUniqueID` behavior for version sidecars:

- always use the Aperture version UUID.

This was added specifically to make DigiKam identity resolution more robust and to support post-import metadata refresh behavior.

### Keyword Serialization

Keyword handling is broader than a single field.

Current behavior:

- Aperture keyword UUIDs are resolved to human-readable names using the keyword maps.
- Non-UUID strings are preserved as direct names, which matters for iPhoto-imported libraries.
- Hierarchical keywords are split on 2+ consecutive spaces.
- Keyword strings are sanitized before XML serialization.

Interop output is written across several namespaces:

- flat keywords to `dc:subject`
- hierarchical keywords to `digiKam:TagsList`
- companion hierarchical forms to `MicrosoftPhoto:LastKeywordXMP`, `lr:hierarchicalSubject`, and `mediapro:CatalogSets`

## DigiKam Helper Utilities

### `extras/digikam-group-utility`

This Python utility groups DigiKam images that share the same `aplib:MasterUUID` in exported sidecars.

Current behavior:

- scans exported image trees recursively,
- parses sidecars for `aplib:MasterUUID`,
- resolves DigiKam `Images.id` rows by layered fallback,
- writes grouping edges to `ImageRelations` with `type = 2`.

Current resolver order is important:

1. direct DigiKam UUID match through `ImageHistory.uuid` when `digiKam:ImageUniqueID` is present,
2. path-based match through `Images`, `Albums`, and `AlbumRoots`,
3. sidecar-derived alternate filename hints,
4. same-name plus file-size match,
5. global unique file-size match.

The utility logs diagnostic reasons such as:

- `no_name_match`
- `dir_mismatch`
- `size_ambiguous`
- `global_size_ambiguous`
- `history_uuid_ambiguous`

It also supports:

- `--dry-run`
- `--backup`
- `--backup-only`
- direct DB credential CLI overrides in addition to environment variables

### `extras/fix-image-uuid`

This Python helper exists for already-exported trees that predate the current exporter behavior.

It recursively scans `.xmp` sidecars and writes missing `digiKam:ImageUniqueID` values using this precedence:

1. `xmp:VersionUUID`
2. `aplib:OriginalVersionUUID`
3. `aplib:MasterUUID`
4. generated UUIDv4

Normally, fresh exports should not need this helper because the exporter now writes `digiKam:ImageUniqueID` automatically.

## Validation Guardrails

The main regression test for export behavior is:

- `test_export_fixture_library_writes_expected_files_and_digikam_xmp_fields`

It asserts:

- one real master-plus-version export path end-to-end,
- expected output filenames,
- creation of both master and version XMP sidecars,
- important DigiKam-facing fields including `digiKam:PickLabel`, `digiKam:ColorLabel`, and `digiKam:ImageUniqueID`,
- preservation of `aplib:` provenance fields.

Other notable exporter tests cover:

- metadata-only version materialization,
- version filename sorting and dedupe behavior,
- fallback behavior when cached versions lack `source_directory`,
- checkpoint log append behavior,
- I/O throttle defaults and overrides.

When changing export behavior, keep `cargo test --bin export exporter::tests:: -- --nocapture` green first.

## Important Implementation Notes

- Progress bars use `pbr` and write to stderr during cache materialization.
- The exporter handles missing source files with warnings instead of crashing the full run.
- SQLite access in the Rust library remains available as a fallback when plist files are missing.
- Some repository files under `extras/` are intentionally local-only and may be ignored because they can contain workstation- or environment-specific information.

## File Map

- `src/lib.rs`: core traits and type definitions
- `src/library.rs`: main library loading logic and layout handling
- `src/master.rs`: master parsing and XMP serialization
- `src/version.rs`: version parsing and XMP serialization
- `src/keyword.rs`: keyword loading and UUID-to-name resolution
- `src/xmp.rs`: XMP helpers, namespace registration, and keyword interop writers
- `src/bin/dumper/main.rs`: CLI entry point and cache management
- `src/bin/dumper/exporter.rs`: export job construction, file copying, sidecar writing, checkpoint logging, and tests
- `extras/digikam-group-utility/`: DigiKam DB grouping helper
- `extras/fix-image-uuid/`: XMP `ImageUniqueID` backfill helper
- `testdata/`: synthetic Aperture fixture library and related notes

## External Dependencies

- `exempi2`: XMP metadata manipulation, requires `libexempi-dev`
- `rusqlite`: SQLite access, requires `libsqlite3-dev`
- `plist`: Apple property list parsing
- `clap`: CLI argument parsing for the export binary
- `pbr`: progress bar output for cache materialization
- `serde` / `serde_json`: cache serialization
- `mysql-connector-python`: required by `extras/digikam-group-utility`
