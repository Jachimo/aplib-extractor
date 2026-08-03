import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))

from fix_image_uuid import process_sidecar, _has_image_unique_id


def test_has_image_unique_id_detects_marker(tmp_path: Path) -> None:
    sidecar = tmp_path / "has.xmp"
    sidecar.write_text('<rdf:Description digiKam:ImageUniqueID="abc"/>', encoding="utf-8")
    assert _has_image_unique_id(sidecar) is True

    empty = tmp_path / "no.xmp"
    empty.write_text("<rdf:Description/>", encoding="utf-8")
    assert _has_image_unique_id(empty) is False


def test_process_sidecar_preserves_xpacket_header_when_inserting_uuid(tmp_path: Path) -> None:
    sidecar = tmp_path / "sample.xmp"
    sidecar.write_text(
        """<?xpacket begin="" id="W5M0MpCehiHzreSzNTczkc9d"?>
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
""",
        encoding="utf-8",
    )

    status, source = process_sidecar(sidecar, dry_run=False, verbose=False, rewrite_all=False)

    assert status == "updated"
    assert source == "xmp:VersionUUID"

    output = sidecar.read_text(encoding="utf-8")
    assert output.startswith('<?xpacket begin="" id="W5M0MpCehiHzreSzNTczkc9d"?>')
    assert "<x:xmpmeta xmlns:x=\"adobe:ns:meta/\">" in output
    assert 'digiKam:ImageUniqueID="12345678-1234-1234-1234-1234567890ab"' in output


def test_process_sidecar_repair_all_restores_xpacket_wrapper(tmp_path: Path) -> None:
    sidecar = tmp_path / "broken.xmp"
    sidecar.write_text(
        """<?xml version='1.0' encoding='utf-8'?>
<ns0:xmpmeta xmlns:ns0="adobe:ns:meta/">
 <rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#">
  <rdf:Description rdf:about=""
    xmlns:digiKam="http://www.digikam.org/ns/1.0/"
    digiKam:ImageUniqueID="existing-uuid">
  </rdf:Description>
 </rdf:RDF>
</ns0:xmpmeta>
""",
        encoding="utf-8",
    )

    status, source = process_sidecar(sidecar, dry_run=False, verbose=False, rewrite_all=True)

    assert status == "updated"
    assert source == "rewrite:repair-envelope"

    output = sidecar.read_text(encoding="utf-8")
    assert output.startswith('<?xpacket begin="" id="W5M0MpCehiHzreSzNTczkc9d"?>')
    assert output.rstrip().endswith('<?xpacket end="w"?>')
    assert "<?xml version='1.0' encoding='utf-8'?>" not in output
    assert '<x:xmpmeta xmlns:x="adobe:ns:meta/">' in output
    assert 'ns0:xmpmeta' not in output
