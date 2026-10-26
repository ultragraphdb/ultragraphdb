/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

mod quad_store_read;

use std::collections::{BTreeMap, BTreeSet};

use ultragraphdb_common_utils::go_farm::fingerprint64;
use ultragraphdb_core_mapping::{CONTRACT_VERSION, ContractVersion, MappingError};
use ultragraphdb_store_posting::{Batch, CachedList, Lists, Request};
use ultragraphdb_traits_store::{ListCache, PersistMode, Scan, Store, StoreSnapshot, WriteBatch};

use crate::errors::quad_error::QuadError;
use crate::types::leases::Leases;
use crate::types::write_terms::WriteTerms;
use crate::utils::dictionary_keys::{CONTRACT_KEY, iri_key};

/// The quads of a store: its posting lists, the term dictionary in its keyspace without versions, the leases its UIDs
/// and statement IDs come from, and the fingerprint its literal postings are ordered by. A write is a
/// [`crate::QuadWrite`] over a request of the lists; its commit confirms the IRIs it created.
#[derive(Debug)]
pub struct QuadStore<S, C> {
    lists: Lists<S, C>,
    leases: Leases,
    fingerprint: fn(&[u8]) -> u64,
}

impl<S, C> QuadStore<S, C>
where
    S: Store,
    C: ListCache<Value = CachedList>,
{
    /// The quads of `lists`' store. A new store records the mapping contract's version; a store recorded under
    /// another major version, or holding data without a version, is refused naming both versions, and nothing is
    /// written.
    pub fn open(lists: Lists<S, C>) -> Result<Self, QuadError> {
        let store = lists.store();
        match store.get_versionless(CONTRACT_KEY)? {
            Some(bytes) => CONTRACT_VERSION.check_stored(ContractVersion::from_bytes(&bytes)?)?,
            None => {
                let snapshot = store.snapshot(u64::MAX)?;
                if snapshot.scan(&Scan::prefix(Vec::new())).next().is_some() {
                    return Err(MappingError::ContractVersion(
                        (0, 0),
                        (CONTRACT_VERSION.major(), CONTRACT_VERSION.minor()),
                    )
                    .into());
                }
                let mut batch = WriteBatch::new();
                batch.put_versionless(CONTRACT_KEY.to_vec(), CONTRACT_VERSION.to_bytes().to_vec());
                store.apply(batch, PersistMode::Sync)?;
            }
        }
        Ok(Self {
            lists,
            leases: Leases::new(),
            fingerprint: fingerprint64,
        })
    }

    /// The store ordering literal postings by `fingerprint`, as a test forces collisions with.
    pub fn with_fingerprint(mut self, fingerprint: fn(&[u8]) -> u64) -> Self {
        self.fingerprint = fingerprint;
        self
    }

    pub fn lists(&self) -> &Lists<S, C> {
        &self.lists
    }

    pub fn store(&self) -> &S {
        self.lists.store()
    }

    pub fn leases(&self) -> &Leases {
        &self.leases
    }

    pub fn fingerprint(&self) -> fn(&[u8]) -> u64 {
        self.fingerprint
    }

    /// Commits `batch`, whose requests carry the writes `terms` resolved, in their order: under the writer lock each
    /// write's new IRIs are confirmed and its dictionary entries added, and a write is refused with
    /// `Transaction has been aborted. Please retry`, writing nothing, when a commit since its read, or an earlier write
    /// of the batch, claimed an IRI it creates, or when it read a list an earlier refused write of the batch wrote. The
    /// commit timestamp of each write, or its refusal.
    pub fn commit(
        &self,
        batch: Batch<'_, S, C>,
        terms: &[WriteTerms],
    ) -> Result<Vec<Result<u64, QuadError>>, QuadError> {
        let mut at = 0usize;
        let mut claimed: BTreeMap<(u64, String), u64> = BTreeMap::new();
        let mut refused_writes: BTreeSet<Vec<u8>> = BTreeSet::new();
        let results = self.lists.commit_with(batch, |request| {
            let write = terms.get(at);
            at += 1;
            let checked = if request.read_keys().any(|k| refused_writes.contains(k)) {
                Err(QuadError::Aborted())
            } else {
                match write {
                    Some(write) => self.confirm(request, write, &mut claimed),
                    None => Ok(()),
                }
            };
            if checked.is_err() {
                refused_writes.extend(request.written_keys().map(<[u8]>::to_vec));
            }
            checked
        })?;
        Ok(results)
    }

    /// Confirms `write`'s new IRIs against the dictionary and the IRIs earlier writes of the batch claimed, and adds
    /// its dictionary entries to `request`.
    fn confirm(
        &self,
        request: &mut Request<'_, S, C>,
        write: &WriteTerms,
        claimed: &mut BTreeMap<(u64, String), u64>,
    ) -> Result<(), QuadError> {
        let ns = write.namespace();
        for (iri, uid) in write.created() {
            let taken = claimed
                .get(&(ns, iri.to_string()))
                .is_some_and(|u| *u != uid)
                || self.store().get_versionless(&iri_key(ns, iri))?.is_some();
            if taken {
                return Err(QuadError::Aborted());
            }
        }
        for (iri, uid) in write.created() {
            claimed.insert((ns, iri.to_string()), uid);
        }
        for (key, value) in write.entries() {
            request.add_versionless(key, value);
        }
        Ok(())
    }
}
