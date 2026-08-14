# digikam-enricher

Selectively copy metadata fields from an Aperture library into the XMP sidecars
of images exported by [aplib-extractor](https://github.com/Jachimo/aplib-extractor).

## Requirements

- Python 3.11+
- [uv](https://docs.astral.sh/uv/)
- `exiftool` (`apt install libimage-exiftool-perl`)

## Setup

```bash
cd extras/digikam-enricher
uv sync --extra dev
```

## Usage

```bash
uv run digikam-enricher \
  --aperture-library ~/Pictures/MyLibrary.aplibrary \
  --export-root /mnt/photos/exported \
  --map rating=XMP-digiKam:xmpRating \
  --map albums=XMP-aplib:AlbumPath \
  --map project=XMP-aplib:ProjectPath \
  --dry-run
```

### Options

| Option | Description |
|---|---|
| `--aperture-library PATH` | Path to the `.aplibrary` bundle (required) |
| `--export-root PATH` | Root of the exported image tree (required) |
| `--map APERTURE_FIELD=XMP_NAMESPACE:PROPERTY` | Field mapping (repeatable) |
| `--dry-run` | Simulate without writing any files |
| `--overwrite` | Overwrite existing XMP properties (default: skip) |
| `--match master` | Match using `aplib:MasterUUID` for master-level metadata |
| `--limit N` | Process at most N XMP files |
| `--list-fields` | List available Aperture fields and exit |
| `-v` / `-q` | Verbose / quiet logging |

### Field Mapping

```
APERTURE_FIELD = XMP_NAMESPACE:PROPERTY
```

- `APERTURE_FIELD`: dotted path into the Aperture plist (e.g. `rating`,
  `exifProperties.FocalLength`, `customInfo.cameraTimeZoneName`). Special fields:
  `keywords` (resolves UUIDs to names), `albums` (multi-valued album paths),
  `project` (single project path).
- `XMP_NAMESPACE:PROPERTY`: destination XMP tag. Both `XMP-ns:Prop` and
  `ns:Prop` forms are accepted.

## How It Works

1. **Load** — parses `.apversion`/`.apmaster` plists and `Keywords.plist`.
2. **Index** — recursively walks the export tree, finds `.xmp` sidecars with the
   `aplib:` marker, extracts `digiKam:ImageUniqueID` and `aplib:MasterUUID`.
3. **Match** — resolves each sidecar to an Aperture version (or master) by UUID.
4. **Enrich** — writes requested fields to sidecars via a persistent ExifTool
   process.
5. **Report** — prints a summary of matched/unmatched/written/errors.

## Testing

```bash
uv run python -m pytest -q
```

Tests use the synthetic Aperture library in `testdata/` and small synthetic
export trees.
