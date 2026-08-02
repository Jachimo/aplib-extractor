from pathlib import Path
import sys

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "src"))

from grouper import build_master_uuid_map


def test_build_master_uuid_map_reads_sidecars(tmp_path):
    image = tmp_path / "img.jpg"
    image.write_bytes(b"JPEG")

    sidecar = tmp_path / "img.jpg.xmp"
    sidecar.write_text(
        """<?xml version='1.0'?>
<x:xmpmeta xmlns:x='adobe:ns:meta/' xmlns:rdf='http://www.w3.org/1999/02/22-rdf-syntax-ns#' xmlns:aplib='http://github.com/Jachimo/aplib-extractor/aplib/1.0/'>
  <rdf:RDF>
    <rdf:Description aplib:MasterUUID='master-1' />
  </rdf:RDF>
</x:xmpmeta>
""",
        encoding="utf-8",
    )

    mapping, stats = build_master_uuid_map(str(tmp_path))

    assert "master-1" in mapping
    assert mapping["master-1"] == [str(image)]
    assert stats.sidecars_parsed == 1
