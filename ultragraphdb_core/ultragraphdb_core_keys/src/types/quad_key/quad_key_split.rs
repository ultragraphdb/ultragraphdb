/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use crate::errors::quad_key_error::QuadKeyError;
use crate::types::keyspace::{Keyspace, SPLIT_FLAG};
use crate::types::quad_key::QuadKey;
use crate::types::split_start::SplitStart;

impl QuadKey {
    /// The key of the part of this key's split list that starts at `start`: the key with the split flag set and the
    /// start appended, an ID as its 16 big-endian bytes or a fingerprint as 8.
    ///
    /// # Errors
    ///
    /// [`QuadKeyError::not_a_list`] for a key of a keyspace that holds no lists, and
    /// [`QuadKeyError::fingerprint_start`] for a fingerprint start on a value-index key, whose token runs to its
    /// start, so its start is always an ID's 16 bytes.
    pub fn split_bytes(&self, start: SplitStart) -> Result<Vec<u8>, QuadKeyError> {
        let mut key = self.to_bytes();
        let keyspace = self.keyspace();
        if !keyspace.is_list() {
            return Err(QuadKeyError::not_a_list(&key));
        }
        if let Some(first) = key.first_mut() {
            *first |= SPLIT_FLAG;
        }
        match start {
            SplitStart::Id(id) => key.extend_from_slice(&id.to_be_bytes()),
            SplitStart::Fingerprint(_) if keyspace == Keyspace::ValueIndex => {
                return Err(QuadKeyError::fingerprint_start(&self.to_bytes()));
            }
            SplitStart::Fingerprint(fingerprint) => {
                key.extend_from_slice(&fingerprint.to_be_bytes())
            }
        }
        debug_assert!(
            key.first().is_some_and(|b| b & SPLIT_FLAG != 0),
            "a split key carries the flag"
        );
        Ok(key)
    }
}
