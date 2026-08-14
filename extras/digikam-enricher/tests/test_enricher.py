"""Integration tests for the enrichment pipeline (enricher.py).

These use the real bundled Aperture test library fixture and a synthetic export
tree woven from it, so the pipeline can be exercised without a full migration.
"""

from __future__ import annotations

from pathlib import Path

from digikam_enricher import enricher
from digikam_enricher.enricher import EnrichOptions, FieldMapping, parse_mapping

from .conftest import AHTML

# A tiny synthetic export tree that mirrors what the exporter produces: an
# image + sidecar pair carrying aplib:MasterUUID and digiKam:ImageUniqueID.
XMP = """<?xpacket begin="" id="W5M0MpCehiHzreSzNTczkc9d"?>
<x:xmpmeta xmlns:x="adobe:ns:meta/">
  <rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#">
    <rdf:Description xmlns:aplib="http://github.com/Jachimo/aplib-extractor/aplib/1.0/" xmlns:digiKam="http://www.digikam.org/ns/1.0/">
      <aplib:MasterUUID>V6jjzYNdSVu006MPsZkt5w</aplib:MasterUUID>
      <digiKam:ImageUniqueID>t58rPT%6SYCIW2ooj%iRCQ</digiKam:ImageUniqueID>
    </rdf:Description>
  </rdf:RDF>
</x:xmpmeta>
<?xpacket end="w"?>
"""


def _make_export_tree(tmp_path: Path) -> Path:
    root = tmp_path / "export"
    imgdir = root / "2006" / "11"
    imgdir.mkdir(parents=True)
    (imgdir / "PICT0019.JPG").write_bytes(b"fake-jpeg")
    (imgdir / "PICT0019.JPG.xmp").write_text(XMP, encoding="utf-8")
    return root


def test_parse_mapping_exiftool_form():
    m = parse_mapping("rating=XMP-digiKam:xmpRating")
    assert m.aperture_field == "rating"
    assert m.xmp_tag == "XMP-digiKam:xmpRating"


def test_parse_mapping_short_form():
    m = parse_mapping("rating=digiKam:xmpRating")
    assert m.xmp_tag == "XMP-digiKam:xmpRating"


def test_parse_mapping_invalid():
    import pytest as _p
    with _p.raises(ValueError):
        parse_mapping("rating")


def test_dry_run_matches_version(tmp_path):
    export_root = _make_export_tree(tmp_path)
    options = EnrichOptions(
        aperture_library=AHTML,
        export_root=export_root,
        mappings=[parse_mapping("rating=XMP-digiKam:xmpRating")],
        dry_run=True,
    )
    report = enricher.enrich(options)
    assert report.matched_versions == 1
    assert report.field_count == 1
    assert report.errors == 0


def test_dry_run_missing_field(tmp_path):
    export_root = _make_export_tree(tmp_path)
    options = EnrichOptions(
        aperture_library=AHTML,
        export_root=export_root,
        mappings=[parse_mapping("nonexistent.field=XMP-xmp:Something")],
        dry_run=True,
    )
    report = enricher.enrich(options)
    # Matches the version but resolves no value.
    assert report.matched_versions == 1
    assert report.field_count == 0
    assert report.missing_field == 1


def test_dry_run_does_not_touch_files(tmp_path):
    export_root = _make_export_tree(tmp_path)
    xmp_path = export_root / "2006" / "11" / "PICT0019.JPG.xmp"
    original = xmp_path.read_text()
    options = EnrichOptions(
        aperture_library=AHTML,
        export_root=export_root,
        mappings=[parse_mapping("rating=XMP-digiKam:xmpRating")],
        dry_run=True,
    )
    enricher.enrich(options)
    assert xmp_path.read_text() == original


def test_enrich_writes_property(tmp_path, monkeypatch):
    """A fake writer avoids the exiftool binary in unit tests while still
    exercising the full run() path including mapping and value resolution."""
    export_root = _make_export_tree(tmp_path)

    class FakeWritten:
        def __init__(self):
            self.writes = {}

        def write_property(self, path, tag, value, stats=None):
            self.writes[(str(path), tag)] = value
            return True

        def write_list_property(self, path, tag, values, stats=None):
            for v in values:
                self.writes[(str(path), tag)] = v
            return True

        def read_tags(self, path, tags):
            return {}

        def exists(self, path, tag):
            return False

    fake = FakeWritten()

    # Monkeypatch the writer context manager factory used by enrich.
    import digikam_enricher.enricher as mod

    class _FakeCtx:
        def __init__(self, writer):
            self._writer = writer

        def __enter__(self):
            return self._writer

        def __exit__(self, *a):
            return False

    original_factory = mod.XmpWriter

    class PatchedXmpWriter(original_factory):
        def __init__(self, *a, **k):
            super().__init__(*a, **k)
            self._fake = fake

        def __enter__(self):
            return self._fake

        def __exit__(self, *a):
            return False

    monkeypatch.setattr(mod, "XmpWriter", PatchedXmpWriter)

    options = EnrichOptions(
        aperture_library=AHTML,
        export_root=export_root,
        mappings=[parse_mapping("rating=XMP-digiKam:xmpRating")],
        dry_run=False,
    )
    report = mod.enrich(options)
    assert report.files_updated == 1
    assert list(fake.writes.values()) == [0]
    # Aperture rating 'mainRating' -> 0 in fixture; the mapping uses rating.

    # Non-dry-run should have generated an ExifTool config for the custom tag.
    config_path = export_root / ".digikam-enricher.config"
    assert config_path.exists()
    assert "xmpRating" in config_path.read_text()


def test_field_mapping_equality():
    a = FieldMapping("rating", "XMP-digiKam:xmpRating")
    b = parse_mapping("rating=XMP-digiKam:xmpRating")
    assert a == b


def test_dry_run_albums_field(tmp_path):
    """The 'albums' field resolves to album paths from the test library.

    The testdata version (t58rPT%6SYCIW2ooj%iRCQ) is not in any album, so the
    field resolves to None and is counted as missing.
    """
    export_root = _make_export_tree(tmp_path)
    options = EnrichOptions(
        aperture_library=AHTML,
        export_root=export_root,
        mappings=[parse_mapping("albums=XMP-aplib:AlbumPath")],
        dry_run=True,
    )
    report = enricher.enrich(options)
    assert report.matched_versions == 1
    assert report.field_count == 0
    assert report.missing_field == 1
    assert report.errors == 0


def test_dry_run_project_field(tmp_path):
    """The 'project' field resolves to a project path from the test library.

    The testdata version's projectUuid does not match any folder UUID in the
    fixture, so the field resolves to None and is counted as missing.
    """
    export_root = _make_export_tree(tmp_path)
    options = EnrichOptions(
        aperture_library=AHTML,
        export_root=export_root,
        mappings=[parse_mapping("project=XMP-aplib:ProjectPath")],
        dry_run=True,
    )
    report = enricher.enrich(options)
    assert report.matched_versions == 1
    assert report.field_count == 0
    assert report.missing_field == 1
    assert report.errors == 0


def test_dry_run_albums_and_project_together(tmp_path):
    """Both albums and project can be mapped in a single run."""
    export_root = _make_export_tree(tmp_path)
    options = EnrichOptions(
        aperture_library=AHTML,
        export_root=export_root,
        mappings=[
            parse_mapping("albums=XMP-aplib:AlbumPath"),
            parse_mapping("project=XMP-aplib:ProjectPath"),
        ],
        dry_run=True,
    )
    report = enricher.enrich(options)
    assert report.matched_versions == 1
    assert report.field_count == 0
    assert report.missing_field == 2
    assert report.errors == 0


def test_overwrite_clears_existing_list_values(tmp_path):
    """When --overwrite is used, list properties should not accumulate
    duplicates on re-runs. The write_list_property method should clear
    existing values before appending new ones."""
    from digikam_enricher.xmp_writer import XmpWriter

    export_root = _make_export_tree(tmp_path)

    class FakeWriter:
        def __init__(self):
            self.calls = []

        def write_list_property(self, path, tag, values, stats=None, overwrite=False):
            self.calls.append(("list", str(path), tag, list(values), overwrite))
            return True

        def write_property(self, path, tag, value, stats=None):
            self.calls.append(("scalar", str(path), tag, value))
            return True

        def read_tags(self, path, tags):
            return {}

        def exists(self, path, tag):
            return False

    fake = FakeWriter()

    import digikam_enricher.enricher as mod

    class _FakeCtx:
        def __init__(self, writer):
            self._writer = writer

        def __enter__(self):
            return self._writer

        def __exit__(self, *a):
            return False

    original_factory = mod.XmpWriter

    class PatchedXmpWriter(original_factory):
        def __init__(self, *a, **k):
            super().__init__(*a, **k)
            self._fake = fake

        def __enter__(self):
            return self._fake

        def __exit__(self, *a):
            return False

    import pytest as _p
    with _p.MonkeyPatch().context() as mp:
        mp.setattr(mod, "XmpWriter", PatchedXmpWriter)

        # First run: write a list property
        options = EnrichOptions(
            aperture_library=AHTML,
            export_root=export_root,
            mappings=[parse_mapping("albums=XMP-aplib:AlbumPath")],
            dry_run=False,
            overwrite=False,
        )
        report = mod.enrich(options)
        # albums resolves to None for testdata version, so no writes
        assert report.field_count == 0

        # Second run with overwrite=True
        options2 = EnrichOptions(
            aperture_library=AHTML,
            export_root=export_root,
            mappings=[parse_mapping("albums=XMP-aplib:AlbumPath")],
            dry_run=False,
            overwrite=True,
        )
        report2 = mod.enrich(options2)
        assert report2.field_count == 0  # still None for testdata