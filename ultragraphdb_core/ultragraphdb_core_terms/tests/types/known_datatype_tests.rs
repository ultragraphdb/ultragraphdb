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
