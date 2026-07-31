# Aperture Library Exporter

This is a migration-focused fork of [hfiguiere/aplib-extractor][orig].
It is designed for one job: exporting an Apple Aperture 3.x library into a
folder tree that open-source photo managers (primarily [DigiKam][dk]) can import.

The software copies master and version images out of an
`.aplibrary` bundle and writes XMP sidecars that preserve Aperture provenance
while also mirroring key fields into DigiKam-friendly namespaces.

> **Note:** This fork has diverged significantly from upstream. Questions about behavior
> in this repository should be directed here, not to the upstream maintainer.

[orig]: https://github.com/hfiguiere/aplib-extractor
[dk]: https://www.digikam.org/

## What It Does

- Reads Apple Aperture 3.x libraries (up to 3.6, the final released version).
- Exports original masters and rendered versions into a normal filesystem
  hierarchy.
- Writes XMP sidecars for both masters and versions.
- Preserves Aperture-specific provenance in a custom `aplib:` namespace.
- Mirrors commonly-used fields into DigiKam-consumed XMP fields,
  including titles, dates, ratings, pick/color labels, and keywords/tags.
- Resolves Aperture keyword UUIDs into readable tag names.
- Materializes metadata-only versions on export, so each version is a distinct
  image in digiKam (which doesn't support metadata-only versions).
- Uses a local cache to make repeated exports from large or network-mounted
  libraries *much* faster.

## What This Fork No Longer Does

Some features have been removed from the upstream codebase for ease of 
debugging and maintenance: 

- The old subcommand-based CLI is gone.
- `dump`, `list`, `audit`, and `tree` are not supported top-level commands.
- The supported entrypoint is `export [OPTIONS] <LIBRARY_PATH>`.

## Requirements

- Rust and Cargo
- `libexempi-dev`
- `libsqlite3-dev`

On Debian or Ubuntu:

```shell
sudo apt install cargo libexempi-dev libsqlite3-dev
```

## Build

```shell
cargo build --release
```

The resulting binary is:

```shell
target/release/export
```

## Usage

```shell
export [OPTIONS] <LIBRARY_PATH>
```

Examples:

```shell
cargo run --release --bin export -- \
  --out-dir ./exported-library \
  ~/Pictures/Aperture\ Library.aplibrary
```

```shell
cargo run --release --bin export -- \
  --dryrun \
  --out-dir ./export-preview \
  ~/Pictures/Aperture\ Library.aplibrary
```

## Aperture to DigiKam Quickstart

Migrate an Aperture library to a directory tree that can be directly imported
by DigiKam, with metadata:

This assumes you have an Aperture Library at `~/Pictures/MyLibrary.aplibrary`
and you want to create the DigiKam-compatible export at `~/Migrated/MyLibrary-export`.

### 1. Build the exporter

```shell
cargo build --release
```

### 2. Dry-run first (recommended)

```shell
cargo run --release --bin export -- \
  --dryrun \
  --out-dir ~/Migrated/MyLibrary-export \
  ~/Pictures/MyLibrary.aplibrary
```

### 3. Run the real export

```shell
cargo run --release --bin export -- \
  --out-dir ~/Migrated/MyLibrary-export \
  ~/Pictures/MyLibrary.aplibrary
```

If the source library lives on a NAS or slow external storage, try:

```shell
cargo run --release --bin export -- \
  --nas-safe \
  --out-dir ~/Migrated/TestLibrary-export \
  ~/Pictures/TestLibrary.aplibrary
```

### 4. Inspect the exported files

You should see a structure like:

```text
~/Migrated/TestLibrary-export/
  export-checkpoint.log
  2006/
    11/
      02/
        20061102-161812/
          PICT0019.JPG
          PICT0019.xmp
          PICT0019__PICT0019.JPG
          PICT0019__PICT0019.xmp
```

Optionally inspect a sidecar with `exiftool`:

```shell
exiftool -a -G1 -s \
  ~/Migrated/TestLibrary-export/2006/11/02/20061102-161812/PICT0019__PICT0019.xmp
```

### 5. Add the export folder as a DigiKam collection

In DigiKam, add `~/Migrated/MyLibrary-export` as a collection root and let the
initial scan finish.  (Settings menu, Configure DigiKam, Collections, and "Add Collection"
for the appropriate Local / Removable Media / Network Share type.)

### 6. Run metadata sync from image files into DigiKam database

This is the critical final step if you are not seeing tags in DigiKam!

In DigiKam:

1. Open `Tools > Maintenance`.
2. Select the Collection that you just added, containing the exported photos.
2. Scroll down and check `Sync Metadata and Database`.
3. Select direction `Image to Database` (read from files).
4. Run it (it is I/O intensive and may take a while).

This should fix an issue where DigiKam sometimes reads the XMP sidecar files but,
for some reason, doesn't actually interpret them as tags and put them into the database
correctly.

Note that if you run the `aplib-extractor` exporter tool more than once, you will
need to redo this process to force DigiKam to read the XMP sidecars over again and
rebuild the database -- otherwise, the values in the database seem to be preferred.

### 7. Verify results

Check one imported image in DigiKam's tags panel:

- Values in `digiKam:TagsList` should appear as hierarchical/nested tags.
- Values in `dc:subject` should appear as keyword tags.

## Options

- `--out-dir DIR`: output directory. Defaults to the current directory.
- `--dryrun`: plan the export and print file operations without writing output.
- `--nas-safe`: apply conservative I/O defaults for fragile NAS or external
  storage.
- `--max-read-mib-per-sec N`: throttle source reads.
- `--max-write-mib-per-sec N`: throttle destination writes.
- `--io-delay-ms N`: sleep after each file or sidecar write.
- `--io-chunk-kib N`: adjust copy chunk size.

`--nas-safe` currently implies conservative defaults when explicit overrides are
not provided:

- read throttle: `4 MiB/s`
- write throttle: `4 MiB/s`
- write delay: `20 ms`
- copy chunk size: `64 KiB`

## Output Notes

The exporter writes a filesystem tree rooted at `--out-dir` that preserves the
date-based structure used inside Aperture masters.

For each export job it writes:

- the master image file
- a master XMP sidecar with the same basename
- zero or more version image files
- a version XMP sidecar for each version image

Important details:

- Some Aperture versions have metadata but no separate rendered image file.
  In those cases the exporter materializes a version image filename from the
  master so that the sidecar still has a matching sibling image for DigiKam.
- Sidecars include both standard XMP fields and custom `aplib:` provenance.
- A run also creates `export-checkpoint.log` in the output directory.

### Metadata Written For DigiKam

The exporter writes Aperture metadata into XMP with two goals:

1. Preserve original Aperture information.
2. Emit fields that DigiKam and similar tools are likely to consume.

Current sidecars may include:

- `dc:title` as standard XMP alt-text
- `photoshop:Headline`
- `xmp:CreateDate`
- `exif:DateTimeOriginal`
- `xmp:Rating`
- `digiKam:PickLabel`
- `digiKam:ColorLabel`
- `dc:subject` keyword bags when flat keyword values are available
- `digiKam:TagsList` hierarchical tags when resolved keyword paths are available
- `aplib:*` provenance fields such as Aperture UUIDs and original library paths

This dual-writing is intentional: `aplib:*` remains the canonical migration
record, while standard and DigiKam-oriented fields improve import behavior.

## Caching

Repeated exports reuse a local cache under `/tmp/aplib_cache_*.bin`.

- First runs can be slow on large libraries, especially over SMB/NFS.
- Later runs can reuse cached master/version metadata.
- The cache assumes the source Aperture library is effectively read-only.

If export planning looks wrong after changing the source subset or library
contents, clear old caches and rerun:

```shell
rm -f /tmp/aplib_cache_*.bin
```

## NAS and Large-Library Operation

For heavy exports against network storage, you can combine built-in throttling
with `ionice` and `nice`:

```shell
ionice -c3 \
nice -n 19 \
cargo run --release --bin export -- \
  --nas-safe \
  --max-read-mib-per-sec 26 \
  --max-write-mib-per-sec 26 \
  --io-delay-ms 10 \
  --io-chunk-kib 128 \
  --out-dir /mnt/photos/tmp/aplib-export \
  "/mnt/photos/IMPORT/Aperture Library.aplibrary"
```

The exporter logs per-job and aggregate throughput so you can tune these values
for your storage.

Optional troubleshooting utilities live in [extras](extras). NAS-specific notes
are in [extras/README.md](extras/README.md).

## Project Notes

- The Aperture file format notes in [docs/format.md](docs/format.md) describe
  the bundle layout this parser understands.
- The core export implementation lives in [src/bin/dumper/exporter.rs](src/bin/dumper/exporter.rs).
- The binary target is still sourced from `src/bin/dumper/main.rs`; the binary
  name is `export`.

## License

This Source Code Form is subject to the terms of the Mozilla Public License,
v. 2.0. If a copy of the MPL was not distributed with this file, You can obtain
one at http://mozilla.org/MPL/2.0/.

See [LICENSE](LICENSE).
