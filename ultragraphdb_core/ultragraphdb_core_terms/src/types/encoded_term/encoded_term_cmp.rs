/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::cmp::Ordering;
use std::hash::{Hash, Hasher};

use crate::types::encoded_term::EncodedTerm;

impl PartialEq for EncodedTerm {
    fn eq(&self, other: &Self) -> bool {
        self.as_bytes() == other.as_bytes()
    }
}

impl Eq for EncodedTerm {}

impl Hash for EncodedTerm {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.as_bytes().hash(state);
    }
}

/// By the bytes of the encoding: the order a list's postings with equal fingerprints keep.
impl Ord for EncodedTerm {
    fn cmp(&self, other: &Self) -> Ordering {
        self.as_bytes().cmp(other.as_bytes())
    }
}

impl PartialOrd for EncodedTerm {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}
