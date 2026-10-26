/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

/// The base IRI of the system vocabulary, `ugdb:`: where the datatypes and IRIs the store defines itself live. Its
/// final value is the stage 4 design's open question; every IRI under it is formed from this constant.
pub const SYSTEM_VOCABULARY: &str = "urn:ultragraphdb:";

/// `ugdb:float32vector`, the datatype of a vector literal: its elements as 32-bit floats.
pub const FLOAT32_VECTOR: &str = "urn:ultragraphdb:float32vector";

const XSD: &str = "http://www.w3.org/2001/XMLSchema#";
const GEO: &str = "http://www.opengis.net/ont/geosparql#";

/// A datatype the encoding names by a one-byte code instead of its IRI's UID, so a literal of it is encoded with no
/// dictionary read: the XSD types encoded by value, the types derived from `xsd:integer`, and the datatypes the
/// tokenizers take. The codes are stored, so each keeps its code for good and a new datatype takes a new one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum KnownDatatype {
    Boolean,
    Integer,
    Decimal,
    Double,
    Float,
    DateTime,
    Time,
    Date,
    GYearMonth,
    GYear,
    GMonthDay,
    GDay,
    GMonth,
    Duration,
    YearMonthDuration,
    DayTimeDuration,
    Long,
    Int,
    Short,
    Byte,
    NonNegativeInteger,
    PositiveInteger,
    NonPositiveInteger,
    NegativeInteger,
    UnsignedLong,
    UnsignedInt,
    UnsignedShort,
    UnsignedByte,
    DateTimeStamp,
    GeoJsonLiteral,
    WktLiteral,
    Float32Vector,
    AnyUri,
}

/// Every known datatype with its code and IRI, in code order.
const TABLE: [(KnownDatatype, u8, &str); 33] = [
    (
        KnownDatatype::Boolean,
        1,
        "http://www.w3.org/2001/XMLSchema#boolean",
    ),
    (
        KnownDatatype::Integer,
        2,
        "http://www.w3.org/2001/XMLSchema#integer",
    ),
    (
        KnownDatatype::Decimal,
        3,
        "http://www.w3.org/2001/XMLSchema#decimal",
    ),
    (
        KnownDatatype::Double,
        4,
        "http://www.w3.org/2001/XMLSchema#double",
    ),
    (
        KnownDatatype::Float,
        5,
        "http://www.w3.org/2001/XMLSchema#float",
    ),
    (
        KnownDatatype::DateTime,
        6,
        "http://www.w3.org/2001/XMLSchema#dateTime",
    ),
    (
        KnownDatatype::Time,
        7,
        "http://www.w3.org/2001/XMLSchema#time",
    ),
    (
        KnownDatatype::Date,
        8,
        "http://www.w3.org/2001/XMLSchema#date",
    ),
    (
        KnownDatatype::GYearMonth,
        9,
        "http://www.w3.org/2001/XMLSchema#gYearMonth",
    ),
    (
        KnownDatatype::GYear,
        10,
        "http://www.w3.org/2001/XMLSchema#gYear",
    ),
    (
        KnownDatatype::GMonthDay,
        11,
        "http://www.w3.org/2001/XMLSchema#gMonthDay",
    ),
    (
        KnownDatatype::GDay,
        12,
        "http://www.w3.org/2001/XMLSchema#gDay",
    ),
    (
        KnownDatatype::GMonth,
        13,
        "http://www.w3.org/2001/XMLSchema#gMonth",
    ),
    (
        KnownDatatype::Duration,
        14,
        "http://www.w3.org/2001/XMLSchema#duration",
    ),
    (
        KnownDatatype::YearMonthDuration,
        15,
        "http://www.w3.org/2001/XMLSchema#yearMonthDuration",
    ),
    (
        KnownDatatype::DayTimeDuration,
        16,
        "http://www.w3.org/2001/XMLSchema#dayTimeDuration",
    ),
    (
        KnownDatatype::Long,
        17,
        "http://www.w3.org/2001/XMLSchema#long",
    ),
    (
        KnownDatatype::Int,
        18,
        "http://www.w3.org/2001/XMLSchema#int",
    ),
    (
        KnownDatatype::Short,
        19,
        "http://www.w3.org/2001/XMLSchema#short",
    ),
    (
        KnownDatatype::Byte,
        20,
        "http://www.w3.org/2001/XMLSchema#byte",
    ),
    (
        KnownDatatype::NonNegativeInteger,
        21,
        "http://www.w3.org/2001/XMLSchema#nonNegativeInteger",
    ),
    (
        KnownDatatype::PositiveInteger,
        22,
        "http://www.w3.org/2001/XMLSchema#positiveInteger",
    ),
    (
        KnownDatatype::NonPositiveInteger,
        23,
        "http://www.w3.org/2001/XMLSchema#nonPositiveInteger",
    ),
    (
        KnownDatatype::NegativeInteger,
        24,
        "http://www.w3.org/2001/XMLSchema#negativeInteger",
    ),
    (
        KnownDatatype::UnsignedLong,
        25,
        "http://www.w3.org/2001/XMLSchema#unsignedLong",
    ),
    (
        KnownDatatype::UnsignedInt,
        26,
        "http://www.w3.org/2001/XMLSchema#unsignedInt",
    ),
    (
        KnownDatatype::UnsignedShort,
        27,
        "http://www.w3.org/2001/XMLSchema#unsignedShort",
    ),
    (
        KnownDatatype::UnsignedByte,
        28,
        "http://www.w3.org/2001/XMLSchema#unsignedByte",
    ),
    (
        KnownDatatype::DateTimeStamp,
        29,
        "http://www.w3.org/2001/XMLSchema#dateTimeStamp",
    ),
    (
        KnownDatatype::GeoJsonLiteral,
        30,
        "http://www.opengis.net/ont/geosparql#geoJSONLiteral",
    ),
    (
        KnownDatatype::WktLiteral,
        31,
        "http://www.opengis.net/ont/geosparql#wktLiteral",
    ),
    (KnownDatatype::Float32Vector, 32, FLOAT32_VECTOR),
    (
        KnownDatatype::AnyUri,
        33,
        "http://www.w3.org/2001/XMLSchema#anyURI",
    ),
];

impl KnownDatatype {
    /// The known datatype `iri` names, if it is one.
    pub fn from_iri(iri: &str) -> Option<Self> {
        if !(iri.starts_with(XSD) || iri.starts_with(GEO) || iri.starts_with(SYSTEM_VOCABULARY)) {
            return None;
        }
        TABLE.iter().find(|(_, _, i)| *i == iri).map(|(d, _, _)| *d)
    }

    /// The known datatype of `code`, if one has it.
    pub fn from_code(code: u8) -> Option<Self> {
        TABLE
            .iter()
            .find(|(_, c, _)| *c == code)
            .map(|(d, _, _)| *d)
    }

    /// The stored code.
    pub fn code(self) -> u8 {
        self.entry().1
    }

    pub fn iri(self) -> &'static str {
        self.entry().2
    }

    /// Whether a canonical literal of the datatype is encoded by value.
    pub fn by_value(self) -> bool {
        self.code() <= KnownDatatype::DayTimeDuration.code()
    }

    fn entry(self) -> &'static (KnownDatatype, u8, &'static str) {
        &TABLE[self as usize]
    }
}
