/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use oxrdf::BaseDirection;
use oxsdatatypes::{
    Boolean, Date, DateTime, DayTimeDuration, Decimal, Double, Duration, Float, GDay, GMonth,
    GMonthDay, GYear, GYearMonth, Integer, Time, YearMonthDuration,
};

use crate::errors::term_error::TermError;
use crate::types::datatype::Datatype;
use crate::types::encoded_term::{EncodedTerm, tag};
use crate::types::hash128::Hash128;
use crate::types::known_datatype::KnownDatatype;
use crate::types::term_view::TermView;
use crate::types::text::Text;
use crate::types::vector_view::VectorView;

impl EncodedTerm {
    /// What the encoding holds, read without the dictionary. Refused when an inline part is not UTF-8 or a known
    /// datatype's code is unknown.
    pub fn view(&self) -> Result<TermView<'_>, TermError> {
        let bytes = self.as_bytes();
        let body = &bytes[1..];
        let inline = |at: usize| inline(bytes, at);
        let hash = |at: usize| {
            let mut raw = [0u8; 16];
            raw.copy_from_slice(&body[at..at + 16]);
            Hash128::from_bytes(raw)
        };
        let u64_at = |at: usize| {
            let mut raw = [0u8; 8];
            raw.copy_from_slice(&body[at..at + 8]);
            u64::from_be_bytes(raw)
        };
        let direction = |set: usize| match set {
            0 => None,
            1 => Some(BaseDirection::Ltr),
            _ => Some(BaseDirection::Rtl),
        };
        let set_of = |tags: [u8; 3]| tags.iter().position(|t| *t == bytes[0]);
        let known =
            |code: u8| KnownDatatype::from_code(code).ok_or(TermError::UnknownDatatype(code));
        Ok(match bytes[0] {
            tag::UID => TermView::Uid(u64_at(0)),
            tag::BOOLEAN => TermView::Boolean(Boolean::from(body[0] != 0)),
            tag::INTEGER => TermView::Integer(Integer::from_be_bytes(fixed::<8>(body))),
            tag::DECIMAL => TermView::Decimal(Decimal::from_be_bytes(fixed::<16>(body))),
            tag::DOUBLE => TermView::Double(Double::from_be_bytes(fixed::<8>(body))),
            tag::FLOAT => TermView::Float(Float::from_be_bytes(fixed::<4>(body))),
            tag::DATE_TIME => TermView::DateTime(DateTime::from_be_bytes(fixed::<18>(body))),
            tag::TIME => TermView::Time(Time::from_be_bytes(fixed::<18>(body))),
            tag::DATE => TermView::Date(Date::from_be_bytes(fixed::<18>(body))),
            tag::G_YEAR_MONTH => TermView::GYearMonth(GYearMonth::from_be_bytes(fixed::<18>(body))),
            tag::G_YEAR => TermView::GYear(GYear::from_be_bytes(fixed::<18>(body))),
            tag::G_MONTH_DAY => TermView::GMonthDay(GMonthDay::from_be_bytes(fixed::<18>(body))),
            tag::G_DAY => TermView::GDay(GDay::from_be_bytes(fixed::<18>(body))),
            tag::G_MONTH => TermView::GMonth(GMonth::from_be_bytes(fixed::<18>(body))),
            tag::DURATION => TermView::Duration(Duration::from_be_bytes(fixed::<24>(body))),
            tag::YEAR_MONTH_DURATION => {
                TermView::YearMonthDuration(YearMonthDuration::from_be_bytes(fixed::<8>(body)))
            }
            tag::DAY_TIME_DURATION => {
                TermView::DayTimeDuration(DayTimeDuration::from_be_bytes(fixed::<16>(body)))
            }
            tag::SMALL_STRING => TermView::String(Text::Inline(inline(0)?.0)),
            tag::BIG_STRING => TermView::String(Text::Hashed(hash(0))),
            tag::SMALL_KNOWN => TermView::Typed {
                datatype: Datatype::Known(known(body[0])?),
                lexical: Text::Inline(inline(1)?.0),
            },
            tag::BIG_KNOWN => TermView::Typed {
                datatype: Datatype::Known(known(body[0])?),
                lexical: Text::Hashed(hash(1)),
            },
            tag::SMALL_TYPED => TermView::Typed {
                datatype: Datatype::Uid(u64_at(0)),
                lexical: Text::Inline(inline(8)?.0),
            },
            tag::BIG_TYPED => TermView::Typed {
                datatype: Datatype::Uid(u64_at(0)),
                lexical: Text::Hashed(hash(8)),
            },
            tag::VECTOR => TermView::Vector(VectorView::new(&body[2..])),
            t => {
                if let Some(set) = set_of(tag::SMALL_LANG) {
                    let (lexical, next) = inline(0)?;
                    TermView::LangString {
                        lexical: Text::Inline(lexical),
                        language: Text::Inline(inline(next)?.0),
                        direction: direction(set),
                    }
                } else if let Some(set) = set_of(tag::BIG_LANG) {
                    TermView::LangString {
                        lexical: Text::Hashed(hash(0)),
                        language: Text::Inline(inline(16)?.0),
                        direction: direction(set),
                    }
                } else if let Some(set) = set_of(tag::BIG_LANG_BIG_TAG) {
                    TermView::LangString {
                        lexical: Text::Hashed(hash(0)),
                        language: Text::Hashed(hash(16)),
                        direction: direction(set),
                    }
                } else {
                    return Err(TermError::UnknownTag(t));
                }
            }
        })
    }
}

/// The first `N` bytes of `body`, which the encoding's length, checked when it was read, guarantees.
fn fixed<const N: usize>(body: &[u8]) -> [u8; N] {
    let mut raw = [0u8; N];
    raw.copy_from_slice(&body[..N]);
    raw
}

/// The inline part at `at` of the body of `bytes`, and where the next part begins.
fn inline(bytes: &[u8], at: usize) -> Result<(&str, usize), TermError> {
    let body = &bytes[1..];
    let n = usize::from(body[at]);
    let text = std::str::from_utf8(&body[at + 1..at + 1 + n])
        .map_err(|_| TermError::Malformed(bytes.to_vec(), "an inline part is not UTF-8"))?;
    Ok((text, at + 1 + n))
}
