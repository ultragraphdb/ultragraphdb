/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

/// The XSD namespace, and RDF's language-string datatypes.
const XSD: &str = "http://www.w3.org/2001/XMLSchema#";
const LANG_STRING: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#langString";
const DIR_LANG_STRING: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#dirLangString";
const GEO_JSON: &str = "http://www.opengis.net/ont/geosparql#geoJSONLiteral";

/// The types derived from `xsd:integer`.
const INTEGER_TYPES: [&str; 13] = [
    "integer",
    "long",
    "int",
    "short",
    "byte",
    "nonNegativeInteger",
    "positiveInteger",
    "nonPositiveInteger",
    "negativeInteger",
    "unsignedLong",
    "unsignedInt",
    "unsignedShort",
    "unsignedByte",
];

/// Whether tokenizer `tokenizer` indexes literals of `datatype`, by the index-maintenance specification's table:
/// `int` the integers, `float` the doubles and floats, `bigfloat` the decimals, `bool` the booleans, the date
/// tokenizers the date-times and, but for `hour`, the dates, the string tokenizers the strings and language strings,
/// `geo` GeoJSON. A literal of another datatype is held and left unindexed.
pub fn tokenizer_takes(tokenizer: &str, datatype: &str) -> bool {
    let xsd = |local: &str| datatype.strip_prefix(XSD) == Some(local);
    match tokenizer {
        "int" => INTEGER_TYPES.iter().any(|t| xsd(t)),
        "float" => xsd("double") || xsd("float"),
        "bigfloat" => xsd("decimal"),
        "bool" => xsd("boolean"),
        "year" | "month" | "day" => xsd("dateTime") || xsd("dateTimeStamp") || xsd("date"),
        "hour" => xsd("dateTime") || xsd("dateTimeStamp"),
        "exact" | "hash" | "term" | "fulltext" | "trigram" | "ngram" | "sha256" => {
            xsd("string") || datatype == LANG_STRING || datatype == DIR_LANG_STRING
        }
        "geo" => datatype == GEO_JSON,
        _ => false,
    }
}
