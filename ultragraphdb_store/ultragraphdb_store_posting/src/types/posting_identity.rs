/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

/// What tells a posting apart in its list, in the list's order: its uid and, in a list that compares values, a value
/// posting's value. Where a list's iteration resumes ([`crate::List::iterate_after`]), since postings of one list may share
/// a uid.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PostingIdentity {
    uid: u128,
    value: Vec<u8>,
}

impl PostingIdentity {
    /// The identity of a posting keyed `uid` holding `value`, empty for a reference.
    pub fn new(uid: u128, value: Vec<u8>) -> Self {
        Self { uid, value }
    }

    /// The posting's uid.
    pub fn uid(&self) -> u128 {
        self.uid
    }

    /// The posting's value, empty for a reference and in a list that does not compare values.
    pub fn value(&self) -> &[u8] {
        &self.value
    }

    /// Becomes the identity `(uid, value)`, keeping its buffer.
    pub(crate) fn assign(&mut self, uid: u128, value: &[u8]) {
        self.uid = uid;
        self.value.clear();
        self.value.extend_from_slice(value);
    }
}
