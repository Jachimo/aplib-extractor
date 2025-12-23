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

### "Database/Versions" Directory

Contains edited versions of photos in the Library.

```
LibraryName.aplibrary
+- Database
|  +- Versions
|     +- 2006
|        +- 11
|           +- 08
|              +- 20061108-161812
|                 +- 1pzPIlOcSayg7qQdE%UVEg
|                    +- Master.apmaster
|                    +- Version-0.apversion
|                    +- Version-1.apversion
|                    +- ... etc.
```

Images are into subdirectories by year, then month, then day, and
then finally into a directory named YYYYMMDD-HHMMSS, 
e.g. `20061108-161812`.  
This appears to correspond to the hierarchy used to store Masters
as well, but this has not been thoroughly verified.

Inside the date/time directory are one or more directories with
GUID-based names, each representing a Master and one or more Versions.

There is no guarantee that each Master in the Library will have an
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
* `fileName`: filename for version
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
