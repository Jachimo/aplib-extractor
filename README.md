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
dumper <COMMAND> [OPTIONS] <LIBRARY_PATH>
```

Commands and Options:

- `dump`  
  Print detailed information about library contents.
  - `--albums`      Dump albums.
  - `--folders`     Dump folders.
  - `--masters`     Dump master images.
  - `--versions`    Dump edited versions.
  - `--keywords`    Dump keywords.
  - `--volumes`     Dump volumes.
  - `--all`         Dump all supported types.

- `list`  
  Print a simple list of items.
  - `--albums`      List albums.
  - `--folders`     List folders.
  - `--masters`     List master images.
  - `--versions`    List edited versions.
  - `--keywords`    List keywords.
  - `--volumes`     List volumes.

- `audit`  
  Audit the library for inconsistencies.
  - `--albums`      Audit albums.
  - `--folders`     Audit folders.
  - `--masters`     Audit master images.
  - `--versions`    Audit edited versions.
  - `--keywords`    Audit keywords.
  - `--volumes`     Audit volumes.
  - `--all`         Audit all supported types.

- `tree`  
  Print a tree view of the folder/album hierarchy.

- `export`  
  Export images to the working directory.
  - `--out-dir DIR` Output directory (default: current directory).
  - `--dryrun`      Print shell commands for file operations instead of performing them.
  - `--nas-safe`    Apply conservative read/write settings for slow/fragile network storage.
  - `--max-read-mib-per-sec N` Limit source read throughput during export (MiB/s).
  - `--max-write-mib-per-sec N` Limit write throughput during export (MiB/s).
  - `--io-delay-ms N` Sleep N milliseconds after each file/sidecar write operation.
  - `--io-chunk-kib N` Chunk size for copy/write loops (smaller chunks reduce burstiness).

Notes:
- `<LIBRARY_PATH>` is the path to the Aperture library bundle.
- Export logs now include per-job and aggregate effective throughput (MiB/s) to make NAS tuning easier.
- Aperture album export is not currently supported; export preserves file/folder layout and writes image/XMP metadata.
- There is no guarantee that a rendered version exists for each Master image.
  Photos may lack versions because they were never rendered into a Preview, or if
  the library was cleaned.

## Examples

Because the tool can create large amounts of I/O (especially when used against a remote file server or NAS head), it can be combined with `ionice` and built-in rate limits to manage resource usage:

```
ionice -c3 \
nice -n 19 \
cargo run --release --bin dumper -- \
export \
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
