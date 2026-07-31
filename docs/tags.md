# Notes on Keywords/Tags

## Background

The names "Keywords" and "Tags" are used interchangably in this document to
refer to a set of human-readable words that a user associates with a particular
photo.

Aperture allowed for hierarchial tags, e.g. "Location/Europe/France/Paris".

Many other photo management applications have a similar concept, and they can
be important links between photos taken on a specific project, at a time, or 
in a place, especially when they are exported and information about how they
were categorized in Aperture's Albums is potentially lost.

Unfortunately, there's no broad agreement between different applications what
XMP property should be used to store tags/keywords.

Some applications use ordered lists, some are unordered.
Some allow for hierarchical tags, with defined separator characters, some don't.

## DigiKam Handling (Source-Backed)

This section describes what DigiKam actually parses, based on DigiKam source code and the official manual.

### What DigiKam treats as tag hierarchy

- DigiKam's metadata settings define the Tags category as nested keyword hierarchy under XMP-first behavior [D1].
- In DigiKam defaults, the first tag mapping is `Xmp.digiKam.TagsList` with:
  - tag mode `TAGPATH`
  - separator `/`
  - storage type `TAG_XMPSEQ`
  This is defined in source, not inferred [S1].
- DigiKam also maps other tag namespaces for interoperability, including:
  - `Xmp.lr.hierarchicalSubject` with separator `|` [S1]
  - `Xmp.dc.subject` as flat tags (not hierarchical path mode) [S1]

### How DigiKam reads/writes XMP tag containers

- For mappings configured as `TAG_XMPSEQ`, DigiKam reads with `getXmpTagStringSeq(...)` [S2].
- For mappings configured as `TAG_XMPBAG`, DigiKam reads with `getXmpTagStringBag(...)` [S2].
- When writing an XMP mapping configured as `TAG_XMPSEQ`, DigiKam writes with `setXmpTagStringSeq(...)` [S3].
- For `TAGPATH` mappings, DigiKam keeps `/` as the canonical internal hierarchy delimiter and only applies conversion when a mapping uses a different separator [S2][S3].

### Critical parser behavior (why formatting matters)

- DigiKam's tag database path parser splits tag paths on `/` directly:
  - `tagForPath(...)` uses `path.split('/')` [S4]
  - `createTag(...)` uses `tagPathToCreate.split('/')` [S4]
- There is no dedicated unescape pass for escaped slash forms like `\/` in these path parsers [S4].

Practical consequence:

- If a literal `/` appears inside one intended tag name, DigiKam will interpret it as hierarchy boundary, not as a character in the same segment.
- In other words, slash escaping is not a safe round-trip strategy for DigiKam tag-path ingestion.

### Formatting rules for exporter output

Use these rules when generating sidecar tags intended for DigiKam import:

1. Write `digiKam:TagsList` as `rdf:Seq` of `rdf:li` strings, where each `li` is a full hierarchy path using `/` between levels [S1][S2][S3].
2. Keep `dc:subject` as flat keyword values for interoperability with non-hierarchical consumers [S1][S2].
3. Do not emit tab-delimited, multi-space-delimited, or custom escaped hierarchy syntax inside `digiKam:TagsList`; DigiKam path parsing is slash-based [S2][S4].
4. Treat `/` inside a single logical tag label as non-representable in DigiKam path semantics without changing meaning (it becomes another level) [S4].

### Sidecar ingestion requirements (operational)

- DigiKam must be configured to read sidecars (or synchronize from files to database) for sidecar changes to populate the DB-backed tag tree [D2][D3].
- Manual priority guidance recommends XMP on top for interoperability [D1].

## Export Behavior

Relevant code: Master images see `master.rs`, lines 344-363; and Versions see `version.rs`, lines 203-222.

Keyword tags are written both to:
- `dc:subject` (rdf:Bag): Flat keyword names for compatibility
- `digiKam:TagsList` (rdf:Seq): Full hierarchical paths for digiKam

Example XMP output:

```xml
<!-- Flat keywords for compatibility -->
<dc:subject>
<rdf:Bag>
	<rdf:li>Location</rdf:li>
	<rdf:li>Europe</rdf:li>
	<rdf:li>France</rdf:li>
</rdf:Bag>
</dc:subject>

<!-- Full hierarchical paths for digiKam -->
<digiKam:TagsList>
<rdf:Seq>
	<rdf:li>Location</rdf:li>
	<rdf:li>Location/Europe</rdf:li>
	<rdf:li>Location/Europe/France</rdf:li>
</rdf:Seq>
</digiKam:TagsList>
```

## References

### DigiKam source code (commit 93bcadb8fbdac34852043e587215e32ea3e00376)

- [S1] `core/libs/metadataengine/dmetadata/dmetadatasettingscontainer.cpp` lines 376-430
	- Default tag mappings, including `Xmp.digiKam.TagsList` as `TAG_XMPSEQ` with separator `/`, and `Xmp.lr.hierarchicalSubject` with separator `|`.
- [S2] `core/libs/metadataengine/dmetadata/dmetadata_tags.cpp` lines 59-105
	- Read path behavior: `TAG_XMPSEQ` reads with `getXmpTagStringSeq`, `TAG_XMPBAG` with `getXmpTagStringBag`, separator conversion rules.
- [S3] `core/libs/metadataengine/dmetadata/dmetadata_tags.cpp` lines 254-279
	- Write path behavior: `TAGPATH` mapping + `setXmpTagStringSeq` for sequence-backed mappings.
- [S4] `core/libs/database/tags/tagscache.cpp` lines 603-667
	- Path parsing/creation split directly on `/`.

### DigiKam official docs

- [D1] Metadata Settings manual page:
	- https://docs.digikam.org/en/setup_application/metadata_settings.html
	- Source text with line context: https://docs.digikam.org/en/_sources/setup_application/metadata_settings.rst.txt
	- Relevant points: Tags are nested hierarchy under XMP-first guidance; advanced mappings and priorities.
- [D2] Sidecar behavior in Metadata Settings:
	- https://docs.digikam.org/en/_sources/setup_application/metadata_settings.rst.txt (lines around 67-88)
	- Relevant points: read from sidecar only option and sidecar write modes.
- [D3] Metadata Synchronizer manual page:
	- https://docs.digikam.org/en/maintenance_tools/maintenance_metadata.html
	- Source text: https://docs.digikam.org/en/_sources/maintenance_tools/maintenance_metadata.rst.txt
	- Relevant points: synchronization direction includes files -> database.
