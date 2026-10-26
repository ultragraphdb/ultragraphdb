/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_core_catalog::tokenizer_takes;

const XSD: &str = "http://www.w3.org/2001/XMLSchema#";

fn xsd(local: &str) -> String {
    format!("{XSD}{local}")
}

#[test]
fn test_each_tokenizer_takes_the_datatypes_of_the_table() {
    let lang = "http://www.w3.org/1999/02/22-rdf-syntax-ns#langString";
    let geo = "http://www.opengis.net/ont/geosparql#geoJSONLiteral";
    let takes = [
        ("int", xsd("integer")),
        ("int", xsd("unsignedByte")),
        ("float", xsd("double")),
        ("float", xsd("float")),
        ("bigfloat", xsd("decimal")),
        ("bool", xsd("boolean")),
        ("day", xsd("date")),
        ("hour", xsd("dateTime")),
        ("year", xsd("dateTimeStamp")),
        ("term", xsd("string")),
        ("fulltext", lang.to_string()),
        ("exact", xsd("string")),
        ("geo", geo.to_string()),
    ];
    for (t, d) in &takes {
        assert!(tokenizer_takes(t, d), "{t} takes {d}");
    }
    let refuses = [
        ("int", xsd("string")),
        ("int", xsd("decimal")),
        ("hour", xsd("date")),
        ("term", xsd("integer")),
        (
            "geo",
            "http://www.opengis.net/ont/geosparql#wktLiteral".to_string(),
        ),
        ("nope", xsd("string")),
    ];
    for (t, d) in &refuses {
        assert!(!tokenizer_takes(t, d), "{t} refuses {d}");
    }
}
