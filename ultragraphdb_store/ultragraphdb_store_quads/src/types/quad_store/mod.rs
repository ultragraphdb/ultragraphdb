/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

mod quad_store_partition;
mod quad_store_read;

use ultragraphdb_core_mapping::{CONTRACT_VERSION, ContractVersion, MappingError, RandomSource};
use ultragraphdb_store_posting::{Batch, CachedList, Lists};
use ultragraphdb_traits_blocks::BlockSource;
use ultragraphdb_traits_store::{ListCache, PersistMode, Scan, Store, StoreSnapshot, WriteBatch};
use xxhash_rust::xxh3::xxh3_64;

use crate::errors::quad_error::QuadError;
use crate::types::dictionary::Dictionary;
use crate::types::owned_buckets::OwnedBuckets;
use crate::types::write_terms::WriteTerms;
use crate::utils::dictionary_keys::CONTRACT_KEY;

/// The most IDs a store holds created and not yet committed, unless it is opened with another bound.
pub const PENDING_MAX: usize = 1 << 16;

/// The most skolem IRIs drawn for one name before the draw is refused: each repeat names an IRI the dictionary holds,
/// which 104 random bits make all but impossible.
pub const SKOLEM_DRAWS_MAX: usize = 4;

/// The quads of a store: its posting lists, the term dictionary in its keyspace without versions with the IDs created
/// and not yet committed, the block source its sequences are leased from, the random source its names are drawn from,
/// and the fingerprint its literal postings are ordered by. A write is a [`crate::QuadWrite`] over a request of the
/// lists; its commit writes the dictionary entries of the terms it names that the store does not hold yet.
#[derive(Debug)]
pub struct QuadStore<S, C, B> {
    lists: Lists<S, C>,
    dictionary: Dictionary<B>,
    fingerprint: fn(&[u8]) -> u64,
}

impl<S, C, B> QuadStore<S, C, B>
where
    S: Store,
    C: ListCache<Value = CachedList>,
    B: BlockSource,
{
    /// The quads of `lists`' store, leasing sequences from `blocks` and drawing names from `random`, owning every
    /// bucket. A new store records the mapping contract's version; a store recorded under another major version, or
    /// holding data without a version, is refused naming both versions, and nothing is written.
    pub fn open(lists: Lists<S, C>, blocks: B, random: RandomSource) -> Result<Self, QuadError> {
        let store = lists.store();
        match store.get_versionless(CONTRACT_KEY)? {
            Some(bytes) => CONTRACT_VERSION.check_stored(ContractVersion::from_bytes(&bytes)?)?,
            None => {
                let snapshot = store.snapshot(u64::MAX)?;
                if snapshot.scan(&Scan::prefix(Vec::new())).next().is_some() {
                    return Err(MappingError::contract_version(
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
            dictionary: Dictionary::new(blocks, random, PENDING_MAX),
            fingerprint: xxh3_64,
        })
    }

    /// The store drawing its blank nodes' buckets among `owned`.
    pub fn owning(mut self, owned: OwnedBuckets) -> Self {
        self.dictionary = self.dictionary.owning(owned);
        self
    }

    /// The buckets the store owns: where its blank nodes' buckets are drawn, and the IRIs it creates IDs for.
    pub fn owned(&self) -> &OwnedBuckets {
        self.dictionary.owned()
    }

    /// The store holding at most `max` IDs created and not yet committed.
    pub fn with_pending_max(mut self, max: usize) -> Self {
        self.dictionary = self.dictionary.with_pending_max(max);
        self
    }

    /// The store ordering literal postings by `fingerprint`, as a test forces collisions with.
    pub fn with_fingerprint(mut self, fingerprint: fn(&[u8]) -> u64) -> Self {
        self.fingerprint = fingerprint;
        self
    }

    /// The posting lists the quads are stored in.
    pub fn lists(&self) -> &Lists<S, C> {
        &self.lists
    }

    /// The store under the posting lists.
    pub fn store(&self) -> &S {
        self.lists.store()
    }

    /// The fingerprint literal postings are ordered by.
    pub fn fingerprint(&self) -> fn(&[u8]) -> u64 {
        self.fingerprint
    }

    pub(crate) fn dictionary(&self) -> &Dictionary<B> {
        &self.dictionary
    }

    /// Commits `batch`, whose requests carry the writes `terms` resolved, in their order, each with the dictionary
    /// entries of its terms the store does not hold yet: those it created and the pending ones it took from another
    /// write, so two writes of one batch may carry one entry, equal in both. Once the batch is written their pending
    /// entries are settled. Creating an ID refuses no write. The commit version of each write.
    pub fn commit(
        &self,
        batch: Batch<'_, S, C>,
        terms: &[WriteTerms],
    ) -> Result<Vec<u64>, QuadError> {
        debug_assert_eq!(batch.len(), terms.len(), "one write's terms per request");
        let mut at = 0usize;
        let results = self.lists.commit_with(batch, |request| {
            for (key, value) in terms.get(at).map(WriteTerms::entries).unwrap_or_default() {
                request.add_versionless(key, value);
            }
            at += 1;
            Ok::<(), QuadError>(())
        })?;
        let versions = results
            .into_iter()
            .collect::<Result<Vec<u64>, QuadError>>()?;
        terms.iter().for_each(|t| self.settle(t));
        Ok(versions)
    }

    /// Forgets the pending entries of the terms `terms` wrote, now that a batch holding its dictionary entries is
    /// written, whichever other writes still hold them, since the store now holds the entries: what the round applying
    /// a write calls once its batch is written.
    pub fn settle(&self, terms: &WriteTerms) {
        self.dictionary.settle(terms.namespace(), terms.written());
    }

    /// Releases the terms of a write that will not commit, writing nothing: each pending entry it created or took is
    /// let go, and forgotten once no other unsettled write holds it. A write that took one from it carries its entries
    /// and keeps its ID; an ID no other write holds is burned, the next writer of its term creating another, so no
    /// entry outside a commit names an ID and every committed reference to an ID has its entries in its own commit or
    /// in one before it.
    pub fn release(&self, terms: &WriteTerms) {
        self.dictionary.release(terms.namespace(), terms.written());
    }

    /// Releases what `terms`, a write continuing `base` on a clone of its terms, took beyond `base`, as
    /// [`QuadStore::release`] releases a write's: each term `terms` writes and `base` does not. `base`'s own holds stay
    /// with it, to be settled or released once, by its own commit or release.
    pub fn release_beyond(&self, terms: &WriteTerms, base: &WriteTerms) {
        self.dictionary
            .release(terms.namespace(), terms.written_beyond(base));
    }
}
