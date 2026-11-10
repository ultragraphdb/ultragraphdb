/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

mod encoded_term_cmp;
mod encoded_term_debug;
mod encoded_term_encode;
mod encoded_term_view;

use ultragraphdb_core_ids::SystemId;
use xxhash_rust::xxh3::xxh3_64;

use crate::errors::term_error::TermError;
use crate::types::known_datatype::KnownDatatype;
use crate::types::tagged_datatype::TaggedDatatype;

/// The tags an encoding begins with. Each fixes how many bytes follow, or where to read their length, so an encoding
/// inside a key ends where its tag says and a key needs no length field. They are stored: none changes meaning.
pub(crate) mod tag {
    /// An IRI or a blank node: its system ID's 16 big-endian bytes.
    pub const ID: u8 = 0x01;
    pub const BOOLEAN: u8 = 0x10;
    pub const INTEGER: u8 = 0x11;
    pub const DECIMAL: u8 = 0x12;
    pub const DOUBLE: u8 = 0x13;
    pub const FLOAT: u8 = 0x14;
    pub const DATE_TIME: u8 = 0x15;
    pub const TIME: u8 = 0x16;
    pub const DATE: u8 = 0x17;
    pub const G_YEAR_MONTH: u8 = 0x18;
    pub const G_YEAR: u8 = 0x19;
    pub const G_MONTH_DAY: u8 = 0x1A;
    pub const G_DAY: u8 = 0x1B;
    pub const G_MONTH: u8 = 0x1C;
    pub const DURATION: u8 = 0x1D;
    pub const YEAR_MONTH_DURATION: u8 = 0x1E;
    pub const DAY_TIME_DURATION: u8 = 0x1F;
    /// A string: its length and lexical form.
    pub const SMALL_STRING: u8 = 0x20;
    /// A string: the hash of its lexical form.
    pub const BIG_STRING: u8 = 0x21;
    /// The first of three language-string tags for each direction: none, left to right and right to left. The first
    /// holds both parts inline, the second the lexical form's hash and the tag inline, the third both hashes.
    pub const SMALL_LANG: [u8; 3] = [0x22, 0x25, 0x28];
    pub const BIG_LANG: [u8; 3] = [0x23, 0x26, 0x29];
    pub const BIG_LANG_BIG_TAG: [u8; 3] = [0x24, 0x27, 0x2A];
    /// A literal of a known datatype by its code: the length and lexical form, or the lexical form's hash.
    pub const SMALL_KNOWN: u8 = 0x30;
    pub const BIG_KNOWN: u8 = 0x31;
    /// A literal of any other datatype by its datatype's system ID: the length and lexical form, or the lexical form's
    /// hash.
    pub const SMALL_TYPED: u8 = 0x32;
    pub const BIG_TYPED: u8 = 0x33;
    /// A vector: its dimension, a little-endian u16, then its elements, little-endian 32-bit floats.
    pub const VECTOR: u8 = 0x40;
    /// A `bfloat16vector`: its dimension, a little-endian u16, then its elements, little-endian bfloat16s.
    pub const BF16_VECTOR: u8 = 0x41;
    /// A `measure` by value: its number, a big-endian binary64, then its unit's code as inline text.
    pub const MEASURE: u8 = 0x50;
    /// A `precision` by value: one byte, 1 millennium to 12 nanosecond.
    pub const PRECISION: u8 = 0x51;
}

/// The precisions a `precision` names (`PROTOCOL.md` §10.5): 1 millennium to 12 nanosecond.
const PRECISIONS: std::ops::RangeInclusive<u8> = 1..=12;

/// The length of an IRI's or a blank node's encoding: the tag and the ID's 16 bytes.
const ID_LEN: usize = 1 + SystemId::BYTES;
const _: () = assert!(ID_LEN <= 255, "an inline encoding's length is a u8");

/// The longest lexical form or language tag held inline; a longer one is hashed.
pub const INLINE_MAX: usize = 15;
/// The most bytes an encoding other than a vector's holds: one tag byte and 32 more.
pub const ENCODED_MAX: usize = 33;
/// The most elements a vector holds.
pub const VECTOR_MAX: usize = u16::MAX as usize;

/// A term as the store keeps it: an IRI or blank node by its system ID, a literal by its content. Every encoding but a
/// vector's is one tag byte and at most 32 more, held inline with no allocation; a vector's holds its elements, at
/// whatever length. Equal terms encode equally and unequal terms unequally, so terms compare by their encodings.
#[derive(Clone)]
pub struct EncodedTerm(Repr);

#[derive(Clone)]
enum Repr {
    Inline { len: u8, bytes: [u8; ENCODED_MAX] },
    Vector(Box<[u8]>),
}

impl EncodedTerm {
    /// The encoding of the IRI or blank node `id` names: the tag and the ID's 16 big-endian bytes.
    #[must_use]
    pub fn id(id: SystemId) -> Self {
        let mut bytes = [0u8; ENCODED_MAX];
        let (tag_byte, rest) = bytes.split_at_mut(1);
        tag_byte.fill(tag::ID);
        if let Some(payload) = rest.first_chunk_mut::<{ SystemId::BYTES }>() {
            *payload = id.to_be_bytes();
        }
        Self(Repr::Inline {
            len: ID_LEN as u8, // lossless: ID_LEN is at most 255, asserted beside it
            bytes,
        })
    }

    /// The encoding of `bytes`, which hold exactly one encoding.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, TermError> {
        let (term, rest) = Self::read(bytes)?;
        if rest.is_empty() {
            Ok(term)
        } else {
            Err(TermError::malformed(
                bytes.to_vec(),
                "bytes follow the encoding",
            ))
        }
    }

    /// Whether `bytes`, received from a peer, hold exactly one whole encoding (`PROTOCOL.md` §10.1): its tag listed,
    /// each inline length at most 15 and its text UTF-8, a boolean's byte 0 or 1, a date or time's timezone offset
    /// within 14 hours and a duration's months and seconds not of opposite signs (§10.5), a known datatype's code one
    /// of [`KnownDatatype`]'s or of the three §10.4 holds by tags of their own, a vector's dimension at least 1, a
    /// measure's unit code at least 1 byte, a precision's byte 1 to 12, and exactly the length its tag gives.
    /// [`EncodedTerm::from_bytes`] holds the rules of length alone, for the store's own keys.
    pub fn check(bytes: &[u8]) -> Result<(), TermError> {
        let _whole: Self = Self::from_bytes(bytes)?;
        let malformed = |reason| Err(TermError::malformed(bytes.to_vec(), reason));
        // An inline part at `at`: its length byte, then its bytes, which the length rules proved present.
        let inline_utf8 = |at: usize| {
            let n = bytes.get(at).copied().map_or(0, usize::from);
            bytes
                .get(at + 1..at + 1 + n)
                .is_some_and(|text| std::str::from_utf8(text).is_ok())
        };
        let known = |at: usize| {
            bytes.get(at).is_some_and(|code| {
                KnownDatatype::from_code(*code).is_some() || TaggedDatatype::from_code(*code).is_some()
            })
        };
        // A duration's i64 months at byte 1 and i128 seconds at byte 9, which the length rules proved present: a
        // duration is negative as a whole, so its parts never have opposite signs.
        let signs_differ = || {
            let months = bytes
                .get(1..9)
                .and_then(|b| b.try_into().ok())
                .map_or(0, i64::from_be_bytes);
            let seconds = bytes
                .get(9..25)
                .and_then(|b| b.try_into().ok())
                .map_or(0, i128::from_be_bytes);
            (months < 0 && seconds > 0) || (months > 0 && seconds < 0)
        };
        // A date or time's i16 timezone offset in minutes at byte 17, which the length rules proved present: XML
        // Schema's run from -14:00 to +14:00, and 0xFFFF, -1, marks none.
        let offset_beyond = || {
            bytes
                .get(17..19)
                .and_then(|b| b.try_into().ok())
                .map_or(0, i16::from_be_bytes)
                .unsigned_abs()
                > 14 * 60
        };
        let lexical_len = bytes.get(1).copied().map_or(0, usize::from);
        match bytes.first().copied().unwrap_or_default() {
            tag::BOOLEAN if bytes.get(1).is_none_or(|b| *b > 1) => {
                malformed("a boolean other than 0 or 1")
            }
            tag::DATE_TIME..=tag::G_MONTH if offset_beyond() => {
                malformed("a timezone offset beyond 14 hours")
            }
            tag::DURATION if signs_differ() => {
                malformed("a duration whose months and seconds differ in sign")
            }
            tag::SMALL_STRING if !inline_utf8(1) => malformed("inline text that is not UTF-8"),
            t if tag::SMALL_LANG.contains(&t)
                && !(inline_utf8(1) && inline_utf8(2 + lexical_len)) =>
            {
                malformed("inline text that is not UTF-8")
            }
            t if tag::BIG_LANG.contains(&t) && !inline_utf8(17) => {
                malformed("inline text that is not UTF-8")
            }
            tag::SMALL_KNOWN | tag::BIG_KNOWN if !known(1) => Err(TermError::unknown_datatype(
                bytes.get(1).copied().unwrap_or_default(),
            )),
            tag::SMALL_KNOWN if !inline_utf8(2) => malformed("inline text that is not UTF-8"),
            tag::SMALL_TYPED if !inline_utf8(17) => malformed("inline text that is not UTF-8"),
            // The unit's code after the 8-byte number: at least one byte, UTF-8.
            tag::MEASURE if bytes.get(9).is_none_or(|n| *n == 0) => {
                malformed("a measure without a unit")
            }
            tag::MEASURE if !inline_utf8(9) => malformed("inline text that is not UTF-8"),
            tag::PRECISION if bytes.get(1).is_none_or(|p| !PRECISIONS.contains(p)) => {
                malformed("a precision other than 1 to 12")
            }
            _ => Ok(()),
        }
    }

    /// The encoding `bytes` begin with, and the bytes after it: how a key holding a term is parsed.
    pub fn read(bytes: &[u8]) -> Result<(Self, &[u8]), TermError> {
        let len = encoded_len(bytes)?;
        let (encoding, rest) = bytes
            .split_at_checked(len)
            .ok_or_else(|| TermError::truncated(bytes.to_vec()))?;
        debug_assert!(!encoding.is_empty(), "an encoding holds at least its tag");
        // A vector's encoding may pass ENCODED_MAX; every other fits inline.
        let term = if matches!(encoding.first(), Some(&(tag::VECTOR | tag::BF16_VECTOR))) {
            Self(Repr::Vector(encoding.into()))
        } else {
            Self::inline(encoding)
        };
        Ok((term, rest))
    }

    /// An encoding of at most [`ENCODED_MAX`] bytes.
    pub(crate) fn inline(encoding: &[u8]) -> Self {
        debug_assert!(!encoding.is_empty(), "an encoding holds at least its tag");
        debug_assert!(
            encoding.len() <= ENCODED_MAX,
            "an inline encoding fits its array"
        );
        let mut bytes = [0u8; ENCODED_MAX];
        bytes[..encoding.len()].copy_from_slice(encoding);
        Self(Repr::Inline {
            len: encoding.len() as u8,
            bytes,
        })
    }

    pub(crate) fn vector(encoding: Vec<u8>) -> Self {
        Self(Repr::Vector(encoding.into_boxed_slice()))
    }

    /// The encoding's bytes.
    pub fn as_bytes(&self) -> &[u8] {
        match &self.0 {
            Repr::Inline { len, bytes } => &bytes[..*len as usize],
            Repr::Vector(bytes) => bytes,
        }
    }

    /// The system ID it names, when it encodes an IRI or a blank node whose ID is valid.
    #[must_use]
    pub fn as_id(&self) -> Option<SystemId> {
        match self.as_bytes().split_first() {
            Some((&tag::ID, payload)) => SystemId::decode(payload).ok(),
            _ => None,
        }
    }

    /// Whether it encodes a literal rather than an IRI or a blank node.
    pub fn is_literal(&self) -> bool {
        self.as_bytes().first() != Some(&tag::ID)
    }

    /// Whether it encodes a vector literal.
    pub fn is_vector(&self) -> bool {
        self.as_bytes().first() == Some(&tag::VECTOR)
    }

    /// The posting fingerprint: XXH3-64 of the encoding, which orders a list's literal postings.
    pub fn fingerprint(&self) -> u64 {
        xxh3_64(self.as_bytes())
    }
}

/// The length of the encoding `bytes` begin with, read from its tag, or why none fits.
fn encoded_len(bytes: &[u8]) -> Result<usize, TermError> {
    let truncated = || TermError::truncated(bytes.to_vec());
    let at = |i: usize| bytes.get(i).copied().map(usize::from).ok_or_else(truncated);
    let first = *bytes.first().ok_or_else(truncated)?;
    let inline_len = |i: usize| -> Result<usize, TermError> {
        let n = at(i)?;
        if n > INLINE_MAX {
            return Err(TermError::malformed(
                bytes.to_vec(),
                "an inline part longer than 15 bytes",
            ));
        }
        Ok(n)
    };
    let len = match first {
        tag::ID => ID_LEN,
        tag::BOOLEAN => 2,
        tag::INTEGER | tag::DOUBLE | tag::YEAR_MONTH_DURATION => 9,
        tag::DECIMAL | tag::DAY_TIME_DURATION => 17,
        tag::FLOAT => 5,
        tag::DATE_TIME..=tag::G_MONTH => 19,
        tag::DURATION => 25,
        tag::SMALL_STRING => 2 + inline_len(1)?,
        tag::BIG_STRING => 17,
        t if tag::SMALL_LANG.contains(&t) => {
            let lexical = inline_len(1)?;
            3 + lexical + inline_len(2 + lexical)?
        }
        t if tag::BIG_LANG.contains(&t) => 18 + inline_len(17)?,
        t if tag::BIG_LANG_BIG_TAG.contains(&t) => 33,
        tag::SMALL_KNOWN => 3 + inline_len(2)?,
        tag::BIG_KNOWN => 18,
        tag::SMALL_TYPED => 18 + inline_len(17)?,
        tag::BIG_TYPED => 33,
        tag::VECTOR => {
            let dimension = at(1)? | at(2)? << 8;
            if dimension == 0 {
                return Err(TermError::malformed(bytes.to_vec(), "an empty vector"));
            }
            3 + 4 * dimension
        }
        tag::BF16_VECTOR => {
            let dimension = at(1)? | at(2)? << 8;
            if dimension == 0 {
                return Err(TermError::malformed(bytes.to_vec(), "an empty vector"));
            }
            3 + 2 * dimension
        }
        tag::MEASURE => 10 + inline_len(9)?,
        tag::PRECISION => 2,
        other => return Err(TermError::unknown_tag(other)),
    };
    if bytes.len() < len {
        return Err(truncated());
    }
    Ok(len)
}
