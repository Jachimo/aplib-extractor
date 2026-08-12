"""Core orchestration: load Aperture metadata, index the export tree, match,
and (optionally) enrich XMP sidecars.

The enrichment driven here is deliberately dry-run friendly: every write goes
through the XmpWriter only in non-dry-run mode; in dry-run mode we only log the
planned write lines.
"""

from __future__ import annotations

import logging
import re
import sys
from dataclasses import dataclass, field
from pathlib import Path

from .aperture import ApertureLibrary, load_library
from .indexer import ExportIndex, ExportItem, index_export_tree
from .matcher import (
    MatchKind,
    MatchRecord,
    match_items,
    resolve_field,
)
from .xmp_config import XmpConfigWriter
from .xmp_writer import WriteStats, XmpWriter

logger = logging.getLogger(__name__)

# Parse `ApertureField=XMP-<ns>:<Property>` mappings.
_MAP_RE = re.compile(r"^(?P<ap>[^=]+)=(?P<xmp>[^=]+)$")
_XMP_TAG_RE = re.compile(r"^(?P<ns>[A-Za-z0-9_-]+):(?P<prop>[^=]+)$")


@dataclass
class FieldMapping:
    aperture_field: str
    xmp_tag: str


def parse_mapping(spec: str) -> FieldMapping:
    """Parse a ``--map`` spec into a FieldMapping.

    Accepts two destination forms:
      * ``XMP-ns:Property``   (ExifTool tag form, e.g. ``XMP-digiKam:xmpRating``)
      * ``ns:Property``       (short form, e.g. ``digiKam:xmpRating``)
    Both normalize to an ExifTool tag ``XMP-<ns>:<Property>``.
    """
    match = _MAP_RE.match(spec)
    if not match:
        raise ValueError(f"Invalid --map value {spec!r}; expected 'APERTURE_FIELD=XMP_NAMESPACE:PROPERTY'")

    ap_field = match.group("ap").strip()
    dest = match.group("xmp").strip()

    tag_match = _XMP_TAG_RE.match(dest)
    if not tag_match:
        raise ValueError(
            f"Invalid XMP destination {dest!r}; expected 'XMP-ns:Property' or 'ns:Property'"
        )

    ns = tag_match.group("ns")
    prop = tag_match.group("prop")
    # Normalize to the ExifTool tag form.
    if ns.upper().startswith("XMP-"):
        tag = f"{ns}:{prop}"
    else:
        tag = f"XMP-{ns}:{prop}"
    return FieldMapping(aperture_field=ap_field, xmp_tag=tag)


@dataclass
class EnrichOptions:
    aperture_library: Path
    export_root: Path
    mappings: list[FieldMapping]
    dry_run: bool = True
    overwrite: bool = False
    prefer_master: bool = False
    limit: int | None = None
    verbose: bool = False
    quiet: bool = False


@dataclass
class EnrichReport:
    field_count: int = 0
    matched_versions: int = 0
    matched_masters: int = 0
    unmatched: int = 0
    missing_field: int = 0
    files_updated: int = 0
    errors: int = 0
    total_xmp_scanned: int = 0
    aplib_xmp: int = 0
    per_mapping_written: dict[str, int] = field(default_factory=dict)
    per_mapping_skipped: dict[str, int] = field(default_factory=dict)
    dry_run: bool = True


def _tag_present_in_xmp(writer: XmpWriter | None, item: ExportItem, tag: str) -> bool:
    """Determine whether ``tag`` already exists in the item's sidecar.

    In dry-run mode (writer is None) we conservatively assume it is not present
    so the plan lists it.
    """
    if writer is None:
        return False
    short = tag.rsplit(":", 1)[-1]
    return tag in writer.read_tags(item.xmp_path, [tag]) or bool(
        writer.exists(item.xmp_path, tag)
    )


def _write_value(
    writer: XmpWriter | None,
    item: ExportItem,
    mapping: FieldMapping,
    value: object,
    report: EnrichReport,
    verbose: bool,
    overwrite: bool,
) -> None:
    """Write (or plan) a single mapping on an export item."""
    key = f"{mapping.aperture_field} -> {mapping.xmp_tag}"
    report.per_mapping_written[key] = report.per_mapping_written.get(key, 0) + 1
    report.field_count += 1

    if writer is None:
        if verbose:
            print(f"  would write {mapping.xmp_tag}={value!r} -> {item.xmp_path}")
        return

    # Don't overwrite existing values unless requested.
    if not overwrite and _tag_present_in_xmp(writer, item, mapping.xmp_tag):
        return

    if isinstance(value, (list, tuple)) and value:
        ok = writer.write_list_property(item.xmp_path, mapping.xmp_tag, list(value))
    else:
        ok = writer.write_property(item.xmp_path, mapping.xmp_tag, value)

    if ok:
        report.files_updated += 1
    else:
        report.errors += 1


def _apply_mappings(
    writer: XmpWriter | None,
    item: ExportItem,
    obj,
    mappings: list[FieldMapping],
    report: EnrichReport,
    library: ApertureLibrary,
    verbose: bool,
    overwrite: bool,
) -> None:
    """Apply each requested mapping whose Aperture field resolves to a value."""
    for mapping in mappings:
        value = resolve_field(library, obj, mapping.aperture_field)
        if value is None:
            report.missing_field += 1
            report.per_mapping_skipped[
                f"{mapping.aperture_field} -> {mapping.xmp_tag}"
            ] = report.per_mapping_skipped.get(
                f"{mapping.aperture_field} -> {mapping.xmp_tag}", 0
            ) + 1
            continue
        _write_value(writer, item, mapping, value, report, verbose, overwrite)


def enrich(options: EnrichOptions) -> EnrichReport:
    """Run the full enrichment pipeline and return a report.

    In dry-run mode ``writer`` is None and no files are touched. Otherwise a
    persistent ExifTool process is opened for the run.
    """
    report = EnrichReport(dry_run=options.dry_run)

    # Phase 1: load Aperture metadata.
    library = load_library(options.aperture_library)
    if not library.masters and not library.versions:
        logger.error("No Aperture masters/versions loaded from %s", options.aperture_library)
        report.errors += 1
        return report
    if not library.masters:
        logger.warning("Library has versions but no masters: %s", options.aperture_library)

    # Phase 2: index export tree.
    index = index_export_tree(options.export_root, options.limit)
    report.total_xmp_scanned = index.scanned_xmp
    report.aplib_xmp = index.scanned_xmp - index.non_aperture_xmp

    # Phase 3: match.
    records, match_stats = match_items(library, index.by_unique_id, options.prefer_master)
    report.matched_versions = match_stats.version_hits
    report.matched_masters = match_stats.master_hits
    report.unmatched = match_stats.unmatched
    for reason, count in match_stats.by_reason.items():
        report.per_mapping_skipped.setdefault(f"unmatched: {reason}", count)

    # `by_unique_id` values are ExportItem objects.
    items_by_id = index.by_unique_id

    def run(et: XmpWriter | None) -> None:
        for rec in records:
            if rec.kind == MatchKind.UNMATCHED:
                continue
            obj = None
            if rec.kind == MatchKind.VERSION:
                obj = library.versions.get(rec.object_uuid)
            else:
                obj = library.masters.get(rec.object_uuid)
            if obj is None:
                report.errors += 1
                continue
            item = items_by_id.get(rec.image_unique_id)
            if item is None:
                report.errors += 1
                continue
            _apply_mappings(
                et,
                item,
                obj,
                options.mappings,
                report,
                library,
                options.verbose,
                options.overwrite,
            )

    if options.dry_run:
        run(None)
    else:
        # Generate an ExifTool config declaring the namespaces/tags we target,
        # so custom namespaces (notably `aplib:`) can be written. ExifTool
        # refuses to write tags/namespaces not declared in its config.
        tag_list = [m.xmp_tag for m in options.mappings]
        config_path = options.export_root / ".digikam-enricher.config"
        XmpConfigWriter(
            config_path,
            tag_list,
            extra_namespaces={"aplib": "http://github.com/Jachimo/aplib-extractor/aplib/1.0/"},
        ).write()
        with XmpWriter(
            exiftool_binary=None,
            overwrite=options.overwrite,
            config_file=config_path,
        ) as et:
            run(et)

    return report


def format_report(report: EnrichReport) -> str:
    """Render a human-readable report to a string."""
    lines: list[str] = []
    mode = "dry-run" if report.dry_run else "write"
    lines.append("=== Enrichment Report (%s) ===" % mode)
    lines.append(f"Total XMP files scanned:  {report.total_xmp_scanned:>10,}")
    lines.append(f"Aperture-exported files:  {report.aplib_xmp:>10,}")
    matched = report.matched_versions + report.matched_masters
    lines.append(f"Matched to Aperture data: {matched:>10,}")
    lines.append(f"  Versions matched:       {report.matched_versions:>10,}")
    lines.append(f"  Masters matched:        {report.matched_masters:>10,}")
    lines.append(f"Unmatched:                {report.unmatched:>10,}")
    lines.append(f"Missing field from Aperture: {report.missing_field:>5,}")
    lines.append(f"Files updated:            {report.files_updated:>10,}")
    lines.append(f"Properties written:       {report.field_count:>10,}")
    if report.dry_run:
        lines.append(f"Planned writes (dry-run): {report.field_count:>3,}")
    for key, count in sorted(report.per_mapping_written.items()):
        lines.append(f"  {key}: {count:,}")
    if report.dry_run:
        lines.append(f"Skipped (dry-run):        {report.field_count:>10,}")
    if report.errors:
        lines.append(f"Errors:                   {report.errors:>10,}")
    return "\n".join(lines)


def print_report(report: EnrichReport, stream=sys.stdout) -> None:
    stream.write(format_report(report) + "\n")