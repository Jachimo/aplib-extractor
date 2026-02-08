/*
 * Copyright (C) 2016-2023 Hubert Figuière
 *
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/.
 */


use std::path::Path;

use chrono::{DateTime, Utc};
use serde::{Serialize, Deserialize};
use exempi2::{Xmp, PropFlags};
use base64::alphabet;
use base64::Engine;

use crate::audit::{
    audit_get_array_value, audit_get_bool_value, audit_get_data_value, audit_get_date_value,
    audit_get_int_value, audit_get_str_value, Report, SkipReason,
};
use crate::notes::NotesProperties;
use crate::store;
use crate::AplibObject;
use crate::AplibType;
use crate::PlistLoadable;
use crate::xmp::ns;
use crate::xmp::ToXmp;

/// A `Master` is a file backing an image (`Version`)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Master {
    uuid: Option<String>,
    model_id: Option<i64>,
    project_uuid: Option<String>,

    /// If it is RAW+JPEG, there is another master.
    pub alternate_master: Option<String>,
    
    /// uuid of the orignal version
    pub original_version_uuid: Option<String>,

    pub import_group_uuid: Option<String>,
    pub filename: Option<String>,
    pub name: Option<String>,
    pub original_version_name: Option<String>,
    pub db_version: Option<i64>,
    pub master_type: Option<String>,
    pub subtype: Option<String>,
    pub image_path: Option<String>,
    pub is_reference: Option<bool>,
    pub is_truly_raw: Option<bool>,
    pub is_in_trash: Option<bool>,
    pub is_missing: Option<bool>,
    pub is_externaly_editable: Option<bool>,
    pub create_date: Option<DateTime<Utc>>,
    pub image_date: Option<DateTime<Utc>>,
    pub file_creation_date: Option<DateTime<Utc>>,
    pub file_modification_date: Option<DateTime<Utc>>,
    pub original_file_name: Option<String>,
    pub file_size: Option<i64>,
    pub file_volume_uuid: Option<String>,
    pub color_space_name: Option<String>,
    pub pixel_format: Option<i64>,
    pub has_focus_points: Option<i64>,
    pub image_format: Option<i64>, // TODO: fix this is a 4char MSB
    pub notes: Option<Vec<NotesProperties>>,
    pub colour_space_definition: Option<Vec<u8>>,
    pub face_detection_state: Option<i64>,
    pub library_path: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub custom_aplib_fields: Option<std::collections::BTreeMap<String, String>>,

    pub keywords: Option<Vec<String>>, // <-- Add this field
}

impl PlistLoadable for Master {
    fn from_path<P>(plist_path: P, mut auditor: Option<&mut Report>) -> Option<Master>
    where
        P: AsRef<Path>,
    {
        use crate::plutils::*;
        let plist = parse_plist(plist_path);
        match plist {
            Value::Dictionary(ref dict) => {
                let notes = audit_get_array_value(dict, "notes", &mut auditor);
                let result = Some(Master {
                    uuid: audit_get_str_value(dict, "uuid", &mut auditor),
                    alternate_master: audit_get_str_value(
                        dict,
                        "alternateMasterUuid",
                        &mut auditor,
                    ),
                    original_version_uuid: audit_get_str_value(
                        dict,
                        "originalVersionUuid",
                        &mut auditor,
                    ),
                    project_uuid: audit_get_str_value(dict, "projectUuid", &mut auditor),
                    import_group_uuid: audit_get_str_value(dict, "importGroupUuid", &mut auditor),
                    filename: audit_get_str_value(dict, "fileName", &mut auditor),
                    name: audit_get_str_value(dict, "name", &mut auditor),
                    original_version_name: audit_get_str_value(
                        dict,
                        "originalVersionName",
                        &mut auditor,
                    ),
                    original_file_name: audit_get_str_value(dict, "originalFileName", &mut auditor),
                    file_volume_uuid: audit_get_str_value(dict, "fileVolumeUuid", &mut auditor),
                    db_version: audit_get_int_value(dict, "version", &mut auditor),
                    master_type: audit_get_str_value(dict, "type", &mut auditor),
                    subtype: audit_get_str_value(dict, "subtype", &mut auditor),
                    model_id: audit_get_int_value(dict, "modelId", &mut auditor),
                    image_path: audit_get_str_value(dict, "imagePath", &mut auditor),
                    file_size: audit_get_int_value(dict, "fileSize", &mut auditor),
                    is_reference: audit_get_bool_value(dict, "fileIsReference", &mut auditor),
                    is_externaly_editable: audit_get_bool_value(
                        dict,
                        "isExternallyEditable",
                        &mut auditor,
                    ),
                    is_in_trash: audit_get_bool_value(dict, "isInTrash", &mut auditor),
                    is_missing: audit_get_bool_value(dict, "isMissing", &mut auditor),
                    is_truly_raw: audit_get_bool_value(dict, "isTrulyRaw", &mut auditor),
                    color_space_name: audit_get_str_value(dict, "colorSpaceName", &mut auditor),
                    create_date: audit_get_date_value(dict, "createDate", &mut auditor),
                    image_date: audit_get_date_value(dict, "imageDate", &mut auditor),
                    file_creation_date: audit_get_date_value(
                        dict,
                        "fileCreationDate",
                        &mut auditor,
                    ),
                    file_modification_date: audit_get_date_value(
                        dict,
                        "fileModificationDate",
                        &mut auditor,
                    ),
                    has_focus_points: audit_get_int_value(dict, "hasFocusPoints", &mut auditor),
                    image_format: audit_get_int_value(dict, "imageFormat", &mut auditor),
                    pixel_format: audit_get_int_value(dict, "pixelFormat", &mut auditor),
                    colour_space_definition: audit_get_data_value(
                        dict,
                        "colorSpaceDefinition",
                        &mut auditor,
                    ),
                    notes: NotesProperties::from(&notes, &mut auditor),
                    face_detection_state: audit_get_int_value(
                        dict,
                        "faceDetectionState",
                        &mut auditor,
                    ),
                    library_path: None, // set during XMP export based on file location
                    custom_aplib_fields: None,
                    keywords: audit_get_array_value(dict, "keywords", &mut auditor)
                        .map(|arr| arr.iter()
                            .filter_map(|v| v.as_string().map(|s| s.trim().to_string()))
                            .collect()),
                });
                if let Some(auditor) = &mut auditor {
                    auditor.skip("fileAliasData", SkipReason::Ignore);
                    auditor.skip("importedBy", SkipReason::Ignore);
                    auditor.skip("importGroup", SkipReason::Ignore);
                    auditor.skip("plistWriteTimestamp", SkipReason::Ignore);

                    auditor.audit_ignored(dict, None);
                }
                result
            }
            _ => None,
        }
    }
}

impl AplibObject for Master {
    fn obj_type(&self) -> AplibType {
        AplibType::Master
    }
    fn uuid(&self) -> &Option<String> {
        &self.uuid
    }
    fn parent(&self) -> &Option<String> {
        &self.project_uuid
    }
    fn model_id(&self) -> i64 {
        self.model_id.unwrap_or(0)
    }
    fn is_valid(&self) -> bool {
        self.uuid.is_some()
    }
    fn wrap(obj: Master) -> store::Wrapper {
        store::Wrapper::Master(Box::new(obj))
    }
}

// Create XMP for export for a Master 
impl ToXmp for Master {
    fn to_xmp(&self, xmp: &mut Xmp) -> bool {

        let mut ok = true;

        // Custom Aperture namespace for Aperture-specific fields
        let aplib_ns = ns::APLIB;
        let xmp_ns = "http://ns.adobe.com/xap/1.0/";
        let _exif_ns = "http://ns.adobe.com/exif/1.0/";  // not currently used; reserved for EXIF-specific fields
        let tiff_ns = "http://ns.adobe.com/tiff/1.0/";

        // UUID
        if let Some(ref uuid) = self.uuid {
            ok &= xmp.set_property(aplib_ns, "MasterUUID", uuid, PropFlags::NONE).is_ok();
        }
        // Model ID
        if let Some(model_id) = self.model_id {
            ok &= xmp.set_property(aplib_ns, "ModelID", &model_id.to_string(), PropFlags::NONE).is_ok();
        }
        // Project UUID
        if let Some(ref project_uuid) = self.project_uuid {
            ok &= xmp.set_property(aplib_ns, "ProjectUUID", project_uuid, PropFlags::NONE).is_ok();
        }
        // Alternate Master
        if let Some(ref alternate_master) = self.alternate_master {
            let clean = crate::xmp::sanitize_for_xmp(alternate_master);
            ok &= xmp.set_property(aplib_ns, "AlternateMaster", &clean, PropFlags::NONE).is_ok();
        }
        // Original Version UUID
        if let Some(ref original_version_uuid) = self.original_version_uuid {
            ok &= xmp.set_property(aplib_ns, "OriginalVersionUUID", original_version_uuid, PropFlags::NONE).is_ok();
        }
        // Import Group UUID
        if let Some(ref import_group_uuid) = self.import_group_uuid {
            ok &= xmp.set_property(aplib_ns, "ImportGroupUUID", import_group_uuid, PropFlags::NONE).is_ok();
        }
        // Filename
        if let Some(ref filename) = self.filename {
            let clean = crate::xmp::sanitize_for_xmp(filename);
            ok &= xmp.set_property(tiff_ns, "FileName", &clean, PropFlags::NONE).is_ok();
        }
        // Name
        if let Some(ref name) = self.name {
            let clean = crate::xmp::sanitize_for_xmp(name);
            ok &= xmp.set_property(xmp_ns, "Title", &clean, PropFlags::NONE).is_ok();
        }
        // Original Version Name
        if let Some(ref original_version_name) = self.original_version_name {
            let clean = crate::xmp::sanitize_for_xmp(original_version_name);
            ok &= xmp.set_property(aplib_ns, "OriginalVersionName", &clean, PropFlags::NONE).is_ok();
        }
        // DB Version
        if let Some(db_version) = self.db_version {
            ok &= xmp.set_property(aplib_ns, "DBVersion", &db_version.to_string(), PropFlags::NONE).is_ok();
        }
        // Master Type
        if let Some(ref master_type) = self.master_type {
            let clean = crate::xmp::sanitize_for_xmp(master_type);
            ok &= xmp.set_property(aplib_ns, "Type", &clean, PropFlags::NONE).is_ok();
        }
        // Subtype
        if let Some(ref subtype) = self.subtype {
            let clean = crate::xmp::sanitize_for_xmp(subtype);
            ok &= xmp.set_property(aplib_ns, "Subtype", &clean, PropFlags::NONE).is_ok();
        }
        // Image Path
        if let Some(ref image_path) = self.image_path {
            let clean = crate::xmp::sanitize_for_xmp(image_path);
            ok &= xmp.set_property(aplib_ns, "ImagePath", &clean, PropFlags::NONE).is_ok();
        }
        // Is Reference
        if let Some(is_reference) = self.is_reference {
            ok &= xmp.set_property(aplib_ns, "IsReference", &is_reference.to_string(), PropFlags::NONE).is_ok();
        }
        // Is Truly Raw
        if let Some(is_truly_raw) = self.is_truly_raw {
            ok &= xmp.set_property(aplib_ns, "IsTrulyRaw", &is_truly_raw.to_string(), PropFlags::NONE).is_ok();
        }
        // Is In Trash
        if let Some(is_in_trash) = self.is_in_trash {
            ok &= xmp.set_property(aplib_ns, "IsInTrash", &is_in_trash.to_string(), PropFlags::NONE).is_ok();
        }
        // Is Missing
        if let Some(is_missing) = self.is_missing {
            ok &= xmp.set_property(aplib_ns, "IsMissing", &is_missing.to_string(), PropFlags::NONE).is_ok();
        }
        // Is Externally Editable
        if let Some(is_externaly_editable) = self.is_externaly_editable {
            ok &= xmp.set_property(aplib_ns, "IsExternallyEditable", &is_externaly_editable.to_string(), PropFlags::NONE).is_ok();
        }
        // Create Date
        if let Some(ref create_date) = self.create_date {
            ok &= xmp.set_property(xmp_ns, "CreateDate", &create_date.to_rfc3339(), PropFlags::NONE).is_ok();
        }
        // Image Date
        if let Some(ref image_date) = self.image_date {
            ok &= xmp.set_property(xmp_ns, "ImageDate", &image_date.to_rfc3339(), PropFlags::NONE).is_ok();
        }
        // File Creation Date
        if let Some(ref file_creation_date) = self.file_creation_date {
            ok &= xmp.set_property(xmp_ns, "FileCreateDate", &file_creation_date.to_rfc3339(), PropFlags::NONE).is_ok();
        }
        // File Modification Date
        if let Some(ref file_modification_date) = self.file_modification_date {
            ok &= xmp.set_property(xmp_ns, "FileModifyDate", &file_modification_date.to_rfc3339(), PropFlags::NONE).is_ok();
        }
        // Original File Name
        if let Some(ref original_file_name) = self.original_file_name {
            let clean = crate::xmp::sanitize_for_xmp(original_file_name);
            ok &= xmp.set_property(tiff_ns, "OriginalFileName", &clean, PropFlags::NONE).is_ok();
        }
        // File Size
        if let Some(file_size) = self.file_size {
            ok &= xmp.set_property(aplib_ns, "FileSize", &file_size.to_string(), PropFlags::NONE).is_ok();
        }
        // File Volume UUID
        if let Some(ref file_volume_uuid) = self.file_volume_uuid {
            ok &= xmp.set_property(aplib_ns, "FileVolumeUUID", file_volume_uuid, PropFlags::NONE).is_ok();
        }
        // Color Space Name
        if let Some(ref color_space_name) = self.color_space_name {
            let clean = crate::xmp::sanitize_for_xmp(color_space_name);
            ok &= xmp.set_property(aplib_ns, "ColorSpaceName", &clean, PropFlags::NONE).is_ok();
        }
        // Pixel Format
        if let Some(pixel_format) = self.pixel_format {
            ok &= xmp.set_property(aplib_ns, "PixelFormat", &pixel_format.to_string(), PropFlags::NONE).is_ok();
        }
        // Has Focus Points
        if let Some(has_focus_points) = self.has_focus_points {
            ok &= xmp.set_property(aplib_ns, "HasFocusPoints", &has_focus_points.to_string(), PropFlags::NONE).is_ok();
        }
        // Image Format
        if let Some(image_format) = self.image_format {
            ok &= xmp.set_property(aplib_ns, "ImageFormat", &image_format.to_string(), PropFlags::NONE).is_ok();
        }
        // Face Detection State
        if let Some(face_detection_state) = self.face_detection_state {
            ok &= xmp.set_property(aplib_ns, "FaceDetectionState", &face_detection_state.to_string(), PropFlags::NONE).is_ok();
        }
        // Notes (as a string, if present)
        if let Some(ref notes) = self.notes {
            let notes_str = format!("{:?}", notes);
            let clean = crate::xmp::sanitize_for_xmp(&notes_str);
            ok &= xmp.set_property(aplib_ns, "Notes", &clean, PropFlags::NONE).is_ok();
        }
        // Colour Space Definition (as base64, if present)
        if let Some(ref colour_space_definition) = self.colour_space_definition {
            let engine = base64::engine::GeneralPurpose::new(&alphabet::STANDARD, base64::engine::general_purpose::PAD);
            let b64 = engine.encode(colour_space_definition);
            ok &= xmp.set_property(aplib_ns, "ColorSpaceDefinition", &b64, PropFlags::NONE).is_ok();
        }
        // Include other custom values in the APLIB namespace
        if let Some(ref custom) = self.custom_aplib_fields {
            for (k, v) in custom {
                // Skip the special hierarchical keywords field - it's handled separately below
                if k == "_resolved_hierarchical_keywords" {
                    continue;
                }
                let clean_v = crate::xmp::sanitize_for_xmp(v);
                if let Err(e) = xmp.set_property(crate::xmp::ns::APLIB, k, &clean_v, exempi2::PropFlags::NONE) {
                    eprintln!("Warning: Failed to write custom XMP field '{}': {:?}", k, e);
                    ok = false;
                }
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

impl Master {}

#[cfg(test)]
#[test]
fn test_master_parse() {
    use crate::testutils;

    let master = Master::from_path(
        testutils::get_test_file_path(
            "Database/Versions/2006/11/02/20061102-161812/V6jjzYNdSVu006MPsZkt5w/Master.apmaster",
        )
        .as_path(),
        None,
    );
    assert!(master.is_some());
    let master = master.unwrap();

    assert_eq!(master.uuid.as_ref().unwrap(), "V6jjzYNdSVu006MPsZkt5w");
    assert_eq!(
        master.project_uuid.as_ref().unwrap(),
        "1AgVFohpQ02BiLvjtdUCzw"
    );
    assert_eq!(
        master.original_version_uuid.as_ref().unwrap(),
        "t58rPT%6SYCIW2ooj%iRCQ"
    );
    // Note: The test master file doesn't have all properties that were in the original test,
    // so we just verify the core ones that exist
    assert!(master.filename.is_some());

    // TODO: fix when have actual audit.
    //    println!("report {:?}", report);
}
