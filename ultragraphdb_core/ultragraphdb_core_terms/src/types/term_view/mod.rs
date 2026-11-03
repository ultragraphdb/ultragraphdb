/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

mod term_view_literal;

use oxrdf::BaseDirection;
use oxsdatatypes::{
    Boolean, Date, DateTime, DayTimeDuration, Decimal, Double, Duration, Float, GDay, GMonth,
    GMonthDay, GYear, GYearMonth, Integer, Time, YearMonthDuration,
};

use ultragraphdb_core_ids::SystemId;

use crate::types::datatype::Datatype;
use crate::types::text::Text;
use crate::types::vector_view::VectorView;

/// What an encoded term holds, read without the dictionary: a system ID, a literal's value when it was encoded by value, or
/// its parts, each inline or as the hash naming its dictionary entry.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TermView<'a> {
    /// The system ID of an IRI or a blank node.
    Id(SystemId),
    /// An `xsd:boolean`, by value.
    Boolean(Boolean),
    /// An `xsd:integer`, by value.
    Integer(Integer),
    /// An `xsd:decimal`, by value.
    Decimal(Decimal),
    /// An `xsd:double`, by value.
    Double(Double),
    /// An `xsd:float`, by value.
    Float(Float),
    /// An `xsd:dateTime`, by value.
    DateTime(DateTime),
    /// An `xsd:time`, by value.
    Time(Time),
    /// An `xsd:date`, by value.
    Date(Date),
    /// An `xsd:gYearMonth`, by value.
    GYearMonth(GYearMonth),
    /// An `xsd:gYear`, by value.
    GYear(GYear),
    /// An `xsd:gMonthDay`, by value.
    GMonthDay(GMonthDay),
    /// An `xsd:gDay`, by value.
    GDay(GDay),
    /// An `xsd:gMonth`, by value.
    GMonth(GMonth),
    /// An `xsd:duration`, by value.
    Duration(Duration),
    /// An `xsd:yearMonthDuration`, by value.
    YearMonthDuration(YearMonthDuration),
    /// An `xsd:dayTimeDuration`, by value.
    DayTimeDuration(DayTimeDuration),
    /// An `xsd:string`.
    String(Text<'a>),
    /// A language-tagged string, with its base direction when it has one.
    LangString {
        /// The lexical form.
        lexical: Text<'a>,
        /// The language tag.
        language: Text<'a>,
        /// The base direction, when it has one.
        direction: Option<BaseDirection>,
    },
    /// A typed literal kept as its datatype and lexical form: of a datatype encoded by value whose lexical form is not
    /// canonical, or of any other datatype.
    Typed {
        /// The datatype.
        datatype: Datatype,
        /// The lexical form.
        lexical: Text<'a>,
    },
    /// A `ugdb:float32vector` literal, by value.
    Vector(VectorView<'a>),
}
