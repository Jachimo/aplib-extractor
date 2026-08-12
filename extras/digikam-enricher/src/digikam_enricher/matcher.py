"""Match exported XMP sidecars / images to Aperture objects and map field
paths to values.

Responsibilities
----------------
1. Resolve a sidecar's ``digiKam:ImageUniqueID`` / ``aplib:MasterUUID`` to an
   ``ApertureObject`` (master or version) in the loaded library.
2. Given a user-requested Aperture field path (e.g. ``rating`` or
   ``iptcProperties.Caption/Abstract``), return the value from that object's
   metadata, resolving keyword UUIDs when ``keywords`` is the target.
3. Record match outcomes for the final report.
"""

from __future__ import annotations

import logging
from dataclasses import dataclass, field
from enum import Enum
from pathlib import Path

from .aperture import ApertureLibrary, ApertureObject

logger = logging.getLogger(__name__)


class MatchKind(Enum):
    VERSION = "version"
    MASTER = "master"
    UNMATCHED = "unmatched"


@dataclass
class MatchRecord:
    """Outcome of attempting to match one export item to an Aperture object."""

    image_unique_id: str
    xmp_path: Path
    kind: MatchKind
    object_uuid: str | None = None
    master_uuid: str | None = None
    reason: str | None = None


@dataclass
class MatchStats:
    version_hits: int = 0
    master_hits: int = 0
    unmatched: int = 0
    by_reason: dict[str, int] = field(default_factory=dict)


def _lookup_object(
    library: ApertureLibrary,
    image_unique_id: str,
    master_uuid: str | None,
    prefer_master: bool,
) -> tuple[ApertureObject | None, str]:
    """Resolve an export item to an Aperture object.

    Returns ``(object, kind)`` where kind is ``"version"`` or ``"master"``.

    Resolution order (unless ``prefer_master``):
      1. Version lookup by ``image_unique_id``.
      2. If the ID is an original-version UUID with no version record, fall back
         to the master it belongs to (master sidecars carry OriginalVersionUUID
         as their ImageUniqueID).
      3. Master lookup by ``master_uuid``.
    """
    if not prefer_master:
        version = library.versions.get(image_unique_id)
        if version is not None:
            return version, "version"

    # If the unique id refers to a master (via original-version linkage) or
    # explicitly to a master UUID, honor it.
    master = library.masters.get(image_unique_id)
    if master is not None:
        return master, "master"

    # Original-version linkage: image_unique_id is an original-version UUID.
    linked_master_uuid = library.version_by_original.get(image_unique_id)
    if linked_master_uuid:
        master = library.masters.get(linked_master_uuid)
        if master is not None:
            return master, "master"

    if master_uuid:
        master = library.masters.get(master_uuid)
        if master is not None:
            return master, "master"

    return None, "unmatched"


def match_items(
    library: ApertureLibrary,
    by_unique_id: dict[str, object],
    prefer_master: bool = False,
) -> tuple[list[MatchRecord], MatchStats]:
    """Match every indexed export item to an Aperture object.

    ``by_unique_id`` maps ``digiKam:ImageUniqueID`` -> ExportItem. Returns a
    list of MatchRecords (one per item) and aggregate stats.
    """
    records: list[MatchRecord] = []
    stats = MatchStats()

    for unique_id, item in by_unique_id.items():
        master_uuid = getattr(item, "master_uuid", None)
        obj, kind = _lookup_object(
            library,
            unique_id,
            master_uuid,
            prefer_master,
        )
        if obj is None:
            reason = "no Aperture metadata found for UUID"
            stats.unmatched += 1
            stats.by_reason[reason] = stats.by_reason.get(reason, 0) + 1
            records.append(
                MatchRecord(
                    image_unique_id=unique_id,
                    xmp_path=getattr(item, "xmp_path", Path()),
                    kind=MatchKind.UNMATCHED,
                    master_uuid=master_uuid,
                    reason=reason,
                )
            )
            continue

        if kind == "version":
            stats.version_hits += 1
        else:
            stats.master_hits += 1

        records.append(
            MatchRecord(
                image_unique_id=unique_id,
                xmp_path=getattr(item, "xmp_path", Path()),
                kind=MatchKind.VERSION if kind == "version" else MatchKind.MASTER,
                object_uuid=obj.uuid,
                master_uuid=master_uuid or obj.master_uuid,
            )
        )

    logger.info(
        "Matching complete: versions=%d masters=%d unmatched=%d",
        stats.version_hits,
        stats.master_hits,
        stats.unmatched,
    )
    return records, stats


def resolve_field(
    library: ApertureLibrary,
    obj: ApertureObject,
    field_path: str,
) -> object | None:
    """Return the value of ``field_path`` on ``obj``'s metadata.

    ``field_path`` is a dotted path into the plist dict (e.g.
    ``exifProperties.FocalLength``). The special ``keywords`` path resolves the
    object's keyword UUID list to human-readable names.
    """
    metadata = obj.metadata

    if field_path == "keywords":
        return _resolve_keywords(library, metadata)

    # Alias: Aperture stores rating under 'mainRating' in .apversion plists.
    if field_path == "rating":
        field_path = "mainRating"

    parts = field_path.split(".")
    current: object = metadata
    for part in parts:
        if not isinstance(current, dict):
            return None
        if part not in current:
            return None
        current = current[part]
    return current


def _resolve_keywords(library: ApertureLibrary, metadata: dict) -> list[str] | None:
    """Resolve an Aperture object's keyword UUID list to human-readable names.

    Aperture versions/masters store keywords as an array of keyword UUID
    strings (in real libraries under the ``keywords`` key; some imports use
    ``zKeywords``). Unresolvable UUIDs fall back to a ``uuid:...`` form rather
    than crashing.
    """
    raw = None
    for key in ("keywords", "zKeywords"):
        value = metadata.get(key)
        if isinstance(value, list):
            raw = value
            break

    if raw is None:
        return None

    names: list[str] = []
    for value in raw:
        if isinstance(value, str):
            uuid = value
        elif isinstance(value, dict):
            uuid = value.get("uuid") or value.get("keywordUuid")
            if not isinstance(uuid, str):
                continue
        else:
            continue
        name = library.keywords_flat.get(uuid)
        if name is None:
            name = f"uuid:{uuid}"
        if name not in names:
            names.append(name)
    return names if names else None