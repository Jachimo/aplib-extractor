"""XMP writing via a persistent PyExifTool process.

ExifTool uses Exiv2 internally -- the same library DigiKam uses for XMP
parsing. This guarantees the XMP packet stays valid for DigiKam, including the
strict requirement that the XMP envelope use the ``x:xmpmeta`` prefix (see
DESIGN.md §3.2). Generic XML parsers (lxml et al.) can corrupt that prefix and
cause DigiKam to silently reject the sidecar, which is why we avoid them here.
"""

from __future__ import annotations

import logging
from dataclasses import dataclass, field
from pathlib import Path
from typing import Iterator

import exiftool

logger = logging.getLogger(__name__)

# Complex values (lists) are written one item at a time with the append
# operator to avoid clobbering any existing values.
APPEND_MARKER = "+="

# Map a Python value to its ExifTool-safe string representation.
def _serialize_value(value: object) -> str:
    """Convert an Aperture plist value to an ExifTool XMP string."""
    if isinstance(value, bool):
        return "True" if value else "False"
    if isinstance(value, (int, float)):
        return str(value)
    if isinstance(value, (list, tuple)):
        # Lists are handled by the caller (one tag per item) for array types,
        # but this provides a safe fallback.
        return " ".join(_serialize_value(v) for v in value)
    return str(value)


@dataclass
class WriteStats:
    """Per-run XMP write counters."""

    properties_written: int = 0
    files_updated: int = 0
    errors: int = 0
    per_property: dict[str, int] = field(default_factory=dict)


class XmpWriter:
    """Wraps a persistent ExifTool process to write XMP properties.

    Use as a context manager::

        with XmpWriter() as writer:
            writer.write_property(xmp_path, "XMP-digiKam:xmpRating", 3)

    On entering, a long-lived ExifTool subprocess is started and reused for all
    reads/writes, matching the performance pattern used by ``fix-image-uuid``.
    """

    def __init__(
        self,
        exiftool_binary: str | None = None,
        overwrite: bool = False,
        config_file: Path | str | None = None,
    ):
        self._overwrite = overwrite
        self._et: exiftool.ExifTool | None = None
        self._exiftool_binary = exiftool_binary
        self._config_file = config_file

    # -- lifecycle --------------------------------------------------------

    def __enter__(self) -> "XmpWriter":
        kwargs = {}
        if self._exiftool_binary:
            kwargs["executable"] = self._exiftool_binary
        if self._config_file is not None:
            kwargs["config_file"] = str(self._config_file)
        self._et = exiftool.ExifTool(**kwargs)
        self._et.run()
        return self

    def __exit__(self, exc_type, exc, tb) -> None:
        if self._et is not None and self._et.running:
            try:
                self._et.terminate()
            except Exception:
                pass
        self._et = None

    @property
    def exiftool(self) -> exiftool.ExifTool:
        return self._et

    # -- reading ----------------------------------------------------------

    def read_tags(self, path: Path, tags: list[str]) -> dict[str, str]:
        """Read the given XMP tags from a sidecar via ExifTool.

        Returns a dict mapping short tag names (e.g. ``ImageUniqueID``) to
        their string values. Missing tags are omitted.
        """
        if self._et is None:
            raise RuntimeError("XmpWriter not started")
        try:
            data = self._et.execute_json(
                *[f"-{tag}" for tag in tags],
                *[str(path)],
            )
        except Exception as exc:
            logger.warning("ExifTool read failed for %s: %s", path, exc)
            return {}

        result: dict[str, str] = {}
        entries = data if isinstance(data, list) else []
        for entry in entries:
            if not isinstance(entry, dict):
                continue
            for key, value in entry.items():
                if key == "SourceFile" or value is None:
                    continue
                short = key.split(":", 1)[-1]
                result[short] = str(value)
        return result

    def exists(self, path: Path, tag: str) -> bool:
        """Return True if ``tag`` is present in the sidecar's XMP."""
        return bool(self.read_tags(path, [tag]))

    # -- writing ----------------------------------------------------------

    def write_property(
        self,
        path: Path,
        tag: str,
        value: object,
        stats: WriteStats | None = None,
    ) -> bool:
        """Write a single XMP property ``tag=value`` to ``path``.

        Returns True if the write was issued (or would be, in dry-run mode the
        caller should skip invoking this entirely).
        """
        if self._et is None:
            raise RuntimeError("XmpWriter not started")

        string_value = _serialize_value(value)
        args = ["-q", "-q"]
        if not self._overwrite:
            # ExifTool: `-tag<value>` maps to a "copy-if-not-exists"-style
            # update only when the tag is missing; but for a plain scalar the
            # normal assignment overwrites. To respect "don't overwrite", we
            # simply guard at the caller by checking `exists` first. Here we
            # use a normal assignment.
            pass
        args.append(f"-{tag}={string_value}")
        args.append(str(path))

        try:
            self._et.execute(*[a if isinstance(a, bytes) else a.encode("utf-8") for a in args])
        except Exception as exc:
            logger.error("Failed writing %s=%r to %s: %s", tag, value, path, exc)
            if stats is not None:
                stats.errors += 1
            return False

        if stats is not None:
            stats.properties_written += 1
            stats.per_property[tag] = stats.per_property.get(tag, 0) + 1
        logger.info("Wrote %s=%r -> %s", tag, string_value, path)
        return True

    def write_list_property(
        self,
        path: Path,
        tag: str,
        values: list,
        stats: WriteStats | None = None,
        overwrite: bool = False,
    ) -> bool:
        """Append each item in ``values`` to a list-typed XMP tag.

        Uses ExifTool's ``+=`` operator so existing bag/seq items are
        preserved rather than replaced. When ``overwrite`` is True, the
        existing tag values are cleared first (via ExifTool's ``-tag=``
        delete syntax) to avoid duplicates on re-runs.
        """
        if self._et is None:
            raise RuntimeError("XmpWriter not started")

        ok = True
        if overwrite:
            # Clear existing values first to avoid duplicates.
            try:
                self._et.execute(
                    *[a.encode("utf-8") for a in ["-q", "-q", f"-{tag}=", str(path)]]
                )
            except Exception as exc:
                logger.error("Failed clearing %s in %s: %s", tag, path, exc)
                ok = False
                if stats is not None:
                    stats.errors += 1

        for value in values:
            string_value = _serialize_value(value)
            args = ["-q", "-q", f"-{tag}{APPEND_MARKER}{string_value}", str(path)]
            try:
                self._et.execute(*[a.encode("utf-8") for a in args])
            except Exception as exc:
                logger.error("Failed appending %s=%r to %s: %s", tag, value, path, exc)
                ok = False
                if stats is not None:
                    stats.errors += 1
                continue
            if stats is not None:
                stats.properties_written += 1
                stats.per_property[tag] = stats.per_property.get(tag, 0) + 1
        return ok


def dry_run_plan(path: Path, tag: str, value: object) -> str:
    """Return a human-readable line describing what a write would do.

    Used by the CLI in ``--dry-run`` mode, which never touches the filesystem.
    """
    return f"would write {tag}={_serialize_value(value)} -> {path}"