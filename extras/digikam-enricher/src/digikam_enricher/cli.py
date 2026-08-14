"""Command-line interface for digikam-enricher."""

from __future__ import annotations

import argparse
import logging
import sys
from pathlib import Path

from .aperture import ApertureLibrary, ApertureObject, load_library
from .enricher import (
    EnrichOptions,
    enrich,
    parse_mapping,
    print_report,
)

logger = logging.getLogger(__name__)

AVAILABLE_FIELDS: list[tuple[str, str, str]] = [
    ("rating", "int", "Version rating (0-5, Aperture scale)"),
    ("mainRating", "int", "Version rating alias (some libs)"),
    ("isFlagged", "bool", "Pick/reject flag"),
    ("colorLabelIndex", "int", "Color label (0-6)"),
    ("keywords", "[uuid]", "Keyword UUIDs -> resolved names"),
    ("name", "str", "Version/master display name"),
    ("rotation", "int", "Rotation in degrees"),
    ("isOriginal", "bool", "True if this is the original version"),
    ("imageDate", "datetime", "Image capture date"),
    ("createDate", "datetime", "Object creation date"),
    ("albums", "[path]", "Aperture album/folder paths (multi-valued)"),
    ("project", "path", "Aperture project path (single)"),
    ("customInfo.cameraTimeZoneName", "str", "Camera timezone"),
    ("customInfo.pictureTimeZoneName", "str", "Picture timezone"),
    ("iptcProperties.<field>", "varies", "See IPTC fields below"),
    ("exifProperties.<field>", "varies", "See EXIF fields below"),
]


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(
        prog="digikam-enricher",
        description=(
            "Selectively copy metadata fields from an Aperture library into the "
            "XMP sidecars of exported images."
        ),
    )
    parser.add_argument(
        "--aperture-library",
        type=Path,
        required=True,
        help="Path to the .aplibrary bundle",
    )
    parser.add_argument(
        "--export-root",
        type=Path,
        required=True,
        help="Root directory of exported images + XMP sidecars",
    )
    parser.add_argument(
        "--map",
        action="append",
        default=[],
        metavar="APERTURE_FIELD=XMP_NAMESPACE:PROPERTY",
        help="Field mapping (repeatable)",
    )
    parser.add_argument(
        "--dry-run",
        action="store_true",
        help="Simulate without writing any files",
    )
    parser.add_argument(
        "--overwrite",
        action="store_true",
        help="Overwrite XMP properties that already exist (default: skip)",
    )
    parser.add_argument(
        "--prefer-master",
        action="store_true",
        help="Match using aplib:MasterUUID and apply master-level metadata",
    )
    parser.add_argument(
        "--match",
        choices=["version", "master"],
        default="version",
        help="Matching mode: version (default) or master",
    )
    parser.add_argument("--limit", type=int, default=None, help="Process at most N XMP files")
    parser.add_argument("--verbose", "-v", action="store_true", help="Verbose logging")
    parser.add_argument("--quiet", "-q", action="store_true", help="Suppress non-error output")
    parser.add_argument(
        "--list-fields",
        action="store_true",
        help="List available Aperture fields and exit",
    )
    return parser


def _setup_logging(verbose: bool, quiet: bool) -> None:
    level = logging.ERROR if quiet else (logging.DEBUG if verbose else logging.INFO)
    logging.basicConfig(
        level=level,
        format="%(levelname)s %(name)s: %(message)s",
        stream=sys.stderr,
    )


def _print_list_fields(library: ApertureLibrary, out) -> None:
    out.write("Available Aperture fields:\n")
    for name, typ, desc in AVAILABLE_FIELDS:
        out.write(f"  {name:<28} {typ:<9} {desc}\n")
    out.write("\nSample exifProperties keys (from this library):\n")
    exif_keys: set[str] = set()
    iptc_keys: set[str] = set()
    for obj in _iter_all_objects(library):
        exif = obj.metadata.get("exifProperties")
        if isinstance(exif, dict):
            exif_keys.update(exif.keys())
        iptc = obj.metadata.get("iptcProperties")
        if isinstance(iptc, dict):
            iptc_keys.update(iptc.keys())
    if exif_keys:
        for k in sorted(exif_keys)[:50]:
            out.write(f"  exifProperties.{k}\n")
    else:
        out.write("  (none found in this library)\n")
    if iptc_keys:
        out.write("\nSample iptcProperties keys (from this library):\n")
        for k in sorted(iptc_keys)[:50]:
            out.write(f"  iptcProperties.{k}\n")


def _iter_all_objects(library: ApertureLibrary):
    for obj in library.masters.values():
        yield obj
    for obj in library.versions.values():
        yield obj


def main(argv: list[str] | None = None) -> int:
    parser = build_parser()
    args = parser.parse_args(argv)

    _setup_logging(args.verbose, args.quiet)

    if args.list_fields:
        library = load_library(args.aperture_library)
        _print_list_fields(library, sys.stdout)
        return 0

    if not args.map:
        parser.error("at least one --map is required (or use --list-fields)")

    try:
        mappings = [parse_mapping(spec) for spec in args.map]
    except ValueError as exc:
        logger.error(str(exc))
        return 2

    options = EnrichOptions(
        aperture_library=args.aperture_library,
        export_root=args.export_root,
        mappings=mappings,
        dry_run=args.dry_run,
        overwrite=args.overwrite,
        prefer_master=args.match == "master" or args.prefer_master,
        limit=args.limit,
        verbose=args.verbose,
        quiet=args.quiet,
    )

    report = enrich(options)

    print_report(report)
    logger.info("Enrichment complete (dry_run=%s)", options.dry_run)

    return 0 if report.errors == 0 else 1