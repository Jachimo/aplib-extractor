# digikam-enricher

Selectively copy metadata fields from an Aperture library into the XMP sidecars
of images exported by [aplib-extractor](https://github.com/Jachimo/aplib-extractor).

After migrating an Aperture library to DigiKam, you may want to bring over
additional metadata that the exporter did not copy by default. This tool lets
you choose, per field, which Aperture property to read and which XMP tag to
write it to in the corresponding sidecar.

## Requirements

- Python 3.11+
- [uv](https://docs.astral.sh/uv/) for environment/dependency management
- `exiftool` on the system (`apt install libimage-exiftool-perl`)

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
  --map "customInfo.cameraTimeZoneName=XMP-photoshop:CameraTimeZone" \
  --dry-run
```

### Options

| Option | Description |
|---|---|
| `--aperture-library PATH` | Path to the `.aplibrary` bundle (required) |
| `--export-root PATH` | Root of the exported image tree (required) |
| `--map APERTURE_FIELD=XMP_NAMESPACE:PROPERTY` | Field mapping (repeatable) |
| `--dry-run` | Simulate without writing any files |
| `--overwrite` | Overwrite XMP properties that already exist (default: skip) |
| `--prefer-master` / `--match master` | Match using `aplib:MasterUUID` and apply master-level metadata |
| `--limit N` | Process at most N XMP files |
| `--list-fields` | List available Aperture fields and exit |
| `--verbose` / `-v` | Verbose logging |
| `--quiet` / `-q` | Suppress non-error output |

### Field Mapping Syntax

```
APERTURE_FIELD = XMP_NAMESPACE:PROPERTY
```

- `APERTURE_FIELD` is a dotted path into the Aperture plist (e.g. `rating`,
  `exifProperties.FocalLength`, `customInfo.cameraTimeZoneName`). The special
  `keywords` field resolves keyword UUIDs to human-readable names.
- `XMP_NAMESPACE:PROPERTY` is the destination XMP tag. Both `XMP-ns:Prop` and
  `ns:Prop` forms are accepted.

Examples:

```bash
# Copy the Aperture rating into a custom digiKam tag.
--map rating=XMP-digiKam:xmpRating

# Copy the camera timezone into the photoshop namespace.
--map "customInfo.cameraTimeZoneName=XMP-photoshop:CameraTimeZone"

# Copy an EXIF-derived field into the aplib provenance namespace.
--map "exifProperties.FocalLength=XMP-aplib:FocalLength"
```

## How It Works

1. **Load Aperture metadata** — parses `.apversion` / `.apmaster` plists and
   `Keywords.plist` from the library bundle.
2. **Index the export tree** — walks the export root, finds `.xmp` sidecars
   carrying the `aplib:` marker, and extracts `digiKam:ImageUniqueID` and
   `aplib:MasterUUID`.
3. **Match** — resolves each sidecar to an Aperture version (or master) via
   `digiKam:ImageUniqueID`.
4. **Enrich** — for each requested mapping whose Aperture field resolves, writes
   the value into the sidecar via a persistent ExifTool process.
5. **Report** — prints a summary of matched/unmatched/written/errors.

### Matching

Matching is at the **version** level by default, because a master may have
multiple versions with different metadata. `digiKam:ImageUniqueID` on a version
sidecar is the version UUID; on a master sidecar it is the original version UUID
(fallback: master UUID). Use `--match master` to apply master-level metadata to
all versions of a master.

### XMP Safety

XMP is written via ExifTool (which uses Exiv2 internally — the same library
DigiKam uses). This preserves the strict `x:xmpmeta` envelope that DigiKam
requires. Because ExifTool refuses to write undeclared tags/namespaces, the tool
generates a small ExifTool config file declaring the target namespaces and tags
(see `xmp_config.py`). The config is written to
`<export-root>/.digikam-enricher.config` only in non-dry-run mode.

## Testing

```bash
uv run python -m pytest -q
```

The test suite uses the synthetic Aperture library in the repository's
`testdata/` directory and builds small synthetic export trees to exercise the
pipeline without a full migration.

## Notes

- Dry-run mode never touches the filesystem.
- Non-Aperture images in the export tree are skipped cheaply via a marker scan.
- Unmatched items and missing fields are reported, not fatal.
