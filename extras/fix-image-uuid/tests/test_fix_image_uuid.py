import sys
from pathlib import Path

import pytest

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))

import exiftool
from fix_image_uuid import (
    _select_unique_id,
    _read_xmp_fields,
    _write_image_unique_id,
    _decide_action,
)

requires_exiftool = pytest.mark.skipif(
    not exiftool.ExifTool.executable,
    reason="exiftool not installed",
)

SAMPLE_XMP = """<?xpacket begin="" id="W5M0MpCehiHzreSzNTczkc9d"?>
<x:xmpmeta xmlns:x="adobe:ns:meta/">
 <rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#">
  <rdf:Description rdf:about=""
    xmlns:xmp="http://ns.adobe.com/xap/1.0/"
    xmlns:digiKam="http://www.digikam.org/ns/1.0/">
   <xmp:VersionUUID>12345678-1234-1234-1234-1234567890ab</xmp:VersionUUID>
  </rdf:Description>
 </rdf:RDF>
</x:xmpmeta>
<?xpacket end="w"?>
"""


@requires_exiftool
def test_select_unique_id_precedence() -> None:
    # VersionUUID wins
    value, source = _select_unique_id(
        {"VersionUUID": "v1", "OriginalVersionUUID": "o1", "MasterUUID": "m1"}
    )
    assert (value, source) == ("v1", "xmp:VersionUUID")

    # OriginalVersionUUID next
    value, source = _select_unique_id({"OriginalVersionUUID": "o1", "MasterUUID": "m1"})
    assert (value, source) == ("o1", "aplib:OriginalVersionUUID")

    # MasterUUID next
    value, source = _select_unique_id({"MasterUUID": "m1"})
    assert (value, source) == ("m1", "aplib:MasterUUID")

    # Generated fallback
    value, source = _select_unique_id({})
    assert source == "generated:uuid4"
    assert len(value) == 36


def test_decide_action_no_uuid_adds() -> None:
    action, value, source = _decide_action({"VersionUUID": "v1"}, rewrite_all=False)
    assert action == "add-uuid"
    assert value == "v1"
    assert source == "xmp:VersionUUID"


def test_decide_action_has_uuid_no_rewrite_no_change() -> None:
    action, value, source = _decide_action({"ImageUniqueID": "existing"}, rewrite_all=False)
    assert action is None
    assert value is None
    assert source is None


def test_decide_action_has_uuid_rewrite_all_rewrites_packet() -> None:
    action, value, source = _decide_action({"ImageUniqueID": "existing"}, rewrite_all=True)
    assert action == "rewrite-packet"
    assert value is None
    assert source is None


@requires_exiftool
def test_read_and_write_roundtrip(tmp_path: Path) -> None:
    sidecar = tmp_path / "sample.xmp"
    sidecar.write_text(SAMPLE_XMP, encoding="utf-8")

    with exiftool.ExifTool() as et:
        # Write a UUID
        assert _write_image_unique_id(et, sidecar, "test-uuid-123")

        # Read it back
        fields = _read_xmp_fields(et, [sidecar])[0]
        assert fields["ImageUniqueID"] == "test-uuid-123"
        assert fields["VersionUUID"] == "12345678-1234-1234-1234-1234567890ab"
