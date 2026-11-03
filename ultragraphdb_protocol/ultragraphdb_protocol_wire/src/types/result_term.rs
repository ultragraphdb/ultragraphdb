/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_core_terms::{EncodedTerm, Hash128};

/// A result's tag of an IRI, followed by the hash of its text.
pub(crate) const IRI_TAG: u8 = 0x01;

/// A result's tag of a blank node, followed by the hash of its skolem IRI; the store holds a blank node under the
/// IRI's tag.
pub(crate) const BLANK_NODE_TAG: u8 = 0x02;

/// What a result term's encoding holds after its tag.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Payload {
    /// A literal or a vector, in the store's own encoding.
    Literal(EncodedTerm),
    /// An IRI, by the hash of its text.
    Iri(Hash128),
    /// A blank node, by the hash of its skolem IRI.
    BlankNode(Hash128),
}

/// A term of a result batch's term table (`PROTOCOL.md` §10.1): a literal in `ultragraphdb_core_terms`' encoding, an
/// IRI or a blank node by the 16-byte hash of its text, never by the system ID the store names it by; with the text
/// the encoding names but does not hold (an IRI's text, a blank node's skolem IRI, the lexical form of a literal held
/// by hash) and the annotation, a language tag held by hash or the IRI of a datatype the store names by ID. A result
/// tells a blank node from an IRI by its own tag, `0x02`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResultTerm {
    payload: Payload,
    text: Option<String>,
    annotation: Option<String>,
}

impl ResultTerm {
    /// The literal or vector `term`, in the store's encoding, with no text and no annotation. An IRI or a blank node
    /// is built by [`ResultTerm::iri`] or [`ResultTerm::blank_node`], so its system ID never reaches a result.
    pub fn new(term: EncodedTerm) -> Self {
        Self {
            payload: Payload::Literal(term),
            text: None,
            annotation: None,
        }
    }

    /// The IRI `text`, named by its text's hash, the text carried beside it.
    pub fn iri(text: impl Into<String>) -> Self {
        let text = text.into();
        Self {
            payload: Payload::Iri(Hash128::of(text.as_bytes())),
            text: Some(text),
            annotation: None,
        }
    }

    /// The blank node named by its `skolem` IRI (`…/.well-known/genid/{bucket}-{random}`), which a request names it
    /// again by: the skolem IRI's hash, the skolem IRI carried beside it.
    pub fn blank_node(skolem: impl Into<String>) -> Self {
        let skolem = skolem.into();
        Self {
            payload: Payload::BlankNode(Hash128::of(skolem.as_bytes())),
            text: Some(skolem),
            annotation: None,
        }
    }

    /// The IRI or blank node a received batch names by `hash`, with the text the batch gives, as given.
    pub(crate) fn named(hash: Hash128, blank_node: bool, text: Option<&str>) -> Self {
        let payload = if blank_node {
            Payload::BlankNode(hash)
        } else {
            Payload::Iri(hash)
        };
        Self {
            payload,
            text: text.map(str::to_string),
            annotation: None,
        }
    }

    /// The term with the text its encoding names but does not hold.
    pub fn with_text(mut self, text: impl Into<String>) -> Self {
        self.text = Some(text.into());
        self
    }

    /// The term with its annotation.
    pub fn with_annotation(mut self, annotation: impl Into<String>) -> Self {
        self.annotation = Some(annotation.into());
        self
    }

    /// The literal or vector in the store's encoding; `None` for an IRI or a blank node, which a result names by hash.
    pub fn term(&self) -> Option<&EncodedTerm> {
        match &self.payload {
            Payload::Literal(term) => Some(term),
            Payload::Iri(_) | Payload::BlankNode(_) => None,
        }
    }

    /// The hash an IRI or a blank node is named by; `None` for a literal or a vector.
    pub fn hash(&self) -> Option<Hash128> {
        match &self.payload {
            Payload::Iri(hash) | Payload::BlankNode(hash) => Some(*hash),
            Payload::Literal(_) => None,
        }
    }

    /// Whether the term is a blank node.
    pub fn is_blank_node(&self) -> bool {
        matches!(self.payload, Payload::BlankNode(_))
    }

    /// The term's bytes in a result: a tag and the hash for an IRI (`0x01`) or a blank node (`0x02`), else the
    /// store's encoding.
    pub fn encoding(&self) -> Vec<u8> {
        match &self.payload {
            Payload::Literal(term) => term.as_bytes().to_vec(),
            Payload::Iri(hash) => [&[IRI_TAG][..], hash.as_bytes()].concat(),
            Payload::BlankNode(hash) => [&[BLANK_NODE_TAG][..], hash.as_bytes()].concat(),
        }
    }

    /// The text the encoding names but does not hold, if any.
    pub fn text(&self) -> Option<&str> {
        self.text.as_deref()
    }

    /// The annotation: a language tag held by hash or a datatype's IRI, if any.
    pub fn annotation(&self) -> Option<&str> {
        self.annotation.as_deref()
    }
}
