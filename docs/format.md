# Aperture Library Format

## Versions

App version | DB version | DB minor | Project vers
------------+------------+----------+-------------
3.1.3       | 110        | 122      | 6
3.2.2       | 110        | 131      | 7
3.2.4       | 110        | 131      | 7
3.3.2       | 110        | 207 (203)| 8
3.4.5       | 110        | 219 (208)| 8
3.6         | 110        | 226 (220)| 8


## Bundle Structure

Below is a tree view of the first several levels of an
".aplibrary" bundle for database version 110. 

In the tree, the number between `[ ]` is the earliest 
DB minor version where we *saw* the file, if known.

```
LibraryName.aplibrary
|
+- Aperture.aplib
|  +- DataModelVersion.plist
|  +- Library.apdb
|
+- ApertureData.xml [131]
|
+- Attachments
|
+- Database
|  +- ActiveWebPublishingAccounts.plist [207]
|  +- Albums
|  |  +- *.apalbum
|  |  +-
|  |
|  +- apdb
|  |  +- BigBlobs.apdb
|  |  +- Faces.db
|  |
|  +- BigBlobs.apdb -> apdb/BigBlobs.apdb
|  +- DataModelVersion.plist
|  +- Faces
|  |  +- Detected
|  |     +- *.apdetected
|  |  +- DetectedExternals
|  |  +- FaceExternals
|  |  +- FaceNames
|  |
|  +- Faces.db -> apdb/Faces.db
|  +- Folders
|  |  +- *.apfolder
|  |
|  +- History
|  |  +- Changes
|  |     +- *.plist
|  |
|  +- History.apdb -> apdb/History.apdb
|  +- ImageProxies.apdb -> apdb/ImageProxies.apdb
|  +- KeywordSets.plist [207]
|  +- Keywords.plist
|  +- Library.apdb -> apdb/Library.apdb
|  +- Places
|  |  +- *.applace
|  |
|  +- Properties.apdb -> apdb/Properties.apdb
|  +- tmSync.plist
|  +- Vaults
|  +- Versions
|  |  +- YYYY
|  |     +- MM
|  |        +- DD
|  |           +- YYYYMMDD-nnnnnn
|  |              +- <id>
|  |                 +- Master.apmaster
|  |                 +- Version-0.apversion
|  |                 +- Version-1.apversion
|  |
|  +- Volumes
|     +- *.apvolume
|  +- tmSync.plist
|
+- iLifeShared
|  +- ApertureDatabaseTimestamp
|
+- iMovie-Thumbnails (optional?)
|
+- iPod Photo Cache (optional?)
|
+- Info.plist
|
+- Masks
|
+- Masters
|  +- YYYY
|     +- MM
|        +- DD
|           +- YYYYMMDD-nnnnnn
|              +- (Master files, named as imported)
|              +- PICT0001.JPG (example)
|              +- PICT0002.JPG (example)
|              +- ... etc.
|
+- Previews
|  +- YYYY
|     +- MM
|        +- DD
|           +- YYYYMMDD-nnnnnn
|              +- (Directories named by GUID)
|              +- 4L5mJPIrSOuArKFBPG0%2g (example)
|                 +- Orange Flower 3.jpg (example)
|
+- Thumbnails
|  +- (Directories named by GUID)
|     +- AP.Thumbnails
```

## Bundle Contents

This section is not necessarily complete or exhaustive.

### "ApertureData.xml" File

The `ApertureData.xml` file seems to contain a dump of the whole data model but
it seems to not be present everywhere.

### "Info.plist" File

This XML-based plist file identifies the containing bundle (directory) as an Aperture Library.

Example contents for a Library called "Family Photos.aplib":

```
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
	<key>CFBundleGetInfoString</key>
	<string>Aperture Library 3.6</string>
	<key>CFBundleIdentifier</key>
	<string>com.apple.Aperture.library</string>
	<key>CFBundleName</key>
	<string>Family Photos</string>
	<key>CFBundleShortVersionString</key>
	<string>3.6</string>
</dict>
</plist>
```

It's unclear if this file was mainly for operating system use, or for Aperture itself,
or both.


### "Aperture.aplib" Directory

This directory seems to always be present inside the bundle root of the Aperture Library.

#### "DataModelVersion.plist" File

In some cases, `Aperture.aplib` may only contain one file: `DataModelVersion.plist`.

It specifies the datamodel, the version for projects and a few other
details

Properties:

* `DatabaseCompatibleBackToMinorVersion`: DB minor version back compatible. (-226)
* `DatabaseMinorVersion` (integer): DB minor version. See Table 1. (-226)
* `DatabaseVersion` (integer): DB version. See Table 1. (-226)
* `adminProperties` (dict): various properties / settings. (226)
* `createDate` (date): creation date (-226)
* `databaseUuid` (string): UUID (-226)
* `imageIOVersion` (string): ? (!226)
* `isIPhotoLibrary` (bool): false (-226)
* `masterCount` (integer): count of masters (-226)
* `migratedMobileMeAccounts` (array): ? (!226)
* `projectVersion` (integer): project version. See Table 1.
* `projectCompatibleBackToVersion` (integer): See 'projectVersion'
* `rawCameraBundleVersion` (string): ? (!226)
* `touchedByAperture` (bool): true ? (226)
* `versionCount` (integer): count of versions (-226)

#### Library.apdb

In some Aperture libraries, a file named `Library.apdb` is also present inside
`Aperture.aplib`.

It appears to be a SQLite3 database.  Its purpose is unclear.
(Performance optimization seems likely.)


### "Database" Directory

`Database/Folders` is all the containers, folders, project, etc.
The .apfolder files are binary plists.

`Database/Albums` contain the albums from the library.
The .apalbum files are binary plists.

Common plist properties:

* `uuid`: a string uuid.
* `modelId`: numeral id. Possibly unique library wide.
* `folderUuid`: uuid of the folder this is contained in
* `parentFolderUuid`: for Folders, the parent.

Notes are stored under a `notes` plist property, found in folders and masters. 
It is an array that contains dictionaries.

Common notes properties:

* `attachedToUuid`: uuid it is attached too (uuid in the containing plist)
* `createDate`
* `uuid`: uuid of the note.

Folders:

* `note`: text note. For Aperture project it is in the project info.

Masters:

Master has `hasFocusPoints` set to true. 
Also, attached in the notes:

* `propertyKey`: focusPoints
* `data`
* `modelId`

### "Database/Folders" Directory

All Aperture "folders" are represented by GUID-named .apfolder files (plists)
in the top level of the "Database/Folders" directory.

#### ".apfolder" Files

In *almost all* cases, these are binary plists (typically under 1kB length), but
at least one Aperture library has been found which contains a mix of both binary
and a few XML plist files, so parsing software should not blindly assume they
are always binary.

* `implicitAlbumUuid`: the uuid of the album that is representing the view
  (Subclass 2 album)
* `posterVersionUuid`: the uuid of the version that is the poster for the
  Project (type = 2 [Project])
* `notes`: an array of dict (multiple Notes)

### "Database/Albums" Directory

All Aperture "albums" are represented by GUID-named .apalbum files
in the top level of the "Database/Albums" directory.

#### ".apalbum" Files

As with Folders, these plists are most often binary, but examples of
XML plists *have* been found in actual Aperture Library examples.
(See `+3dqw557RM2Z0nh2PzRW9Q.apalbum` in the "testdata" directory of
this repo for an example.)

Unlike other plist definitions, .apalbum files have two levels.

Top-level properties:

* `UserQueryInfo`: the query for smart album. DATA.
* `InfoDictionary`: these are the actual properties
* `attachments`: attachments like track path
* `FilterInfo`: display filter. DATA.
* `versionUuids`: An array of uuid: the versions it contains. (Subclass 3)

InfoDictionary properties:

This is the main set of properties.

* `selectedTrackPathUuid`: the UUID of the track selected. See attachments.
* `albumSubclass`:
  * Subclass 1 Albums are attached to a folder. Linked via the folder
    `implicitAlbumUuid` property and back with albums `folderUuid`. They
    represent the view of the folder.
  * Sublclass 2 Albums are "smart", they are backed by a query.
  * Sublclass 3 Albums are "user", ie created by the user to contain versions.
   (See the `versionUuids` array for the list of albums it contains.)

### "Database/Keywords.plist" File

Defines the keywords in the database.
A plist with hierarchial keywords.

Properties:
* `keywords_verions` (integer): 6 or 7. Not sure which is what, I don't
  see difference otherwise.

#### Keyword Storage Formats

Aperture stores keywords in three different formats:

1. **UUIDs** - References to keywords defined in Keywords.plist (normal case)
2. **Direct names** - Plain text keyword names (e.g., "iPhoto Original" from imported iPhoto libraries)
3. **Hierarchical keywords with multi-space delimiters** - Multiple keywords encoded in a single string

**Multi-Space Delimiter Convention**: When a keyword string contains 2 or more consecutive spaces,
it represents hierarchical keywords separated by those spaces. Single spaces are part of the keyword name.

Examples:
- `"iPhoto Original"` (1 space) → Single keyword: "iPhoto Original"
- `"Wedding  Stock Category"` (2+ spaces) → Two keywords: "Wedding" (child of "Stock Category")
- `"New York  USA"` (2+ spaces) → Two keywords: "New York" (child of "USA")

The rightmost keyword in the hierarchy is the parent, with keywords to the left being progressively
more specific children.

**Keyword Sanitization**: All keywords are sanitized before export to XMP to remove null bytes and
illegal XML control characters. This ensures compatibility with the XMP specification.
more specific children.

### "Database/Versions" Directory

Contains edited versions of photos in the Library.

**IMPORTANT STRUCTURE NOTE**: The Versions directory tree contains ONLY metadata plist files:
- **Metadata plist files** (`Master.apmaster`, `Version-N.apversion`) - stored here
- **Actual image files** - stored in the `Masters/` tree, NOT in `Database/Versions/`

The `.apversion` extension is on **plist FILES**, not directories. Do not confuse
the file extension with a directory name.

```
LibraryName.aplibrary
+- Database
|  +- Versions
|     +- 2006
|        +- 11
|           +- 08
|              +- 20061108-161812
|                 +- 1pzPIlOcSayg7qQdE%UVEg
|                    +- Master.apmaster           (plist file - metadata ONLY)
|                    +- Version-0.apversion       (plist file - metadata ONLY)
|                    +- Version-1.apversion       (plist file - metadata ONLY)
+- Masters
   +- 2006
      +- 11
         +- 08
            +- 20061108-161812
               +- PICT0019.JPG                    (actual master image file)
               +- PICT0019-edited.JPG            (actual version image file)
               +- ... etc.
```

**KEY INSIGHT**: Version images are stored in the `Masters/` directory tree using the
same date-based path structure (`YYYY/MM/DD/YYYYMMDD-HHMMSS/`), but WITHOUT the UUID
subdirectories that exist in `Database/Versions/`.

The Versions directories organize metadata by year, then month, then day, and
then finally into a directory named YYYYMMDD-HHMMSS, 
e.g. `20061108-161812`.  
This hierarchy mirrors the structure used to store Masters.

Inside the date/time directory are one or more directories with
GUID-based names (UUIDs), each representing a Master and its associated Versions' metadata.

**Important**: There is no guarantee that each Master in the Library will have an
associated GUID-named folder inside the Versions directory tree, as
they are (it seems) created only when the user begins editing a Master.

Similarly, there is no guarantee that there is an actual Master
available in a Library for each Version.  Aperture had several mechanisms
for referencing Master files that were outside the Library bundle, and
prompting a user to mount/insert the required storage volume when needed.

If all you have is the Aperture Library and the Master is not
present, recreating it from the Version information is likely not possible.
Recovering a Thumbnail or rendered Preview (from the appropriate directories
in the Library) may, in some cases, be the best you can do.

#### Locating Version Image Files

**Critical Implementation Detail**: The `Version-N.apversion` plist files contain
a `fileName` property with ONLY the filename, no directory path information.

**IMPORTANT**: Version image files are NOT stored in the `Database/Versions/` tree!
They are stored in the `Masters/` tree.

To locate the actual image file for a version:
1. When parsing a `Version-N.apversion` plist file, capture its parent directory path
2. Transform the Versions path to a Masters path by:
   - Stripping the "Database/Versions/" prefix (or just "Versions/")
   - Removing the UUID directory (last path component)
   - Prepending "Masters/"
3. The image file is: `{transformed_path}/{fileName_from_plist}`

**Example**:
- Plist file path: `Database/Versions/2014/12/20/20141220-173831/5DLLeHPLQhCFDLrRsFmSOQ/Version-1.apversion`
- Parent directory: `Database/Versions/2014/12/20/20141220-173831/5DLLeHPLQhCFDLrRsFmSOQ/`
- Strip prefix: `2014/12/20/20141220-173831/5DLLeHPLQhCFDLrRsFmSOQ/`
- Remove UUID: `2014/12/20/20141220-173831/`
- Add Masters prefix: `Masters/2014/12/20/20141220-173831/`
- `fileName` property from plist: `"2014-12-20 17.38.31.jpg"`
- **Image file location**: `Masters/2014/12/20/20141220-173831/2014-12-20 17.38.31.jpg`

**Common Mistakes** (causes "file not found" errors):
- ❌ Looking for images in `Database/Versions/.../UUID/` directory
- ❌ Assuming UUID-based structure for image files
- ❌ Treating `.apversion` as a directory instead of a file extension
- ❌ Using `source_directory` path directly without transformation

See "Implementation Notes for Library Parsers" section below for detailed guidance.

### Versions

Each GUID-named directory inside a dated leaf node (named according to 
YYYYMMDD-HHMMSS) represents a *Version*, which is a set of lossless edits
to a Master.

Information regarding each Version's Master is in the `Master.apmaster` file,
while information about each Version are in the `Version-N.apversion` files,
where `N` is a unique integer, zero-indexed.

Common properties:

* `createDate`: date of when the master/version was created.
  (Unclear if this represents the actual creation date of the media, taken
  from EXIF or other metadata, or if it's the creation date of the logical
  structure within Aperture.)

#### "Master.apmaster" Files

Property list (plist) file inside each Version directory, containing information
about the master that the Version was derived from. 
Each version has a master.

* `type`: IMGT is image.
* `subtype`: RAWST is RAW. JPGST is JPEG. TIFST is TIFF.
* `importGroupUuid`: uuid for the import group. - apparently no other info.
* `alternateMasterUuid`: the other master (for JPEG+RAW) - reciprocal
* `originalVersionUuid`: the uuid of the original version. Likely n=0.
* `modelId`: numerical ID
* `fileVolumeUuid`: the UUID of the volume. See Volumes
* `fileIsReference`: true if not physically in library (referenced file in UI)
* `projectUuid`: the uuid of the project it is in (see Folders)
* `pixelFormat`: (int). 6 for a CR2.
* `hasFocusPoints`: If set to true the data is found in the `notes` property.
* `colorSpaceDefinition`: Found with TIFF masters.
* `faceDetectionState`: int. Values found: 9.

#### "Version-N.apversion" Files

Binary plist file containing information about a specific version of a
master.  One master can (and frequently does, in practice) have multiple versions.

**Note on Locating Image Files**: The `fileName` property in this plist contains
ONLY the filename (e.g., `"photo-edited.jpg"`), with NO directory path information.
To locate the actual image file, you must use the parent directory of this plist file.
See "Locating Version Image Files" in the Database/Versions section above.

Properties:

* `fileName`: **filename only**, no path (e.g., `"2014-12-20 17.38.31.jpg"`)
* `isFlagged`: version flagged
* `isOriginal`: this is the original version. Usually n=0.
* `isEditable`
* `isHidden`
* `isInTrash`
* `imageTimeZoneName`: timezone name for the image dates.
* `exportImageChangeDate`: (date) when it was last exported.
* `exportMetadataChangeDate`: (date) when metadata was last changed
   (not on version 0)
* `rawMasterUuid`: uuid of RAW master
* `nonRawMasterUuid`: uuid of non-RAW master.
* `showInLibrary`: whether to show. false likely to be implicit version of
  master.
* `name`: version name
* `fileName`: **filename only**, no directory path (e.g., `"IMG_1234-edit.jpg"`)
  - To locate the image file, use the parent directory of this plist file
  - Example: if plist is at `Database/Versions/2014/12/20/.../uuid/Version-1.apversion`
  - Then image is at `Database/Versions/2014/12/20/.../uuid/{fileName}`
* `mainRating`: rating
* `rotation`: Image rotation in degrees.
* `versionNumber`: the version number. n in the filename.
* `iptcProperties`: IPTC
* `exifProperties`: EXIF
* `renderVersion`: ???? (is this related to the RAW decoder version
   from `adjustmentProperties.RawDecodeVersion`)
* `customInfo`: struct containing timezone of the camera and picture's.
* `hasAdjustments`: bool. Always true.
* `hasEnabledAdjustments`: bool. If any adjustement past RAW decode
   is applied.
* `RKImageAdjustments`: array of dict for adjustement. Always one item
   for RAW decode.

### "Volumes" Directory

This directory, within "Database", stores information related to filesystem
volumes that Aperture knew about, including ones on which referenced Master
files might have been stored.

#### "[UUID].apvolume" Files

A binary plist file, containing information a volume where files could be found.

* `diskUuid`: the disk UUID (OS?)
* `modelId`: numerical model ID
* `uuid`: the object UUID. Referenced from fileVolumeUuid in master
* `volumeName`: OS volume name.

### Other Items

There are a variety of other files which can be found inside "Database", including
but not limited to:

* `ActiveWebPublishingAccounts.plist`
* `KeywordSets.plist` - XML plist containing information about "Keyword Sets",
  an Aperture feature that allowed a group of keywords to be added as a group.
* `MasterGroups` (directory) - ??
* `Places` (directory) - Contains `.applace` files.
* `tmSync.plist` - Appears to contain information about when the Library was last
  backed-up via Time Machine?  Keys include `tmSyncCounter` and `tmSyncUuid`.
* `Vaults` (directory) - Contains one or more `.apvault` files, which presumably
  contain information about connected or previously-connected Aperture Vaults.
* `VersionGroups` (directory) - ??


## Implementation Notes for Library Parsers

This section provides guidance for developers implementing Aperture library parsers,
based on lessons learned during development of this tool.

### Capturing Directory Paths and Locating Image Files

When loading Version plist files, you MUST capture the parent directory path AND
transform it to locate the actual image files (which are in the Masters tree, not
the Versions tree).

**Correct approach** (from this codebase - see `src/version.rs` and `src/bin/dumper/exporter.rs`):

```rust
// Step 1: When loading the plist, capture its parent directory
fn from_path<P>(plist_path: P) -> Option<Version>
where
    P: AsRef<Path>,
{
    // Capture the parent directory of the plist file
    let source_directory = plist_path.as_ref().parent().map(|p| p.to_path_buf());
    
    // ... parse plist data ...
    
    let version = Version {
        // ... populate fields from plist ...
        file_name: Some("2014-12-20 17.38.31.jpg".to_string()),
        source_directory,  // Store for later use (contains Versions path)
        // ...
    };
    
    Some(version)
}

// Step 2: Transform Versions path to Masters path
fn transform_versions_to_masters_path(versions_path: &Path) -> Option<PathBuf> {
    let path_str = versions_path.to_str()?;
    
    // Strip "Database/Versions/" or "Versions/" prefix
    let relative_path = if let Some(stripped) = path_str.strip_prefix("Database/Versions/") {
        stripped
    } else if let Some(stripped) = path_str.strip_prefix("Versions/") {
        stripped
    } else {
        return None;
    };
    
    // Remove UUID (last path component) to get date path
    let path_without_uuid = Path::new(relative_path);
    let parent_path = path_without_uuid.parent()?;
    
    // Build the Masters path
    Some(Path::new("Masters").join(parent_path))
}

// Step 3: Locate the image file
fn get_image_path(version: &Version, library_root: &Path) -> PathBuf {
    let masters_path = transform_versions_to_masters_path(
        version.source_directory.as_ref().unwrap()
    ).unwrap();
    
    library_root.join(masters_path).join(version.file_name.as_ref().unwrap())
}
// Result: /path/to/Library.aplibrary/Masters/2014/12/20/20141220-173831/image.jpg
```

### Wrong Approaches That Cause "File Not Found" Errors

❌ **Approach 1: Looking for images in Versions tree**

This is WRONG and will result in missing files:

```rust
// WRONG - Images are NOT in the Versions tree!
let image_path = version.source_directory
    .as_ref()
    .unwrap()
    .join(version.file_name.as_ref().unwrap());
// Result: Database/Versions/2014/12/20/20141220-173831/UUID/image.jpg
// This path does NOT exist - there are only plist files here!
```

**Why this is wrong**:
- The Versions tree contains ONLY metadata plist files
- Actual image files are in the Masters tree
- You must transform the path from Versions to Masters

❌ **Approach 2: UUID-based directory assumption**

```rust
// WRONG - Do not do this!
let image_path = format!(
    "Database/Versions/{}/{}.apversion/{}",
    &version_uuid[0..2],  // First 2 chars of UUID
    version_uuid,
    version.file_name
);
// Result: Database/Versions/5D/5DLLeHPLQhCFDLrRsFmSOQ.apversion/image.jpg
// This path does NOT exist!
```

**Why this is wrong**:
- Aperture uses date-based directories, not UUID-prefix-based
- The actual path is: `Masters/2014/12/20/20141220-173831/image.jpg`
- The `.apversion` is a file extension, not a directory

❌ **Approach 3: Treating `.apversion` as a directory**

```rust
// WRONG - .apversion is a file extension, not a directory!
let image_path = format!(
    "Database/Versions/.../uuid/{}.apversion/{}",
    version_number,
    file_name
);
```

**Why this is wrong**:
- `Version-0.apversion` is a FILE (a plist), not a directory
- Image files are in the Masters tree, not alongside plist files

✅ **Correct approach**: Capture parent directory during loading, transform to Masters path, then join with filename

### Real-World Impact

The incorrect assumptions about version file locations resulted in:
- **Thousands of "file does not exist" warnings** during export
- **Unable to export version (edited) images** - only masters exported
- Major data loss for libraries with significant editing history

After implementing the correct approach (transform Versions paths to Masters paths):
- All version images correctly located ✅
- Full export with both masters and versions successful ✅
- Zero "file not found" errors for existing files ✅

### Cache and Serialization Considerations

If you cache/serialize Version objects:

**Problem**: The `source_directory` field contains a Versions tree path, which must
be transformed to a Masters path when locating image files.

**Solutions**:
1. Always transform Versions paths to Masters paths before file access
2. Store both paths (Versions for metadata context, Masters for image access)
3. Regenerate cache if structure understanding changes

Example from this codebase (`src/version.rs` and `src/bin/dumper/exporter.rs`):

```rust
pub struct Version {
    // ... other fields ...
    
    /// Directory where this version's plist file was loaded from.
    #[serde(skip)]  // Don't serialize - will be None after cache load
    pub source_directory: Option<PathBuf>,
}
```

### Code References

For the actual implementation in this repository:
- Version loading with directory capture: `src/version.rs` (lines 75-85)
- Image path construction: `src/bin/dumper/exporter.rs` (lines 220-246)
- Deprecated wrong approach: `src/bin/dumper/exporter.rs` `get_version_image_path()` function


## Common Pitfalls

### Pitfall #1: Assuming UUID-Based Directory Structure

**Misconception**: Version directories follow `Database/Versions/{first2chars}/{uuid}.apversion/` pattern

**Reality**: Versions use date-based structure: `{year}/{month}/{day}/{timestamp}/{uuid}/`

**Why this mistake is easy to make**:
- Some Apple data structures DO use UUID-based directories (e.g., iOS app sandboxes)
- The `.apversion` file extension suggests it might be a directory container
- Older documentation didn't explicitly state where image files are located
- The UUID-based approach seems logical and consistent

**Impact**: Files won't be found, leading to significant data loss during export/migration.

### Pitfall #2: Not Storing Directory Paths During Loading

**Problem**: The `fileName` property in Version plists contains no path information.

**Consequence**: After loading metadata, you cannot locate the image files without
the directory context.

**Solution**: 
- Capture the parent directory when parsing each plist file
- Store it in your Version object/struct
- Use it later to construct the full image file path

**Example of the problem**:
```python
# You load a Version plist and get:
version.fileName = "2014-12-20 17.38.31.jpg"
version.uuid = "5DLLeHPLQhCFDLrRsFmSOQ"

# Later, you try to find the image:
image_path = ???  # No way to know where it is!
```

### Pitfall #3: File Extension Confusion

**Problem**: The `.apversion` extension appears on plist FILES, not directories.

**Misconception**: Thinking `Version-1.apversion` is a directory that contains the image.

**Reality**: 
```
Directory structure:
  uuid/
    ├── Version-1.apversion  (this is a FILE, not a directory)
    └── image.jpg            (this is a sibling, not a child)
```

### Pitfall #4: Ignoring Multiple Library Layouts

**Issue**: Aperture libraries can have different structural layouts:
- Masters in `Masters/` or `Database/Masters/`
- Different Aperture versions may organize files differently

**Solution**: Code must check for both possible locations and handle both gracefully.

See `src/library.rs` `resolve_subdir()` function for an example implementation.

### Pitfall #5: Cache Invalidation

**Problem**: Cached Version objects may lose directory path information.

**Consequence**: After loading from cache, you can't find image files anymore.

**Solution**:
- Don't serialize directory paths (mark as transient/skip)
- Rebuild paths from library structure when needed
- Or, accept that cache miss forces full reload to recapture paths

This codebase uses `#[serde(skip)]` to avoid caching directory paths entirely.


## Masters

### Directory Location

There appear to be two locations where the `Masters` directory
can be found.  In most tested Libraries, it appears *directly* in the bundle
root, but based on various examples of Library parsing code, it appears other
developers have found it inside `Databases`.

### Extraction

Simply extracting Master items without any Aperture-specific metadata is trivial:
the directory (once found), can simply be copied elsewhere and imported directly
into another photo management program.

Original files *should* be unchanged from when they were imported into Aperture,
although this may not be the same format in which they were saved by the camera.
Aperture had a variety of import options that could change the "master" format,
including conversion from "camera raw" to DNG.

**Extraction by Album** 

To extract Masters while preserving the Album structure, the `.apalbum` files
inside the `Database/Albums` directory must be parsed.
