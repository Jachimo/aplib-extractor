# fix-image-uuid

Small helper for XMP sidecars. It fills missing `digiKam:ImageUniqueID` values,
which provide a link between images on disk and rows in the DigiKam database.

Use it when exported sidecars lack this field. (In most cases, exports from `aplib-extractor` already include it.)

## Requirements

- `exiftool` on PATH (used for all XMP reading and writing)
- `pyexiftool` Python package (`pip install pyexiftool`)

## What it does

- Scans a directory tree for `.xmp` files.
- Leaves a sidecar unchanged if `digiKam:ImageUniqueID` is already present.
- Adds `digiKam:ImageUniqueID` when missing, using this order:
  1. `xmp:VersionUUID`
  2. `aplib:OriginalVersionUUID`
  3. `aplib:MasterUUID`
  4. A new UUIDv4

## Performance

The script reuses a single persistent ExifTool process and batches reads across
many files, so ExifTool's process-startup cost is paid once instead of once per
file. This is roughly 15-20x faster than launching ExifTool per file, which
matters for large export trees on slow storage.

## Usage

```bash
python3 extras/fix-image-uuid/fix_image_uuid.py /path/to/export --dry-run --verbose
```

## Exit codes

- `0`: scan completed with no errors.
- `1`: one or more sidecars could not be processed.
- `2`: input path is invalid or exiftool is missing.
