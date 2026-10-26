/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use crate::errors::quad_key_error::QuadKeyError;
use crate::types::keyspace::SPLIT_FLAG;
use crate::types::quad_key::QuadKey;

impl QuadKey {
    /// The key of the part of this key's split list that starts at `start`: the key with the split flag set and the
    /// start, a UID or a literal's fingerprint, appended as a u64 big-endian. Refused for a key of a keyspace that
    /// holds no lists.
    pub fn split_bytes(&self, start: u64) -> Result<Vec<u8>, QuadKeyError> {
        let mut key = self.to_bytes();
        if !self.keyspace().is_list() {
            return Err(QuadKeyError::NotAList(&key));
        }
        key[0] |= SPLIT_FLAG;
        key.extend_from_slice(&start.to_be_bytes());
        Ok(key)
    }
}
