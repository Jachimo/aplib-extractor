#!/usr/bin/env python3
"""Backfill digiKam ImageUniqueID in exported XMP sidecars.

Uses PyExifTool (a Python wrapper around ExifTool) for all XMP reading and
writing. ExifTool handles the XMP packet envelope, namespace prefixes, and XML
validation correctly, which avoids the fragility of manual XML manipulation.

A single persistent ExifTool process is reused for the whole run, and reads are
batched across many files, so the cost of process startup is paid once instead
of once per file.

Identity source precedence:
1) xmp:VersionUUID
2) aplib:OriginalVersionUUID
3) aplib:MasterUUID
4) generated UUIDv4

After running, trigger a metadata-to-database sync in digiKam to pick up the
new ImageUniqueID values.
"""

from __future__ import annotations

import argparse
import sys
import uuid
from dataclasses import dataclass
from pathlib import Path
from typing import Optional

import exiftool

# Number of files to read in a single ExifTool invocation.
READ_BATCH_SIZE = 500


@dataclass
class Stats:
    """Counters for the run summary printed at the end of execution."""

    scanned: int = 0
    updated: int = 0
    unchanged: int = 0
    errors: int = 0


def _read_xmp_fields(et: exiftool.ExifTool, paths: list[Path]) -> list[dict[str, str]]:
    """Read selected XMP fields from many sidecars in one ExifTool call.

    Returns one dict per file, keyed by the short field name (ImageUniqueID,
    VersionUUID, OriginalVersionUUID, MasterUUID). Missing fields are omitted.
    """

    try:
        data = et.execute_json(
            "-XMP-digiKam:ImageUniqueID",
            "-XMP-xmp:VersionUUID",
            "-XMP-aplib:OriginalVersionUUID",
            "-XMP-aplib:MasterUUID",
            *[str(p) for p in paths],
        )
    except Exception:
        return [{} for _ in paths]

    result: list[dict[str, str]] = []
    for entry in data:
        fields: dict[str, str] = {}
        for key, value in entry.items():
            if key == "SourceFile" or value is None:
                continue
            short = key.split(":", 1)[-1]
            fields[short] = str(value)
        result.append(fields)
    return result


def _select_unique_id(fields: dict[str, str]) -> tuple[str, str]:
    """Pick an ImageUniqueID value using migration-safe precedence.

    Returns (value, source_label).
    """

    if fields.get("VersionUUID"):
        return fields["VersionUUID"], "xmp:VersionUUID"

    if fields.get("OriginalVersionUUID"):
        return fields["OriginalVersionUUID"], "aplib:OriginalVersionUUID"

    if fields.get("MasterUUID"):
        return fields["MasterUUID"], "aplib:MasterUUID"

    return str(uuid.uuid4()), "generated:uuid4"


def _decide_action(
    fields: dict[str, str],
    rewrite_all: bool,
) -> tuple[Optional[str], Optional[str], Optional[str]]:
    """Decide what action to take for a sidecar.

    Returns (action, value, source):
    - action: "add-uuid", "rewrite-packet", or None (no change)
    - value: the UUID to write (only for add-uuid)
    - source: where the UUID value came from (only for add-uuid)
    """

    existing = fields.get("ImageUniqueID")

    if not existing:
        # No UUID present: add one.
        value, source = _select_unique_id(fields)
        return "add-uuid", value, source

    if rewrite_all:
        # UUID present but rewrite-all requested: ExifTool will rewrite the
        # packet (normalizing namespaces/envelope) even though the UUID value
        # itself is unchanged.
        return "rewrite-packet", None, None

    # UUID present and no rewrite requested: no change.
    return None, None, None


def _write_image_unique_id(et: exiftool.ExifTool, path: Path, value: str) -> bool:
    """Write digiKam:ImageUniqueID to a sidecar using the persistent ExifTool.

    Returns True on success.
    """

    try:
        et.execute(
            b"-overwrite_original",
            b"-q",
            b"-q",
            f"-XMP-digiKam:ImageUniqueID={value}".encode(),
            str(path).encode(),
        )
        return True
    except Exception:
        return False


def _process_batch(
    et: exiftool.ExifTool,
    paths: list[Path],
    dry_run: bool,
    verbose: bool,
    rewrite_all: bool,
    stats: Stats,
) -> None:
    """Read a batch of sidecars, decide what to write, and write it."""

    fields_list = _read_xmp_fields(et, paths)

    for path, fields in zip(paths, fields_list):
        stats.scanned += 1

        action, value, source = _decide_action(fields, rewrite_all)

        if action is None:
            stats.unchanged += 1
            continue

        if action == "add-uuid":
            detail = f"add ImageUniqueID from {source}"
        else:  # rewrite-packet
            detail = "rewrite XMP packet (normalize namespaces/envelope)"

        if dry_run:
            if verbose:
                print(f"would {detail}: {path}")
            stats.updated += 1
            continue

        if action == "add-uuid":
            # _decide_action guarantees value is set for add-uuid.
            assert value is not None
            if not _write_image_unique_id(et, path, value):
                stats.errors += 1
                print(f"warning: failed to process {path}", file=sys.stderr)
                continue
        else:  # rewrite-packet
            # Rewrite the packet by writing the existing UUID back through
            # ExifTool, which normalizes namespaces/envelope.
            if not _write_image_unique_id(et, path, fields["ImageUniqueID"]):
                stats.errors += 1
                print(f"warning: failed to process {path}", file=sys.stderr)
                continue

        if verbose:
            print(f"{detail}: {path}")
        stats.updated += 1


def iter_sidecars(root: Path):
    """Recursively yield .xmp files under root."""

    yield from root.rglob("*.xmp")


def main(argv: Optional[list[str]] = None) -> int:
    """CLI entrypoint.

    Exit codes:
    - 0: completed with no errors
    - 1: one or more sidecars could not be processed
    - 2: invalid arguments or missing exiftool
    """

    parser = argparse.ArgumentParser(
        description="Add digiKam:ImageUniqueID to exported XMP sidecars when missing."
    )
    parser.add_argument("root", type=Path, help="Export root directory to scan recursively")
    parser.add_argument("--dry-run", action="store_true", help="Report changes without writing files")
    parser.add_argument("--verbose", action="store_true", help="Print each changed file")
    parser.add_argument(
        "--rewrite-all",
        action="store_true",
        help="Rewrite all sidecars even if ImageUniqueID already exists",
    )
    args = parser.parse_args(argv)

    root = args.root.expanduser().resolve()
    if not root.exists() or not root.is_dir():
        print(f"error: not a directory: {root}", file=sys.stderr)
        return 2

    stats = Stats()

    # One persistent ExifTool process for the whole run.
    with exiftool.ExifTool() as et:
        batch: list[Path] = []
        for sidecar in iter_sidecars(root):
            batch.append(sidecar)
            if len(batch) >= READ_BATCH_SIZE:
                _process_batch(et, batch, args.dry_run, args.verbose, args.rewrite_all, stats)
                batch = []
        if batch:
            _process_batch(et, batch, args.dry_run, args.verbose, args.rewrite_all, stats)

    mode = "dry-run" if args.dry_run else "write"
    print(
        f"fix-image-uuid ({mode}): scanned={stats.scanned} "
        f"updated={stats.updated} unchanged={stats.unchanged} errors={stats.errors}"
    )

    return 0 if stats.errors == 0 else 1


if __name__ == "__main__":
    raise SystemExit(main())
