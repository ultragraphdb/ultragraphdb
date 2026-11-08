/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_core_terms::{EncodedTerm, Hash128};

use crate::types::result_term::{BLANK_NODE_TAG, IRI_TAG};

/// The hash `encoding` names an IRI or a blank node by, and whether it is a blank node: a tag `0x01` or `0x02` and 16
/// bytes; `None` for any other bytes.
pub(crate) fn named_hash(encoding: &[u8]) -> Option<(Hash128, bool)> {
    let (tag, hash) = encoding.split_first()?;
    let hash = Hash128::from_bytes(<[u8; 16]>::try_from(hash).ok()?);
    match *tag {
        IRI_TAG => Some((hash, false)),
        BLANK_NODE_TAG => Some((hash, true)),
        _ => None,
    }
}

/// Whether `encoding`, a term of a received batch, holds exactly one whole encoding (`PROTOCOL.md` §10.1): an IRI's or
/// a blank node's tag and hash, or one of the store's literal encodings by [`EncodedTerm::check`].
pub(crate) fn result_encoding_whole(encoding: &[u8]) -> bool {
    named_hash(encoding).is_some() || EncodedTerm::check(encoding).is_ok()
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

    /// The term's bytes as the result holds them.
    pub fn encoding(&self) -> &'a [u8] {
        self.encoding
    }

    /// The literal or vector in a Result's encoding, the store's but for a datatype no code names, which it names by
    /// the hash of its IRI (`PROTOCOL.md` §10.1); read from its encoding, which the batch's check proved whole. A client
    /// reads it through [`EncodedTerm::result_view`]. `None` for an IRI or a blank node, named by its hash.
    pub fn term(&self) -> Option<EncodedTerm> {
        match self.hash() {
            Some(_) => None,
            None => EncodedTerm::from_bytes(self.encoding).ok(),
        }
    }

    /// The hash of an IRI's text or a blank node's skolem IRI, which [`ResultTermView::text`] holds; `None` for a
    /// literal or a vector.
    #[must_use]
    pub fn hash(&self) -> Option<Hash128> {
        named_hash(self.encoding).map(|(hash, _)| hash)
    }

    /// Whether the term is a blank node, tag `0x02`, named by its skolem IRI in [`ResultTermView::text`].
    pub fn is_blank_node(&self) -> bool {
        self.encoding.first() == Some(&BLANK_NODE_TAG)
    }

    /// The text the encoding names but does not hold, if any.
    pub fn text(&self) -> Option<&'a str> {
        self.text
    }

    /// The annotation: a language tag held by hash or a datatype's IRI, if any.
    pub fn annotation(&self) -> Option<&'a str> {
        self.annotation
    }
}
