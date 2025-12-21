aplib extractor
===============

> This is a fork of the [original aplib-extractor][orig] project, with
> significant modifications and probably many new bugs added.  Please
> do not bother the upstream maintainer with questions about this version.

[orig]: https://github.com/hfiguiere/aplib-extractor

Purpose: Extract data from Apple Aperture libraries in usable formats,
to aid migration to other photo management applications.

Supported versions of Aperture are 3.x up to 3.6 (the final version).

Written in Rust.

Requires:
- Rust and cargo (2018+)
- exempi (try `sudo apt install libexempi-dev`)
- SQLite (try `sudo apt install libsqlite3-dev`)

Building
--------

To build the dumper tool:

```shell
$ cargo build --release
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
  Export images using hardlinks.
  - `--masters`     Export original master files (default is edited versions).
  - `--versions`    Export all edited versions as separate files.
  - `--albums`      Export images into directories named after albums.
  - `--out-dir DIR` Output directory (default: current directory).
  - `--dryrun`      Print shell commands for actions instead of performing them.



Notes:
- If neither `--albums` nor `--folders` is passed, and `--masters` is passed,
  all master images are exported into a directory named `Masters` in the output directory.
- If neither `--albums` nor `--folders` is passed, and `--versions` is passed,
  all rendered images are exported into a directory named `Versions` in the output directory.
- Source library and export destination must be on the same filesystem for `export`.
- `<LIBRARY_PATH>` is the path to the Aperture library bundle.
- There is no guarantee that a rendered version exists for each Master image.
  Photos may lack versions because they were never rendered into a Preview, or if
  the library was cleaned.


Major Changes
-------------

Significant changes from upstream include:
- Implement a new `export` command that uses hardlinks to create a directory structure containing
  masters or versions in the Aperture library, organized by folders, albums, etc.
- Use locally cached hashmaps to improve performance on repeated runs of the program,
  especially if the Aperture library is on a network filesystem where accesses are expensive.
  - The first run may still be slow (40 minutes for a 60k image library, using SMB over 1Gb Ethernet),
    but subsequent runs will use the local cache if available.
  - **Note that this feature assumes the Aperture Library is no longer being actively modified.**
- Add a `--dryrun` option that shows the operations that the program would have run, but without
  actually running them against the filesystem.
  - Note that the commands shown are the rough shell equivalents of the operations that the program
    will execute via the Rust `std::fs` API; it does not actually run the commands in a (sub)shell.


License
-------

  This Source Code Form is subject to the terms of the Mozilla Public
  License, v. 2.0. If a copy of the MPL was not distributed with this
  file, You can obtain one at http://mozilla.org/MPL/2.0/.

See the LICENSE file in this repository.
