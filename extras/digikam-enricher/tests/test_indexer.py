"""Tests for the export tree indexer (indexer.py)."""

from __future__ import annotations

from pathlib import Path

import pytest

from digikam_enricher import indexer

APLIB_NS = "http://github.com/Jachimo/aplib-extractor/aplib/1.0/"
DIGIKAM_NS = "http://www.digikam.org/ns/1.0/"

XMP_TEMPLATE = """<?xpacket begin="" id="W5M0MpCehiHzreSzNTczkc9d"?>
<x:xmpmeta xmlns:x="adobe:ns:meta/">
  <rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#">
    <rdf:Description xmlns:aplib="{APLIB_NS}" xmlns:digiKam="{DIGIKAM_NS}">
      <aplib:MasterUUID>{master_uuid}</aplib:MasterUUID>
      <digiKam:ImageUniqueID>{image_unique_id}</digiKam:ImageUniqueID>
    </rdf:Description>
  </rdf:RDF>
</x:xmpmeta>
<?xpacket end="w"?>
""".format(APLIB_NS=APLIB_NS, DIGIKAM_NS=DIGIKAM_NS, master_uuid="{master_uuid}", image_unique_id="{image_unique_id}")  # noqa: E501


def _write_pair(
    tmp_path: Path,
    subdir: str,
    image_name: str,
    master_uuid: str,
    unique_id: str,
) -> Path:
    d = tmp_path / subdir
    d.mkdir(parents=True, exist_ok=True)
    image = d / image_name
    image.write_bytes(b"fake-jpeg")
    xmp = d / f"{image_name}.xmp"
    xmp.write_text(
        XMP_TEMPLATE.format(master_uuid=master_uuid, image_unique_id=unique_id),
        encoding="utf-8",
    )
    return xmp


def _write_non_aperture(tmp_path: Path, subdir: str, image_name: str) -> Path:
    d = tmp_path / subdir
    d.mkdir(parents=True, exist_ok=True)
    image = d / image_name
    image.write_bytes(b"fake-png")
    xmp = d / f"{image_name}.xmp"
    xmp.write_text("<x:xmpmeta><rdf></rdf></x:xmpmeta>", encoding="utf-8")
    return xmp


def test_index_export_tree_counts(tmp_path):
    _write_pair(tmp_path, "a", "IMG1.JPG", "MASTER-1", "VERSION-1")
    _write_pair(tmp_path, "b", "IMG2.jpg", "MASTER-1", "VERSION-2")
    _write_pair(tmp_path, "c", "IMG3.jpeg", "MASTER-2", "VERSION-3")
    _write_pair(tmp_path, "a/nested", "IMG4.png", "MASTER-3", "VERSION-4")
    _write_non_aperture(tmp_path, "x", "PHOTO1.jpg")

    index = indexer.index_export_tree(tmp_path)
    assert index.scanned_xmp == 5
    assert index.non_aperture_xmp == 1
    assert len(index.by_unique_id) == 4
    assert len(index.by_master_uuid["MASTER-1"]) == 2
    assert "VERSION-1" in index.by_unique_id


def test_index_master_uuid_attribute_form(tmp_path):
    # Use attribute form instead of element form.
    d = tmp_path / "att"
    d.mkdir(parents=True, exist_ok=True)
    image = d / "IMG1.JPG"
    image.write_bytes(b"x")
    xmp = d / "IMG1.JPG.xmp"
    xmp.write_text(
        f'<x:xmpmeta><rdf:Description aplib:MasterUUID="M-1" digiKam:ImageUniqueID="V-1"/></x:xmpmeta>',
        encoding="utf-8",
    )
    index = indexer.index_export_tree(tmp_path)
    assert "V-1" in index.by_unique_id
    assert index.by_unique_id["V-1"].master_uuid == "M-1"


def test_index_skips_missing_sibling(tmp_path):
    d = tmp_path / "solo"
    d.mkdir(parents=True, exist_ok=True)
    xmp = d / "ORPHAN.JPG.xmp"
    xmp.write_text(
        XMP_TEMPLATE.format(master_uuid="M", image_unique_id="V"),
        encoding="utf-8",
    )
    index = indexer.index_export_tree(tmp_path)
    assert len(index.by_unique_id) == 0
    # XMP still counted as scanned.


def test_index_uppercase_extension(tmp_path):
    """Sidecar named <stem>.xmp should find <stem>.JPG (uppercase ext)."""
    d = tmp_path / "upper"
    d.mkdir(parents=True, exist_ok=True)
    image = d / "PICT2167.JPG"
    image.write_bytes(b"fake-jpeg")
    xmp = d / "PICT2167.xmp"
    xmp.write_text(
        XMP_TEMPLATE.format(master_uuid="M-1", image_unique_id="V-1"),
        encoding="utf-8",
    )
    index = indexer.index_export_tree(tmp_path)
    assert "V-1" in index.by_unique_id
    assert index.by_unique_id["V-1"].image_path == image

def test_extract_functions():
    text = '<aplib:MasterUUID>M-9</aplib:MasterUUID><digiKam:ImageUniqueID>V-9</digiKam:ImageUniqueID>'
    assert indexer.extract_master_uuid(text) == "M-9"
    assert indexer.extract_image_unique_id(text) == "V-9"