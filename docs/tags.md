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

## DigiKam Handling

DigiKam takes a broad-brush approach by reading from and writing to several
different properties in the XMP sidecar.  It seems to prefer the
`digiKam:TagsList` property if it exists, but it also maintains the same data in other
elements/properties within the XMP:

- `digiKam:TagsList` - this is the place to write tags, since DigiKam will ingest from here, and then automatically write out into other applications' preferred formats (if writing to sidecar files is enabled) when a change is made triggering an update.
	- DigiKam uses a `rdf:Seq` inside the tag, which then contains `rdf:li` elements (seems like common practice, see other examples below)
	- The `li` elements are just strings which can be either single-valued or hierarchical/nested
	- Nested tags are separated by `/` characters similar to Unix paths
- `dc:subject` - I don't think the Dublin Core people meant for this field to be used for general organizational tags, but it seems to get used for that purpose. 
	- Within that property, there's an `rdf:Bag` (unordered list)
	- Inside the "bag" are `rdf:li` elements, each containing a single tag
	- There's no provision for hierarchical or nested tags, each one is independent
- `lr:hierarchicalSubject` - this is in the Adobe Lightroom namespace (`lr:`), so it's pretty widely used
	- It seems to always immediately contain an `rdf:Bag` which contains `rdf:li` elements
	- Each `rdf:li` element can be single-valued or contain a hierarchical list; hierarchial lists are delimited by `|` characters (note this is not the same as DigiKam's preferred `/` because of course nobody can agree on anything)
- `MicrosoftPhoto:LastKeywordXMP` - not sure where this comes from
- `mediapro:CatalogSets` - not sure who or what MediaPro is
- And then just to further confuse things, there's also the `acdsee:categories` ATTRIBUTE, which is part of the upper-level `rdf:Description` ELEMENT, which contains the tag tree with some sort of awful HTML-style escaping.  It's gross.

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
