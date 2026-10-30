/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_core_terms::{FLOAT32_VECTOR, KnownDatatype, SYSTEM_VOCABULARY};

#[test]
fn test_codes_and_iris_name_each_known_datatype_once() {
    let all: Vec<KnownDatatype> = (0u8..=255).filter_map(KnownDatatype::from_code).collect();
    assert_eq!(all.len(), 33);
    for (i, d) in all.iter().enumerate() {
        assert_eq!(
            usize::from(d.code()),
            i + 1,
            "codes are dense from 1, in declaration order"
        );
        assert_eq!(KnownDatatype::from_iri(d.iri()), Some(*d));
    }
    assert_eq!(KnownDatatype::from_code(0), None);
    assert_eq!(KnownDatatype::from_iri("http://example.org/x"), None);
    assert_eq!(
        KnownDatatype::from_iri("http://www.w3.org/2001/XMLSchema#string"),
        None
    );
}

#[test]
fn test_the_by_value_datatypes_are_the_xsd_values_and_durations() {
    assert!(KnownDatatype::Integer.by_value() && KnownDatatype::DayTimeDuration.by_value());
    assert!(!KnownDatatype::Int.by_value() && !KnownDatatype::Float32Vector.by_value());
    assert_eq!(KnownDatatype::Float32Vector.iri(), FLOAT32_VECTOR);
    assert!(FLOAT32_VECTOR.starts_with(SYSTEM_VOCABULARY));
}

/// PROTOCOL.md section 10.4, transcribed: each stored code and the datatype it names.
const SECTION_10_4: [(u8, &str); 33] = [
    (1, "http://www.w3.org/2001/XMLSchema#boolean"),
    (2, "http://www.w3.org/2001/XMLSchema#integer"),
    (3, "http://www.w3.org/2001/XMLSchema#decimal"),
    (4, "http://www.w3.org/2001/XMLSchema#double"),
    (5, "http://www.w3.org/2001/XMLSchema#float"),
    (6, "http://www.w3.org/2001/XMLSchema#dateTime"),
    (7, "http://www.w3.org/2001/XMLSchema#time"),
    (8, "http://www.w3.org/2001/XMLSchema#date"),
    (9, "http://www.w3.org/2001/XMLSchema#gYearMonth"),
    (10, "http://www.w3.org/2001/XMLSchema#gYear"),
    (11, "http://www.w3.org/2001/XMLSchema#gMonthDay"),
    (12, "http://www.w3.org/2001/XMLSchema#gDay"),
    (13, "http://www.w3.org/2001/XMLSchema#gMonth"),
    (14, "http://www.w3.org/2001/XMLSchema#duration"),
    (15, "http://www.w3.org/2001/XMLSchema#yearMonthDuration"),
    (16, "http://www.w3.org/2001/XMLSchema#dayTimeDuration"),
    (17, "http://www.w3.org/2001/XMLSchema#long"),
    (18, "http://www.w3.org/2001/XMLSchema#int"),
    (19, "http://www.w3.org/2001/XMLSchema#short"),
    (20, "http://www.w3.org/2001/XMLSchema#byte"),
    (21, "http://www.w3.org/2001/XMLSchema#nonNegativeInteger"),
    (22, "http://www.w3.org/2001/XMLSchema#positiveInteger"),
    (23, "http://www.w3.org/2001/XMLSchema#nonPositiveInteger"),
    (24, "http://www.w3.org/2001/XMLSchema#negativeInteger"),
    (25, "http://www.w3.org/2001/XMLSchema#unsignedLong"),
    (26, "http://www.w3.org/2001/XMLSchema#unsignedInt"),
    (27, "http://www.w3.org/2001/XMLSchema#unsignedShort"),
    (28, "http://www.w3.org/2001/XMLSchema#unsignedByte"),
    (29, "http://www.w3.org/2001/XMLSchema#dateTimeStamp"),
    (30, "http://www.opengis.net/ont/geosparql#geoJSONLiteral"),
    (31, "http://www.opengis.net/ont/geosparql#wktLiteral"),
    (32, "https://w3id.org/ultragraphdb/ns/float32vector"),
    (33, "http://www.w3.org/2001/XMLSchema#anyURI"),
];

#[test]
fn test_each_code_names_the_datatype_section_10_4_gives_it() {
    for (code, iri) in SECTION_10_4 {
        let datatype = KnownDatatype::from_code(code).unwrap_or_else(|| panic!("code {code}"));
        assert_eq!(datatype.iri(), iri, "code {code}");
        assert_eq!(KnownDatatype::from_iri(iri), Some(datatype), "{iri}");
        assert_eq!(datatype.code(), code, "{iri}");
    }
    assert_eq!(KnownDatatype::from_code(34), None);
    assert_eq!(KnownDatatype::from_code(u8::MAX), None);
}

#[test]
fn test_exactly_codes_1_to_16_are_encoded_by_value() {
    // Section 10.4: "codes 1 to 16 are also the by-value datatypes"; 16 and 17 are the threshold and one past it.
    for (code, iri) in SECTION_10_4 {
        let datatype = KnownDatatype::from_code(code).unwrap_or_else(|| panic!("code {code}"));
        assert_eq!(datatype.by_value(), code <= 16, "{iri}");
    }
    assert!(KnownDatatype::Boolean.by_value() && !KnownDatatype::Long.by_value());
}

#[test]
fn test_an_iri_is_known_only_when_it_matches_exactly() {
    for iri in [
        "",
        // A vocabulary's base alone.
        "http://www.w3.org/2001/XMLSchema#",
        "https://w3id.org/ultragraphdb/ns/",
        // IRIs compare case-sensitively, and a known IRI with a character more or less is another IRI.
        "http://www.w3.org/2001/XMLSchema#Integer",
        "http://www.w3.org/2001/XMLSchema#integer2",
        "http://www.w3.org/2001/XMLSchema#intege",
        "https://www.w3.org/2001/XMLSchema#integer",
        "http://www.opengis.net/ont/geosparql#geoJSON",
    ] {
        assert_eq!(KnownDatatype::from_iri(iri), None, "{iri:?}");
    }
}
