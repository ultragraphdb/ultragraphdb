/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use oxrdf::vocab::xsd;
use oxrdf::{BaseDirection, LiteralRef, NamedNodeRef};
use oxsdatatypes::{
    Boolean, Date, DateTime, DayTimeDuration, Decimal, Double, Duration, Float, GDay, GMonth,
    GMonthDay, GYear, GYearMonth, Integer, Time, YearMonthDuration,
};
use ultragraphdb_core_ids::SystemId;

use crate::errors::term_error::TermError;
use crate::types::encoded_literal::EncodedLiteral;
use crate::types::encoded_term::{EncodedTerm, INLINE_MAX, tag};
use crate::types::hash128::Hash128;
use crate::types::known_datatype::{FLOAT32_VECTOR, KnownDatatype};
use crate::utils::canonical::canonical;
use crate::utils::vector::parse_vector;

/// The bytes of an encoding being built: at most 33 for any literal but a vector.
struct Builder {
    bytes: [u8; 33],
    len: usize,
}

impl Builder {
    fn new(tag: u8) -> Self {
        let mut bytes = [0u8; 33];
        let [first, ..] = &mut bytes;
        *first = tag;
        Self { bytes, len: 1 }
    }

    /// The builder with `part` appended, refused as malformed past 33 bytes.
    fn push(mut self, part: &[u8]) -> Result<Self, TermError> {
        let (slot, end) = self
            .len
            .checked_add(part.len())
            .and_then(|end| Some((self.bytes.get_mut(self.len..end)?, end)))
            .ok_or_else(|| TermError::malformed(part.to_vec(), "an encoding past 33 bytes"))?;
        slot.copy_from_slice(part);
        self.len = end;
        Ok(self)
    }

    /// An inline part: its length, then its bytes; refused as malformed past 15 bytes.
    fn inline(self, part: &str) -> Result<Self, TermError> {
        let len = u8::try_from(part.len())
            .ok()
            .filter(|len| usize::from(*len) <= INLINE_MAX)
            .ok_or_else(|| {
                TermError::malformed(
                    part.as_bytes().to_vec(),
                    "an inline part longer than 15 bytes",
                )
            })?;
        self.push(&[len])?.push(part.as_bytes())
    }

    fn build(self) -> Result<EncodedTerm, TermError> {
        EncodedTerm::inline(self.bytes.get(..self.len).unwrap_or(&self.bytes))
    }
}

impl EncodedTerm {
    /// The encoding of `literal`, with the strings it names by hash. A literal of a datatype encoded by value is
    /// encoded by value when its lexical form is canonical, and by its known datatype's code and lexical form
    /// otherwise; any other typed literal by its datatype's system ID, which `datatype_id` gives, and its lexical form. A
    /// lexical form or language tag over 15 bytes is hashed; a language tag over 15 bytes hashes the lexical form too.
    /// A vector is encoded by value and must be in its canonical form.
    pub fn literal<'a, E: From<TermError>>(
        literal: LiteralRef<'a>,
        datatype_id: impl FnOnce(NamedNodeRef<'a>) -> Result<SystemId, E>,
    ) -> Result<EncodedLiteral<'a>, E> {
        let lexical = literal.value();
        if let Some(language) = literal.language() {
            return Ok(lang_string(lexical, language, literal.direction())?);
        }
        let datatype = literal.datatype();
        if datatype == xsd::STRING {
            return Ok(if lexical.len() <= INLINE_MAX {
                EncodedLiteral::new(
                    Builder::new(tag::SMALL_STRING).inline(lexical)?.build()?,
                    [None, None],
                )
            } else {
                let hash = Hash128::of(lexical.as_bytes());
                EncodedLiteral::new(
                    Builder::new(tag::BIG_STRING)
                        .push(hash.as_bytes())?
                        .build()?,
                    [Some((hash, lexical)), None],
                )
            });
        }
        if datatype.as_str() == FLOAT32_VECTOR {
            let elements = parse_vector(lexical)?;
            let count = u16::try_from(elements.len()).map_err(|_| {
                TermError::invalid_vector(lexical.to_string(), "more than 65,535 elements")
            })?;
            // A u16 count of 4-byte elements after 3 bytes: the capacity never saturates.
            let capacity = usize::from(count).saturating_mul(4).saturating_add(3);
            let mut bytes = Vec::with_capacity(capacity);
            bytes.push(tag::VECTOR);
            bytes.extend_from_slice(&count.to_le_bytes());
            elements
                .iter()
                .for_each(|e| bytes.extend_from_slice(&e.to_le_bytes()));
            return Ok(EncodedLiteral::new(
                EncodedTerm::vector(bytes),
                [None, None],
            ));
        }
        let known = KnownDatatype::from_iri(datatype.as_str());
        if let Some(term) = known.and_then(|k| by_value(k, lexical)) {
            return Ok(EncodedLiteral::new(term?, [None, None]));
        }
        let (small, big, head) = match known {
            Some(k) => (tag::SMALL_KNOWN, tag::BIG_KNOWN, Head::Code(k.code())),
            None => (
                tag::SMALL_TYPED,
                tag::BIG_TYPED,
                Head::Id(datatype_id(datatype)?),
            ),
        };
        let head = |b: Builder| match head {
            Head::Code(code) => b.push(&[code]),
            Head::Id(id) => b.push(&id.to_be_bytes()),
        };
        Ok(if lexical.len() <= INLINE_MAX {
            EncodedLiteral::new(
                head(Builder::new(small))?.inline(lexical)?.build()?,
                [None, None],
            )
        } else {
            let hash = Hash128::of(lexical.as_bytes());
            EncodedLiteral::new(
                head(Builder::new(big))?.push(hash.as_bytes())?.build()?,
                [Some((hash, lexical)), None],
            )
        })
    }
}

/// How a typed literal names its datatype.
enum Head {
    Code(u8),
    Id(SystemId),
}

/// A language-tagged string, its direction choosing among the three sets of tags.
fn lang_string<'a>(
    lexical: &'a str,
    language: &'a str,
    direction: Option<BaseDirection>,
) -> Result<EncodedLiteral<'a>, TermError> {
    // Each set of tags holds the tag of no direction, of ltr and of rtl.
    let tag_of = |[none, ltr, rtl]: [u8; 3]| match direction {
        None => none,
        Some(BaseDirection::Ltr) => ltr,
        Some(BaseDirection::Rtl) => rtl,
    };
    if language.len() > INLINE_MAX {
        let (lex, lang) = (
            Hash128::of(lexical.as_bytes()),
            Hash128::of(language.as_bytes()),
        );
        return Ok(EncodedLiteral::new(
            Builder::new(tag_of(tag::BIG_LANG_BIG_TAG))
                .push(lex.as_bytes())?
                .push(lang.as_bytes())?
                .build()?,
            [Some((lex, lexical)), Some((lang, language))],
        ));
    }
    if lexical.len() > INLINE_MAX {
        let lex = Hash128::of(lexical.as_bytes());
        return Ok(EncodedLiteral::new(
            Builder::new(tag_of(tag::BIG_LANG))
                .push(lex.as_bytes())?
                .inline(language)?
                .build()?,
            [Some((lex, lexical)), None],
        ));
    }
    Ok(EncodedLiteral::new(
        Builder::new(tag_of(tag::SMALL_LANG))
            .inline(lexical)?
            .inline(language)?
            .build()?,
        [None, None],
    ))
}

/// The by-value encoding of `lexical` as `datatype`, when the datatype is encoded by value and the lexical form is
/// canonical.
/// The offset of a timezone one minute west, which a by-value payload cannot hold: its minutes, -1 as an i16, are the
/// payload's marker of no timezone (`PROTOCOL.md` §10.2). A canonical form ends with its timezone, so the form names it.
const WEST_ONE_MINUTE: &str = "-00:01";

/// February 29 of a gMonthDay, which XML Schema 1.1 takes (the day-of-month constraint) and oxsdatatypes 0.2.4
/// refuses as "29 is not a valid day of 2"; its payload is February 28's and one day.
const FEBRUARY_29: &str = "--02-29";

/// One day in a by-value payload's seconds × 10¹⁸.
const DAY: i128 = 86_400 * 1_000_000_000_000_000_000;

fn by_value(datatype: KnownDatatype, lexical: &str) -> Option<Result<EncodedTerm, TermError>> {
    let dated = matches!(
        datatype,
        KnownDatatype::DateTime
            | KnownDatatype::Time
            | KnownDatatype::Date
            | KnownDatatype::GYearMonth
            | KnownDatatype::GYear
            | KnownDatatype::GMonthDay
            | KnownDatatype::GDay
            | KnownDatatype::GMonth
    );
    if dated && lexical.ends_with(WEST_ONE_MINUTE) {
        return None;
    }
    if datatype == KnownDatatype::GMonthDay
        && let Some(timezone) = lexical.strip_prefix(FEBRUARY_29)
    {
        let mut bytes = canonical::<GMonthDay>(&format!("--02-28{timezone}"))?.to_be_bytes();
        let (instant, _) = bytes.split_first_chunk_mut::<16>()?;
        *instant = i128::from_be_bytes(*instant)
            .checked_add(DAY)?
            .to_be_bytes();
        return Some(encoded(tag::G_MONTH_DAY, &bytes));
    }
    canonical_term(datatype, lexical)
}

/// The by-value term of `lexical` in its canonical form, `None` for a datatype not kept by value.
fn canonical_term(
    datatype: KnownDatatype,
    lexical: &str,
) -> Option<Result<EncodedTerm, TermError>> {
    match datatype {
        KnownDatatype::Boolean => {
            canonical::<Boolean>(lexical).map(|v| encoded(tag::BOOLEAN, &[u8::from(bool::from(v))]))
        }
        KnownDatatype::Integer => {
            canonical::<Integer>(lexical).map(|v| encoded(tag::INTEGER, &v.to_be_bytes()))
        }
        KnownDatatype::Decimal => {
            canonical::<Decimal>(lexical).map(|v| encoded(tag::DECIMAL, &v.to_be_bytes()))
        }
        KnownDatatype::Double => {
            canonical::<Double>(lexical).map(|v| encoded(tag::DOUBLE, &v.to_be_bytes()))
        }
        KnownDatatype::Float => {
            canonical::<Float>(lexical).map(|v| encoded(tag::FLOAT, &v.to_be_bytes()))
        }
        KnownDatatype::DateTime => {
            canonical::<DateTime>(lexical).map(|v| encoded(tag::DATE_TIME, &v.to_be_bytes()))
        }
        KnownDatatype::Time => {
            canonical::<Time>(lexical).map(|v| encoded(tag::TIME, &v.to_be_bytes()))
        }
        KnownDatatype::Date => {
            canonical::<Date>(lexical).map(|v| encoded(tag::DATE, &v.to_be_bytes()))
        }
        KnownDatatype::GYearMonth => {
            canonical::<GYearMonth>(lexical).map(|v| encoded(tag::G_YEAR_MONTH, &v.to_be_bytes()))
        }
        KnownDatatype::GYear => {
            canonical::<GYear>(lexical).map(|v| encoded(tag::G_YEAR, &v.to_be_bytes()))
        }
        KnownDatatype::GMonthDay => {
            canonical::<GMonthDay>(lexical).map(|v| encoded(tag::G_MONTH_DAY, &v.to_be_bytes()))
        }
        KnownDatatype::GDay => {
            canonical::<GDay>(lexical).map(|v| encoded(tag::G_DAY, &v.to_be_bytes()))
        }
        KnownDatatype::GMonth => {
            canonical::<GMonth>(lexical).map(|v| encoded(tag::G_MONTH, &v.to_be_bytes()))
        }
        KnownDatatype::Duration => {
            canonical::<Duration>(lexical).map(|v| encoded(tag::DURATION, &v.to_be_bytes()))
        }
        KnownDatatype::YearMonthDuration => canonical::<YearMonthDuration>(lexical)
            .map(|v| encoded(tag::YEAR_MONTH_DURATION, &v.to_be_bytes())),
        KnownDatatype::DayTimeDuration => canonical::<DayTimeDuration>(lexical)
            .map(|v| encoded(tag::DAY_TIME_DURATION, &v.to_be_bytes())),
        KnownDatatype::Long
        | KnownDatatype::Int
        | KnownDatatype::Short
        | KnownDatatype::Byte
        | KnownDatatype::NonNegativeInteger
        | KnownDatatype::PositiveInteger
        | KnownDatatype::NonPositiveInteger
        | KnownDatatype::NegativeInteger
        | KnownDatatype::UnsignedLong
        | KnownDatatype::UnsignedInt
        | KnownDatatype::UnsignedShort
        | KnownDatatype::UnsignedByte
        | KnownDatatype::DateTimeStamp
        | KnownDatatype::GeoJsonLiteral
        | KnownDatatype::WktLiteral
        | KnownDatatype::Float32Vector
        | KnownDatatype::AnyUri => None,
    }
}

/// The term of tag `t` with payload `bytes`.
fn encoded(t: u8, bytes: &[u8]) -> Result<EncodedTerm, TermError> {
    Builder::new(t).push(bytes)?.build()
}
