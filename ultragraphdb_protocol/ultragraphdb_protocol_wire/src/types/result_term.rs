/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_core_terms::EncodedTerm;

/// A term of a result batch's term table: its encoding in `ultragraphdb_core_terms`, with the text the encoding names
/// but does not hold: an IRI's or a blank node's for a UID, the lexical form of a literal held by hash; and the
/// annotation, a language tag held by hash or the IRI of a datatype held by UID.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResultTerm {
    term: EncodedTerm,
    text: Option<String>,
    annotation: Option<String>,
}

impl ResultTerm {
    pub fn new(term: EncodedTerm) -> Self {
        Self {
            term,
            text: None,
            annotation: None,
        }
    }

    pub fn with_text(mut self, text: impl Into<String>) -> Self {
        self.text = Some(text.into());
        self
    }

    pub fn with_annotation(mut self, annotation: impl Into<String>) -> Self {
        self.annotation = Some(annotation.into());
        self
    }

    pub fn term(&self) -> &EncodedTerm {
        &self.term
    }

    pub fn text(&self) -> Option<&str> {
        self.text.as_deref()
    }

    pub fn annotation(&self) -> Option<&str> {
        self.annotation.as_deref()
    }
}
