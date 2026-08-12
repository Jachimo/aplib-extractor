"""Tests for CLI parsing and entrypoint (cli.py)."""

from __future__ import annotations

from pathlib import Path

import pytest

from digikam_enricher.cli import build_parser

from .conftest import AHTML


def test_list_fields_exit(tmp_path, capsys):
    from digikam_enricher import cli

    rc = cli.main(
        [
            "--aperture-library",
            str(AHTML),
            "--export-root",
            str(tmp_path),
            "--list-fields",
        ]
    )
    assert rc == 0
    out = capsys.readouterr().out
    assert "Available Aperture fields:" in out
    assert "rating" in out


def test_requires_map(tmp_path):
    from digikam_enricher import cli

    with pytest.raises(SystemExit) as exc:
        cli.main(
            [
                "--aperture-library",
                str(AHTML),
                "--export-root",
                str(tmp_path),
            ]
        )
    assert exc.value.code == 2  # argparse error


def test_dry_run_cli(tmp_path):
    from digikam_enricher import cli

    # Build a tiny export tree.
    imgdir = tmp_path / "export" / "2006"
    imgdir.mkdir(parents=True)
    (imgdir / "PICT0019.JPG").write_bytes(b"x")
    xmp = (
        '<x:xmpmeta><rdf:Description xmlns:aplib="http://github.com/Jachimo/aplib-extractor/aplib/1.0/" '
        'aplib:MasterUUID="V6jjzYNdSVu006MPsZkt5w" '
        'digiKam:ImageUniqueID="t58rPT%6SYCIW2ooj%iRCQ"/></x:xmpmeta>'
    )
    (imgdir / "PICT0019.JPG.xmp").write_text(xmp, encoding="utf-8")

    rc = cli.main(
        [
            "--aperture-library",
            str(AHTML),
            "--export-root",
            str(tmp_path / "export"),
            "--map",
            "rating=XMP-digiKam:xmpRating",
            "--dry-run",
            "--quiet",
        ]
    )
    assert rc == 0


def test_bad_map_returns_2(tmp_path):
    from digikam_enricher import cli

    rc = cli.main(
        [
            "--aperture-library",
            str(AHTML),
            "--export-root",
            str(tmp_path),
            "--map",
            "notamap",
            "--dry-run",
        ]
    )
    assert rc == 2