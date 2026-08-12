"""Generate an ExifTool config file declaring the XMP namespace(s) needed to
write the enrichment's target tags.

ExifTool refuses to write XMP tags that are not defined in its tag database and
whose namespace is not registered (e.g. ``Tag 'XMP-aplib:HasFocusPoints' is not
defined``). To write the custom ``aplib:`` namespace fields the exporter uses --
or any other custom namespace -- ExifTool must be pointed at a config file that
declares them.

This module writes a minimal, deterministic config file that declares exactly
the namespaces and tag names the current run needs. The generated file is passed
to ExifTool via the ``-config`` argument, and is safe to reuse across runs.

Reference: the ExifTool config file format is documented at
https://exiftool.org/config.html (see the ``XMP-xxx`` example).
"""

from __future__ import annotations

import logging
from pathlib import Path

logger = logging.getLogger(__name__)

# Core namespaces the exporter may already have written. We only need to declare
# namespaces that are NOT built into ExifTool. Standard ones (dc, xmp, exif,
# tiff, photoshop, lr, MicrosoftPhoto, mediapro) are already present, but we
# declare them idempotently to be safe for tags ExifTool does not know.
#
# Maps ExifTool family-1 namespace prefix -> XMP URI.
_BUILTIN_NAMESPACES = {}


def make_namespace_config(
    tags: list[str],
    extra_namespaces: dict[str, str] | None = None,
) -> str:
    """Return the text of an ExifTool config file that declares the
    namespaces/tags required to write ``tags``.

    ``tags`` are ExifTool tag names of the form ``XMP-<ns>:<Property>``.
    ``extra_namespaces`` maps an XMP namespace prefix to its URI for namespaces
    the tool should register even if no tag in that namespace is targeted
    directly (e.g. the `aplib` provenance namespace).

    The output declares one namespace table per unique namespace present in
    ``tags``, plus any in ``extra_namespaces``, and registers each under
    ``Image::ExifTool::XMP::Main``.
    """
    entries: dict[str, dict[str, str]] = {}  # prefix -> {tag -> uri}
    uri_by_prefix: dict[str, str] = {}

    for tag in tags:
        prefix, prop = _split_tag(tag)
        if prefix is None:
            continue
        entries.setdefault(prefix, {})[prop] = ""
        if prefix not in uri_by_prefix:
            uri_by_prefix[prefix] = _default_uri(prefix)

    for prefix, uri in (extra_namespaces or {}).items():
        uri_by_prefix.setdefault(prefix, uri)
        entries.setdefault(prefix, {})

    if not entries:
        return ""

    lines: list[str] = []
    lines.append("%Image::ExifTool::UserDefined = (")
    lines.append("    'Image::ExifTool::XMP::Main' => {")
    prefix_list = sorted(uri_by_prefix.keys())
    for i, prefix in enumerate(prefix_list):
        comma = "," if i < len(prefix_list) - 1 else ""
        lines.append(f"        {prefix} => {{")
        lines.append("            SubDirectory => {")
        lines.append(f"                TagTable => 'Image::ExifTool::UserDefined::{prefix}',")
        lines.append("            },")
        lines.append(f"        }}{comma}")
    lines.append("    },")
    lines.append(");")

    for prefix in prefix_list:
        lines.append(f"%Image::ExifTool::UserDefined::{prefix} = (")
        lines.append("    GROUPS => { 0 => 'XMP', 1 => 'XMP-%s', 2 => 'Image' }," % prefix)
        lines.append(f"    NAMESPACE => {{ '{prefix}' => '{uri_by_prefix[prefix]}' }},")
        lines.append("    WRITABLE => 'string',")
        for prop in sorted(entries[prefix]):
            lines.append(f"    {prop} => {{ }},")
        lines.append(");")
        lines.append("")

    return "\n".join(lines)


def _split_tag(tag: str) -> tuple[str | None, str | None]:
    """Split ``XMP-<ns>:<Prop>`` into (ns, prop). Returns (None, None) if the
    tag is not an XMP tag form we can declare."""
    if not tag.upper().startswith("XMP-"):
        return None, None
    ns, _, prop = tag[4:].partition(":")
    if not ns or not prop:
        return None, None
    return ns, prop


def _default_uri(prefix: str) -> str:
    """Return a URI for a namespace prefix we don't otherwise know.

    For the built-in `aplib` provenance namespace we use its canonical URI.
    Other namespaces get a synthetic but deterministic URI derived from the
    prefix so ExifTool can register them.
    """
    if prefix == "aplib":
        return "http://github.com/Jachimo/aplib-extractor/aplib/1.0/"
    return f"http://ns.digikam-enricher.local/{prefix}/"


class XmpConfigWriter:
    """Writes a config file to disk for the lifetime of a run."""

    def __init__(self, path: Path, tags: list[str], extra_namespaces=None):
        self.path = path
        self.tags = tags
        self.extra_namespaces = extra_namespaces

    def write(self) -> None:
        config = make_namespace_config(self.tags, self.extra_namespaces)
        if not config:
            logger.debug("No custom XMP namespaces required; no config written.")
            return
        self.path.parent.mkdir(parents=True, exist_ok=True)
        self.path.write_text(config, encoding="utf-8")
        logger.debug("Wrote ExifTool config to %s", self.path)

    def arg(self) -> list[str]:
        """Return the ExifTool command-line args to load this config."""
        return ["-config", str(self.path)]