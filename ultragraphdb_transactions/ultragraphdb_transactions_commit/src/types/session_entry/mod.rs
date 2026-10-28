/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_error_store::StoreError;
use ultragraphdb_protocol_wire::OperationResult;

/// The fixed part of an entry: request, version, result count and 4 zero bytes.
const FIXED: usize = 24;
/// The size of one result.
const RESULT: usize = 16;

/// A session's entry in the keyspace without versions (7.8): its last committed request and that request's whole
/// reply, the commit version and every commit-time operation's result, so a retry after a restart is answered byte
/// for byte as the original was. Request 0 is a session registered and not yet committed, with no version and no
/// results.
///
/// The entry sits at [`SessionEntry::key`] under the system prefix, outside every namespace, so no drop removes it;
/// [`SessionEntry::INDEX_KEY`] holds the numbers of the sessions held, ascending, 8 bytes each, little-endian. An
/// entry's bytes, little-endian: `request` u64, `version` u64, the result count u32, 4 zero bytes, then each result's
/// `uid` u64, `created` u8 (0 or 1) and 7 zero bytes, as a Commit reply holds them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionEntry {
    request: u64,
    version: u64,
    results: Vec<OperationResult>,
}

impl SessionEntry {
    /// The key holding the numbers of the sessions held.
    pub const INDEX_KEY: &'static [u8] = b"\xffultragraphdb/sessions";

    /// The prefix of every session's key.
    pub const KEY_PREFIX: &'static [u8] = b"\xffultragraphdb/session/";

    pub fn new(request: u64, version: u64, results: Vec<OperationResult>) -> Self {
        Self {
            request,
            version,
            results,
        }
    }

    /// The key of session `session`'s entry: the prefix, then the number big-endian, so keys sort by number.
    pub fn key(session: u64) -> Vec<u8> {
        let mut key = Vec::with_capacity(Self::KEY_PREFIX.len() + 8);
        key.extend_from_slice(Self::KEY_PREFIX);
        key.extend_from_slice(&session.to_be_bytes());
        key
    }

    pub fn request(&self) -> u64 {
        self.request
    }

    pub fn version(&self) -> u64 {
        self.version
    }

    pub fn results(&self) -> &[OperationResult] {
        &self.results
    }

    pub fn into_results(self) -> Vec<OperationResult> {
        self.results
    }

    /// The entry's bytes.
    pub fn encode(&self) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(FIXED + RESULT * self.results.len());
        bytes.extend_from_slice(&self.request.to_le_bytes());
        bytes.extend_from_slice(&self.version.to_le_bytes());
        // A session's results are those of one transaction's operations, far below u32.
        bytes.extend_from_slice(&(self.results.len() as u32).to_le_bytes());
        bytes.extend_from_slice(&[0; 4]);
        for result in &self.results {
            bytes.extend_from_slice(&result.uid().to_le_bytes());
            bytes.push(u8::from(result.created()));
            bytes.extend_from_slice(&[0; 7]);
        }
        bytes
    }

    /// The entry `bytes` hold, stored at `key`.
    ///
    /// # Errors
    ///
    /// [`StoreError::Corrupted`] naming `key` for bytes shorter than the fixed part, results not filling the rest
    /// exactly, a padding byte not zero, a `created` above 1, a request 0 with a version or results, or a committed
    /// request without a version.
    pub fn decode(key: &[u8], bytes: &[u8]) -> Result<Self, StoreError> {
        let corrupted = |reason: &str| StoreError::Corrupted(key.to_vec(), reason.to_string());
        let (Some(request), Some(version), Some(count), Some(padding)) = (
            bytes.get(..8),
            bytes.get(8..16),
            bytes.get(16..20),
            bytes.get(20..FIXED),
        ) else {
            return Err(corrupted("a session entry shorter than its fixed part"));
        };
        let request = u64::from_le_bytes(request.try_into().unwrap_or_default());
        let version = u64::from_le_bytes(version.try_into().unwrap_or_default());
        let count = u32::from_le_bytes(count.try_into().unwrap_or_default()) as usize;
        let records = bytes.get(FIXED..).unwrap_or_default();
        if count.checked_mul(RESULT) != Some(records.len()) {
            return Err(corrupted("a session entry whose results do not fill it"));
        }
        if padding.iter().any(|b| *b != 0)
            || records
                .chunks_exact(RESULT)
                .any(|r| r[9..].iter().any(|b| *b != 0))
        {
            return Err(corrupted("a session entry's padding is not zero"));
        }
        if records.chunks_exact(RESULT).any(|r| r[8] > 1) {
            return Err(corrupted("a session entry's result has a created above 1"));
        }
        if request == 0 && (version != 0 || count != 0) {
            return Err(corrupted(
                "a session entry of request 0 with a version or results",
            ));
        }
        if request != 0 && version == 0 {
            return Err(corrupted(
                "a session entry of a committed request without a version",
            ));
        }
        let results = records
            .chunks_exact(RESULT)
            .map(|r| {
                let uid = u64::from_le_bytes(r[..8].try_into().unwrap_or_default());
                OperationResult::new(uid, r[8] == 1)
            })
            .collect();
        Ok(Self::new(request, version, results))
    }
}
