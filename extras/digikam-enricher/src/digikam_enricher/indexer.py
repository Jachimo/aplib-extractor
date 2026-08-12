"""Index an exported image tree: find `.xmp` sidecars that carry Aperture
provenance (`aplib:` namespace) and build lookups by `digiKam:ImageUniqueID`
and `aplib:MasterUUID`.
"""

from __future__ import annotations

import logging
import os
import re
from dataclasses import dataclass, field
from pathlib import Path

logger = logging.getLogger(__name__)

os_scandir = os.scandir

# The exporter always emits `aplib:MasterUUID` and `digiKam:ImageUniqueID`. We
# only need these two identifiers from the sidecar head.
_PREFIX_HEAD_BYTES = 4096
_APLIB_MARKER = b"aplib:"

MASTER_UUID_ELEMENT_RE = re.compile(
    r"<aplib:MasterUUID>\s*([^<\s][^<]*)\s*</aplib:MasterUUID>"
)
MASTER_UUID_ATTR_RE = re.compile(
    r"aplib:MasterUUID=\"([^\"]+)\"|aplib:MasterUUID='([^']+)'"
)
IMAGE_UNIQUE_ID_ELEMENT_RE = re.compile(
    r"<digiKam:ImageUniqueID>\s*([^<\s][^<]*)\s*</digiKam:ImageUniqueID>"
)
IMAGE_UNIQUE_ID_ATTR_RE = re.compile(
    r"digiKam:ImageUniqueID=\"([^\"]+)\"|digiKam:ImageUniqueID='([^']+)'"
)


@dataclass
class ExportItem:
    """An image + sidecar pair found in the export tree that carries Aperture
    provenance.
    """

    image_path: Path
    xmp_path: Path
    image_unique_id: str
    master_uuid: str | None


@dataclass
class ExportIndex:
    """Index of Aperture-exported items.

    Attributes
    ----------
    by_unique_id : dict[str, ExportItem]
        Keyed by ``digiKam:ImageUniqueID``.
    by_master_uuid : dict[str, list[ExportItem]]
        Keyed by ``aplib:MasterUUID``.
    scanned_xmp : int
        Total `.xmp` files inspected.
    non_aperture_xmp : int
        `.xmp` files that did not contain the `aplib:` marker.
    """

    by_unique_id: dict[str, ExportItem] = field(default_factory=dict)
    by_master_uuid: dict[str, list[ExportItem]] = field(default_factory=dict)
    scanned_xmp: int = 0
    non_aperture_xmp: int = 0


def _read_head(path: Path) -> str:
    """Read a small prefix of a sidecar as text for fast regex matching."""
    try:
        with path.open("r", encoding="utf-8", errors="ignore") as fh:
            return fh.read(_PREFIX_HEAD_BYTES)
    except OSError as exc:
        logger.warning("Failed reading sidecar %s: %s", path, exc)
        return ""


def _extract_first(pattern: re.Pattern, text: str) -> str | None:
    """Return the first capture of ``pattern`` in ``text`` (element first, then
    attribute form)."""
    match = pattern.search(text)
    if not match:
        return None
    # Regexes with alternation put the attribute value in group 1 or 2.
    if match.lastindex and match.lastindex >= 2 and match.group(2):
        return match.group(2)
    value = match.group(1)
    return value.strip() if value else None


def extract_image_unique_id(text: str) -> str | None:
    """Extract `digiKam:ImageUniqueID` from the head of a sidecar text."""
    value = _extract_first(IMAGE_UNIQUE_ID_ELEMENT_RE, text)
    if value:
        return value
    return _extract_first(IMAGE_UNIQUE_ID_ATTR_RE, text)


def extract_master_uuid(text: str) -> str | None:
    """Extract `aplib:MasterUUID` from the head of a sidecar text."""
    value = _extract_first(MASTER_UUID_ELEMENT_RE, text)
    if value:
        return value
    return _extract_first(MASTER_UUID_ATTR_RE, text)


def index_export_tree(export_root: Path, limit: int | None = None) -> ExportIndex:
    """Walk ``export_root`` and index Aperture-exported sidecar/image pairs.

    Uses a recursive scandir walk and a fast marker pre-filter so non-Aperture
    images in a large tree are skipped cheaply.
    """
    index = ExportIndex()
    if not export_root.is_dir():
        logger.error("Export root is not a directory: %s", export_root)
        return index

    count = 0
    for xmp_path in _iter_xmp(export_root):
        if limit is not None and count >= limit:
            break
        count += 1
        index.scanned_xmp += 1

        head = _read_head(xmp_path)
        if _APLIB_MARKER not in head.encode("utf-8", errors="ignore"):
            index.non_aperture_xmp += 1
            continue

        image_unique_id = extract_image_unique_id(head)
        master_uuid = extract_master_uuid(head)

        if not image_unique_id:
            logger.debug("Aperture sidecar without digiKam:ImageUniqueID: %s", xmp_path)
            continue

        # Find the sibling image. Prefer `<name>.xmp` -> `<name>` then
        # `<stem>.xmp`.
        image_path = _find_sibling_image(xmp_path)
        if image_path is None:
            logger.warning("No sibling image found for sidecar: %s", xmp_path)
            continue

        item = ExportItem(
            image_path=image_path,
            xmp_path=xmp_path,
            image_unique_id=image_unique_id,
            master_uuid=master_uuid,
        )
        index.by_unique_id[image_unique_id] = item
        if master_uuid:
            index.by_master_uuid.setdefault(master_uuid, []).append(item)

    logger.info(
        "Indexed export tree: %d XMP files (scanned=%d, aplib=%d, "
        "unique_id=%d, master_uuid=%d)",
        index.scanned_xmp,
        index.scanned_xmp,
        index.scanned_xmp - index.non_aperture_xmp,
        len(index.by_unique_id),
        len(index.by_master_uuid),
    )
    return index


def _iter_xmp(root: Path):
    """Recursively yield `.xmp` files via an explicit scandir stack."""
    stack = [root]
    while stack:
        current = stack.pop()
        try:
            entries = list(os_scandir(current))
        except OSError as exc:
            logger.warning("Cannot read directory %s: %s", current, exc)
            continue
        for entry in entries:
            if entry.is_dir(follow_symlinks=False):
                stack.append(Path(entry.path))
            elif entry.is_file() and entry.name.lower().endswith(XMP_SUFFIX):
                yield Path(entry.path)


XMP_SUFFIX = ".xmp"

IMAGE_SUFFIXES = {
    ".jpg",
    ".jpeg",
    ".png",
    ".tif",
    ".tiff",
    ".heic",
    ".heif",
    ".dng",
    ".cr2",
    ".cr3",
    ".nef",
    ".arw",
    ".raw",
}


def _find_sibling_image(xmp_path: Path) -> Path | None:
    """Find the image file associated with ``xmp_path``.

    The exporter names sidecars ``<image>.<ext>.xmp`` or ``<stem>.xmp`` (and
    the exporter's ``materialize`` step may use `<stem>-<label>.xmp`). We try
    in order:
      1. ``<xmp full name minus trailing .xmp>``
      2. ``<stem>`` with each known image extension
    """
    parent = xmp_path.parent

    # Case 1: <image.ext>.xmp -> strip ".xmp"
    direct = parent / (xmp_path.name[: -len(XMP_SUFFIX)])
    if _is_image(direct):
        return direct

    # Case 2: <stem>.xmp -> <stem>.<ext>
    stem = xmp_path.with_suffix("").name
    for suffix in sorted(IMAGE_SUFFIXES):
        candidate = parent / f"{stem}{suffix}"
        if _is_image(candidate):
            return candidate

    return None


def _is_image(path: Path) -> bool:
    return path.is_file() and path.suffix.lower() in IMAGE_SUFFIXES