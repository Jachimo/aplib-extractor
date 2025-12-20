aplib extractor
===============

Extract the data from Apple Aperture™ libraries in order to facilitate
importing it into another application.

Supported version are 3.x up to 3.6 (the final version).

This is written in Rust.

Requires:
- Rust and cargo (edition 2018)
- exempi (pulled by the exempi2 crate)

Building
--------

If you use this in your project, just add to your Cargo.toml:
```toml
aplib-extractor = { version = "0.1.0", default-features = false }
```
To build the dumper tool:

```shell
$ cargo build
```

Usage
-----

```
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
  Export images as hardlinks.
  - `--masters`     Export original master files (default is edited versions).
  - `--versions`    Export all edited versions as separate files.
  - `--albums`      Export images into directories named after albums.
  - `--out-dir DIR` Output directory (default: current directory).
  - `--dryrun`      Print shell commands for actions instead of performing them.

If neither `--albums` nor `--folders` is passed, and `--masters` is passed,
all master images are exported into a directory named `Masters` in the output directory.

If neither `--albums` nor `--folders` is passed, and `--versions` is passed,
all rendered, edited images are exported into a directory named `Versions` in the output directory.

Notes:
- Source library and export destination must be on the same filesystem for `export`.
- `<LIBRARY_PATH>` is the path to the Aperture library bundle.
- There is no guarantee that a rendered version exists for each Master image.
  Photos may lack versions because they were never rendered into a Preview, or if
  the library was cleaned.

Other
-----

If you are interested in extracting Lightroom catalogs, there is the
`lrcat` crate.

License
-------

  This Source Code Form is subject to the terms of the Mozilla Public
  License, v. 2.0. If a copy of the MPL was not distributed with this
  file, You can obtain one at http://mozilla.org/MPL/2.0/.

See the LICENSE file in this repository.

Maintainer:
Hubert Figuière <hub@figuiere.net>
