/*
 * Copyright (C) 2016-2025 Hubert Figuière
 *
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/.
 */

use std::path::Path;

use crate::audit::{audit_get_int_value, Report};
use crate::plutils::*;
use crate::store;
use crate::AplibObject;
use crate::AplibType;
use serde::{Serialize, Deserialize};

/// An Aperture keyword.
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct Keyword {
    /// The uuid
    pub uuid: Option<String>,

    /// The numeric id in the model
    model_id: Option<i64>,

    /// The parent uuid
    parent_uuid: Option<String>,

    /// Name of the keyword
    pub name: String,

    /// Children keywords.  parent_uuid = self.uuid
    pub children: Option<Vec<Keyword>>,
}

impl AplibObject for Keyword {
    fn obj_type(&self) -> AplibType {
        AplibType::Keyword
    }
    fn uuid(&self) -> &Option<String> {
        &self.uuid
    }
    fn parent(&self) -> &Option<String> {
        &self.parent_uuid
    }
    fn model_id(&self) -> i64 {
        self.model_id.unwrap_or(0)
    }
    fn is_valid(&self) -> bool {
        self.uuid.is_some()
    }
    #[doc(hidden)]
    fn wrap(_: Keyword) -> store::Wrapper {
        store::Wrapper::None
    }
}

/// Parse keywords from the .plist file
pub fn parse_keywords<P>(path: P, auditor: &mut Option<&mut Report>) -> Option<Vec<Keyword>>
where
    P: AsRef<Path>,
{
    let plist = parse_plist(path);

    match plist {
        Value::Dictionary(ref dict) => {
            let version = audit_get_int_value(dict, "keywords_version", auditor)?;
            // XXX deal with proper errors here.
            // Version 3.4.5 has version 7.
            if version != 6 && version != 7 {
                println!("Wrong keyword version {version} !");
            }
            Keyword::from_array(get_array_value(dict, "keywords"))
        }
        _ => None,
    }
}

impl Keyword {
    /// convert a Plist array to a vec of keyword.
    fn from_array(oa: Option<Vec<Value>>) -> Option<Vec<Keyword>> {
        let a = oa?;

        let mut keywords = Vec::new();
        for item in a {
            if let Value::Dictionary(ref kw) = item {
                keywords.push(Keyword::from(kw));
            }
        }
        Some(keywords)
    }
}

impl From<&plist::Dictionary> for Keyword {
    /// Create a new keyword from a plist dictionary
    /// will recursively create the children
    fn from(d: &plist::Dictionary) -> Keyword {
        Keyword {
            uuid: get_str_value(d, "uuid"),
            model_id: get_int_value(d, "modelId"),
            parent_uuid: get_str_value(d, "parentUuid"),
            name: get_str_value(d, "name").unwrap_or_default(),
            children: Keyword::from_array(get_array_value(d, "zChildren")),
        }
    }
}

// Helper to resolve a keyword by uuid or name from a slice of keywords.
pub fn resolve_keyword(keywords: &[Keyword], uuid_or_name: &str) -> Option<String> {
    keywords.iter()
        .find(|kw| kw.uuid.as_deref() == Some(uuid_or_name) || kw.name == uuid_or_name)
        .map(|kw| kw.name.clone())
}

/// Build hierarchical keyword maps from a flat keyword list.
///
/// Returns two maps:
/// - `flat_map`: UUID → simple name (e.g., "France")
/// - `hierarchical_map`: UUID → full path (e.g., "Location/Europe/France")
pub fn build_keyword_maps(
    keywords: &[Keyword],
) -> (
    std::collections::HashMap<String, String>,
    std::collections::HashMap<String, String>,
) {
    let mut flat_map = std::collections::HashMap::new();
    let mut hierarchical_map = std::collections::HashMap::new();

    fn traverse(
        keyword: &Keyword,
        parent_path: String,
        flat_map: &mut std::collections::HashMap<String, String>,
        hierarchical_map: &mut std::collections::HashMap<String, String>,
    ) {
        if let Some(ref uuid) = keyword.uuid {
            // Add to flat map
            flat_map.insert(uuid.clone(), keyword.name.clone());

            // Build hierarchical path
            let current_path = if parent_path.is_empty() {
                keyword.name.clone()
            } else {
                format!("{}/{}", parent_path, keyword.name)
            };

            // Add to hierarchical map
            hierarchical_map.insert(uuid.clone(), current_path.clone());

            // Recurse into children
            if let Some(ref children) = keyword.children {
                for child in children {
                    traverse(child, current_path.clone(), flat_map, hierarchical_map);
                }
            }
        }
    }

    for keyword in keywords {
        traverse(keyword, String::new(), &mut flat_map, &mut hierarchical_map);
    }

    (flat_map, hierarchical_map)
}

/// Resolve a list of keyword UUIDs to names using the provided map.
///
/// Prints a warning to stderr for any UUIDs that cannot be resolved.
/// Returns a vector of resolved names (unresolved UUIDs are skipped).
pub fn resolve_keyword_uuids(
    uuids: &[String],
    keyword_map: &std::collections::HashMap<String, String>,
    context: &str,
) -> Vec<String> {
    let mut result = Vec::new();
    for uuid in uuids {
        if let Some(name) = keyword_map.get(uuid) {
            result.push(name.clone());
        } else {
            eprintln!("Warning: Could not resolve keyword UUID '{}' for {}", uuid, context);
        }
    }
    result
}
