"""Tests for matching logic (matcher.py)."""

from __future__ import annotations

from pathlib import Path

import pytest

from digikam_enricher.aperture import ApertureLibrary, ApertureObject
from digikam_enricher.indexer import ExportItem
from digikam_enricher.matcher import (
    MatchKind,
    match_items,
    resolve_field,
)


def _obj(uuid, obj_type, master_uuid=None, metadata=None):
    return ApertureObject(
        uuid=uuid,
        obj_type=obj_type,
        master_uuid=master_uuid,
        metadata=metadata or {},
        source_path=Path("/nonexistent"),
    )


def _item(unique_id, master_uuid=None):
    return ExportItem(
        image_path=Path("/i.jpg"),
        xmp_path=Path("/i.jpg.xmp"),
        image_unique_id=unique_id,
        master_uuid=master_uuid,
    )


def test_match_version_preferred():
    lib = ApertureLibrary()
    lib.versions["V-1"] = _obj("V-1", "version", "M-1", {"rating": 3})
    lib.masters["M-1"] = _obj("M-1", "master", None, {"name": "Master"})

    items = {"V-1": _item("V-1", "M-1")}
    records, stats = match_items(lib, items)
    assert records[0].kind == MatchKind.VERSION
    assert stats.version_hits == 1


def test_match_master_via_unique_id():
    lib = ApertureLibrary()
    lib.masters["M-1"] = _obj("M-1", "master", None, {"name": "Master"})
    items = {"M-1": _item("M-1", "M-1")}
    records, stats = match_items(lib, items)
    assert records[0].kind == MatchKind.MASTER
    assert stats.master_hits == 1


def test_match_master_via_master_uuid_fallback():
    lib = ApertureLibrary()
    lib.masters["M-1"] = _obj("M-1", "master", None, {"name": "Master"})
    # unique id not in versions/masters, but master_uuid matches.
    items = {"UNKNOWN": _item("UNKNOWN", "M-1")}
    records, stats = match_items(lib, items)
    assert records[0].kind == MatchKind.MASTER


def test_match_unmatched():
    lib = ApertureLibrary()
    items = {"NOPE": _item("NOPE", None)}
    records, stats = match_items(lib, items)
    assert records[0].kind == MatchKind.UNMATCHED
    assert stats.unmatched == 1


def test_prefer_master_mode():
    lib = ApertureLibrary()
    lib.versions["V-1"] = _obj("V-1", "version", "M-1", {"rating": 3})
    lib.masters["M-1"] = _obj("M-1", "master", None, {"name": "Master"})
    items = {"V-1": _item("V-1", "M-1")}
    records, _ = match_items(lib, items, prefer_master=True)
    assert records[0].kind == MatchKind.MASTER


def test_resolve_field_nested():
    lib = ApertureLibrary()
    obj = _obj("V", "version", None, {"exifProperties": {"FocalLength": 40}})
    assert resolve_field(lib, obj, "exifProperties.FocalLength") == 40


def test_resolve_field_missing():
    lib = ApertureLibrary()
    obj = _obj("V", "version", None, {"name": "X"})
    assert resolve_field(lib, obj, "exifProperties.FocalLength") is None
    assert resolve_field(lib, obj, "missing") is None


def test_resolve_keywords_resolves_uuids():
    lib = ApertureLibrary()
    lib.keywords_flat["K-1"] = "Nature"
    lib.keywords_flat["K-2"] = "Landscape"
    obj = _obj("V", "version", None, {"keywords": ["K-1", "K-2"]})
    result = resolve_field(lib, obj, "keywords")
    assert result == ["Nature", "Landscape"]


def test_resolve_keywords_fallback_uuid():
    lib = ApertureLibrary()
    obj = _obj("V", "version", None, {"keywords": ["K-MISSING"]})
    result = resolve_field(lib, obj, "keywords")
    assert result == ["uuid:K-MISSING"]