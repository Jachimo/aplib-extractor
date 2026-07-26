# Aperture Library Exporter

This repository is a migration-focused fork of [hfiguiere/aplib-extractor][orig].
It is aimed at one job: exporting an Apple Aperture 3.x library into a
folder tree that photo managers such as DigiKam can import.

The current binary is `export`. It copies master and version images out of an
`.aplibrary` bundle and writes XMP sidecars that preserve Aperture provenance
while also mirroring key fields into DigiKam-friendly namespaces.

> This fork has diverged significantly from upstream. Questions about behavior
> in this repository should be directed here, not to the upstream maintainer.

[orig]: https://github.com/hfiguiere/aplib-extractor

## What It Does

- Reads Apple Aperture 3.x libraries up to 3.6.
- Exports original masters and rendered versions into a normal filesystem
  hierarchy.
- Writes XMP sidecars for both masters and versions.
- Preserves Aperture-specific provenance in a custom `aplib:` namespace.
- Mirrors commonly useful fields into standard and DigiKam-consumed XMP fields,
  including titles, dates, ratings, pick/color labels, and keyword tags.
- Resolves Aperture keyword UUIDs into readable tag names.
- Materializes metadata-only versions so each exported version sidecar has a
  matching same-basename image file for importers like DigiKam.
- Uses a local cache to make repeated exports from large or network-mounted
  libraries much faster.

## What This Fork No Longer Does

This repository is no longer a general-purpose Aperture inspection tool.

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

### Options

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

## Output Contract

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

## Metadata Written For DigiKam

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

## End-to-End Example: Migrate a Small Aperture Library to DigiKam

Assume you have:

- Aperture library: `~/Pictures/TestLibrary.aplibrary`
- Destination export directory: `~/Migrated/TestLibrary-export`

### 1. Dry-run the export

```shell
cargo run --release --bin export -- \
  --dryrun \
  --out-dir ~/Migrated/TestLibrary-export \
  ~/Pictures/TestLibrary.aplibrary
```

Use this to confirm that the library opens, jobs are discovered, and the output
path is what you expect.

### 2. Run the real export

```shell
cargo run --release --bin export -- \
  --out-dir ~/Migrated/TestLibrary-export \
  ~/Pictures/TestLibrary.aplibrary
```

If the source library lives on a NAS or flaky external storage, prefer:

```shell
cargo run --release --bin export -- \
  --nas-safe \
  --out-dir ~/Migrated/TestLibrary-export \
  ~/Pictures/TestLibrary.aplibrary
```

### 3. Inspect the exported files

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

### 4. Import into DigiKam

In DigiKam:

1. Add `~/Migrated/TestLibrary-export` as a collection root.
2. Let DigiKam scan the files.
3. If needed, run a metadata read from files so DigiKam refreshes from the XMP
   sidecars.

Expected results:

- original and version images appear as normal files outside Aperture
- titles and dates come from XMP sidecars
- ratings and pick/color labels are available where present
- tags import from `dc:subject` and/or `digiKam:TagsList`

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

## Testing

The most important regression coverage in this fork is the exporter-focused test
slice:

```shell
cargo test --bin export exporter::tests:: -- --nocapture
```

That test slice uses the synthetic fixture in [testdata](testdata) and checks:

- end-to-end export of a master plus version
- emitted filenames
- creation of matching sidecars
- DigiKam-relevant XMP fields
- preserved `aplib:` provenance fields

If you change export naming, XMP mapping, or fixture metadata, update the test
expectations and [testdata/README.md](testdata/README.md) together.

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
