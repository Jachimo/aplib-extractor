"""Tests for Aperture plist loading (aperture.py)."""

from __future__ import annotations

import pytest

from digikam_enricher.aperture import load_library

# The repository test fixture bundle (repo root is parents[3]).
from .conftest import AHTML


@pytest.fixture(scope="module")
def library():
    return load_library(AHTML)


def test_load_library_masters(library):
    assert len(library.masters) >= 1
    master = library.masters["V6jjzYNdSVu006MPsZkt5w"]
    assert master.obj_type == "master"
    assert master.uuid == "V6jjzYNdSVu006MPsZkt5w"


def test_load_library_versions(library):
    assert len(library.versions) >= 2
    v0 = library.versions["t58rPT%6SYCIW2ooj%iRCQ"]
    assert v0.obj_type == "version"
    assert v0.master_uuid == "V6jjzYNdSVu006MPsZkt5w"
    # Version-0 is the original.
    assert v0.metadata.get("isOriginal") is True


def test_load_library_keywords(library):
    assert "KEYWORD-UUID-001" in library.keywords_flat
    assert library.keywords_flat["KEYWORD-UUID-001"] == "Nature"
    # Hierarchical path (children nested under parent).
    assert library.keywords_path["KEYWORD-UUID-002"] == "Nature/Landscape"


def test_original_version_linkage(library):
    # Master carries originalVersionUuid -> V6jjzYNdSVu006MPsZkt5w's original.
    master = library.masters["V6jjzYNdSVu006MPsZkt5w"]
    orig = master.metadata.get("originalVersionUuid")
    assert orig is not None
    # version_by_original maps the original version UUID to either the version
    # or master that owns it.
    assert orig in library.version_by_original


def test_exif_properties_available(library):
    v0 = library.versions["t58rPT%6SYCIW2ooj%iRCQ"]
    exif = v0.metadata.get("exifProperties")
    assert isinstance(exif, dict)
    assert "FocalLength" in exif


def test_custom_info_available(library):
    v0 = library.versions["t58rPT%6SYCIW2ooj%iRCQ"]
    cust = v0.metadata.get("customInfo")
    assert isinstance(cust, dict)
    assert cust.get("cameraTimeZoneName") == "US/Eastern"


def test_master_fields(library):
    master = library.masters["V6jjzYNdSVu006MPsZkt5w"]
    assert master.metadata.get("fileName") == "PICT0019.JPG"
    assert master.metadata.get("imagePath") is not None


def test_album_paths_loaded(library):
    """The subclass-3 'Flickr' album contains version BF6nuoBnTumzoXyexdmXlw."""
    assert "BF6nuoBnTumzoXyexdmXlw" in library.album_paths
    paths = library.album_paths["BF6nuoBnTumzoXyexdmXlw"]
    assert len(paths) == 1
    # The album's folderUuid is 'TopLevelAlbums' (a root sentinel), so the
    # path is just the album name.
    assert paths[0] == "Flickr"


def test_project_paths_loaded(library):
    """The testdata fixture's projectUuid ('1AgVFohpQ02BiLvjtdUCzw') does not
    match any folder UUID in the fixture, so project_paths is empty for this
    minimal library. The mechanism is exercised by test_matcher.py with
    synthetic data."""
    # projectUuid in the fixture doesn't resolve to a folder, so no paths.
    assert len(library.project_paths) == 0


def test_project_paths_resolves_with_matching_folder(tmp_path):
    """When a version's projectUuid matches a folder UUID, the path resolves."""
    from digikam_enricher.aperture import (
        ApertureLibrary,
        ApertureObject,
        load_project_paths,
    )
    from pathlib import Path

    lib = ApertureLibrary()
    lib.versions["V-1"] = ApertureObject(
        uuid="V-1",
        obj_type="version",
        master_uuid="M-1",
        metadata={"projectUuid": "FOLDER-1"},
    )
    # Create a fake folder plist
    folders_dir = tmp_path / "Database" / "Folders"
    folders_dir.mkdir(parents=True)
    folder_plist = folders_dir / "FOLDER-1.apfolder"
    import plistlib
    plistlib.dump(
        {
            "uuid": "FOLDER-1",
            "name": "MyProject",
            "parentFolderUuid": "AllProjectsItem",
        },
        folder_plist.open("wb"),
    )
    load_project_paths(tmp_path, lib)
    assert lib.project_paths["V-1"] == "MyProject"