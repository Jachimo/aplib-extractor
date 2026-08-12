"""Read Aperture metadata from plist files (``.apversion``, ``.apmaster``,
``Keywords.plist``).

Aperture stores its master/version metadata as Apple binary property lists.
Python's :mod:`plistlib` parses both the binary and XML plist formats used by
Aperture.

Object hierarchy
----------------
A ``Master`` is the source image file. Each master has one or more ``Version``
objects that render it (the first is always the "original"). Metadata that can
be selectively re-copied into XMP lives on either the master or the version:

- Version-level fields: ``rating``/``mainRating``, ``isFlagged``,
  ``colorLabelIndex``, ``keywords``, IPTC/EXIF properties, custom info, dates.
- Master-level fields: ``name``, ``imagePath``, ``colorSpaceName``,
  ``isTrulyRaw``, ``fileSize``, ``faceDetectionState``, etc.

This module builds in-memory indexes keyed by Aperture UUID so the rest of the
tool can look up metadata for a given object quickly.
"""

from __future__ import annotations

import logging
import plistlib
from dataclasses import dataclass, field
from pathlib import Path

logger = logging.getLogger(__name__)


@dataclass
class ApertureObject:
    """A resolved Aperture master or version with its plist-derived metadata.

    ``metadata`` is the raw parsed plist dictionary. It is kept whole (rather
    than flattened) so callers can address nested fields via dotted paths such
    as ``exifProperties.FocalLength`` or ``iptcProperties.Caption/Abstract``.
    """

    uuid: str
    obj_type: str  # "master" | "version"
    master_uuid: str | None
    metadata: dict
    source_path: Path = field(default_factory=Path)


@dataclass
class ApertureLibrary:
    """In-memory view of an Aperture library's masters and versions.

    Attributes
    ----------
    masters : dict[str, ApertureObject]
        Keyed by master UUID.
    versions : dict[str, ApertureObject]
        Keyed by version UUID.
    version_by_original : dict[str, str]
        Maps a master's ``aplib:OriginalVersionUUID`` to the version UUID of
        the object that is its original Version-0 (where known). Keys are the
        original-version UUID values.
    keywords_flat : dict[str, str]
        Maps keyword UUID to a simple keyword name.
    keywords_path : dict[str, str]
        Maps keyword UUID to a hierarchical ``/``-separated path.
    """

    masters: dict[str, ApertureObject] = field(default_factory=dict)
    versions: dict[str, ApertureObject] = field(default_factory=dict)
    version_by_original: dict[str, str] = field(default_factory=dict)
    keywords_flat: dict[str, str] = field(default_factory=dict)
    keywords_path: dict[str, str] = field(default_factory=dict)


def _load_plist(path: Path) -> dict | None:
    """Load a plist file, returning its dict or ``None`` on failure."""
    try:
        with path.open("rb") as fh:
            value = plistlib.load(fh)
    except (OSError, plistlib.InvalidFileException, ValueError):
        logger.warning("Failed to parse plist: %s", path)
        return None
    if isinstance(value, dict):
        return value
    logger.warning("Unexpected plist root type in %s: %s", path, type(value).__name__)
    return None


def _collect_keywords(
    entries: list,
    keyword_path: str,
    flat: dict[str, str],
    path_map: dict[str, str],
) -> None:
    """Recursively walk the ``Keywords.plist`` ``keywords`` array.

    ``entries`` items are dicts with ``uuid``, ``name`` and optionally
    ``children`` (in older versions, ``zChildren``).
    """
    for item in entries:
        if not isinstance(item, dict):
            continue
        uuid = item.get("uuid")
        name = item.get("name")
        if not uuid:
            continue
        name = name if isinstance(name, str) else str(name)
        current_path = f"{keyword_path}/{name}" if keyword_path else name
        flat[uuid] = name
        path_map[uuid] = current_path
        children = item.get("children")
        if children is None:
            children = item.get("zChildren")
        if isinstance(children, list):
            _collect_keywords(children, current_path, flat, path_map)


def load_keywords(plist_path: Path, library: ApertureLibrary) -> None:
    """Populate ``library`` keyword maps from ``Keywords.plist``."""
    data = _load_plist(plist_path)
    if data is None:
        logger.warning("No keywords loaded (missing/invalid %s)", plist_path)
        return
    keywords = data.get("keywords")
    if isinstance(keywords, list):
        _collect_keywords(keywords, "", library.keywords_flat, library.keywords_path)
        logger.info(
            "Loaded %d keywords from %s",
            len(library.keywords_flat),
            plist_path,
        )
    else:
        logger.warning("No 'keywords' array in %s", plist_path)


def _index_file(
    path: Path,
    obj_type: str,
    library: ApertureLibrary,
) -> ApertureObject | None:
    """Parse one ``.apversion`` or ``.apmaster`` file and index it."""
    data = _load_plist(path)
    if data is None:
        return None

    uuid = data.get("uuid")
    if not isinstance(uuid, str):
        logger.warning("Object in %s has no string uuid; skipped", path)
        return None

    master_uuid = data.get("masterUuid")
    if not isinstance(master_uuid, str):
        master_uuid = data.get("masterUuid")  # non-string masterUuid/None
    master_uuid = master_uuid if isinstance(master_uuid, str) else None

    obj = ApertureObject(
        uuid=uuid,
        obj_type=obj_type,
        master_uuid=master_uuid,
        metadata=data,
        source_path=path,
    )

    if obj_type == "master":
        library.masters[uuid] = obj
        # Record original-version linkage if present.
        orig = data.get("originalVersionUuid")
        if isinstance(orig, str):
            library.version_by_original.setdefault(orig, uuid)
    else:
        library.versions[uuid] = obj
        # If this is the original version (Version-0) of a master, remember the
        # mapping so a master sidecar's ImageUniqueID (= OriginalVersionUUID)
        # can be resolved to the version's actual UUID when needed.
        if data.get("isOriginal") is True and isinstance(master_uuid, str):
            library.version_by_original[uuid] = master_uuid

    return obj


def _iter_version_dirs(versions_root: Path):
    """Yield (directory, identifier) for each UUID object dir under
    ``Database/Versions/YYYY/MM/DD/YYYYMMDD-HHMMSS/<uuid>/``.

    Items are discovered via a bounded walk; the tree is 6 levels deep.
    """
    if not versions_root.is_dir():
        return
    for date_dir in versions_root.iterdir():
        if not date_dir.is_dir():
            continue
        for mon_dir in date_dir.iterdir():
            if not mon_dir.is_dir():
                continue
            for day_dir in mon_dir.iterdir():
                if not day_dir.is_dir():
                    continue
                for stamp_dir in day_dir.iterdir():
                    if not stamp_dir.is_dir():
                        continue
                    for obj_dir in stamp_dir.iterdir():
                        if obj_dir.is_dir():
                            yield obj_dir


def load_library(library_path: Path) -> ApertureLibrary:
    """Load all masters, versions and keywords from an ``.aplibrary`` bundle.

    The plist files live under ``<bundle>/Database/Versions/.../<uuid>/`` and
    are named ``Master.apmaster`` / ``Version-N.apversion``. Keywords live at
    ``<bundle>/Database/Keywords.plist``.
    """
    library = ApertureLibrary()

    database_dir = library_path / "Database"

    # Keywords first (version plists reference keyword UUIDs).
    keywords_plist = database_dir / "Keywords.plist"
    if keywords_plist.exists():
        load_keywords(keywords_plist, library)
    else:
        # Some bundles keep it at the top level.
        alt_keywords = library_path / "Keywords.plist"
        if alt_keywords.exists():
            load_keywords(alt_keywords, library)

    versions_root = database_dir / "Versions"
    count = 0
    for obj_dir in _iter_version_dirs(versions_root):
        apmaster = obj_dir / "Master.apmaster"
        if apmaster.exists():
            if _index_file(apmaster, "master", library) is not None:
                count += 1
        # Version-*.apversion files.
        for apversion in sorted(obj_dir.glob("Version-*.apversion")):
            if _index_file(apversion, "version", library) is not None:
                count += 1

    logger.info(
        "Aperture library loaded: %d masters, %d versions, %d keywords "
        "(path=%s)",
        len(library.masters),
        len(library.versions),
        len(library.keywords_flat),
        library_path,
    )
    return library