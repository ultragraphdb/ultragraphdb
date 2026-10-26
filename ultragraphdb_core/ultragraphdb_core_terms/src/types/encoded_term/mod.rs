/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

mod encoded_term_cmp;
mod encoded_term_debug;
mod encoded_term_encode;
mod encoded_term_view;

use ultragraphdb_common_utils::go_farm::fingerprint64;

use crate::errors::term_error::TermError;

/// The tags an encoding begins with. Each fixes how many bytes follow, or where to read their length, so an encoding
/// inside a key ends where its tag says and a key needs no length field. They are stored: none changes meaning.
pub(crate) mod tag {
    pub const UID: u8 = 0x01;
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
    /// A literal of any other datatype by its UID: the length and lexical form, or the lexical form's hash.
    pub const SMALL_TYPED: u8 = 0x32;
    pub const BIG_TYPED: u8 = 0x33;
    /// A vector: its dimension, a little-endian u16, then its elements, little-endian 32-bit floats.
    pub const VECTOR: u8 = 0x40;
}

/// The longest lexical form or language tag held inline; a longer one is hashed.
pub const INLINE_MAX: usize = 15;
/// The most bytes an encoding other than a vector's holds: one tag byte and 32 more.
pub const ENCODED_MAX: usize = 33;
/// The most elements a vector holds.
pub const VECTOR_MAX: usize = u16::MAX as usize;

/// A term as the store keeps it: an IRI or blank node by its UID, a literal by its content. Every encoding but a
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
    /// The encoding of the IRI or blank node `uid` names.
    pub fn uid(uid: u64) -> Self {
        let mut bytes = [0u8; ENCODED_MAX];
        bytes[0] = tag::UID;
        bytes[1..9].copy_from_slice(&uid.to_be_bytes());
        Self(Repr::Inline { len: 9, bytes })
    }

    /// The encoding of `bytes`, which hold exactly one encoding.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, TermError> {
        let (term, rest) = Self::read(bytes)?;
        if rest.is_empty() {
            Ok(term)
        } else {
            Err(TermError::Malformed(
                bytes.to_vec(),
                "bytes follow the encoding",
            ))
        }
    }

    /// The encoding `bytes` begin with, and the bytes after it: how a key holding a term is parsed.
    pub fn read(bytes: &[u8]) -> Result<(Self, &[u8]), TermError> {
        let len = encoded_len(bytes)?;
        let (encoding, rest) = bytes.split_at(len);
        let term = if encoding[0] == tag::VECTOR {
            Self(Repr::Vector(encoding.into()))
        } else {
            Self::inline(encoding)
        };
        Ok((term, rest))
    }

    /// An encoding of at most [`ENCODED_MAX`] bytes.
    pub(crate) fn inline(encoding: &[u8]) -> Self {
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

    pub fn as_bytes(&self) -> &[u8] {
        match &self.0 {
            Repr::Inline { len, bytes } => &bytes[..*len as usize],
            Repr::Vector(bytes) => bytes,
        }
    }

    /// The UID it names, when it encodes an IRI or a blank node.
    pub fn as_uid(&self) -> Option<u64> {
        let bytes = self.as_bytes();
        (bytes[0] == tag::UID).then(|| {
            let mut raw = [0u8; 8];
            raw.copy_from_slice(&bytes[1..9]);
            u64::from_be_bytes(raw)
        })
    }

    pub fn is_literal(&self) -> bool {
        self.as_bytes()[0] != tag::UID
    }

    pub fn is_vector(&self) -> bool {
        self.as_bytes()[0] == tag::VECTOR
    }

    /// The posting fingerprint: go-farm's `Fingerprint64` of the encoding, which orders a list's literal postings.
    pub fn fingerprint(&self) -> u64 {
        fingerprint64(self.as_bytes())
    }
}

/// The length of the encoding `bytes` begin with, read from its tag, or why none fits.
fn encoded_len(bytes: &[u8]) -> Result<usize, TermError> {
    let truncated = || TermError::Truncated(bytes.to_vec());
    let at = |i: usize| bytes.get(i).copied().map(usize::from).ok_or_else(truncated);
    let first = *bytes.first().ok_or_else(truncated)?;
    let inline_len = |i: usize| -> Result<usize, TermError> {
        let n = at(i)?;
        if n > INLINE_MAX {
            return Err(TermError::Malformed(
                bytes.to_vec(),
                "an inline part longer than 15 bytes",
            ));
        }
        Ok(n)
    };
    let len = match first {
        tag::UID => 9,
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
        tag::SMALL_TYPED => 10 + inline_len(9)?,
        tag::BIG_TYPED => 25,
        tag::VECTOR => {
            let dimension = at(1)? | at(2)? << 8;
            if dimension == 0 {
                return Err(TermError::Malformed(bytes.to_vec(), "an empty vector"));
            }
            3 + 4 * dimension
        }
        other => return Err(TermError::UnknownTag(other)),
    };
    if bytes.len() < len {
        return Err(truncated());
    }
    Ok(len)
}
