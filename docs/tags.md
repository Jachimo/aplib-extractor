# Notes on Keywords/Tags

## Background

This document uses Keywords and Tags interchangeably for user-assigned labels.

Aperture supported hierarchical tags such as `Location/Europe/France/Paris`.
When migrating to digiKam, keyword interoperability matters because album/project
structure may not map directly.

There is no universal XMP convention for keyword storage across applications:
some tools use ordered lists, some unordered lists, and hierarchy separators vary.

## Scope and Confidence

This file is intentionally source-backed.

- Source-backed means behavior confirmed from digiKam/Exiv2 code or official docs.
- Operational observation means behavior seen in live logs and DB checks during this project.

Where possible, claims are tagged with references.

## digiKam Tag Mapping (Source-Backed)

### Hierarchical tag semantics

- digiKam metadata settings treat Tags as hierarchical and recommend XMP-first interoperability [D1].
- Default mapping includes `Xmp.digiKam.TagsList` with tag mode `TAGPATH`, separator `/`, storage `TAG_XMPSEQ` [S1].
- Additional mappings include `Xmp.lr.hierarchicalSubject` using `|` and flat `Xmp.dc.subject` interoperability mappings [S1].

### Read/write container behavior

- `TAG_XMPSEQ` mappings are read via `getXmpTagStringSeq(...)` [S2].
- `TAG_XMPBAG` mappings are read via `getXmpTagStringBag(...)` [S2].
- `TAG_XMPSEQ` mappings are written via `setXmpTagStringSeq(...)` [S3].
- `TAGPATH` mappings keep `/` as canonical internal hierarchy delimiter and convert only when mapping separators differ [S2][S3].

### Path parser implications

- Tag-path parsing in the tag cache splits directly on `/` (`tagForPath`, `createTag`) [S4].
- There is no dedicated unescape stage for escaped slash forms.

Practical result:

- A literal `/` inside one logical label is interpreted as a hierarchy boundary.
- Escaping slash is not a reliable round-trip representation for digiKam tag paths.

## XMP Sidecar Parse Requirements (Critical)

### Exiv2 sidecar type gate

Exiv2 sidecar detection (`isXmpType`) accepts files that start (after optional XML declaration/BOM) with either:

- `<?xpacket ...` or
- `<x:xmpmeta ...`

This behavior is in Exiv2 `xmpsidecar.cpp` [S5].

Operational consequence observed in this project:

- Sidecars with non-canonical envelope/prefix (for example `ns0:xmpmeta`) can be rejected by Exiv2 as unknown image type.
- digiKam then logs `Cannot load XMP sidecar ... (unknown image type)` because sidecar loading is delegated through Exiv2 [S6], which matched live logs in this migration.

### Why blank XMP pane happens

- digiKam XMP widget returns empty when `DMetadata::hasXmp()` is false [S7].
- `hasXmp()` reflects whether Exiv2-loaded XMP metadata container is non-empty [S8].

If sidecar parse fails, XMP metadata remains unavailable and the XMP pane can appear blank.

## Sidecar Read/Merge and Scan Flow

### Read pipeline

- `MetaEngine::load(file)` reads embedded metadata first, then calls `loadFromSidecarAndMerge(file)` [S6].
- Sidecar merge is gated by `useXMPSidecar4Reading` [S6].
- On successful sidecar load, sidecar XMP replaces file XMP in merge logic (`xmpMetadata() = xmpsidecar->xmpData()`) [S9].

### Scan/rescan gating

- Collection scan compares file mtime/size, and optionally sidecar mtime when sidecar reading and timestamp update are enabled [S10].
- If changed and `rescanImageIfModified` is true, scanner performs full rescan path (`rescanFile`) [S10].
- digiKam tests confirm behavior differs when `rescanImageIfModified` is true vs false [S11].

Operationally this explains why sidecar edits may not populate DB tags until a full metadata rescan path runs.

## Exporter Rules for digiKam Compatibility

### Required output format

1. Write `digiKam:TagsList` as `rdf:Seq` with one full path per `rdf:li`, using `/` between levels [S1][S2][S3].
2. Write `dc:subject` as flat keyword values (interop for non-hierarchical consumers) [S1][S2].
3. Do not emit alternate custom hierarchy delimiters inside `digiKam:TagsList` values.
4. Do not rely on escaped slash within one label; `/` will be parsed as hierarchy separator [S4].
5. Ensure sidecar envelope is Exiv2-detectable (`x:xmpmeta` and/or `xpacket`) [S5].

### Current aplib-extractor implementation

- Keyword interop writing is centralized in `src/xmp.rs` (`write_interop_keywords`).
- Master and version serializers apply resolved keyword sets in `src/master.rs` and `src/version.rs`.
- Export regression test validates DigiKam-facing fields and now also asserts Exiv2-detectable sidecar envelope in `src/bin/dumper/exporter.rs`.

## Notes on DB Storage (Operational)

The following are observed in this project and should be treated as operational facts, not digiKam schema specification:

- Tag assignment checks were validated against `ImageTags` and `Tags` in the active DB.
- Identity linkage checks used `ImageHistory.uuid` for `digiKam:ImageUniqueID` workflows.

These checks are useful for migration verification, but source-level schema references are preferred when documenting long-term contracts.

## References

### digiKam / Exiv2 Source

- [S1] `core/libs/metadataengine/dmetadata/dmetadatasettingscontainer.cpp`
	- Default tag mappings (`Xmp.digiKam.TagsList`, separators, container types).
- [S2] `core/libs/metadataengine/dmetadata/dmetadata_tags.cpp` (read path)
	- `getXmpTagStringSeq`, `getXmpTagStringBag`, separator conversion.
- [S3] `core/libs/metadataengine/dmetadata/dmetadata_tags.cpp` (write path)
	- Sequence write behavior for TAGPATH mappings.
- [S4] `core/libs/database/tags/tagscache.cpp`
	- Tag path parsing/creation split directly on `/`.
- [S5] Exiv2 `src/xmpsidecar.cpp`
	- `isXmpType(...)` sidecar detection (`<?xpacket` or `<x:xmpmeta` after XML declaration/BOM handling).
- [S6] `core/libs/metadataengine/engine/metaengine_fileio.cpp`
	- `MetaEngine::load` and `loadFromSidecarAndMerge`; sidecar load exception path.
- [S7] `core/libs/widgets/metadata/xmpwidget.cpp`
	- `loadFromURL`/`decodeMetadata` require `hasXmp()`.
- [S8] `core/libs/metadataengine/engine/metaengine_xmp.cpp`
	- `MetaEngine::hasXmp()` returns true only when XMP container is non-empty.
- [S9] `core/libs/metadataengine/engine/metaengine_p.cpp`
	- Sidecar merge behavior where sidecar XMP dominates file XMP.
- [S10] `core/libs/database/collection/collectionscanner_scan.cpp`
	- mtime/size + sidecar-mtime modification detection and rescan branch.
- [S11] `core/tests/timestampupdate/timestampupdatetest.cpp`
	- Behavior difference with `rescanImageIfModified` true/false.

### Official Documentation

- [D1] Metadata Settings:
	- https://docs.digikam.org/en/setup_application/metadata_settings.html
- [D2] Metadata settings source text (sidecar and mapping options):
	- https://docs.digikam.org/en/_sources/setup_application/metadata_settings.rst.txt
- [D3] Metadata Synchronizer (files -> database workflow):
	- https://docs.digikam.org/en/maintenance_tools/maintenance_metadata.html
