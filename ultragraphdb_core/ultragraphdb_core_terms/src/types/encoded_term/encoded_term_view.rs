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
use crate::types::encoded_term::{EncodedTerm, INLINE_MAX, tag};
use crate::types::hash128::Hash128;
use crate::types::known_datatype::KnownDatatype;
use crate::types::term_view::TermView;
use crate::types::text::Text;
use crate::types::vector_view::VectorView;
use ultragraphdb_core_ids::SystemId;

impl EncodedTerm {
    /// What the encoding holds as the store keeps it, read without the dictionary: a datatype no code names, tags
    /// `0x32` and `0x33`, by its IRI's system ID. Refused when an inline part is not UTF-8, a known datatype's code is
    /// unknown or an ID is no valid system ID.
    pub fn view(&self) -> Result<TermView<'_>, TermError> {
        read_view(self.as_bytes(), |bytes| id_at(bytes, 0).map(Datatype::Id))
    }

    /// What the encoding holds as a Result holds it (`PROTOCOL.md` §10.1), read as [`EncodedTerm::view`] reads the
    /// store's but for one difference: a datatype no code names, tags `0x32` and `0x33`, by the hash of its IRI
    /// ([`Datatype::Hashed`]), whose text the Result's annotation holds. Refused as [`EncodedTerm::view`] refuses, and
    /// for tag `0x01`, an IRI or a blank node, which a Result names by its hash and no view reads.
    ///
    /// # Errors
    ///
    /// [`TermError::malformed`] for tag `0x01` and for an inline part that is not UTF-8; [`TermError::truncated`] when
    /// the bytes end before a part its tag names; [`TermError::unknown_datatype`] for a known datatype's code no
    /// datatype has; [`TermError::unknown_tag`] for a tag no encoding has.
    pub fn result_view(&self) -> Result<TermView<'_>, TermError> {
        let bytes = self.as_bytes();
        if bytes.first() == Some(&tag::ID) {
            return Err(TermError::malformed(
                bytes.to_vec(),
                "an IRI or a blank node, which a Result names by its hash",
            ));
        }
        read_view(bytes, |bytes| hash_at(bytes, 0).map(Datatype::Hashed))
    }
}

/// What `bytes`, one encoding, hold, a datatype no code names read by `other_datatype` from the bytes.
fn read_view(
    bytes: &[u8],
    other_datatype: impl Fn(&[u8]) -> Result<Datatype, TermError>,
) -> Result<TermView<'_>, TermError> {
    let body = bytes.get(1..).unwrap_or_default();
    let inline = |at: usize| inline(bytes, at);
    let hash = |at: usize| hash_at(bytes, at);
    let first = || {
        body.first()
            .copied()
            .ok_or_else(|| TermError::truncated(bytes.to_vec()))
    };
    let known = |code: u8| KnownDatatype::from_code(code).ok_or(TermError::unknown_datatype(code));
    Ok(match bytes.first().copied().unwrap_or_default() {
        tag::ID => TermView::Id(id_at(bytes, 0)?),
        tag::BOOLEAN => TermView::Boolean(Boolean::from(first()? != 0)),
        tag::INTEGER => TermView::Integer(Integer::from_be_bytes(fixed_at(bytes, 0)?)),
        tag::DECIMAL => TermView::Decimal(Decimal::from_be_bytes(fixed_at(bytes, 0)?)),
        tag::DOUBLE => TermView::Double(Double::from_be_bytes(fixed_at(bytes, 0)?)),
        tag::FLOAT => TermView::Float(Float::from_be_bytes(fixed_at(bytes, 0)?)),
        tag::DATE_TIME => TermView::DateTime(DateTime::from_be_bytes(fixed_at(bytes, 0)?)),
        tag::TIME => TermView::Time(Time::from_be_bytes(fixed_at(bytes, 0)?)),
        tag::DATE => TermView::Date(Date::from_be_bytes(fixed_at(bytes, 0)?)),
        tag::G_YEAR_MONTH => TermView::GYearMonth(GYearMonth::from_be_bytes(fixed_at(bytes, 0)?)),
        tag::G_YEAR => TermView::GYear(GYear::from_be_bytes(fixed_at(bytes, 0)?)),
        tag::G_MONTH_DAY => TermView::GMonthDay(GMonthDay::from_be_bytes(fixed_at(bytes, 0)?)),
        tag::G_DAY => TermView::GDay(GDay::from_be_bytes(fixed_at(bytes, 0)?)),
        tag::G_MONTH => TermView::GMonth(GMonth::from_be_bytes(fixed_at(bytes, 0)?)),
        tag::DURATION => TermView::Duration(Duration::from_be_bytes(fixed_at(bytes, 0)?)),
        tag::YEAR_MONTH_DURATION => {
            TermView::YearMonthDuration(YearMonthDuration::from_be_bytes(fixed_at(bytes, 0)?))
        }
        tag::DAY_TIME_DURATION => {
            TermView::DayTimeDuration(DayTimeDuration::from_be_bytes(fixed_at(bytes, 0)?))
        }
        tag::SMALL_STRING => TermView::String(Text::Inline(inline(0)?.0)),
        tag::BIG_STRING => TermView::String(Text::Hashed(hash(0)?)),
        tag::SMALL_KNOWN => TermView::Typed {
            datatype: Datatype::Known(known(first()?)?),
            lexical: Text::Inline(inline(1)?.0),
        },
        tag::BIG_KNOWN => TermView::Typed {
            datatype: Datatype::Known(known(first()?)?),
            lexical: Text::Hashed(hash(1)?),
        },
        tag::SMALL_TYPED => TermView::Typed {
            datatype: other_datatype(bytes)?,
            lexical: Text::Inline(inline(16)?.0),
        },
        tag::BIG_TYPED => TermView::Typed {
            datatype: other_datatype(bytes)?,
            lexical: Text::Hashed(hash(16)?),
        },
        tag::VECTOR => TermView::Vector(VectorView::new(body.get(2..).unwrap_or_default())),
        _ => lang_string(bytes)?,
    })
}

/// The system ID at `at` in the body of `bytes`, refused when its 16 bytes are no valid ID.
fn id_at(bytes: &[u8], at: usize) -> Result<SystemId, TermError> {
    SystemId::decode(&fixed_at::<{ SystemId::BYTES }>(bytes, at)?)
        .map_err(|_| TermError::malformed(bytes.to_vec(), "an ID that is no valid system ID"))
}

/// The 16-byte hash at `at` in the body of `bytes`.
fn hash_at(bytes: &[u8], at: usize) -> Result<Hash128, TermError> {
    fixed_at::<16>(bytes, at).map(Hash128::from_bytes)
}

/// The base direction the tag set at `set` encodes: none, left to right, right to left.
fn direction(set: usize) -> Option<BaseDirection> {
    match set {
        0 => None,
        1 => Some(BaseDirection::Ltr),
        _ => Some(BaseDirection::Rtl),
    }
}

/// The language-tagged string `bytes` encodes, refused when its tag is no tag at all.
fn lang_string(bytes: &[u8]) -> Result<TermView<'_>, TermError> {
    let first = bytes.first().copied().unwrap_or_default();
    let set_of = |tags: [u8; 3]| tags.iter().position(|t| *t == first);
    if let Some(set) = set_of(tag::SMALL_LANG) {
        let (lexical, next) = inline(bytes, 0)?;
        Ok(TermView::LangString {
            lexical: Text::Inline(lexical),
            language: Text::Inline(inline(bytes, next)?.0),
            direction: direction(set),
        })
    } else if let Some(set) = set_of(tag::BIG_LANG) {
        Ok(TermView::LangString {
            lexical: Text::Hashed(hash_at(bytes, 0)?),
            language: Text::Inline(inline(bytes, 16)?.0),
            direction: direction(set),
        })
    } else if let Some(set) = set_of(tag::BIG_LANG_BIG_TAG) {
        Ok(TermView::LangString {
            lexical: Text::Hashed(hash_at(bytes, 0)?),
            language: Text::Hashed(hash_at(bytes, 16)?),
            direction: direction(set),
        })
    } else {
        Err(TermError::unknown_tag(first))
    }
}

/// The `N` bytes at `at` in the body of `bytes`, after the tag, which the encoding's length, checked when it was read,
/// guarantees; refused as truncated otherwise.
fn fixed_at<const N: usize>(bytes: &[u8], at: usize) -> Result<[u8; N], TermError> {
    at.checked_add(1)
        .and_then(|start| bytes.get(start..start.checked_add(N)?))
        .and_then(|raw| <[u8; N]>::try_from(raw).ok())
        .ok_or_else(|| TermError::truncated(bytes.to_vec()))
}

/// The inline part at `at` of the body of `bytes`, and where the next part begins.
fn inline(bytes: &[u8], at: usize) -> Result<(&str, usize), TermError> {
    let truncated = || TermError::truncated(bytes.to_vec());
    let [n] = fixed_at::<1>(bytes, at)?;
    let start = at + 2;
    let end = start + usize::from(n);
    let text = std::str::from_utf8(bytes.get(start..end).ok_or_else(truncated)?)
        .map_err(|_| TermError::malformed(bytes.to_vec(), "an inline part is not UTF-8"))?;
    debug_assert!(
        text.len() <= INLINE_MAX,
        "an inline part is at most 15 bytes"
    );
    Ok((text, at + 1 + usize::from(n)))
}
