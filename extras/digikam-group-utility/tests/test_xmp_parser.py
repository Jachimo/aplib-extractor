from pathlib import Path
import sys

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "src"))

from xmp_parser import extract_candidate_filenames, extract_master_uuid


def test_extract_master_uuid_from_element_text(tmp_path):
    xmp = tmp_path / "photo.jpg.xmp"
    xmp.write_text(
        """<?xml version='1.0'?>
<x:xmpmeta xmlns:x='adobe:ns:meta/' xmlns:rdf='http://www.w3.org/1999/02/22-rdf-syntax-ns#' xmlns:aplib='http://github.com/Jachimo/aplib-extractor/aplib/1.0/'>
  <rdf:RDF>
    <rdf:Description>
      <aplib:MasterUUID>abc-123</aplib:MasterUUID>
    </rdf:Description>
  </rdf:RDF>
</x:xmpmeta>
""",
        encoding="utf-8",
    )

    assert extract_master_uuid(str(xmp)) == "abc-123"


def test_extract_master_uuid_from_attribute_fallback(tmp_path):
    xmp = tmp_path / "photo.jpg.xmp"
    xmp.write_text(
        """<?xml version='1.0'?>
<x:xmpmeta xmlns:x='adobe:ns:meta/' xmlns:rdf='http://www.w3.org/1999/02/22-rdf-syntax-ns#' xmlns:aplib='http://github.com/Jachimo/aplib-extractor/aplib/1.0/'>
  <rdf:RDF>
    <rdf:Description aplib:MasterUUID='attr-uuid' />
  </rdf:RDF>
</x:xmpmeta>
""",
        encoding="utf-8",
    )

    assert extract_master_uuid(str(xmp)) == "attr-uuid"


def test_extract_master_uuid_missing_field(tmp_path):
    xmp = tmp_path / "photo.jpg.xmp"
    xmp.write_text("<root />", encoding="utf-8")

    assert extract_master_uuid(str(xmp)) is None


def test_extract_candidate_filenames_from_common_fields(tmp_path):
    xmp = tmp_path / "photo.jpg.xmp"
    xmp.write_text(
        """<?xml version='1.0'?>
<x:xmpmeta xmlns:x='adobe:ns:meta/'
           xmlns:rdf='http://www.w3.org/1999/02/22-rdf-syntax-ns#'
           xmlns:aplib='http://github.com/Jachimo/aplib-extractor/aplib/1.0/'
           xmlns:xmp='http://ns.adobe.com/xap/1.0/'
           xmlns:tiff='http://ns.adobe.com/tiff/1.0/'>
  <rdf:RDF>
    <rdf:Description>
      <xmp:VersionFileName>IMG_0001.JPG</xmp:VersionFileName>
      <aplib:MasterFilename>MASTER_0001.JPG</aplib:MasterFilename>
      <tiff:FileName>TIFF_0001.JPG</tiff:FileName>
    </rdf:Description>
  </rdf:RDF>
</x:xmpmeta>
""",
        encoding="utf-8",
    )

    names = extract_candidate_filenames(str(xmp))
    assert names == {"IMG_0001.JPG", "MASTER_0001.JPG", "TIFF_0001.JPG"}


def test_extract_candidate_filenames_parses_beyond_head_limit(tmp_path):
    xmp = tmp_path / "photo.jpg.xmp"
    xmp.write_text(
        """<?xml version='1.0'?>
<x:xmpmeta xmlns:x='adobe:ns:meta/'
           xmlns:rdf='http://www.w3.org/1999/02/22-rdf-syntax-ns#'
           xmlns:tiff='http://ns.adobe.com/tiff/1.0/'>
  <rdf:RDF>
    <rdf:Description>
"""
        + (" " * 70000)
        + """
      <tiff:FileName>DEEP_0001.JPG</tiff:FileName>
    </rdf:Description>
  </rdf:RDF>
</x:xmpmeta>
""",
        encoding="utf-8",
    )

    names = extract_candidate_filenames(str(xmp))
    assert names == {"DEEP_0001.JPG"}
