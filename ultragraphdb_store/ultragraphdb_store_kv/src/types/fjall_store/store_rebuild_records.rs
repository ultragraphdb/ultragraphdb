/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use fjall::Readable;
use ultragraphdb_error_store::StoreError;
use ultragraphdb_traits_store::RebuildRecord;

use crate::types::fjall_store::Shared;
use crate::types::fjall_store::store_meta::{REBUILD_PREFIX, rebuild_key};
use crate::utils::lock::lock;

impl Shared {
    /// Writes `record` to the metadata keyspace and syncs it, the writer lock held.
    pub fn write_rebuild_record(&self, record: &RebuildRecord) -> Result<(), StoreError> {
        let _writer = lock(&self.writer);
        self.refuse_if_poisoned()?;
        let mut batch = self.synced_batch();
        batch.insert(&self.meta, rebuild_key(record.id()), encode(record));
        self.commit(batch)
    }

    /// The rebuild records the metadata keyspace holds, by number.
    pub fn rebuild_records(&self) -> Result<Vec<RebuildRecord>, StoreError> {
        self.db
            .snapshot()
            .prefix(&self.meta, REBUILD_PREFIX)
            .map(|guard| {
                let (key, value) = guard
                    .into_inner()
                    .map_err(|e| StoreError::ReadFailed(REBUILD_PREFIX.to_vec(), e.to_string()))?;
                let id: [u8; 8] = key[REBUILD_PREFIX.len()..].try_into().map_err(|_| {
                    StoreError::Corrupted(
                        key.to_vec(),
                        "a rebuild record's number is eight bytes".into(),
                    )
                })?;
                let prefixes =
                    decode(&value).map_err(|reason| StoreError::Corrupted(key.to_vec(), reason))?;
                Ok(RebuildRecord::new(u64::from_be_bytes(id), prefixes))
            })
            .collect()
    }
}

/// A record's prefixes as a four-byte big-endian count, then each as a four-byte length and its bytes.
fn encode(record: &RebuildRecord) -> Vec<u8> {
    let len = |n: usize| u32::try_from(n).unwrap_or(u32::MAX).to_be_bytes();
    let mut out = len(record.prefixes().len()).to_vec();
    for p in record.prefixes() {
        out.extend_from_slice(&len(p.len()));
        out.extend_from_slice(p);
    }
    out
}

/// The prefixes `bytes` encode, or why they do not encode any.
fn decode(bytes: &[u8]) -> Result<Vec<Vec<u8>>, String> {
    let mut rest = bytes;
    let mut take = |n: usize| -> Result<&[u8], String> {
        if rest.len() < n {
            return Err(format!(
                "a rebuild record ends after {} of {n} bytes",
                rest.len()
            ));
        }
        let (head, tail) = rest.split_at(n);
        rest = tail;
        Ok(head)
    };
    let u32_of = |b: &[u8]| -> usize {
        let mut buf = [0u8; 4];
        buf.copy_from_slice(b);
        u32::from_be_bytes(buf) as usize
    };
    let count = u32_of(take(4)?);
    let mut prefixes = Vec::with_capacity(count.min(1024));
    for _ in 0..count {
        let n = u32_of(take(4)?);
        prefixes.push(take(n)?.to_vec());
    }
    if !rest.is_empty() {
        return Err(format!("{} bytes after the rebuild record", rest.len()));
    }
    Ok(prefixes)
}
