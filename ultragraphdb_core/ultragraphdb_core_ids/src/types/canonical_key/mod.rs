/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use crate::errors::id_error::IdError;

/// The bytes of the length that precedes the IRI.
const LENGTH_BYTES: usize = 4;

/// A key in its canonical encoding, the bytes a bucket is hashed from. Until UGS keys exist, every key is an IRI:
/// its UTF-8 bytes as RDF 1.2 writes the IRI, without angle brackets, after its length as a 4-byte big-endian count.
/// The key borrows the IRI; [`CanonicalKey::encode_into`] writes its bytes into the caller's buffer.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct CanonicalKey<'a> {
    iri: &'a str,
    length: u32,
}

impl<'a> CanonicalKey<'a> {
    /// The key of the IRI `iri`, written without angle brackets. The text is not checked as an IRI: the term
    /// layer parses IRIs; an empty one encodes as a length of 0.
    ///
    /// # Errors
    ///
    /// [`IdError::iri_too_long`] when `iri` is longer than `u32::MAX` bytes, which a 4-byte length cannot count.
    pub fn iri(iri: &'a str) -> Result<Self, IdError> {
        let length = Self::length(iri.len())?;
        Ok(Self { iri, length })
    }

    /// The 4-byte length an IRI of `len` bytes is encoded with, so a decoder can refuse an oversized IRI before it
    /// reads the IRI's bytes.
    ///
    /// # Errors
    ///
    /// [`IdError::iri_too_long`] when `len` is above `u32::MAX`, which a 4-byte length cannot count.
    pub fn length(len: usize) -> Result<u32, IdError> {
        u32::try_from(len).map_err(|_| IdError::iri_too_long(len))
    }

    /// The IRI the key encodes.
    #[must_use]
    pub fn as_str(&self) -> &'a str {
        self.iri
    }

    /// The length of the canonical encoding in bytes: 4 and the IRI's length.
    #[must_use]
    pub fn encoded_len(&self) -> usize {
        // The IRI's length fits in a u32, so adding 4 cannot overflow a 64-bit usize.
        self.iri.len().saturating_add(LENGTH_BYTES)
    }

    /// Appends the canonical encoding to `out`.
    pub fn encode_into(&self, out: &mut Vec<u8>) {
        out.reserve(self.encoded_len());
        out.extend_from_slice(&self.length_bytes());
        out.extend_from_slice(self.iri.as_bytes());
    }

    /// The IRI's length as 4 big-endian bytes, the encoding's first bytes.
    pub(crate) fn length_bytes(&self) -> [u8; LENGTH_BYTES] {
        self.length.to_be_bytes()
    }
}
