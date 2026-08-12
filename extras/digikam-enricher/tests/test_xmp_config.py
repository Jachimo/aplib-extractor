"""Tests for ExifTool config generation (xmp_config.py)."""

from __future__ import annotations

from digikam_enricher.xmp_config import make_namespace_config, XmpConfigWriter


def test_make_config_declares_aplib_namespace():
    cfg = make_namespace_config(["XMP-aplib:MainRating", "XMP-aplib:FocalLength"])
    assert "Image::ExifTool::UserDefined::aplib" in cfg
    assert "MainRating" in cfg
    assert "FocalLength" in cfg
    assert "http://github.com/Jachimo/aplib-extractor/aplib/1.0/" in cfg


def test_make_config_registers_main_entry():
    cfg = make_namespace_config(["XMP-aplib:MainRating"])
    assert "'Image::ExifTool::XMP::Main'" in cfg
    assert "aplib => {" in cfg
    assert "SubDirectory" in cfg


def test_make_config_multiple_namespaces():
    cfg = make_namespace_config(
        ["XMP-aplib:MainRating", "XMP-custom:MyField"],
        extra_namespaces={"custom": "http://example.com/custom/"},
    )
    assert "UserDefined::aplib" in cfg
    assert "UserDefined::custom" in cfg
    assert "XMP-custom" in cfg


def test_make_config_empty():
    assert make_namespace_config([]) == ""


def test_xmp_config_writer_roundtrip(tmp_path):
    writer = XmpConfigWriter(tmp_path / "cfg" / "x.config", ["XMP-aplib:MainRating"])
    writer.write()
    assert (tmp_path / "cfg" / "x.config").exists()
    assert "MainRating" in (tmp_path / "cfg" / "x.config").read_text()
    assert writer.arg() == ["-config", str(tmp_path / "cfg" / "x.config")]


def test_builtin_namespace_handled():
    # Standard namespaces also get declared (idempotently) so unknown tags in
    # them can be written too.
    cfg = make_namespace_config(["XMP-dc:MyCustom"])
    assert "UserDefined::dc" in cfg
    assert "MyCustom" in cfg