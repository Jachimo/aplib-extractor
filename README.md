# Aperture Library Extractor

> This is a fork of [hfiguiere/aplib-extractor][orig], with various modifications (and probably *many* new bugs) added.  
> Please do not bother the upstream maintainer with questions about this version.

[orig]: https://github.com/hfiguiere/aplib-extractor

Purpose: Extract data from Apple Aperture libraries in usable formats,
to aid migration to other photo management applications.

Supported versions of Aperture are 3.x up to 3.6 (the final version).

Written in Rust.

Requires:
- Rust and Cargo ([install instructions](https://rust-lang.org/tools/install/))
- exempi (try `sudo apt install libexempi-dev`)
- SQLite (try `sudo apt install libsqlite3-dev`)

## Building

```shell
cargo build --release
```

## Basic Usage

```shell
export [OPTIONS] <LIBRARY_PATH>
```

Options:

- `--out-dir DIR` Output directory (default: current directory).
- `--dryrun` Print shell commands for file operations instead of performing them.
- `--nas-safe` Apply conservative read/write settings for slow/fragile network storage.
- `--max-read-mib-per-sec N` Limit source read throughput during export (MiB/s).
- `--max-write-mib-per-sec N` Limit write throughput during export (MiB/s).
- `--io-delay-ms N` Sleep N milliseconds after each file/sidecar write operation.
- `--io-chunk-kib N` Chunk size for copy/write loops (smaller chunks reduce burstiness).

Notes:
- `<LIBRARY_PATH>` is the path to the Aperture library bundle.
- Export logs now include per-job and aggregate effective throughput (MiB/s) to make NAS tuning easier.
- This fork is now focused on one task: exporting an Aperture library into a DigiKam-importable folder hierarchy containing images and XMP sidecars.
- The old `export` subcommand has been removed; if you type it now, the CLI will reject it and point you back to `export [OPTIONS] <LIBRARY_PATH>`.
- There is no guarantee that a rendered version exists for each Master image.
  Photos may lack versions because they were never rendered into a Preview, or if
  the library was cleaned.

## Examples

Because the tool can create large amounts of I/O (especially when used against a remote file server or NAS head), it can be combined with `ionice` and built-in rate limits to manage resource usage:

```
ionice -c3 \
nice -n 19 \
cargo run --release --bin export -- \
--nas-safe \
--max-read-mib-per-sec 26 \
--max-write-mib-per-sec 26 \
--io-delay-ms 10 \
--io-chunk-kib 128 \
--out-dir "/mnt/photos/tmp/EXPORT" \
"/mnt/photos/IMPORT/Aperture Library.aplibrary"
```

## Extras

Optional helper utilities are in [extras](extras).

For NAS troubleshooting details and usage, see [extras/README.md](extras/README.md).

## Testing

The most important regression coverage in this fork is now the exporter-focused test slice:

```shell
cargo test --bin export exporter::tests:: -- --nocapture
```

This includes a migration-oriented golden test that exports the synthetic fixture library in [testdata](testdata) and verifies:

- the emitted master and version filenames
- sidecar creation for both master and exported version outputs
- DigiKam-critical XMP fields such as title, dates, rating, and pick/color labels
- master/version linkage fields in the custom `aplib:` namespace

The baseline golden test currently lives in [src/bin/dumper/exporter.rs](/home/jtuttle/src/aplib-extractor/src/bin/dumper/exporter.rs) as `test_export_fixture_library_writes_expected_files_and_digikam_xmp_fields`.

If you change export filenames, XMP mapping, or fixture metadata, update that test and the corresponding fixture notes in [testdata/README.md](/home/jtuttle/src/aplib-extractor/testdata/README.md).

## Major Changes

Significant changes from upstream include:

- Implemented a new `export` command that copies master/version images and their accompanying metadata from the Aperture Library, to facilitate migration to other management systems (e.g. [DigiKam][]).
- Use locally cached hashmaps to improve performance on repeated runs of the program, especially if the Aperture library is on a network filesystem where accesses are expensive.
  - The first run may still be slow (40 minutes for a 60k image library, using SMB over 1Gb Ethernet), but subsequent runs will use the local cache if available.
  - **Note that this feature assumes the Aperture Library is no longer being actively modified.** If you are still actively using your Aperture Library (implying you have access to Aperture and a Mac to run it on), there are probably many easier ways of exporting your data from it...
- Added a `--dryrun` option, which can be used to "pre-warm" the cache hashmaps.

[DigiKam]: https://www.digikam.org/


## License

  This Source Code Form is subject to the terms of the Mozilla Public
  License, v. 2.0. If a copy of the MPL was not distributed with this
  file, You can obtain one at http://mozilla.org/MPL/2.0/.

See the LICENSE file in this repository.
