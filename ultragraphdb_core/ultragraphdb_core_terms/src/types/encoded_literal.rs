/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use crate::types::encoded_term::EncodedTerm;
use crate::types::hash128::Hash128;

/// A literal's encoding with the strings it names by hash, which the writer enters in the dictionary in the same
/// batch: none for an inline or by-value literal, up to two for a long lexical form and a long language tag.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EncodedLiteral<'a> {
    term: EncodedTerm,
    strings: [Option<(Hash128, &'a str)>; 2],
}

impl<'a> EncodedLiteral<'a> {
    pub(crate) fn new(term: EncodedTerm, strings: [Option<(Hash128, &'a str)>; 2]) -> Self {
        Self { term, strings }
    }

    pub fn term(&self) -> &EncodedTerm {
        &self.term
    }

    pub fn into_term(self) -> EncodedTerm {
        self.term
    }

    /// The strings the encoding names by hash, each with its hash.
    pub fn strings(&self) -> impl Iterator<Item = (Hash128, &'a str)> + '_ {
        self.strings.iter().flatten().copied()
    }
}
