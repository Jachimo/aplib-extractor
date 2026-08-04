import logging
import re

logger = logging.getLogger(__name__)

MASTER_UUID_ELEMENT_RE = re.compile(r"<aplib:MasterUUID>\s*([^<\s][^<]*)\s*</aplib:MasterUUID>")
MASTER_UUID_ATTR_RE = re.compile(r"aplib:MasterUUID=\"([^\"]+)\"|aplib:MasterUUID='([^']+)'")
DIGIKAM_IMAGE_UNIQUE_ID_ELEMENT_RE = re.compile(
    r"<digiKam:ImageUniqueID>\s*([^<\s][^<]*)\s*</digiKam:ImageUniqueID>"
)
DIGIKAM_IMAGE_UNIQUE_ID_ATTR_RE = re.compile(
    r"digiKam:ImageUniqueID=\"([^\"]+)\"|digiKam:ImageUniqueID='([^']+)'"
)
VERSION_FILENAME_ELEMENT_RE = re.compile(
    r"<(?:xmp:)?VersionFileName>\s*([^<\s][^<]*)\s*</(?:xmp:)?VersionFileName>"
)
MASTER_FILENAME_ELEMENT_RE = re.compile(r"<aplib:MasterFilename>\s*([^<\s][^<]*)\s*</aplib:MasterFilename>")
TIFF_FILENAME_ELEMENT_RE = re.compile(r"<tiff:FileName>\s*([^<\s][^<]*)\s*</tiff:FileName>")


def _read_head(xmp_file_path: str, max_bytes: int = 65536) -> str:
    with open(xmp_file_path, "r", encoding="utf-8", errors="ignore") as fh:
        return fh.read(max_bytes)


def extract_master_uuid(xmp_file_path: str) -> str | None:
    """Extract aplib:MasterUUID from an XMP sidecar file.

    Returns None if the field is missing or if the file cannot be read/parsed.
    """
    try:
        # Fast path: in this exporter output, MasterUUID is near the top of the file.
        # Reading a small prefix avoids full XML parsing overhead on large sidecars.
        head = _read_head(xmp_file_path)

        element_match = MASTER_UUID_ELEMENT_RE.search(head)
        if element_match:
            value = element_match.group(1).strip()
            if value:
                return value

        attr_match = MASTER_UUID_ATTR_RE.search(head)
        if attr_match:
            value = (attr_match.group(1) or attr_match.group(2) or "").strip()
            if value:
                return value

        return None
    except OSError:
        logger.warning("Failed reading sidecar: %s", xmp_file_path)
        return None


def extract_candidate_filenames(xmp_file_path: str) -> set[str]:
    """Extract original filename hints from an XMP sidecar.

    These fields are used as fallbacks when the exported filename does not
    match DigiKam's imported filename.
    """
    candidates: set[str] = set()

    try:
        head = _read_head(xmp_file_path)

        for pattern in (
            VERSION_FILENAME_ELEMENT_RE,
            MASTER_FILENAME_ELEMENT_RE,
            TIFF_FILENAME_ELEMENT_RE,
        ):
            match = pattern.search(head)
            if match:
                value = match.group(1).strip()
                if value:
                    candidates.add(value)

        return candidates
    except OSError:
        logger.warning("Failed reading sidecar: %s", xmp_file_path)
        return set()


def extract_digikam_image_unique_id(xmp_file_path: str) -> str | None:
    """Extract digiKam image UUID from an XMP sidecar file.

    Returns None if the field is missing or if the file cannot be read/parsed.
    """
    try:
        head = _read_head(xmp_file_path)

        element_match = DIGIKAM_IMAGE_UNIQUE_ID_ELEMENT_RE.search(head)
        if element_match:
            value = element_match.group(1).strip()
            if value:
                return value

        attr_match = DIGIKAM_IMAGE_UNIQUE_ID_ATTR_RE.search(head)
        if attr_match:
            value = (attr_match.group(1) or attr_match.group(2) or "").strip()
            if value:
                return value

        return None
    except OSError:
        logger.warning("Failed reading sidecar: %s", xmp_file_path)
        return None
