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

/// Parse a keyword string that may contain hierarchical keywords separated by multiple spaces.
///
/// **Parsing Rule**: Multiple consecutive whitespace characters (2+) are treated as delimiters.
/// Single spaces are part of the keyword name.
///
/// **Hierarchy Convention**: When split on multiple spaces, the rightmost keyword is the parent,
/// and keywords to the left are children (progressively more specific).
///
/// **Sanitization**: All parsed keywords are sanitized to remove null bytes and illegal XML
/// characters before being returned.
///
/// # Examples
/// - `"iPhoto Original"` (single space) → `vec!["iPhoto Original"]` (one keyword)
/// - `"Wedding  Stock Category"` (double space) → `vec!["Wedding", "Stock Category"]`
///   where "Wedding" is a child of "Stock Category"
///
/// # Returns
/// A vector of sanitized keyword names. If no multi-space delimiters are found, returns a 
/// single-element vector with the original string (trimmed and sanitized).
fn parse_hierarchical_keyword(keyword_str: &str) -> Vec<String> {
    use crate::xmp::sanitize_for_xmp;
    
    // Check if the string contains multiple consecutive spaces (delimiter pattern)
    let has_multi_space = keyword_str.contains("  "); // Two or more spaces
    
    if !has_multi_space {
        // No multi-space delimiter - this is a single keyword
        let trimmed = keyword_str.trim();
        if trimmed.is_empty() {
            return vec![];
        }
        let sanitized = sanitize_for_xmp(trimmed);
        if sanitized.is_empty() {
            return vec![];
        }
        return vec![sanitized];
    }
    
    // Split on multiple consecutive spaces (2 or more)
    // Use a regex-like approach: split on runs of 2+ spaces
    let mut keywords = Vec::new();
    let mut current_keyword = String::new();
    let mut space_count = 0;
    
    for ch in keyword_str.chars() {
        if ch == ' ' {
            space_count += 1;
        } else {
            // Non-space character
            if space_count >= 2 {
                // We had a delimiter - save the current keyword and start a new one
                if !current_keyword.is_empty() {
                    keywords.push(sanitize_for_xmp(current_keyword.trim()));
                    current_keyword = String::new();
                }
            } else if space_count == 1 {
                // Single space - add it to the current keyword
                current_keyword.push(' ');
            }
            // Reset space counter and add the character
            space_count = 0;
            current_keyword.push(ch);
        }
    }
    
    // Don't forget the last keyword
    if !current_keyword.is_empty() {
        keywords.push(sanitize_for_xmp(current_keyword.trim()));
    }
    
    // Filter out any empty strings that may have resulted from sanitization
    keywords.retain(|k| !k.is_empty());
    
    keywords
}

/// Resolve a list of keyword UUIDs to names using the provided map.
///
/// If a value cannot be found in the map, it's assumed to be an already-resolved
/// name (not a UUID) and is passed through as-is. This handles cases where
/// Aperture stores keyword names directly instead of UUIDs (e.g., for iPhoto imports).
///
/// **Special handling**: If a keyword name contains multiple consecutive spaces (2+),
/// it's parsed as hierarchical keywords (see `parse_hierarchical_keyword()`).
///
/// Returns a vector of resolved names.
pub fn resolve_keyword_uuids(
    uuids: &[String],
    keyword_map: &std::collections::HashMap<String, String>,
    _context: &str,
) -> Vec<String> {
    let mut result = Vec::new();
    for uuid in uuids {
        if let Some(name) = keyword_map.get(uuid) {
            // Found in map - it was a UUID, use the resolved name
            // Check if the name contains hierarchical keywords (multiple spaces)
            let parsed_keywords = parse_hierarchical_keyword(name);
            result.extend(parsed_keywords);
        } else {
            // Not in map - assume it's already a name, not a UUID
            // This happens with imported iPhoto libraries and other edge cases
            // Check if it contains hierarchical keywords (multiple spaces)
            let parsed_keywords = parse_hierarchical_keyword(uuid);
            result.extend(parsed_keywords);
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_hierarchical_keyword_single_space() {
        // Single space - should be treated as one keyword
        let result = parse_hierarchical_keyword("iPhoto Original");
        assert_eq!(result, vec!["iPhoto Original"]);
        
        let result = parse_hierarchical_keyword("My Vacation Photos");
        assert_eq!(result, vec!["My Vacation Photos"]);
    }

    #[test]
    fn test_parse_hierarchical_keyword_double_space() {
        // Double space - delimiter, splits into two keywords
        let result = parse_hierarchical_keyword("Wedding  Stock Category");
        assert_eq!(result, vec!["Wedding", "Stock Category"]);
    }

    #[test]
    fn test_parse_hierarchical_keyword_multiple_spaces() {
        // Three spaces - still a delimiter
        let result = parse_hierarchical_keyword("Child   Parent");
        assert_eq!(result, vec!["Child", "Parent"]);
        
        // Many spaces
        let result = parse_hierarchical_keyword("A     B");
        assert_eq!(result, vec!["A", "B"]);
    }

    #[test]
    fn test_parse_hierarchical_keyword_three_levels() {
        // Three-level hierarchy
        let result = parse_hierarchical_keyword("Child  Parent  Grandparent");
        assert_eq!(result, vec!["Child", "Parent", "Grandparent"]);
    }

    #[test]
    fn test_parse_hierarchical_keyword_mixed_spaces() {
        // Mix of single and double spaces
        let result = parse_hierarchical_keyword("New York  United States");
        assert_eq!(result, vec!["New York", "United States"]);
        
        let result = parse_hierarchical_keyword("San Francisco Bay  California  USA");
        assert_eq!(result, vec!["San Francisco Bay", "California", "USA"]);
    }

    #[test]
    fn test_parse_hierarchical_keyword_edge_cases() {
        // Leading/trailing spaces
        let result = parse_hierarchical_keyword("  Keyword  ");
        assert_eq!(result, vec!["Keyword"]);
        
        let result = parse_hierarchical_keyword("  A  B  ");
        assert_eq!(result, vec!["A", "B"]);
        
        // Empty string
        let result = parse_hierarchical_keyword("");
        assert_eq!(result, vec![] as Vec<String>);
        
        // Only spaces
        let result = parse_hierarchical_keyword("   ");
        assert_eq!(result, vec![] as Vec<String>);
    }

    #[test]
    fn test_parse_hierarchical_keyword_no_spaces() {
        // No spaces at all
        let result = parse_hierarchical_keyword("Keyword");
        assert_eq!(result, vec!["Keyword"]);
    }

    #[test]
    fn test_resolve_keyword_uuids_with_hierarchical() {
        let mut keyword_map = std::collections::HashMap::new();
        
        // Regular UUID -> single name
        keyword_map.insert("uuid1".to_string(), "Simple Keyword".to_string());
        
        // UUID -> hierarchical name with double space
        keyword_map.insert("uuid2".to_string(), "Child  Parent".to_string());
        
        let input = vec!["uuid1".to_string(), "uuid2".to_string()];
        let result = resolve_keyword_uuids(&input, &keyword_map, "test");
        
        // uuid1 -> "Simple Keyword" (one keyword)
        // uuid2 -> "Child  Parent" -> ["Child", "Parent"] (two keywords)
        assert_eq!(result, vec!["Simple Keyword", "Child", "Parent"]);
    }

    #[test]
    fn test_resolve_keyword_uuids_with_direct_hierarchical() {
        let keyword_map = std::collections::HashMap::new();
        
        // Non-UUID direct names (like iPhoto imports)
        let input = vec![
            "iPhoto Original".to_string(),
            "Wedding  Stock Category".to_string(),
        ];
        let result = resolve_keyword_uuids(&input, &keyword_map, "test");
        
        // "iPhoto Original" -> one keyword
        // "Wedding  Stock Category" -> two keywords
        assert_eq!(result, vec!["iPhoto Original", "Wedding", "Stock Category"]);
    }

    #[test]
    fn test_parse_hierarchical_keyword_sanitization() {
        // Test null byte removal
        let result = parse_hierarchical_keyword("Keyword\0WithNull");
        assert_eq!(result, vec!["KeywordWithNull"]);
        
        // Test control character removal
        let result = parse_hierarchical_keyword("Test\x01\x02Keyword");
        assert_eq!(result, vec!["TestKeyword"]);
        
        // Test hierarchical with null bytes
        let result = parse_hierarchical_keyword("Child\0  Parent\0");
        assert_eq!(result, vec!["Child", "Parent"]);
        
        // Test that it returns empty vec for strings that are only illegal chars
        let result = parse_hierarchical_keyword("\0\0\0");
        assert_eq!(result, vec![] as Vec<String>);
    }
}
