/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::num::NonZeroU64;

use ultragraphdb_core_ids::{BucketGroup, SystemId};
use ultragraphdb_core_keys::{Graph, QuadKey, key_prefix};
use ultragraphdb_core_terms::EncodedTerm;
use ultragraphdb_store_posting::CachedList;
use ultragraphdb_traits_blocks::BlockSource;
use ultragraphdb_traits_store::{ListCache, Scan, Store, StoreSnapshot};

use crate::errors::quad_error::QuadError;
use crate::types::kernel_row::KernelRow;
use crate::types::kernel_scan::KernelScan;
use crate::types::quad_store::QuadStore;
use crate::types::referrer_set::ReferrerSet;
use crate::utils::postings::list_holds;

/// The reads a partition answers beside its quad reads (partitions, quad-reads): referrer sets and marks, its own
/// reverse entries, its predicate lists and reverse counts, and the read kernel. Each reads the store at `version`,
/// never a running execution's writes.
impl<S, C, B> QuadStore<S, C, B>
where
    S: Store,
    C: ListCache<Value = CachedList>,
    B: BlockSource,
{
    /// `entity`'s referrer set in `namespace` as of `version`, the empty set when its key holds none.
    ///
    /// # Errors
    ///
    /// The store's error; [`QuadError::corrupted`] for a value [`ReferrerSet::decode`] refuses.
    pub fn referrers(
        &self,
        namespace: u64,
        entity: SystemId,
        version: u64,
    ) -> Result<ReferrerSet, QuadError> {
        let key = QuadKey::Referrers { namespace, entity }.to_bytes();
        match self.store().get_at(&key, version)? {
            Some(item) => ReferrerSet::decode(&key, item.value()),
            None => Ok(ReferrerSet::new()),
        }
    }

    /// The generation of the mark of `(entity, group)` in `namespace` as of `version`, the partition's record of its
    /// registration at `entity`'s home (term-dictionary, "A reference to another partition's entity registers at its
    /// home"), or `None` when the partition holds no mark for it.
    ///
    /// # Errors
    ///
    /// The store's error; [`QuadError::corrupted`] for a value that is no 8-byte generation of at least 1.
    pub fn mark(
        &self,
        namespace: u64,
        entity: SystemId,
        group: BucketGroup,
        version: u64,
    ) -> Result<Option<NonZeroU64>, QuadError> {
        let key = QuadKey::Mark {
            namespace,
            entity,
            group: group.get(),
        }
        .to_bytes();
        let Some(item) = self.store().get_at(&key, version)? else {
            return Ok(None);
        };
        let generation = <[u8; 8]>::try_from(item.value())
            .ok()
            .and_then(|bytes| NonZeroU64::new(u64::from_be_bytes(bytes)))
            .ok_or_else(|| {
                QuadError::corrupted(&key, "a mark holds no generation of at least 1")
            })?;
        Ok(Some(generation))
    }

    /// The partition's own subjects holding `predicate` to `object` in `graph` of `namespace` as of `version`, sorted:
    /// its reverse list of `object`, which names its own subjects alone (partitions, "A partition owns every entry its
    /// subjects' writes derive"). Read only after `object`'s object-predicates list names `predicate` (partitions,
    /// "Absence is proven by a node's predicate lists"): a list the predicate list does not name is not read.
    ///
    /// # Errors
    ///
    /// The store's error; corruption of a list.
    pub fn subjects(
        &self,
        namespace: u64,
        graph: Graph,
        predicate: SystemId,
        object: &EncodedTerm,
        version: u64,
    ) -> Result<Vec<SystemId>, QuadError> {
        if !self.points_at(namespace, graph, predicate, object, version)? {
            return Ok(Vec::new());
        }
        let key = QuadKey::Reverse {
            namespace,
            graph,
            predicate,
            object: object.clone(),
        }
        .to_bytes();
        let mut subjects = ids(self.objects(&key, version)?);
        subjects.sort_unstable();
        Ok(subjects)
    }

    /// The predicates `subject` holds in `graph` of `namespace` as of `version`: its subject-predicates list, sorted.
    ///
    /// # Errors
    ///
    /// The store's error; corruption of a list.
    pub fn subject_predicates(
        &self,
        namespace: u64,
        graph: Graph,
        subject: SystemId,
        version: u64,
    ) -> Result<Vec<SystemId>, QuadError> {
        let key = QuadKey::SubjectPredicates {
            namespace,
            graph,
            subject,
        }
        .to_bytes();
        let mut predicates = ids(self.objects(&key, version)?);
        predicates.sort_unstable();
        Ok(predicates)
    }

    /// The predicates the partition's own subjects hold to `object` in `graph` of `namespace` as of `version`: its
    /// object-predicates list of `object`, sorted.
    ///
    /// # Errors
    ///
    /// The store's error; corruption of a list.
    pub fn object_predicates(
        &self,
        namespace: u64,
        graph: Graph,
        object: &EncodedTerm,
        version: u64,
    ) -> Result<Vec<SystemId>, QuadError> {
        let key = object_predicates_key(namespace, graph, object);
        let mut predicates = ids(self.objects(&key, version)?);
        predicates.sort_unstable();
        Ok(predicates)
    }

    /// How many of the partition's own subjects hold `predicate` to `object` in `graph` of `namespace` as of
    /// `version`: its share of `object`'s reverse count, which a lookup sums over the partitions `object`'s referrer
    /// set names (index-maintenance, "Count indexes").
    ///
    /// # Errors
    ///
    /// The store's error; corruption of a list.
    pub fn reverse_count(
        &self,
        namespace: u64,
        graph: Graph,
        predicate: SystemId,
        object: &EncodedTerm,
        version: u64,
    ) -> Result<u64, QuadError> {
        let subjects = self.subjects(namespace, graph, predicate, object, version)?;
        Ok(u64::try_from(subjects.len()).unwrap_or(u64::MAX))
    }

    /// The read kernel: the lists `scan` names, read as of its version, in key order, at most its limit of them,
    /// appended to `out`. A list holding nothing at the version gives no row. The key a next scan resumes from, the
    /// first list of the range not read, as its `from`; or `None` when the range is exhausted.
    ///
    /// # Errors
    ///
    /// The store's error; corruption of a list; [`QuadError::corrupted`] for a keyspace holding no lists.
    pub fn scan(
        &self,
        scan: &KernelScan,
        out: &mut Vec<KernelRow>,
    ) -> Result<Option<Vec<u8>>, QuadError> {
        let keyspace = scan.keyspace();
        let prefix = key_prefix(keyspace, false, scan.namespace(), None, None);
        if !keyspace.is_list() {
            return Err(QuadError::corrupted(
                &prefix,
                "the read kernel reads lists, and the keyspace holds none",
            ));
        }
        // The range within the namespace's keyspace: a bound outside it is clamped to it.
        let whole = Scan::prefix(prefix);
        let start = whole.start().max(scan.from()).to_vec();
        let end = match (whole.end(), scan.end()) {
            (Some(a), Some(b)) => Some(a.min(b).to_vec()),
            (a, b) => a.or(b).map(<[u8]>::to_vec),
        };
        let limit = scan.limit().get();
        let mut rows = 0usize;
        let snapshot = self.store().snapshot(scan.version())?;
        for item in snapshot.scan(&Scan::range(start, end)) {
            let item = item?;
            if rows == limit {
                return Ok(Some(item.key().to_vec()));
            }
            let objects = self.objects(item.key(), scan.version())?;
            if !objects.is_empty() {
                out.push(KernelRow::new(item.key().to_vec(), objects));
                rows += 1;
            }
        }
        Ok(None)
    }

    /// Whether `object`'s object-predicates list in `graph` of `namespace` names `predicate` as of `version`.
    fn points_at(
        &self,
        namespace: u64,
        graph: Graph,
        predicate: SystemId,
        object: &EncodedTerm,
        version: u64,
    ) -> Result<bool, QuadError> {
        let key = object_predicates_key(namespace, graph, object);
        let list = self.lists().read(&key, version)?;
        Ok(list_holds(&list, version, predicate)?)
    }
}

/// The key of `object`'s object-predicates list in `graph` of `namespace`.
fn object_predicates_key(namespace: u64, graph: Graph, object: &EncodedTerm) -> Vec<u8> {
    QuadKey::ObjectPredicates {
        namespace,
        graph,
        object: object.clone(),
    }
    .to_bytes()
}

/// The IDs a list of IDs holds, each object's.
fn ids(objects: Vec<(EncodedTerm, Vec<SystemId>)>) -> Vec<SystemId> {
    objects.into_iter().filter_map(|(o, _)| o.as_id()).collect()
}
