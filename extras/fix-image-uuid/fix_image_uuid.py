#!/usr/bin/env python3
"""Backfill digiKam ImageUniqueID in exported XMP sidecars.

DigiKam will read the ImageUniqueID value from an XMP sidecar and copy it into
the backend DB as a key, but only if the XMP sidecar contains one.

This script recursively scans an export tree for .xmp sidecars and ensures each
contains Xmp.digiKam.ImageUniqueID. Existing non-empty values are preserved.

Identity source precedence:
1) xmp:VersionUUID
2) aplib:OriginalVersionUUID
3) aplib:MasterUUID
4) generated UUIDv4

You must manually trigger a metadata-to-database sync in digiKam after running
this script to update the database with the new ImageUniqueID values.
"""

from __future__ import annotations

import argparse
import re
import sys
import uuid
import xml.etree.ElementTree as ET
from dataclasses import dataclass
from pathlib import Path
from typing import Iterable, Optional
from xml.sax.saxutils import escape

NS_RDF = "http://www.w3.org/1999/02/22-rdf-syntax-ns#"
NS_XMP = "http://ns.adobe.com/xap/1.0/"
NS_APLIB = "http://github.com/Jachimo/aplib-extractor/aplib/1.0/"
NS_DIGIKAM = "http://www.digikam.org/ns/1.0/"


@dataclass
class Stats:
    """Counters for the run summary printed at the end of execution."""

    scanned: int = 0
    updated: int = 0
    unchanged: int = 0
    parse_errors: int = 0
    skipped_no_description: int = 0


def _qname(namespace: str, local_name: str) -> str:
    """Build an ElementTree qualified name for namespaced XML tags/attributes."""

    return f"{{{namespace}}}{local_name}"


def _iter_descriptions(root: ET.Element) -> Iterable[ET.Element]:
    """Yield all rdf:Description nodes in the XMP packet."""

    return root.findall(f".//{_qname(NS_RDF, 'Description')}")


def _read_property(description: ET.Element, namespace: str, local_name: str) -> Optional[str]:
    """Read one property from a description, supporting attribute and element forms.

    Some tools serialize XMP properties as attributes on rdf:Description, while
    others emit child elements. We support both so the fixer works across
    mixed sidecar styles.
    """

    attr_key = _qname(namespace, local_name)
    attr_val = description.attrib.get(attr_key)
    if attr_val and attr_val.strip():
        return attr_val.strip()

    node = description.find(_qname(namespace, local_name))
    if node is not None and node.text and node.text.strip():
        return node.text.strip()

    return None


def _find_first_property(root: ET.Element, namespace: str, local_name: str) -> Optional[str]:
    """Return the first property value found in any rdf:Description."""

    for description in _iter_descriptions(root):
        value = _read_property(description, namespace, local_name)
        if value:
            return value
    return None


def _select_unique_id(root: ET.Element) -> tuple[str, str]:
    """Pick an ImageUniqueID value using migration-safe precedence.

    Preference order intentionally follows Aperture object identity:
    Version UUID first, then OriginalVersionUUID, then MasterUUID.
    UUIDv4 is only a last-resort fallback when no stable source identity exists.
    """

    version_uuid = _find_first_property(root, NS_XMP, "VersionUUID")
    if version_uuid:
        return version_uuid, "xmp:VersionUUID"

    original_version_uuid = _find_first_property(root, NS_APLIB, "OriginalVersionUUID")
    if original_version_uuid:
        return original_version_uuid, "aplib:OriginalVersionUUID"

    master_uuid = _find_first_property(root, NS_APLIB, "MasterUUID")
    if master_uuid:
        return master_uuid, "aplib:MasterUUID"

    return str(uuid.uuid4()), "generated:uuid4"


_RDF_DESCRIPTION_START_RE = re.compile(r"<rdf:Description\b[^>]*>", re.DOTALL)
_DIGIKAM_IMAGE_UNIQUE_ID_ATTR_RE = re.compile(r"\bdigiKam:ImageUniqueID\s*=\s*(['\"])(.*?)\1", re.DOTALL)
_XMPMETA_OPEN_RE = re.compile(r"<(?P<prefix>[A-Za-z_][\w.-]*):xmpmeta\b[^>]*>", re.DOTALL)
_XML_DECLARATION_RE = re.compile(r"^\s*<\?xml[^>]*>\s*", re.DOTALL)
_XPACKET_BEGIN = '<?xpacket begin="" id="W5M0MpCehiHzreSzNTczkc9d"?>'
_XPACKET_END = '<?xpacket end="w"?>'


def _insert_or_update_image_unique_id(text: str, value: str) -> str:
    """Update the first rdf:Description start tag without reserializing the packet.

    The update is intentionally textual so the existing packet wrapper and
    namespace declarations stay intact.
    """

    match = _RDF_DESCRIPTION_START_RE.search(text)
    if not match:
        raise ValueError("no rdf:Description start tag found")

    start_tag = match.group(0)
    attr_match = _DIGIKAM_IMAGE_UNIQUE_ID_ATTR_RE.search(start_tag)
    escaped_value = escape(value, {'"': '&quot;'})
    if attr_match:
        replacement = f'digiKam:ImageUniqueID="{escaped_value}"'
        new_start_tag = _DIGIKAM_IMAGE_UNIQUE_ID_ATTR_RE.sub(replacement, start_tag, count=1)
    else:
        new_start_tag = start_tag[:-1] + f' digiKam:ImageUniqueID="{escaped_value}">'

    return text[: match.start()] + new_start_tag + text[match.end() :]


def _ensure_xpacket_wrapper(text: str) -> str:
    """Restore the XMP packet wrapper when a file was serialized incorrectly."""

    stripped = text.lstrip()
    if stripped.startswith("<?xpacket"):
        return text

    without_xml_declaration = _XML_DECLARATION_RE.sub("", text, count=1).lstrip()
    wrapped = f"{_XPACKET_BEGIN}\n{without_xml_declaration.rstrip()}\n{_XPACKET_END}\n"
    return wrapped


def _normalize_xmpmeta_prefix(text: str) -> str:
    """Force the root packet prefix to x so DigiKam sees a familiar envelope."""

    match = _XMPMETA_OPEN_RE.search(text)
    if not match:
        return text

    prefix = match.group("prefix")
    if prefix == "x":
        return text

    open_tag = match.group(0)
    normalized_open_tag = open_tag.replace(f"<{prefix}:xmpmeta", "<x:xmpmeta", 1)
    normalized_open_tag = normalized_open_tag.replace(
        f'xmlns:{prefix}="adobe:ns:meta/"', 'xmlns:x="adobe:ns:meta/"', 1
    )
    normalized_text = text[: match.start()] + normalized_open_tag + text[match.end() :]
    normalized_text = normalized_text.replace(f"</{prefix}:xmpmeta>", "</x:xmpmeta>", 1)
    return normalized_text


def _has_existing_image_unique_id_marker(path: Path) -> bool:
    """Do a cheap text scan for ImageUniqueID before parsing XML.

    Most sidecars already contain the field, so this avoids a full XML parse for
    the common unchanged case and makes large export trees much faster.
    """

    try:
        with path.open("rb") as handle:
            while True:
                chunk = handle.read(65536)
                if not chunk:
                    return False
                if b"ImageUniqueID" in chunk:
                    return True
    except OSError:
        return False


def process_sidecar(
    path: Path,
    dry_run: bool,
    verbose: bool,
    rewrite_all: bool,
) -> tuple[str, Optional[str]]:
    """Process a single sidecar and return a status plus source label.

    Status values:
    - "updated": file would be/was written
    - "unchanged": no write needed
    - "parse_error": XML parse failed
    - "no_description": XMP packet has no rdf:Description node
    """

    if not rewrite_all and _has_existing_image_unique_id_marker(path):
        return "unchanged", None

    raw_text = path.read_text(encoding="utf-8")
    try:
        root = ET.fromstring(raw_text)
    except ET.ParseError:
        return "parse_error", None

    descriptions = list(_iter_descriptions(root))
    if not descriptions:
        return "no_description", None

    existing = _find_first_property(root, NS_DIGIKAM, "ImageUniqueID")
    if existing and not rewrite_all:
        return "unchanged", None

    source: Optional[str] = None
    updated_text = raw_text

    if not existing:
        value, source = _select_unique_id(root)
        updated_text = _insert_or_update_image_unique_id(updated_text, value)

    updated_text = _normalize_xmpmeta_prefix(updated_text)
    updated_text = _ensure_xpacket_wrapper(updated_text)

    if updated_text != raw_text:
        if not dry_run:
            path.write_text(updated_text, encoding="utf-8")
    else:
        return "unchanged", None

    if verbose:
        print(f"updated {path} [{source or 'rewrite:repair-envelope'}]")

    return "updated", source or "rewrite:repair-envelope"


def iter_sidecars(root: Path) -> Iterable[Path]:
    """Recursively yield .xmp files under root."""

    for path in root.rglob("*"):
        if path.is_file() and path.suffix.lower() == ".xmp":
            yield path


def main(argv: Optional[list[str]] = None) -> int:
    """CLI entrypoint.

    Exit codes:
    - 0: completed with no parse errors
    - 1: one or more sidecars could not be parsed
    - 2: invalid root path argument
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
        help="Rewrite all sidecars to normalize XML envelope/prefixes even if ImageUniqueID exists",
    )
    args = parser.parse_args(argv)

    root = args.root.expanduser().resolve()
    if not root.exists() or not root.is_dir():
        print(f"error: not a directory: {root}", file=sys.stderr)
        return 2

    stats = Stats()

    for sidecar in iter_sidecars(root):
        stats.scanned += 1
        status, _ = process_sidecar(
            sidecar,
            dry_run=args.dry_run,
            verbose=args.verbose,
            rewrite_all=args.rewrite_all,
        )
        if status == "updated":
            stats.updated += 1
        elif status == "unchanged":
            stats.unchanged += 1
        elif status == "parse_error":
            stats.parse_errors += 1
            print(f"warning: failed to parse {sidecar}", file=sys.stderr)
        elif status == "no_description":
            stats.skipped_no_description += 1
            print(f"warning: no rdf:Description in {sidecar}", file=sys.stderr)

    mode = "dry-run" if args.dry_run else "write"
    print(
        f"fix-image-uuid ({mode}): scanned={stats.scanned} "
        f"updated={stats.updated} unchanged={stats.unchanged} "
        f"parse_errors={stats.parse_errors} no_description={stats.skipped_no_description}"
    )

    return 0 if stats.parse_errors == 0 else 1


if __name__ == "__main__":
    raise SystemExit(main())
