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

use crate::types::datatype::Datatype;
use crate::types::text::Text;
use crate::types::vector_view::VectorView;

/// What an encoded term holds, read without the dictionary: a UID, a literal's value when it was encoded by value, or
/// its parts, each inline or as the hash naming its dictionary entry.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TermView<'a> {
    /// The UID of an IRI or a blank node.
    Uid(u64),
    Boolean(Boolean),
    Integer(Integer),
    Decimal(Decimal),
    Double(Double),
    Float(Float),
    DateTime(DateTime),
    Time(Time),
    Date(Date),
    GYearMonth(GYearMonth),
    GYear(GYear),
    GMonthDay(GMonthDay),
    GDay(GDay),
    GMonth(GMonth),
    Duration(Duration),
    YearMonthDuration(YearMonthDuration),
    DayTimeDuration(DayTimeDuration),
    /// An `xsd:string`.
    String(Text<'a>),
    /// A language-tagged string, with its base direction when it has one.
    LangString {
        lexical: Text<'a>,
        language: Text<'a>,
        direction: Option<BaseDirection>,
    },
    /// A typed literal kept as its datatype and lexical form: of a datatype encoded by value whose lexical form is not
    /// canonical, or of any other datatype.
    Typed {
        datatype: Datatype,
        lexical: Text<'a>,
    },
    /// A `ugdb:float32vector` literal, by value.
    Vector(VectorView<'a>),
}
