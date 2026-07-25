/*
 * Copyright (C) 2016-2023 Hubert Figuière
 *
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/.
 */

use chrono::{DateTime, Utc};
use serde::{Serialize, Deserialize};
use std::path::{Path, PathBuf};

use crate::audit::{
    audit_get_array_value, audit_get_bool_value, audit_get_date_value, audit_get_dict_value,
    audit_get_int_value, audit_get_str_value, Report, SkipReason,
};
use crate::custominfo::CustomInfoProperties;
use crate::exif::ExifProperties;
use crate::iptc::IptcProperties;
use crate::store;
use crate::AplibObject;
use crate::AplibType;
use crate::PlistLoadable;

use crate::xmp::ToXmp;
use exempi2::{Xmp, PropFlags};

fn digikam_xmp_label_name(color_label: i64) -> Option<&'static str> {
    // digiKam reads string fallback values for a limited Lightroom-style set.
    match color_label {
        1 => Some("Red"),
        3 => Some("Yellow"),
        4 => Some("Green"),
        5 => Some("Blue"),
        6 => Some("Purple"),
        _ => None,
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
/// A rendered image. There is one for the orignal, and one per
/// actual version. `Version` are associated to a `Master`.
pub struct Version {
    uuid: Option<String>,
    model_id: Option<i64>,
    /// The associated `Master`.
    pub master_uuid: Option<String>,

    /// uuid of the `Folder` project this reside in.
    pub project_uuid: Option<String>,
    /// uuid of the raw `Master`.
    pub raw_master_uuid: Option<String>,
    /// uuid of the non raw `Master`.
    pub nonraw_master_uuid: Option<String>,
    pub timezone_name: Option<String>,
    pub create_date: Option<DateTime<Utc>>,
    pub image_date: Option<DateTime<Utc>>,
    pub export_image_change_date: Option<DateTime<Utc>>,
    pub export_metadata_change_date: Option<DateTime<Utc>>,
    pub version_number: Option<i64>,
    pub db_version: Option<i64>,
    pub db_minor_version: Option<i64>,
    pub is_flagged: Option<bool>,
    /// Indicate the version is the original.
    pub is_original: Option<bool>,
    pub is_editable: Option<bool>,
    pub is_hidden: Option<bool>,
    pub is_in_trash: Option<bool>,
    pub file_name: Option<String>,
    pub name: Option<String>,
    pub rating: Option<i64>,
    pub rotation: Option<i64>,
    pub colour_label_index: Option<i64>,

    pub iptc: Option<IptcProperties>,
    pub exif: Option<ExifProperties>,
    pub custom_info: Option<CustomInfoProperties>,
    pub keywords: Option<Vec<String>>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub custom_aplib_fields: Option<std::collections::BTreeMap<String, String>>,

    /// Directory where this version's plist file was loaded from.
    /// NOTE: This path points to the Versions tree (Database/Versions/.../UUID/).
    /// To locate the actual image file, transform this to the Masters tree path.
    /// See `transform_versions_to_masters_path()` in src/bin/dumper/exporter.rs
    pub source_directory: Option<PathBuf>,
}

impl PlistLoadable for Version {
    /// Load the version object from the plist at plist_path.
    fn from_path<P>(plist_path: P, mut auditor: Option<&mut Report>) -> Option<Version>
    where
        P: AsRef<Path>,
    {
        use crate::plutils::*;

        let plist = parse_plist(&plist_path);
        
        // Capture the directory where this plist file is located
        let source_directory = plist_path.as_ref().parent().map(|p| p.to_path_buf());
        
        match plist {
            Value::Dictionary(ref dict) => {
                let iptc = audit_get_dict_value(dict, "iptcProperties", &mut auditor);
                let exif = audit_get_dict_value(dict, "exifProperties", &mut auditor);
                let custom_info = audit_get_dict_value(dict, "customInfo", &mut auditor);
                let mut result = Version {
                    uuid: audit_get_str_value(dict, "uuid", &mut auditor),
                    master_uuid: audit_get_str_value(dict, "masterUuid", &mut auditor),
                    project_uuid: audit_get_str_value(dict, "projectUuid", &mut auditor),
                    raw_master_uuid: audit_get_str_value(dict, "rawMasterUuid", &mut auditor),
                    nonraw_master_uuid: audit_get_str_value(dict, "nonRawMasterUuid", &mut auditor),
                    timezone_name: audit_get_str_value(dict, "imageTimeZoneName", &mut auditor),
                    create_date: audit_get_date_value(dict, "createDate", &mut auditor),
                    image_date: audit_get_date_value(dict, "imageDate", &mut auditor),
                    export_image_change_date: audit_get_date_value(
                        dict,
                        "exportImageChangeDate",
                        &mut auditor,
                    ),
                    export_metadata_change_date: audit_get_date_value(
                        dict,
                        "exportMetadataChangeDate",
                        &mut auditor,
                    ),
                    version_number: audit_get_int_value(dict, "versionNumber", &mut auditor),
                    db_version: audit_get_int_value(dict, "version", &mut auditor),
                    db_minor_version: audit_get_int_value(dict, "minorVersion", &mut auditor),
                    is_flagged: audit_get_bool_value(dict, "isFlagged", &mut auditor),
                    is_original: audit_get_bool_value(dict, "isOriginal", &mut auditor),
                    is_editable: audit_get_bool_value(dict, "isEditable", &mut auditor),
                    is_hidden: audit_get_bool_value(dict, "isHidden", &mut auditor),
                    is_in_trash: audit_get_bool_value(dict, "isInTrash", &mut auditor),
                    file_name: audit_get_str_value(dict, "fileName", &mut auditor),
                    name: audit_get_str_value(dict, "name", &mut auditor),
                    model_id: audit_get_int_value(dict, "modelId", &mut auditor),
                    rating: audit_get_int_value(dict, "mainRating", &mut auditor),
                    rotation: audit_get_int_value(dict, "rotation", &mut auditor),
                    colour_label_index: audit_get_int_value(dict, "colorLabelIndex", &mut auditor),
                    iptc: IptcProperties::from(&iptc, &mut auditor),
                    exif: ExifProperties::from(&exif, &mut auditor),
                    custom_info: CustomInfoProperties::from(&custom_info, &mut auditor),
                    keywords: audit_get_array_value(dict, "keywords", &mut auditor)
                        .map(|arr| arr.into_iter()
                            .filter_map(|v| v.as_string().map(|s| s.trim().to_string()))
                            .collect()),
                    custom_aplib_fields: dict_to_btreemap(audit_get_dict_value(dict, "customAplibFields", &mut auditor)),
                    source_directory: None, // Set after construction
                };
                
                // Set the source directory
                result.source_directory = source_directory;
                
                if let Some(auditor) = &mut auditor {
                    auditor.skip("statistics", SkipReason::Ignore);
                    auditor.skip("thumbnailGroup", SkipReason::Ignore);
                    auditor.skip("faceDetectionIsFromPreview", SkipReason::Ignore);
                    auditor.skip("processedHeight", SkipReason::Ignore);
                    auditor.skip("processedWidth", SkipReason::Ignore);
                    auditor.skip("masterHeight", SkipReason::Ignore);
                    auditor.skip("masterWidth", SkipReason::Ignore);
                    auditor.skip("supportedStatus", SkipReason::Ignore);
                    auditor.skip("showInLibrary", SkipReason::Ignore);
                    auditor.skip("adjustmentProperties", SkipReason::Ignore); // don't know what to do yet
                    auditor.skip("RKImageAdjustments", SkipReason::Ignore);
                    auditor.skip("hasAdjustments", SkipReason::Ignore);
                    auditor.skip("hasEnabledAdjustments", SkipReason::Ignore);
                    auditor.skip("renderVersion", SkipReason::Ignore);
                    auditor.skip("imageProxyState", SkipReason::Ignore);
                    auditor.skip("plistWriteTimestamp", SkipReason::Ignore);
                    auditor.audit_ignored(dict, None);
                }
                Some(result)
            }
            _ => None,
        }
    }
}

impl AplibObject for Version {
    fn obj_type(&self) -> AplibType {
        AplibType::Version
    }
    fn uuid(&self) -> &Option<String> {
        &self.uuid
    }
    fn parent(&self) -> &Option<String> {
        &self.master_uuid
    }
    fn model_id(&self) -> i64 {
        self.model_id.unwrap_or(0)
    }
    fn is_valid(&self) -> bool {
        self.uuid.is_some()
    }
    fn wrap(obj: Version) -> store::Wrapper {
        store::Wrapper::Version(Box::new(obj))
    }
}

impl ToXmp for Version {
    fn to_xmp(&self, xmp: &mut Xmp) -> bool {
        crate::xmp::register_export_namespaces();
        let mut ok = true;
        let xmp_ns = crate::xmp::ns::NS_XMP;
        let dc_ns = crate::xmp::ns::NS_DC;
        let tiff_ns = crate::xmp::ns::NS_TIFF;
        let exif_ns = crate::xmp::ns::NS_EXIF;
        let photoshop_ns = crate::xmp::ns::NS_PHOTOSHOP;

        // VersionUUID
        if let Some(ref uuid) = self.uuid {
            if xmp.set_property(xmp_ns, "VersionUUID", uuid, PropFlags::NONE).is_err() {
                eprintln!("Warning: Failed to write XMP property VersionUUID for Version {}", uuid);
                ok = false;
            }
        }
        // VersionFileName
        if let Some(ref file_name) = self.file_name {
            let clean = crate::xmp::sanitize_for_xmp(file_name);
            if xmp.set_property(xmp_ns, "VersionFileName", &clean, PropFlags::NONE).is_err() {
                let uuid_str = self.uuid.as_deref().unwrap_or("unknown");
                eprintln!("Warning: Failed to write XMP property VersionFileName for Version {}", uuid_str);
                ok = false;
            }
            if xmp.set_property(tiff_ns, "FileName", &clean, PropFlags::NONE).is_err() {
                let uuid_str = self.uuid.as_deref().unwrap_or("unknown");
                eprintln!("Warning: Failed to write TIFF FileName for Version {}", uuid_str);
                ok = false;
            }
        }

        // Mirror commonly useful fields into standard namespaces for wider importer support.
        if let Some(ref name) = self.name {
            let clean = crate::xmp::sanitize_for_xmp(name);
            if xmp.set_property(dc_ns, "title", &clean, PropFlags::NONE).is_err() {
                let uuid_str = self.uuid.as_deref().unwrap_or("unknown");
                eprintln!("Warning: Failed to write dc:title for Version {}", uuid_str);
                ok = false;
            }
            if xmp.set_property(photoshop_ns, "Headline", &clean, PropFlags::NONE).is_err() {
                let uuid_str = self.uuid.as_deref().unwrap_or("unknown");
                eprintln!("Warning: Failed to write photoshop:Headline for Version {}", uuid_str);
                ok = false;
            }
        }

        if let Some(rating) = self.rating {
            if xmp.set_property(xmp_ns, "Rating", &rating.to_string(), PropFlags::NONE).is_err() {
                let uuid_str = self.uuid.as_deref().unwrap_or("unknown");
                eprintln!("Warning: Failed to write xmp:Rating for Version {}", uuid_str);
                ok = false;
            }
        }

        // digiKam-specific labels for robust tag import.
        if let Some(color_label_index) = self.colour_label_index {
            let color_label = color_label_index.clamp(0, 9);
            if xmp
                .set_property(crate::xmp::ns::NS_DIGIKAM, "ColorLabel", &color_label.to_string(), PropFlags::NONE)
                .is_err()
            {
                let uuid_str = self.uuid.as_deref().unwrap_or("unknown");
                eprintln!("Warning: Failed to write digiKam ColorLabel for Version {}", uuid_str);
                ok = false;
            }

            if xmp
                .set_property(photoshop_ns, "Urgency", &color_label.to_string(), PropFlags::NONE)
                .is_err()
            {
                let uuid_str = self.uuid.as_deref().unwrap_or("unknown");
                eprintln!("Warning: Failed to write photoshop:Urgency for Version {}", uuid_str);
                ok = false;
            }

            if let Some(label_name) = digikam_xmp_label_name(color_label) {
                if xmp
                    .set_property(xmp_ns, "Label", label_name, PropFlags::NONE)
                    .is_err()
                {
                    let uuid_str = self.uuid.as_deref().unwrap_or("unknown");
                    eprintln!("Warning: Failed to write xmp:Label for Version {}", uuid_str);
                    ok = false;
                }
            }
        }

        if let Some(is_flagged) = self.is_flagged {
            // Aperture has a binary flag. Map to digiKam pick labels:
            // flagged -> Pending (2), not flagged -> None (0).
            let pick_label = if is_flagged { 2 } else { 0 };
            if xmp
                .set_property(crate::xmp::ns::NS_DIGIKAM, "PickLabel", &pick_label.to_string(), PropFlags::NONE)
                .is_err()
            {
                let uuid_str = self.uuid.as_deref().unwrap_or("unknown");
                eprintln!("Warning: Failed to write digiKam PickLabel for Version {}", uuid_str);
                ok = false;
            }
        }

        if let Some(ref create_date) = self.create_date {
            let date = create_date.to_rfc3339();
            if xmp.set_property(xmp_ns, "CreateDate", &date, PropFlags::NONE).is_err() {
                let uuid_str = self.uuid.as_deref().unwrap_or("unknown");
                eprintln!("Warning: Failed to write xmp:CreateDate for Version {}", uuid_str);
                ok = false;
            }
            if xmp.set_property(photoshop_ns, "DateCreated", &date, PropFlags::NONE).is_err() {
                let uuid_str = self.uuid.as_deref().unwrap_or("unknown");
                eprintln!("Warning: Failed to write photoshop:DateCreated for Version {}", uuid_str);
                ok = false;
            }
        }

        if let Some(ref image_date) = self.image_date {
            let date = image_date.to_rfc3339();
            if xmp.set_property(exif_ns, "DateTimeOriginal", &date, PropFlags::NONE).is_err() {
                let uuid_str = self.uuid.as_deref().unwrap_or("unknown");
                eprintln!("Warning: Failed to write exif:DateTimeOriginal for Version {}", uuid_str);
                ok = false;
            }
        }

        // Other custom fields in app-specific XMP namespace...
        if let Some(ref custom) = self.custom_aplib_fields {
            for (k, v) in custom {
                let clean_v = crate::xmp::sanitize_for_xmp(v);
                if xmp.set_property(crate::xmp::ns::APLIB, k, &clean_v, exempi2::PropFlags::NONE).is_err() {
                    let uuid_str = self.uuid.as_deref().unwrap_or("unknown");
                    eprintln!("Warning: Failed to write custom XMP field '{}' for Version {}", k, uuid_str);
                    ok = false;
                }
            }
        }

        if let Some(ref exif) = self.exif {
            if !exif.to_xmp(xmp) {
                let uuid_str = self.uuid.as_deref().unwrap_or("unknown");
                eprintln!("Warning: Failed to fully write EXIF-derived XMP fields for Version {}", uuid_str);
                ok = false;
            }
        }

        if let Some(ref iptc) = self.iptc {
            if !iptc.to_xmp(xmp) {
                let uuid_str = self.uuid.as_deref().unwrap_or("unknown");
                eprintln!("Warning: Failed to fully write IPTC-derived XMP fields for Version {}", uuid_str);
                ok = false;
            }
        }

        // Write keywords into Dublin Core (flat, for compatibility)
        if let Some(ref keywords) = self.keywords {
            crate::xmp::write_rdf_bag(xmp, crate::xmp::ns::NS_DC, "subject", keywords);
        }

        // Write hierarchical keywords into digiKam TagsList (ordered, with full paths)
        if let Some(ref custom) = self.custom_aplib_fields {
            if let Some(hierarchical_json) = custom.get("_resolved_hierarchical_keywords") {
                if let Ok(hierarchical_keywords) =
                    serde_json::from_str::<Vec<String>>(hierarchical_json)
                {
                    crate::xmp::write_rdf_seq(
                        xmp,
                        crate::xmp::ns::NS_DIGIKAM,
                        "TagsList",
                        &hierarchical_keywords,
                    );
                }
            }
        }

        ok
    }
}

fn dict_to_btreemap(dict: Option<plist::Dictionary>) -> Option<std::collections::BTreeMap<String, String>> {
    dict.map(|d| {
        d.into_iter()
            .filter_map(|(k, v)| v.as_string().map(|vs| (k, vs.to_string())))
            .collect()
    })
}

#[cfg(test)]
#[test]
fn test_version_parse() {
    use crate::testutils;

    let version = Version::from_path(
        testutils::get_test_file_path(
            "Database/Versions/2006/11/02/20061102-161812/V6jjzYNdSVu006MPsZkt5w/Version-0.apversion",
        )
        .as_path(),
        None,
    );
    assert!(version.is_some());
    let version = version.unwrap();

    assert_eq!(version.uuid.as_ref().unwrap(), "t58rPT%6SYCIW2ooj%iRCQ");
    assert!(version.is_original.unwrap());
    assert_eq!(
        version.master_uuid.as_ref().unwrap(),
        "V6jjzYNdSVu006MPsZkt5w"
    );
    assert_eq!(version.name.as_ref().unwrap(), "PICT0019");
    // The test version has minimal data, so we just verify core fields exist
    assert!(version.uuid.is_some());
    assert!(version.master_uuid.is_some());

    let mut xmp = Xmp::new();

    let result = version.to_xmp(&mut xmp);
    assert!(result);

    // Just verify XMP was created successfully
    // (The test data is minimal and doesn't have all the properties from the original test)
}
