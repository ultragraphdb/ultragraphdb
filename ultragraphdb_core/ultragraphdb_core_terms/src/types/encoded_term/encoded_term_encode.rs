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
        bytes[0] = tag;
        Self { bytes, len: 1 }
    }

    fn push(mut self, part: &[u8]) -> Self {
        self.bytes[self.len..self.len + part.len()].copy_from_slice(part);
        self.len += part.len();
        self
    }

    /// An inline part: its length, then its bytes.
    fn inline(self, part: &str) -> Self {
        self.push(&[part.len() as u8]).push(part.as_bytes())
    }

    fn build(self) -> EncodedTerm {
        EncodedTerm::inline(&self.bytes[..self.len])
    }
}

impl EncodedTerm {
    /// The encoding of `literal`, with the strings it names by hash. A literal of a datatype encoded by value is
    /// encoded by value when its lexical form is canonical, and by its known datatype's code and lexical form
    /// otherwise; any other typed literal by its datatype's UID, which `datatype_uid` gives, and its lexical form. A
    /// lexical form or language tag over 15 bytes is hashed; a language tag over 15 bytes hashes the lexical form too.
    /// A vector is encoded by value and must be in its canonical form.
    pub fn literal<'a, E: From<TermError>>(
        literal: LiteralRef<'a>,
        datatype_uid: impl FnOnce(NamedNodeRef<'a>) -> Result<u64, E>,
    ) -> Result<EncodedLiteral<'a>, E> {
        let lexical = literal.value();
        if let Some(language) = literal.language() {
            return Ok(lang_string(lexical, language, literal.direction()));
        }
        let datatype = literal.datatype();
        if datatype == xsd::STRING {
            return Ok(if lexical.len() <= INLINE_MAX {
                EncodedLiteral::new(
                    Builder::new(tag::SMALL_STRING).inline(lexical).build(),
                    [None, None],
                )
            } else {
                let hash = Hash128::of(lexical.as_bytes());
                EncodedLiteral::new(
                    Builder::new(tag::BIG_STRING).push(hash.as_bytes()).build(),
                    [Some((hash, lexical)), None],
                )
            });
        }
        if datatype.as_str() == FLOAT32_VECTOR {
            let elements = parse_vector(lexical)?;
            let mut bytes = Vec::with_capacity(3 + 4 * elements.len());
            bytes.push(tag::VECTOR);
            bytes.extend_from_slice(&(elements.len() as u16).to_le_bytes());
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
            return Ok(EncodedLiteral::new(term, [None, None]));
        }
        let (small, big, head) = match known {
            Some(k) => (tag::SMALL_KNOWN, tag::BIG_KNOWN, Head::Code(k.code())),
            None => (
                tag::SMALL_TYPED,
                tag::BIG_TYPED,
                Head::Uid(datatype_uid(datatype)?),
            ),
        };
        let head = |b: Builder| match head {
            Head::Code(code) => b.push(&[code]),
            Head::Uid(uid) => b.push(&uid.to_be_bytes()),
        };
        Ok(if lexical.len() <= INLINE_MAX {
            EncodedLiteral::new(
                head(Builder::new(small)).inline(lexical).build(),
                [None, None],
            )
        } else {
            let hash = Hash128::of(lexical.as_bytes());
            EncodedLiteral::new(
                head(Builder::new(big)).push(hash.as_bytes()).build(),
                [Some((hash, lexical)), None],
            )
        })
    }
}

/// How a typed literal names its datatype.
enum Head {
    Code(u8),
    Uid(u64),
}

/// A language-tagged string, its direction choosing among the three sets of tags.
fn lang_string<'a>(
    lexical: &'a str,
    language: &'a str,
    direction: Option<BaseDirection>,
) -> EncodedLiteral<'a> {
    let set = match direction {
        None => 0,
        Some(BaseDirection::Ltr) => 1,
        Some(BaseDirection::Rtl) => 2,
    };
    if language.len() > INLINE_MAX {
        let (lex, lang) = (
            Hash128::of(lexical.as_bytes()),
            Hash128::of(language.as_bytes()),
        );
        return EncodedLiteral::new(
            Builder::new(tag::BIG_LANG_BIG_TAG[set])
                .push(lex.as_bytes())
                .push(lang.as_bytes())
                .build(),
            [Some((lex, lexical)), Some((lang, language))],
        );
    }
    if lexical.len() > INLINE_MAX {
        let lex = Hash128::of(lexical.as_bytes());
        return EncodedLiteral::new(
            Builder::new(tag::BIG_LANG[set])
                .push(lex.as_bytes())
                .inline(language)
                .build(),
            [Some((lex, lexical)), None],
        );
    }
    EncodedLiteral::new(
        Builder::new(tag::SMALL_LANG[set])
            .inline(lexical)
            .inline(language)
            .build(),
        [None, None],
    )
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

fn by_value(datatype: KnownDatatype, lexical: &str) -> Option<EncodedTerm> {
    let encoded = |t: u8, bytes: &[u8]| Builder::new(t).push(bytes).build();
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
        _ => None,
    }
}
