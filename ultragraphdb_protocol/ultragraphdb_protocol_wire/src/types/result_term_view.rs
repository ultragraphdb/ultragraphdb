/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_core_terms::EncodedTerm;

use crate::types::result_term::BLANK_NODE_TAG;

/// The UID of `encoding`, a blank node's tag and a big-endian u64; `None` for any other bytes.
pub(crate) fn blank_uid(encoding: &[u8]) -> Option<u64> {
    match encoding {
        [BLANK_NODE_TAG, uid @ ..] => uid.try_into().ok().map(u64::from_be_bytes),
        _ => None,
    }
}

/// Whether `encoding`, a term of a received batch, holds exactly one whole encoding (`PROTOCOL.md` §10.1): a blank
/// node's tag and UID, or one of the store's encodings by [`EncodedTerm::check`].
pub(crate) fn result_encoding_whole(encoding: &[u8]) -> bool {
    blank_uid(encoding).is_some() || EncodedTerm::check(encoding).is_ok()
}

/// A term of a received result batch, read in place: its encoding, checked once, and its text and annotation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ResultTermView<'a> {
    encoding: &'a [u8],
    text: Option<&'a str>,
    annotation: Option<&'a str>,
}

impl<'a> ResultTermView<'a> {
    pub(crate) fn new(
        encoding: &'a [u8],
        text: Option<&'a str>,
        annotation: Option<&'a str>,
    ) -> Self {
        Self {
            encoding,
            text,
            annotation,
        }
    }

    /// The term's bytes in `ultragraphdb_core_terms`' encoding.
    pub fn encoding(&self) -> &'a [u8] {
        self.encoding
    }

    /// The term in the store's encoding, a blank node under the IRI's tag, read from its encoding, which the batch's
    /// check proved whole; a client reads it through [`EncodedTerm::view`].
    pub fn term(&self) -> Option<EncodedTerm> {
        match self.blank_uid() {
            Some(uid) => Some(EncodedTerm::uid(uid)),
            None => EncodedTerm::from_bytes(self.encoding).ok(),
        }
    }

    /// Whether the term is a blank node, tag `0x02`, named by its skolem IRI in [`ResultTermView::text`].
    pub fn is_blank_node(&self) -> bool {
        self.encoding.first() == Some(&BLANK_NODE_TAG)
    }

    /// A blank node's UID, read from its nine bytes; `None` for any other term.
    pub(crate) fn blank_uid(&self) -> Option<u64> {
        blank_uid(self.encoding)
    }

    pub fn text(&self) -> Option<&'a str> {
        self.text
    }

    pub fn annotation(&self) -> Option<&'a str> {
        self.annotation
    }
}
