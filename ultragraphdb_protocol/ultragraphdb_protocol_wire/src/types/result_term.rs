/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_core_terms::EncodedTerm;

/// A result's tag of a blank node, which the store holds under the IRI's, `0x01`.
pub(crate) const BLANK_NODE_TAG: u8 = 0x02;

/// A term of a result batch's term table: its encoding in `ultragraphdb_core_terms`, with the text the encoding names
/// but does not hold: an IRI's text or a blank node's skolem IRI for a UID, the lexical form of a literal held by hash;
/// and the annotation, a language tag held by hash or the IRI of a datatype held by UID. The store keeps an IRI and a
/// blank node under one tag, `0x01`; a result tells a blank node apart by its own, `0x02`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResultTerm {
    term: EncodedTerm,
    blank_node: bool,
    text: Option<String>,
    annotation: Option<String>,
}

impl ResultTerm {
    /// The term of `term`, with no text and no annotation.
    pub fn new(term: EncodedTerm) -> Self {
        Self {
            term,
            blank_node: false,
            text: None,
            annotation: None,
        }
    }

    /// The blank node of `uid`, named by its `skolem` IRI (`…/.well-known/genid/{uid}`), which a request names it again
    /// by.
    pub fn blank_node(uid: u64, skolem: impl Into<String>) -> Self {
        Self {
            term: EncodedTerm::uid(uid),
            blank_node: true,
            text: Some(skolem.into()),
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

    /// The term in the store's encoding, a blank node under the IRI's tag.
    pub fn term(&self) -> &EncodedTerm {
        &self.term
    }

    /// Whether the term is a blank node.
    pub fn is_blank_node(&self) -> bool {
        self.blank_node
    }

    /// The term's bytes in a result: the store's encoding, a blank node's tag `0x02`.
    pub fn encoding(&self) -> Vec<u8> {
        let mut bytes = self.term.as_bytes().to_vec();
        if self.blank_node
            && let Some(tag) = bytes.first_mut()
        {
            *tag = BLANK_NODE_TAG;
        }
        bytes
    }

    /// The blank node of `uid` as a received batch names it, its skolem IRI as given: `None` when the batch's text
    /// names none.
    pub(crate) fn blank(uid: u64, text: Option<&str>) -> Self {
        Self {
            term: EncodedTerm::uid(uid),
            blank_node: true,
            text: text.map(str::to_string),
            annotation: None,
        }
    }

    /// The text the encoding names but does not hold, if any.
    pub fn text(&self) -> Option<&str> {
        self.text.as_deref()
    }

    /// The annotation: a language tag held by hash or a datatype's IRI held by UID, if any.
    pub fn annotation(&self) -> Option<&str> {
        self.annotation.as_deref()
    }
}
