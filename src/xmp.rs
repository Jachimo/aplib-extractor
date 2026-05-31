/*
 * Copyright (C) 2017-2023 Hubert Figuière
 *
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/.
 */

use exempi2::Xmp;

/// Define namespace constants until we can get them out of Exempi.
pub mod ns {
    pub const NS_DC: &str = "http://purl.org/dc/elements/1.1/";
    pub const NS_IPTC4XMP: &str = "http://iptc.org/std/Iptc4xmpCore/1.0/xmlns/";
    pub const NS_XMP: &str = "http://ns.adobe.com/xap/1.0/";
    pub const NS_XMP_RIGHTS: &str = "http://ns.adobe.com/xap/1.0/rights/";
    pub const NS_PHOTOSHOP: &str = "http://ns.adobe.com/photoshop/1.0/";
    pub const NS_EXIF: &str = "http://ns.adobe.com/exif/1.0/";
    pub const NS_EXIF_AUX: &str = "http://ns.adobe.com/exif/1.0/aux/";
    pub const NS_TIFF: &str = "http://ns.adobe.com/tiff/1.0/";
    pub const APLIB: &str = "http://github.com/Jachimo/aplib-extractor/aplib/1.0/";
    pub const NS_DIGIKAM: &str = "http://www.digikam.org/ns/1.0/";
}

#[derive(Clone, Debug)]
/// Define a property
pub struct XmpProperty {
    /// The namespace URI
    ns: &'static str,
    /// The property name
    property: &'static str,
    /// The index (if an array)
    index: Option<i32>,
    /// The sub property if applicable
    field: Option<Box<XmpProperty>>,
}

impl XmpProperty {
    /// Create a new basic property.
    pub fn new(ns: &'static str, property: &'static str) -> XmpProperty {
        XmpProperty {
            ns,
            property,
            index: None,
            field: None,
        }
    }

    /// Create a new property that address a struct field.
    pub fn new_field(ns: &'static str, property: &'static str, field: XmpProperty) -> XmpProperty {
        XmpProperty {
            ns,
            property,
            index: None,
            field: Some(Box::new(field)),
        }
    }

    /// Put the property `value` into the XMP meta.
    pub fn put_into_xmp(&self, value: &str, xmp: &mut Xmp) -> bool {
        let clean_value = sanitize_for_xmp(value);
        if self.index.is_none() && self.field.is_none() {
            return xmp
                .set_property(self.ns, self.property, &clean_value, exempi2::PropFlags::NONE)
                .is_ok();
        } else if let Some(ref field) = self.field {
            // XXX when there is the API in exempi, use it.
            // For now we have to compose the path by hand.
            if let Ok(prefix) = exempi2::namespace_prefix(field.ns) {
                let property = format!("{}/{}{}", self.property, prefix, field.property);
                return xmp
                    .set_property(self.ns, &property, &clean_value, exempi2::PropFlags::NONE)
                    .is_ok();
            }
        } else if let Some(index) = self.index {
            return xmp
                .set_array_item(
                    self.ns,
                    self.property,
                    index,
                    &clean_value,
                    exempi2::PropFlags::NONE,
                )
                .is_ok();
        }
        false
    }
}

/// Specify a translation method.
pub(crate) enum XmpTranslator {
    /// Simple property mapping.
    Property(XmpProperty),
    /// Custom (TBD).
    Custom,
    /// None. Ignore the property.
    None,
}

/// Trait for conversion to XMP.
pub trait ToXmp {
    /// Push the object properties to the `xmp` XMP meta.
    fn to_xmp(&self, xmp: &mut Xmp) -> bool;
}

/// Sanitize a string for safe use in XMP by removing NUL bytes and illegal XML characters
pub fn sanitize_for_xmp(value: &str) -> String {
    value
        .chars()
        .filter(|c| {
            // Remove NUL bytes
            if *c == '\0' {
                return false;
            }
            // Remove other illegal XML characters (control characters except whitespace)
            !matches!(*c, '\x01'..='\x08' | '\x0B'..='\x0C' | '\x0E'..='\x1F' | '\x7F')
        })
        .collect()
}

/// Vector of strings -> rdf:Bag property in XMP
pub fn write_rdf_bag(xmp: &mut Xmp, namespace: &str, property: &str, values: &[String]) {
    if values.is_empty() {
        return;
    }
    // Create the array property first
    if let Err(e) = xmp.set_property(namespace, property, "", exempi2::PropFlags::VALUE_IS_ARRAY) {
        eprintln!(
            "Warning: Failed to create XMP array '{}:{}': {:?}",
            namespace, property, e
        );
        return;
    }
    for (i, v) in values.iter().enumerate() {
        let index = (i as i32) + 1;
        let clean = sanitize_for_xmp(v);
        let result = xmp.set_array_item(namespace, property, index, &clean, exempi2::PropFlags::NONE);
        if let Err(e) = result {
            eprintln!(
                "Warning: Failed to set XMP array item '{}:{}[{}]': {:?}",
                namespace, property, index, e
            );
        }
    }
}

/// Vector of strings -> rdf:Seq property in XMP
pub fn write_rdf_seq(xmp: &mut Xmp, namespace: &str, property: &str, values: &[String]) {
    if values.is_empty() {
        return;
    }
    // Create the ordered array property first
    if let Err(e) = xmp.set_property(
        namespace,
        property,
        "",
        exempi2::PropFlags::VALUE_IS_ARRAY | exempi2::PropFlags::ARRAY_IS_ORDERED,
    ) {
        eprintln!(
            "Warning: Failed to create XMP ordered array '{}:{}': {:?}",
            namespace, property, e
        );
        return;
    }
    for (i, v) in values.iter().enumerate() {
        let index = (i as i32) + 1;
        let clean = sanitize_for_xmp(v);
        let result = xmp.set_array_item(namespace, property, index, &clean, exempi2::PropFlags::NONE);
        if let Err(e) = result {
            eprintln!(
                "Warning: Failed to set XMP array item '{}:{}[{}]': {:?}",
                namespace, property, index, e
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sanitize_for_xmp() {
        // Test null byte removal
        assert_eq!(sanitize_for_xmp("sRGB IEC61966-2.1\0"), "sRGB IEC61966-2.1");
        
        // Test embedded null bytes
        assert_eq!(sanitize_for_xmp("hello\0world"), "helloworld");
        
        // Test control characters (except whitespace)
        assert_eq!(sanitize_for_xmp("test\x01\x02\x03"), "test");
        
        // Test that normal whitespace is preserved
        assert_eq!(sanitize_for_xmp("hello world\n\t"), "hello world\n\t");
        
        // Test empty string
        assert_eq!(sanitize_for_xmp(""), "");
        
        // Test string with only null bytes
        assert_eq!(sanitize_for_xmp("\0\0\0"), "");
    }

    #[test]
    fn test_xmp() {
        let mut xmp = Xmp::new();

        let prop1 = XmpProperty::new(ns::NS_DC, "creator");
        let prop2 = XmpProperty::new_field(
            ns::NS_IPTC4XMP,
            "CreatorContactInfo",
            XmpProperty::new(ns::NS_IPTC4XMP, "CiAdrCity"),
        );
        assert!(prop1.put_into_xmp("Batman", &mut xmp));
        assert!(prop2.put_into_xmp("Gotham", &mut xmp));

        let mut options: exempi2::PropFlags = exempi2::PropFlags::NONE;
        let value = xmp.get_property(prop1.ns, prop1.property, &mut options);
        assert!(value.is_ok());
        assert_eq!(value.unwrap().to_str(), Ok("Batman"));
    }
    
    #[test]
    fn test_xmp_with_null_bytes() {
        let mut xmp = Xmp::new();
        let prop = XmpProperty::new(ns::NS_DC, "description");
        
        // This should not panic - null bytes should be removed
        assert!(prop.put_into_xmp("sRGB IEC61966-2.1\0", &mut xmp));
        
        let mut options: exempi2::PropFlags = exempi2::PropFlags::NONE;
        let value = xmp.get_property(prop.ns, prop.property, &mut options);
        assert!(value.is_ok());
        assert_eq!(value.unwrap().to_str(), Ok("sRGB IEC61966-2.1"));
    }
}

