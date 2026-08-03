# DigiKam Group-by-Aperture-UUID Utility

This utility groups DigiKam images that share the same XMP `aplib:MasterUUID`.

It is meant to be run *after* exporting your Aperture Library to the filesystem with the main utility.

**Implementation Note:** DigiKam grouping is written to `ImageRelations` using `type = 2` (Grouped), not to `Images.groupImage`.

## Environment

The utility can be configured using environment variables (recommended) or using command-line flags and options.

A `.env.example` file is provided in this directory; you can copy it to `.env`, fill in the correct values, and then `source` it into your shell.

Required variables:
- `DIGIKAM_DB_HOST`
- `DIGIKAM_DB_PORT`
- `DIGIKAM_DB_NAME`
- `DIGIKAM_DB_USER`
- `DIGIKAM_DB_PASSWORD`

Optional variables:
- `BACKUP_DIR` (default `./backups`)
- `LOG_FILE` (default `./digikam-grouping.log`)
- `DRY_RUN` (`true`/`false`)

## Usage

```bash
cd extras/digikam-group-utility
/home/jtuttle/src/aplib-extractor/.venv/bin/python src/cli.py /path/to/test-export --dry-run --backup
```

You can pass DB credentials directly instead of exporting environment variables:

```bash
/home/jtuttle/src/aplib-extractor/.venv/bin/python src/cli.py /path/to/test-export \
	--dry-run --verbose \
	--db-host minion.jwt.zt --db-port 3306 --db-name digikam --db-user digikam --db-password 'YOUR_PASSWORD'
```

## Safety defaults

- Supports `--dry-run` for planning without writes.
- Supports `--backup` and `--backup-only`.
- Uses per-group transaction with rollback on failure.
- Skips groups if any candidate image is already in a group relation.

## Progress visibility

With `--verbose`, the script emits periodic progress lines during:
- filesystem scan of export files,
- group planning by `MasterUUID`,
- plan execution (including dry-run mode).

This is intentional so long runs do not appear stuck.

## Current scope

- Parses sidecars for `aplib:MasterUUID`.
- Resolves image IDs via `Images` + `Albums` + `AlbumRoots` path matching.
- Falls back to sidecar-derived filename hints (`xmp:VersionFileName`, `aplib:MasterFilename`, `tiff:FileName`) when exported filenames differ from DigiKam-imported filenames.
- Falls back to same-name + file-size matching when path roots differ.
- Falls back to global unique file-size matching when filename-based matching fails entirely.
- Plans and applies `ImageRelations(subject=member, object=leader, type=2)`.

## Resolver diagnostics

Planning logs include unresolved-path reason counters so mismatches can be diagnosed quickly:

- `no_name_match`: filename not found in DigiKam `Images.name`.
- `dir_mismatch`: filename found, but album-root/relative-path mapping did not match the exported directory.
- `size_ambiguous`: fallback by filename+size found multiple candidates.
- `global_size_ambiguous`: global filesize fallback found multiple DB candidates.
- `*_alt_name`: result came from sidecar filename fallback rather than exported filename.
