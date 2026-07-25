# Aperture Library Test Data

This directory contains a synthetic Aperture Library bundle used for testing the aplib-extractor tool.

## Structure

`TestLibrary.aplibrary/` is a complete Aperture Library bundle with the following structure:

```
TestLibrary.aplibrary/
├── Info.plist                  # Bundle metadata (version, identifier)
├── Aperture.aplib/
│   ├── DataModelVersion.plist  # Data model version info
│   └── Library.apdb            # SQLite database (minimal schema)
├── Database/
│   ├── apdb/
│   │   ├── Library.apdb        # Main SQLite database
│   │   └── AllProjectsItem.apfolder  # Special projects folder
│   ├── Keywords.plist          # Keyword hierarchy
│   ├── Projects.plist          # Project list
│   ├── Albums/                 # Album metadata (.apalbum files)
│   ├── Folders/                # Folder/Project metadata (.apfolder files)
│   └── Versions/               # Version metadata in date hierarchy
│       └── YYYY/MM/DD/YYYYMMDD-HHMMSS/UUID/
│           ├── Master.apmaster
│           ├── Version-0.apversion
│           └── Version-1.apversion
├── Masters/                    # Actual image files
│   └── YYYY/MM/DD/YYYYMMDD-HHMMSS/
│       └── *.JPG
├── Previews/                   # Preview images (empty in tests)
└── Thumbnails/                 # Thumbnail cache (empty in tests)
```

## Test Dataset

The test library contains:
- **1 master image**: `PICT0019.JPG` from 2006-11-02
- **2 versions**: Original (Version-0) and edited (Version-1)
- **3 albums**: Various album types for testing
- **1 project folder**: Test project structure
- **5 keywords**: Hierarchical keyword structure (Nature, Landscape, Wildlife, Events, Wedding)

## Golden Export Baseline

This fixture is now used by the main migration regression test:

- `exporter::tests::test_export_fixture_library_writes_expected_files_and_digikam_xmp_fields`

That test exports the fixture bundle end-to-end and verifies the current expected contract for this fork:

- emitted file layout for the master and exported non-original version
- XMP sidecar generation for both outputs
- DigiKam-critical serialized XMP fields including title, dates, rating, and pick/color labels
- `aplib:` provenance fields linking the exported version back to its source master

If you intentionally change export naming, sidecar field mapping, or fixture metadata, update the test in [src/bin/dumper/exporter.rs](/home/jtuttle/src/aplib-extractor/src/bin/dumper/exporter.rs) alongside this document.

## Keywords Test Coverage

The Keywords.plist includes examples for testing all keyword formats:
1. **UUID references**: `KEYWORD-UUID-001` through `KEYWORD-UUID-005`
2. **Direct names**: Can be added to version metadata (e.g., "iPhoto Original")
3. **Hierarchical multi-space**: Can be added to test hierarchical parsing (e.g., "Child  Parent")

## SQLite Databases

The `.apdb` files are SQLite 3.x databases with minimal schemas:
- `RKVersion` table (version metadata)
- `RKMaster` table (master metadata)
- `RKVolume` table (volume/storage metadata)

These are sufficient for testing fallback behavior when plist files are missing.

## Regenerating Test Data

If you need to modify the test library:

1. **Add more images**: 
   - Add JPG files to `Masters/YYYY/MM/DD/YYYYMMDD-HHMMSS/`
   - Create corresponding `.apmaster` and `.apversion` files in `Database/Versions/`

2. **Add albums/projects**:
   - Create `.apalbum` files in `Database/Albums/`
   - Create `.apfolder` files in `Database/Folders/`

3. **Modify keywords**:
   - Edit `Database/Keywords.plist` to add/remove keyword hierarchy

## Legacy Structure

The old flat structure (Database/, Masters/ at testdata root) is preserved for backward compatibility but may be removed once all tests are migrated to use TestLibrary.aplibrary.

## Notes

- UUIDs in test data are synthetic but follow Aperture's format (22-char base64-like strings)
- Dates use 2006-11-02 for consistency with the existing test image
- The bundle structure matches real Aperture 3.x libraries as documented in `docs/structure.txt`
- This fixture is intentionally small. It is good for pinning the core export contract, but not sufficient by itself to cover large-library hierarchy, keyword-heavy, or corruption edge cases.
