/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_core_terms::EncodedTerm;

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

    /// The term, read from its encoding, which the batch's check proved whole; a client reads it through
    /// [`EncodedTerm::view`].
    pub fn term(&self) -> Option<EncodedTerm> {
        EncodedTerm::from_bytes(self.encoding).ok()
    }

    pub fn text(&self) -> Option<&'a str> {
        self.text
    }

    pub fn annotation(&self) -> Option<&'a str> {
        self.annotation
    }
}
