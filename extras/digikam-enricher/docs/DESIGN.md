# Design Document for "digikam-enricher"

## Purpose

The purpose of the "digikam-enricher" utility is to enrich the DigiKam-accessible metadata
in image libraries exported by "aplib-extractor", after export.

### Rationale

The Aperture Library internal database contains a significant amount of metadata for each
image, not all of which is relevant to DigiKam or any subsequent workflow. Rather than copy
*all* data from the Aperture DB to the exported XMP sidecar files, the export utility by
default copies a subset of properties that map closely to DigiKam's basic database fields.

This utility allows a user to selectively copy additional metadata properties from the
Aperture database to the XMP sidecar files of exported images.

---

## 1. Data Model & Identity Matching

### 1.1 Aperture Object Identity

Aperture has a master/version hierarchy:

```
Master (one .apmaster file per imported image)
├── Version-0  (the "original" version — always exists)
├── Version-1  (first edited variant — optional)
├── Version-2  (second edited variant — optional)
└── ...etc
```

Each object has its own UUID:
- **Master UUID** — identifies the source image file (`master.uuid`)
- **Version UUID** — identifies a specific rendering of that master (`version.uuid`)

Every version (including the original) has its own `.apversion` plist file containing:
- Its own UUID
- Its parent `masterUuid`
- Version-specific metadata: rating, keywords, flagged status, color label, IPTC/EXIF
  properties, custom info, adjustment history, etc.

### 1.2 What the Exporter Writes to XMP

The exporter writes these identity fields into XMP sidecars:

**Master sidecars** (`.xmp` next to the master image file):
- `aplib:MasterUUID` — the master UUID
- `aplib:OriginalVersionUUID` — UUID of the original version (Version-0)
- `digiKam:ImageUniqueID` — set to `OriginalVersionUUID` (fallback: `MasterUUID`)
- `tiff:FileName` — master filename
- `xmp:Title` — master display name
- `dc:title` (lang-alt) — master display name
- `photoshop:Headline` — master display name
- `xmp:CreateDate` — creation timestamp (RFC3339)
- `photoshop:DateCreated` — creation timestamp (RFC3339)
- `xmp:ImageDate` — image capture date (RFC3339)
- `exif:DateTimeOriginal` — image capture date (RFC3339)
- `xmp:FileCreateDate` — file creation date (RFC3339)
- `xmp:FileModifyDate` — file modification date (RFC3339)
- `tiff:OriginalFileName` — original filename
- `aplib:ModelID` — Aperture model ID
- `aplib:ProjectUUID` — project UUID
- `aplib:AlternateMaster` — alternate master UUID (RAW+JPEG pairs)
- `aplib:ImportGroupUUID` — import group UUID
- `aplib:DBVersion` — database version
- `aplib:Type` — master type
- `aplib:Subtype` — master subtype
- `aplib:ImagePath` — original image path
- `aplib:IsReference` — whether referenced (not managed)
- `aplib:IsTrulyRaw` — whether RAW file
- `aplib:IsInTrash` — whether in trash
- `aplib:IsMissing` — whether file is missing
- `aplib:IsExternallyEditable` — whether externally editable
- `aplib:FileSize` — file size in bytes
- `aplib:FileVolumeUUID` — volume UUID
- `aplib:ColorSpaceName` — color space name
- `aplib:PixelFormat` — pixel format code
- `aplib:HasFocusPoints` — focus point count
- `aplib:ImageFormat` — image format code
- `aplib:FaceDetectionState` — face detection status
- `aplib:Notes` — notes (debug format)
- `aplib:ColorSpaceDefinition` — base64-encoded color space definition
- `aplib:ApertureLibraryPath` — path to source library
- Keywords (see §1.4)

**Version sidecars** (`.xmp` next to each version image file):
- `xmp:VersionUUID` — the version UUID
- `xmp:VersionFileName` — version export filename
- `tiff:FileName` — version export filename
- `digiKam:ImageUniqueID` — set to version UUID (always)
- `aplib:MasterUUID` — reference to master UUID
- `aplib:MasterFilename` — reference to master filename
- `xmp:Rating` — numeric rating (0-5, Aperture scale)
- `digiKam:PickLabel` — `0` (None) or `2` (Pending), from `is_flagged`
- `digiKam:ColorLabel` — `0-9`, clamped from `colour_label_index`
- `xmp:Label` — string label name (Red, Yellow, Green, Blue, Purple)
- `photoshop:Urgency` — mirrors color label
- `xmp:CreateDate` — creation timestamp (RFC3339)
- `photoshop:DateCreated` — creation timestamp (RFC3339)
- `exif:DateTimeOriginal` — image capture date (RFC3339)
- `dc:title` (lang-alt) — version display name
- `photoshop:Headline` — version display name
- `aplib:VersionNumber` — version number
- `aplib:DBVersion` — database version
- `aplib:DBMinorVersion` — database minor version
- `aplib:IsOriginal` — whether original version
- `aplib:IsEditable` — whether editable
- `aplib:IsHidden` — whether hidden
- `aplib:IsInTrash` — whether in trash
- `aplib:RawMasterUUID` — raw master UUID
- `aplib:NonRawMasterUUID` — non-raw master UUID
- `aplib:TimezoneName` — timezone name
- `aplib:ExportImageChangeDate` — export image change date
- `aplib:ExportMetadataChangeDate` — export metadata change date
- `aplib:ApertureLibraryPath` — path to source library
- `aplib:CameraTimeZoneName` — camera timezone (from customInfo)
- `aplib:PictureTimeZoneName` — picture timezone (from customInfo)
- EXIF fields (mapped via `src/exif.rs`)
- IPTC fields (mapped via `src/iptc.rs`)
- Keywords (see §1.4)

### 1.3 Matching Strategy

**We must match at the VERSION level**, not the master level, because:
- A master may have multiple versions with different metadata (e.g., different ratings,
  keywords, or IPTC/EXIF overrides)
- Metadata we want to enrich (e.g., rating, keywords) is stored per-version in Aperture,
  not per-master

**Primary match key**: `digiKam:ImageUniqueID` → Aperture object UUID.

From the exporter:
- Master sidecars: `digiKam:ImageUniqueID` = `OriginalVersionUUID` (fallback: `MasterUUID`)
- Version sidecars: `digiKam:ImageUniqueID` = Version UUID

**Important subtlety — what a master sidecar's `ImageUniqueID` actually points to.**

A master sidecar's `digiKam:ImageUniqueID` is set to the **original version's UUID**
(`aplib:OriginalVersionUUID`), *not* the master's own UUID. This matters because it
determines which Aperture record a lookup by `ImageUniqueID` will find:

- Looking up that ID in the **version index** resolves to the original version's
  `.apversion` record — i.e. **version-level** metadata (rating, keywords, etc.).
- It does **not** resolve to the `.apmaster` record, so it gives you **no
  master-level** metadata (like `imagePath` or `colorSpaceName`).

To get master-level metadata, you must use the sidecar's **`aplib:MasterUUID`** field
as a separate lookup key into the **master index** (the `.apmaster` records).

In short: `ImageUniqueID` → version-level data; `aplib:MasterUUID` → master-level data.
The two fields point at different Aperture objects and must not be conflated.

**Secondary match key**: `aplib:MasterUUID` → master UUID (for master-level fields).

### 1.4 Keyword Serialization

Keywords are written across 5 namespaces for interoperability:
- `dc:subject` (rdf:Bag) — flat keywords (leaf names only)
- `digiKam:TagsList` (rdf:Seq, **ordered**) — hierarchical with `/` separators
- `MicrosoftPhoto:LastKeywordXMP` (rdf:Bag) — hierarchical with `/` separators
- `lr:hierarchicalSubject` (rdf:Bag) — hierarchical with `|` separators
- `mediapro:CatalogSets` (rdf:Bag) — hierarchical with `|` separators

### 1.5 Index Construction

For efficient search across large export trees:

```
Phase 1: Walk export tree, find all .xmp files
Phase 2: Quick scan first 4KB of each .xmp for "aplib:" string
         → Skip files without it (not Aperture-exported)
Phase 3: For matching files, extract digiKam:ImageUniqueID and aplib:MasterUUID
Phase 4: Build two indices:
         - uuid_index:  {ImageUniqueID → (image_path, xmp_path)}
         - master_index: {MasterUUID → [(ImageUniqueID, image_path, xmp_path), ...]}
```

Index construction should use `os.scandir()` for efficient directory walking and regex
scanning (matching the approach already proven in `xmp_parser.py`).

---

## 2. Aperture Data Source

### 2.1 Reading Aperture Metadata

The tool reads Aperture plist files directly using Python's built-in `plistlib`.
No dependency on the Rust exporter binary is required.

**Source files**:
- `Database/Versions/*/*/UUID/Version-N.apversion` — per-version metadata
- `Database/Versions/*/*/UUID/Master.apmaster` — per-master metadata
- `Database/Keywords.plist` — keyword hierarchy (resolve UUIDs → names)

### 2.2 Available Metadata Fields

From the Rust library's parsing (`src/version.rs`, `src/master.rs`, `src/exif.rs`,
`src/iptc.rs`, `src/custominfo.rs`), the following fields are available in Aperture
plists and are candidates for enrichment:

**Version-level fields** (`.apversion`):
| Aperture Field | Type | Description |
|---|---|---|
| `rating` | int (0-5) | User rating (Aperture star scale) |
| `isFlagged` | bool | Pick/reject flag |
| `colourLabelIndex` | int (0-6) | Color label (0=none, 1=Red, 2=Orange, 3=Yellow, 4=Green, 5=Blue, 6=Purple) |
| `keywords` | array of UUID strings | Keyword UUIDs assigned to this version |
| `name` | string | Version display name |
| `rotation` | int (degrees) | Rotation angle |
| `iptcProperties` | dict | IPTC metadata (see `src/iptc.rs` for full mapping) |
| `exifProperties` | dict | EXIF metadata (see `src/exif.rs` for full mapping) |
| `customInfo` | dict | `cameraTimeZoneName`, `pictureTimeZoneName` |
| `isOriginal` | bool | Whether this is the original version |
| `imageDate` | datetime | Image capture date |
| `createDate` | datetime | Version creation date |

**Master-level fields** (`.apmaster`):
| Aperture Field | Type | Description |
|---|---|---|
| `name` | string | Master display name |
| `imagePath` | string | Original image path relative to volume |
| `imageDate` | datetime | Image capture date |
| `pixelFormat` | int | Pixel format code |
| `colorSpaceName` | string | Color space name |
| `hasFocusPoints` | int | Focus point count |
| `faceDetectionState` | int | Face detection status |
| `isTrulyRaw` | bool | Whether this is a RAW file |
| `fileSize` | int | File size in bytes |
| `keywords` | array of UUID strings | Keywords assigned at master level |
| `notes` | array | Notes/comments attached to the master |

**IPTC Properties** (in `iptcProperties` dict):
See `src/iptc.rs` for the full mapping. Key fields include:
- `Caption/Abstract`, `Byline`, `BylineTitle`, `Copyright`, `Credit`
- `Source`, `Headline`, `City`, `Province/State`, `Country/PrimaryLocationName`
- `Keywords`, `Category`, `SupplementalCategories`, `Writer/Editor`
- Creator contact info: `CiAdrCity`, `CiAdrCtry`, `CiAdrExtadr`, etc.

**EXIF Properties** (in `exifProperties` dict):
See `src/exif.rs` for the full mapping. Key fields include:
- `ApertureValue`, `ExposureTime`, `FNumber`, `FocalLength`, `ISOSpeedRatings`
- `Make`, `Model`, `LensModel`, `CameraSerialNumber`
- `DateTimeOriginal`, `DateTimeDigitized`
- `GPSLatitude`, `GPSLongitude`, `GPSAltitude`, etc.

### 2.3 Plist Loading

The tool scans `Database/Versions/` for `.apversion` and `.apmaster` files, building
a dictionary keyed by UUID. Since a library may contain tens of thousands of versions,
loading is done lazily or with a progress indicator.

---

## 2.4 Album Structure Enrichment (Planned Feature)

This section describes a planned feature: copying the Aperture **album/folder
structure** into the exported images' XMP sidecars, so a user can see *where* an
image appears in the Aperture album tree. This is **not yet implemented** — the
work items below are the specification for what needs to be built.

### 2.4.1 Motivation & Requirements

- An image can appear in **more than one** Aperture album, so the album output
  field must be **multi-valued** (an `rdf:Bag` / `rdf:Seq`).
- Every image belongs to **exactly one** Aperture project, so the project output
  field is **single-valued** (a plain string).
- The values should be **paths** through the folder/album tree, e.g.
  `2011/Europe/Flickr`, so the user can reconstruct the image's location in the
  Aperture hierarchy.
- The data must come from the **plist files** (`Database/Albums/*.apalbum` and
  `Database/Folders/*.apfolder`). **No `Library.apdb` access is required.**

**Two separate fields are written to the sidecar** (see §2.4.6):

| Field | XMP tag | Cardinality | Source |
|---|---|---|---|
| `albums` | `aplib:AlbumPath` | multi-valued (`rdf:Bag`) | subclass-3 album `versionUuids` |
| `project` | `aplib:ProjectPath` | single-valued | `version.projectUuid` → folder path |

### 2.4.2 Data Source (verified against testdata)

**`Database/Albums/*.apalbum`** — each album is a plist with:

| Key | Type | Meaning |
|---|---|---|
| `InfoDictionary.albumSubclass` | int | `1` = folder view, `2` = smart album, `3` = user album |
| `InfoDictionary.folderUuid` | string | UUID of the containing folder |
| `InfoDictionary.name` | string | Album display name |
| `versionUuids` (top level) | array of string | Version UUIDs contained in the album (**subclass 3 only**) |

**`Database/Folders/*.apfolder`** — each folder is a plist with:

| Key | Type | Meaning |
|---|---|---|
| `uuid` | string | Folder UUID |
| `name` | string | Folder display name |
| `parentFolderUuid` | string | Parent folder UUID (chain up to the root sentinel, e.g. `AllProjectsItem`) |
| `implicitAlbumUuid` | string | UUID of the subclass-1 album representing this folder's view |

Verified in `testdata/`:
- `x6yNun58SB2sImfCarTJHA.apalbum` — subclass 3, name `Flickr`,
  `versionUuids=['BF6nuoBnTumzoXyexdmXlw']`.
- `a%TX9lmjQVWvuK9u6RNhGQ.apfolder` — name `2011`,
  `parentFolderUuid=AllProjectsItem`.

**Validated against a real library** (`/mnt/photos/TO IMPORT/Aperture Library.aplibrary`,
1253 albums, 2681 folders):

| Subclass | Count | Carries `versionUuids`? |
|---|---|---|
| 1 | 790 | **No** (all 790 empty) |
| 2 | 10 | No (smart albums) |
| 3 | 453 | Yes (447 of 453) |

Folder `folderType` distribution: `{2: 2628, 1: 52, 3: 1}` (type 2 = project,
type 1 = plain folder). Root sentinels observed: `AllProjectsItem` and
`PublishedProjects`.

### 2.4.3 Matching Semantics

Albums reference **version UUIDs** (the `versionUuids` array). This aligns with
the existing identity model:

- Version sidecars carry `digiKam:ImageUniqueID` = version UUID.
- Master sidecars carry `digiKam:ImageUniqueID` = `aplib:OriginalVersionUUID`
  (the original version's UUID).

Therefore the reverse index is keyed by **version UUID**, and a master sidecar
resolves through its original-version UUID to the same index.

### 2.4.4 Album Subclass Handling

| Subclass | Meaning | Contributes paths? |
|---|---|---|
| 1 | Folder / project view | **No** — validated against a real library: all 790 subclass-1 albums have **no** `versionUuids`. They contribute nothing. |
| 2 | Smart album | **No** — backed by a query (`UserQueryInfo`), no `versionUuids`; membership cannot be resolved from plists without evaluating the query. Log a warning and skip. |
| 3 | User album | Yes — contributes `<folder path>/<album name>` for each UUID in `versionUuids` |

**Project membership is handled separately** (see §2.4.9): a version's project is
resolved from its `projectUuid` field, not from subclass-1 albums.

### 2.4.5 Path Construction

Build a `folder_uuid -> name` map from `Database/Folders/*.apfolder`, then resolve
each folder's full path by chaining `parentFolderUuid` up to the root sentinel
(e.g. `AllProjectsItem`). Join components with `/` (mirroring the existing
keyword hierarchical-path convention). Example output value:
`2011/Europe/Flickr`.

### 2.4.6 Implementation Work Items

1. **`aperture.py`**
   - Add `album_paths: dict[str, list[str]]` to `ApertureLibrary`, mapping a
     version UUID to the list of album paths containing it.
   - Add `project_paths: dict[str, str]` to `ApertureLibrary`, mapping a
     version UUID to its project path (see §2.4.9).
   - Add `load_albums(library_path, library)`:
     - Walk `Database/Albums/*.apalbum` and `Database/Folders/*.apfolder`.
     - Build the `folder_uuid -> name` map and resolve folder paths via the
       `parentFolderUuid` chain (stop before the root sentinel; guard against
       cycles).
     - For each subclass-3 album, append `<folder path>/<album name>` to
       `album_paths[uuid]` for every UUID in `versionUuids`.
     - Skip subclass-1 albums (no `versionUuids`) and subclass-2 (smart)
       albums with a warning.
   - Add `load_project_paths(library_path, library)`: for each version, resolve
     its `projectUuid` through the folder tree to a path, storing it in
     `project_paths[version_uuid]`.
   - Call `load_albums()` and `load_project_paths()` from `load_library()`.

2. **`matcher.py`**
   - Add special field paths in `resolve_field()`. `resolve_field` receives
     only the matched `ApertureObject` (`obj`), so derive the version UUID to
     look up as follows:
       - If `obj.obj_type == "version"`: use `obj.uuid`.
       - If `obj.obj_type == "master"`: use
         `obj.metadata.get("originalVersionUuid")` (the master's original
         version UUID, matching the master sidecar's `digiKam:ImageUniqueID`).
         If that key is absent, fall back to `obj.uuid` (the master UUID) —
         which will simply miss unless the master UUID also appears in an
         album's `versionUuids`.
   - `albums` field: return `library.album_paths.get(version_uuid)`; return
     `None` (→ "missing field") when the image is in no album. Written to
     `aplib:AlbumPath` as an `rdf:Bag`.
   - `project` field: return `library.project_paths.get(version_uuid)`; return
     `None` when the version has no resolvable project path. Written to
     `aplib:ProjectPath` as a single string.

3. **`cli.py`**
   - Add `("albums", "[path]", "Aperture album/folder paths (multi-valued)")`
     to `AVAILABLE_FIELDS`.
   - Add `("project", "path", "Aperture project path (single)")` to
     `AVAILABLE_FIELDS`.

4. **`enricher.py`** — no change required. `_write_value` already routes list
   values through `write_list_property`, which uses ExifTool's `+=` append
   operator to build an `rdf:Bag`.

5. **`xmp_config.py`** — no change required. The config generator already
   declares every tag in the `--map` list, so `aplib:AlbumPath` is
   auto-declared as a writable string tag.

6. **Tests**
   - `test_aperture.py`: album index and project-path index construction from
     testdata.
   - `test_matcher.py`: `albums` and `project` field resolution (version and
     master sidecar cases).
   - `test_enricher.py`: end-to-end — verify `aplib:AlbumPath` bag and
     `aplib:ProjectPath` string are written.

### 2.4.7 Usage

```bash
uv run digikam-enricher \
  --aperture-library ~/Pictures/MyLibrary.aplibrary \
  --export-root /mnt/photos/exported \
  --map albums=XMP-aplib:AlbumPath \
  --map project=XMP-aplib:ProjectPath \
  --dry-run
```

### 2.4.8 Known Limitations

- **Smart albums (subclass 2)** cannot be resolved from plists without
  evaluating their query logic; they are skipped.
- The path is derived from the folder/album **names**, which are not guaranteed
  unique across the tree; two distinct albums may yield the same path string.
- Only albums that contain a version contribute a path; an image in no album
  yields no `AlbumPath` value.
- **Project membership** is resolved from `version.projectUuid` (see §2.4.9).
  A version whose `projectUuid` does not resolve to a known folder yields no
  `project` path.

### 2.4.9 Project Membership (Resolved)

**Validated against the real library**: subclass-1 albums never carry
`versionUuids`, so project membership is **not** available from albums. Instead,
every version plist carries a `projectUuid` field (a folder UUID) — confirmed in
3000 sampled versions. Project membership is therefore resolved from
`version.projectUuid`:

1. Read `projectUuid` from each version's `.apversion` plist.
2. Resolve that folder UUID through the `parentFolderUuid` chain to a path
   (same path-construction logic as §2.4.5, stopping before the root sentinel).
3. Store the result in `project_paths[version_uuid]`.

This is a **separate field** from `albums` (user-album membership). An image's
full Aperture location is the union of:

- `albums` — paths of every subclass-3 user album containing the version, and
- `project` — the path of the project the version belongs to.

Both are multi-valued-capable (a version can be in several albums; it has one
project). The `project` field is a single path string.

---

## 3. XMP Output

### 3.1 Writing to Sidecars

The tool adds new properties to existing `.xmp` sidecar files. It does **not** rewrite
or regenerate the entire sidecar — it only adds the requested properties.

**XMP library choice: `PyExifTool`** (Python wrapper around `exiftool`)

Rationale:
- `python-xmp-toolkit` is abandoned (last updated ~2017)
- Exempi Python bindings are poorly maintained
- `lxml` is not safe for XMP modification (see §3.2 below)
- `PyExifTool` is actively maintained and wraps `exiftool`, which uses Exiv2
  internally — the same library DigiKam uses for XMP reading
- `fix-image-uuid` (an existing tool in this workspace) already uses `PyExifTool`
  successfully for XMP modification

**Performance**: A single persistent `exiftool` process is reused for the whole run,
as demonstrated in `fix-image-uuid.py`. This avoids per-file startup costs.

### 3.2 Critical: XMP Namespace Prefix Requirements

**This is a known pitfall that the workspace has already hit and solved.**

DigiKam uses Exiv2 internally for XMP parsing. Exiv2's sidecar detection is **strict**
about the XMP packet wrapper format. The key requirement:

> An XMP sidecar must begin with `x:xmpmeta` or `xpacket` (after optional XML declaration).
> `ns0:xmpmeta` is rejected as an unknown image type in DigiKam/Exiv2.

This means the `x` prefix **must** be mapped to the Adobe XMP meta namespace
(`adobe:ns:meta/`) in the XMP packet envelope:

```xml
<?xpacket begin="" id="W5M0MpCehiHzreSzNTczkc9d"?>
<x:xmpmeta xmlns:x="adobe:ns:meta/">
  <rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#">
    ...
```

If `lxml` or any generic XML parser re-serializes the XMP and substitutes `x`
with `ns0` or any other auto-generated prefix, DigiKam will silently reject the
entire sidecar. This is a **hard compatibility requirement**, not a nice-to-have.

This is why the existing `fix-image-uuid` utility was migrated from a regex-based
approach to `PyExifTool` — ExifTool uses Exiv2 internally and preserves the
correct namespace prefix mapping.

The Rust exporter solves this by explicitly registering the namespace:
```rust
// src/xmp.rs
pub const NS_XMPMETA: &str = "adobe:ns:meta/";
exempi2::register_namespace(ns::NS_XMPMETA, "x");
```

**Our approach**: Use `PyExifTool` for all XMP writes. ExifTool handles namespace
registration internally and always writes `x:xmpmeta`.

### 3.2a ExifTool Requires Declared Tags/Namespaces

A second, equally important constraint surfaced during implementation: **ExifTool
refuses to write XMP tags that are not declared in its tag database**, even when
the namespace is registered. For example:

```
$ exiftool -overwrite_original "-XMP-aplib:HasFocusPoints=5" photo.xmp
Warning: Tag 'XMP-aplib:HasFocusPoints' is not defined
Nothing to do.
```

This affects the `aplib:` namespace (and any custom namespace) because ExifTool
does not ship definitions for it. The exporter's own tags like
`XMP-digiKam:ImageUniqueID`, `PickLabel`, `ColorLabel`, and `TagsList` *are*
defined in ExifTool, but arbitrary `aplib:` fields and custom digiKam tags are
not.

**Solution**: The tool generates a small ExifTool config file (via
`xmp_config.py`) that declares exactly the namespaces and tag names targeted by
the current run's `--map` options, then passes it to ExifTool with `-config`.
The config registers each namespace under `Image::ExifTool::XMP::Main` and
declares each requested property as a writable string tag. This is deterministic,
reusable, and keeps the XMP packet format correct.

The generated config is written to `<export-root>/.digikam-enricher.config` and
is only created in non-dry-run mode (dry-run never touches the filesystem).

### 3.3 Namespace Design for New Properties

Custom properties will be written under the `aplib:` namespace
(`http://github.com/Jachimo/aplib-extractor/aplib/1.0/`), which is already registered
in the exporter's sidecars.

Using ExifTool's tag syntax, the user specifies the property name, e.g.:
```
--map rating=XMP-digiKam:xmpRating
--map "iptcProperties.Caption/Abstract=XMP-dc:description"
--map "exifProperties.Make=XMP-exifEX:LensModel"
```

Where:
- Left side: Aperture field path (dot-separated for nested dicts)
- Right side: ExifTool XMP tag name (`XMP-ns:PropertyName`)

`PyExifTool` translates these directly to ExifTool command-line arguments:
```
-XMP-digiKam:xmpRating=3
-XMP-dc:description=Some caption text
```

### 3.4 Value Conversion

Values are converted from Aperture's plist representation to XMP-appropriate types,
then serialized as strings for ExifTool. ExifTool handles the XMP type coercion:

| Aperture Type | XMP Type | Conversion before ExifTool |
|---|---|---|
| `int` | XMP Integer | `str(value)` |
| `bool` | XMP Boolean | `"True"` / `"False"` |
| `float` | XMP Real | `str(value)` |
| `string` | XMP Text | Direct |
| `datetime` | XMP Date | ISO 8601 string |
| `array` of strings | XMP Bag | Comma-separated or one `-tag+=value` per item |
| `array` of UUIDs (keywords) | XMP Bag | Resolved to names via Keywords.plist |
| `dict` | XMP Struct | Not directly supported; map individual sub-fields |

For array types (like keyword lists), ExifTool's `+=` (add to list) and `-=`
operators are used to avoid overwriting existing values.

---

## 4. CLI Design

### 4.1 Basic Invocation

```bash
uv run digikam-enricher \
  --aperture-library ~/Pictures/MyLibrary.aplibrary \
  --export-root /mnt/photos/exported \
  --map rating=XMP-digiKam:xmpRating \
  --map "iptcProperties.Caption/Abstract=XMP-dc:description" \
  --dry-run
```

### 4.2 Arguments

| Argument | Required | Description |
|---|---|---|
| `--aperture-library PATH` | Yes | Path to `.aplibrary` bundle |
| `--export-root PATH` | Yes | Root of exported image tree |
| `--map APERTURE_FIELD=XMP_DEST` | Yes (repeatable) | Field mapping (see below) |
| `--dry-run` | No | Simulate without writing |
| `--verbose` / `-v` | No | Increase log verbosity |
| `--quiet` / `-q` | No | Suppress non-error output |
| `--limit N` | No | Process at most N files (for testing) |
| `--resume CHECKPOINT_FILE` | No | Resume from a previous interrupted run |
| `--match MODE` | No | Matching mode: `version` (default) or `master` |
| `--list-fields` | No | List all available Aperture fields and exit |

### 4.3 Field Mapping Syntax

```
APERTURE_FIELD = XMP_NAMESPACE:PROPERTY_NAME

APERTURE_FIELD:
  - Dotted path into Aperture plist dicts, e.g.:
    - "rating"
    - "iptcProperties.Caption/Abstract"
    - "exifProperties.ISOSpeedRatings"
    - "keywords"  (special: resolves UUIDs to names)

XMP_NAMESPACE:
  - ExifTool XMP tag prefix, e.g.:
    - "XMP-dc", "XMP-digiKam", "XMP-xmp", "XMP-photoshop", "XMP-exif",
      "XMP-exifEX", "XMP-tiff", "XMP-lr", "XMP-MicrosoftPhoto",
      "XMP-mediapro", "XMP-aplib"
  - Or a custom namespace registered via --namespace

PROPERTY_NAME:
  - The XMP property name within that namespace
  - For struct fields: "ParentField/ChildField"

Examples:
  --map rating=XMP-digiKam:xmpRating
  --map "iptcProperties.Caption/Abstract=XMP-dc:description"
  --map "exifProperties.LensModel=XMP-aux:Lens"
  --map keywords=XMP-dc:subject
```

### 4.4 `--list-fields` Output

```
Available Aperture fields:
  rating                     int       Version rating (0-5, Aperture scale)
  isFlagged                  bool      Pick/reject flag
  colourLabelIndex           int       Color label (0-6)
  keywords                   [uuid]    Keyword UUIDs → resolved names
  name                       str       Version display name
  rotation                   int       Rotation in degrees
  isOriginal                 bool      True if this is the original version
  imageDate                  datetime  Image capture date
  iptcProperties.<field>     varies    See IPTC fields below
  exifProperties.<field>     varies    See EXIF fields below
  customInfo.<field>         str       cameraTimeZoneName, pictureTimeZoneName
  ...
```

---

## 5. Processing Pipeline

### 5.1 High-Level Flow

```
  ┌──────────────────┐
  │ 1. Load Aperture │    Parse all .apversion/.apmaster plists
  │    metadata      │    Build uuid → metadata dict
  └────────┬─────────┘
           │
  ┌────────▼─────────┐
  │ 2. Index export  │    Walk export tree, find XMPs with aplib: namespace
  │    tree          │    Build uuid → (image_path, xmp_path) index
  └────────┬─────────┘
           │
  ┌────────▼─────────┐
  │ 3. Match &       │    For each export item, look up Aperture metadata
  │    validate      │    by ImageUniqueID → version UUID
  └────────┬─────────┘
           │
  ┌────────▼─────────┐
  │ 4. Enrich XMP    │    For each matched item with available metadata,
  │    sidecars      │    add requested properties to .xmp file
  └────────┬─────────┘
           │
  ┌────────▼─────────┐
  │ 5. Report        │    Summary: enriched / skipped / unmatched / errors
  └──────────────────┘
```

### 5.2 Phase Details

#### Phase 1: Load Aperture Metadata

```
For each .apversion file in Database/Versions/*/*/UUID/:
    Parse with plistlib
    Extract version.uuid → version metadata dict
For each .apmaster file in Database/Versions/*/*/UUID/:
    Parse with plistlib
    Extract master.uuid → master metadata dict
For Keywords.plist:
    Build UUID → keyword name map
```

Performance: For ~100k versions, this takes a few seconds. Use progress bar via `tqdm`.

#### Phase 2: Index Export Tree

```
Walk export_root with os.scandir()
For each .xmp file:
    Read first 4096 bytes
    If "aplib:" not present → skip
    Extract digiKam:ImageUniqueID via regex
    Extract aplib:MasterUUID via regex
    Add to uuid_index and master_index
```

This is fast even for 100k+ files. The `"aplib:"` pre-filter avoids reading full files.

#### Phase 3: Match & Validate

```
For each entry in uuid_index:
    image_unique_id = entry.key
    If image_unique_id in aperture_version_index:
        → Match! Version-specific metadata available.
    Else if image_unique_id in aperture_master_index:
        → Match! Master-level metadata only available.
    Else:
        → Skipped (unmatched). Record reason: "no Aperture metadata found for UUID"
```

When `--match master` is used, the tool additionally looks up the master UUID and
applies master-level metadata to ALL versions of that master. This is useful for
fields like `imagePath` or `colorSpaceName` that are the same across versions.

#### Phase 4: Enrich XMP

For each matched item with the requested field present:
1. Convert Aperture value to XMP-appropriate string (see §3.4)
2. Build the ExifTool tag argument: `-XMP-ns:PropertyName=value`
3. Pass to the persistent `PyExifTool` process
4. ExifTool handles all XMP envelope/packet formatting correctly

For array-type properties (like keywords), use ExifTool's `+=` append operator
to add items without overwriting existing values.

In `--dry-run` mode, log what would be written but don't call ExifTool.

#### Phase 5: Report

```
=== Enrichment Report ===
Total XMP files scanned:     50,234
Aperture-exported files:      8,421
Matched to Aperture data:     7,983
  Masters matched:            2,105
  Versions matched:           5,878
Unmatched:                      438
  No Aperture UUID found:      312
  Metadata field missing:      126
Properties written:          15,966
  rating → digiKam:xmpRating: 7,983
  Caption → dc:description:   7,983
Skipped (dry-run):                0
Errors:                            2
  XMP parse failure:              2
Time elapsed:                  42.3s
```

---

## 6. Project Structure

```
extras/digikam-enricher/
├── README.md
├── pyproject.toml
├── uv.lock
├── .python-version          (3.11+)
├── docs/
│   └── DESIGN.md
├── src/
│   └── digikam_enricher/
│       ├── __init__.py
│       ├── __main__.py      (entry point)
│       ├── cli.py           (argument parsing)
│       ├── aperture.py      (Aperture plist reading)
│       ├── indexer.py       (export tree indexing)
│       ├── matcher.py       (match export items to Aperture)
│       ├── xmp_writer.py    (XMP write via PyExifTool)
│       ├── xmp_config.py    (ExifTool config generation for custom namespaces)
│       ├── enricher.py      (orchestration)
│       └── report.py        (report generation)
└── tests/
    ├── __init__.py
    ├── conftest.py          (shared testdata paths)
    ├── test_cli.py
    ├── test_aperture.py
    ├── test_indexer.py
    ├── test_matcher.py
    ├── test_xmp_writer.py
    ├── test_xmp_config.py
    └── test_enricher.py
```

---

## 7. Dependencies

```toml
[project]
name = "digikam-enricher"
version = "0.1.0"
requires-python = ">=3.11"
dependencies = [
    "PyExifTool>=0.5",
    "tqdm>=4.66",
]

[project.optional-dependencies]
dev = [
    "pytest>=8.0",
    "pytest-cov>=5.0",
]
```

Key choices:
- **`PyExifTool`**: Python wrapper around `exiftool` (Perl-based, uses Exiv2 internally
  for XMP). Required for safe XMP modification — avoids the namespace prefix corruption
  issue (§3.2) that affects generic XML libraries. Already proven in the workspace's
  `fix-image-uuid` tool. Requires `exiftool` to be installed on the system:
  `apt install libimage-exiftool-perl`.
- **`tqdm`**: Progress bars for long-running operations (loading metadata, indexing).
- **`plistlib`**: Built-in Python library for reading Apple plist files. No extra
  dependency needed.
- **No `lxml` for XMP writing**: Unsafe due to the `x:xmpmeta` / `ns0:xmpmeta`
  namespace prefix issue with DigiKam/Exiv2 (§3.2).

---

## 8. Error Handling & Edge Cases

### 8.1 Unmatched Items
Items whose `digiKam:ImageUniqueID` doesn't correspond to any Aperture object:
- Logged with UUID
- Counted in report as "unmatched"
- Not an error — the export tree may contain non-Aperture images

### 8.2 Missing Fields
If the user requests `--map rating=XMP-digiKam:xmpRating` but a specific Aperture
version has no `rating` field:
- Logged at DEBUG level
- Skipped gracefully — no XMP modification for that property on that file
- Counted in report under "metadata field missing"

### 8.3 XMP Write Failures
If `exiftool` fails to write to an `.xmp` file (malformed XMP, permissions, disk
full, etc.):
- `PyExifTool` raises an exception with the ExifTool error message
- Logged as ERROR
- File is skipped
- Counted in report under "errors"
- Does not abort the entire run

### 8.4 File Write Failures
If writing to an `.xmp` file fails (permissions, disk full, etc.):
- Logged as ERROR
- Counted in report under "errors"
- Does not abort the run; continues to next file

### 8.5 Duplicate Property Names
If the property already exists in the XMP:
- By default, skip (don't overwrite)
- `--overwrite` flag to replace existing value

### 8.6 Keyword Resolution
Aperture stores keywords as UUID arrays. The tool resolves these to human-readable
names using `Keywords.plist`. If a keyword UUID cannot be resolved:
- Logged at WARNING level
- The UUID is used as a fallback value (with format `uuid:XXXX-XXXX-...`)

### 8.7 Large Libraries
For libraries with 100k+ images:
- Indexing the export tree should complete in under 30 seconds
- Loading Aperture plists should complete in under 60 seconds
- Both operations show tqdm progress bars
- Memory usage: ~50-100 bytes per indexed item, acceptable for libraries up to
  several hundred thousand images

---

## 9. Resumability & Checkpoints

To support resuming interrupted runs on very large libraries:

```
--resume CHECKPOINT_FILE

Checkpoint file format (JSONL):
{"image_unique_id": "xxx-xxx", "status": "done", "properties_written": ["rating"]}
{"image_unique_id": "yyy-yyy", "status": "skipped", "reason": "no match"}
...
```

When `--resume` is specified, the tool:
1. Loads the checkpoint file
2. Skips any items already marked as `done` or `skipped`
3. Appends new entries as processing continues

This is optional — for most libraries, a full run is fast enough.

---

## 10. Testing Strategy

### 10.1 Unit Tests
- `test_aperture.py`: Test plist parsing with testdata fixtures
- `test_xmp_writer.py`: Test ExifTool tag construction, value conversion logic
  (mock the ExifTool subprocess for unit tests)
- `test_indexer.py`: Test export tree walking with a temp directory
- `test_matcher.py`: Test UUID matching logic

### 10.2 Integration Tests
- `test_enricher.py`: End-to-end test using `testdata/` from the workspace
- Verify that properties are correctly written to sidecars
- Verify `--dry-run` produces expected output without modifying files
- Verify `--resume` correctly skips already-processed items

### 10.3 Test Fixtures
Reuse the existing `testdata/` directory which contains:
- A synthetic Aperture library with known masters and versions
- Already-exported images with `.xmp` sidecars

---

## 11. Future Considerations

### 11.1 ImageProperties DB Enrichment

As researched separately, the `ImageProperties` table in DigiKam's database is a
key-value store accessible via `ItemExtendedProperties`. A future enhancement could
write directly to this table for metadata that doesn't have a natural XMP mapping.

This would require:
- MariaDB/MySQL connection parameters
- Resolution of exported filenames → `Images.id` (as done in `digikam-group-utility`)

### 11.2 Direct SQLite Access to Library.apdb

The Aperture `Library.apdb` SQLite database likely contains additional metadata tables
not available in the plist files. If the schema is reverse-engineered, direct SQLite
access could provide richer data.

### 11.3 Tag/Keyword Hierarchies

Aperture keywords are hierarchical (defined in `Keywords.plist`). Currently the tool
resolves UUIDs to flat names. A future enhancement could write hierarchical tag paths
to `digiKam:TagsList` in the format DigiKam expects.

### 11.4 Batch Mode with Config File

Support a YAML/TOML config file for specifying multiple mappings at once:

```yaml
mappings:
  - aperture_field: rating
    xmp_namespace: digiKam
    xmp_property: xmpRating
  - aperture_field: "iptcProperties.Caption/Abstract"
    xmp_namespace: dc
    xmp_property: description
```
