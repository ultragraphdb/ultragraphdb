/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

mod fjall_scan_iterator;

use std::collections::{BTreeSet, VecDeque};
use std::sync::Arc;

use fjall::{Iter, Slice};
use ultragraphdb_error_store::StoreError;
use ultragraphdb_traits_store::{Item, Scan, key_namespace};

use crate::utils::stored_key::decode_stored_key;
use crate::utils::stored_value::decode_value;

/// One version read from the fjall iterator: the user key, the version and the stored value.
type Read = (Vec<u8>, u64, Slice);

/// The items of a scan of a [`crate::FjallSnapshot`], read lazily from its fjall snapshot: each key once with its
/// newest version at or below the read timestamp, or with each of its versions newest first, in the scan's
/// direction. A failed read is the last item.
pub struct FjallScan {
    iter: Option<Iter>,
    scan: Scan,
    read_ts: u64,
    banned: Arc<BTreeSet<u64>>,
    pending: VecDeque<Result<Item, StoreError>>,
    /// The user key being read, and whether the scan is done with it.
    key: Option<Vec<u8>>,
    key_done: bool,
    /// In reverse, the first version read of the next key, which fjall yields oldest first.
    carry: Option<Read>,
}

impl FjallScan {
    pub(crate) fn new(iter: Iter, scan: Scan, read_ts: u64, banned: Arc<BTreeSet<u64>>) -> Self {
        Self {
            iter: Some(iter),
            scan,
            read_ts,
            banned,
            pending: VecDeque::new(),
            key: None,
            key_done: false,
            carry: None,
        }
    }

    /// A scan of nothing.
    pub(crate) fn empty() -> Self {
        Self {
            iter: None,
            scan: Scan::prefix(Vec::new()),
            read_ts: 0,
            banned: Arc::default(),
            pending: VecDeque::new(),
            key: None,
            key_done: true,
            carry: None,
        }
    }

    /// The user key, version and stored value of the next item of the fjall iterator, from its front or its back;
    /// `None` at the end.
    fn read(&mut self, back: bool) -> Option<Result<Read, StoreError>> {
        let iter = self.iter.as_mut()?;
        let guard = if back { iter.next_back() } else { iter.next() }?;
        let at = || {
            self.key
                .clone()
                .unwrap_or_else(|| self.scan.start().to_vec())
        };
        Some(match guard.into_inner() {
            Err(e) => Err(StoreError::ReadFailed(at(), e.to_string())),
            Ok((stored, bytes)) => decode_stored_key(&stored)
                .map(|(key, version)| (key, version, bytes))
                .map_err(|reason| StoreError::Corrupted(stored.to_vec(), reason)),
        })
    }

    /// Takes the next version of the key being read, versions of one key arriving newest first.
    fn consume(&mut self, key: Vec<u8>, version: u64, bytes: &[u8]) {
        if self.key.as_deref() != Some(key.as_slice()) {
            self.key_done = !self.scan.selects(&key)
                || key_namespace(&key).is_some_and(|ns| self.banned.contains(&ns));
            self.key = Some(key);
        }
        if self.key_done || version > self.read_ts {
            return;
        }
        let key = self.key.clone().unwrap_or_default();
        match decode_value(bytes) {
            Err(reason) => self
                .pending
                .push_back(Err(StoreError::Corrupted(key, reason))),
            Ok(v) if v.deletion => self.key_done = true,
            Ok(v) => {
                self.pending.push_back(Ok(Item::new(
                    key,
                    version,
                    v.meta,
                    v.discard,
                    v.value.to_vec(),
                )));
                self.key_done = !self.scan.is_all_versions();
            }
        }
    }

    /// Reads the next key in reverse: its versions until the first of the key before it, consumed newest first.
    fn read_back_key(&mut self) -> Result<bool, StoreError> {
        let mut group: Vec<Read> = self.carry.take().into_iter().collect();
        loop {
            match self.read(true) {
                None => {
                    self.iter = None;
                    break;
                }
                Some(item) => {
                    let item = item?;
                    if group.first().is_some_and(|(k, _, _)| *k != item.0) {
                        self.carry = Some(item);
                        break;
                    }
                    group.push(item);
                }
            }
        }
        let read = !group.is_empty();
        group
            .into_iter()
            .rev()
            .for_each(|(key, version, bytes)| self.consume(key, version, &bytes));
        Ok(read)
    }
}
