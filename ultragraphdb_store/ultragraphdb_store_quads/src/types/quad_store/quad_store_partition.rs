/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::cmp::Ordering;
use std::collections::BTreeMap;
use std::iter::Peekable;
use std::num::NonZeroU64;
use std::ops::ControlFlow;

use ultragraphdb_core_ids::{BucketGroup, SystemId};
use ultragraphdb_core_keys::{Graph, Keyspace, QuadKey};
use ultragraphdb_core_terms::{EncodedTerm, TermError};
use ultragraphdb_error_store::StoreError;
use ultragraphdb_store_posting::{CachedList, PostingIdentity, walk_lists};
use ultragraphdb_traits_blocks::BlockSource;
use ultragraphdb_traits_store::{Item, ListCache, Scan, Store, StoreSnapshot};

use crate::errors::quad_error::QuadError;
use crate::types::kernel_row::KernelRow;
use crate::types::kernel_scan::KernelScan;
use crate::types::page::Page;
use crate::types::page_end::PageEnd;
use crate::types::page_walk::PageWalk;
use crate::types::quad_store::QuadStore;
use crate::types::referrer_set::ReferrerSet;
use crate::utils::postings::{list_holds, object_identity};

/// The reads a partition answers beside its quad reads (partitions, quad-reads): referrer sets and marks, its own
/// reverse entries, its predicate lists and reverse counts, and the read kernel. Each reads the store at `version`,
/// never a running execution's writes.
impl<S, C, B> QuadStore<S, C, B>
where
    S: Store,
    C: ListCache<Value = CachedList>,
    B: BlockSource,
{
    /// `entity`'s referrer set in `tenant` as of `version`, the empty set when its key holds none.
    ///
    /// # Errors
    ///
    /// The store's error; [`QuadError::corrupted`] for a value [`ReferrerSet::decode`] refuses.
    pub fn referrers(
        &self,
        tenant: u64,
        entity: SystemId,
        version: u64,
    ) -> Result<ReferrerSet, QuadError> {
        let key = QuadKey::Referrers { tenant, entity }.to_bytes();
        match self.store().get_at(&key, version)? {
            Some(item) => ReferrerSet::decode(&key, item.value()),
            None => Ok(ReferrerSet::new()),
        }
    }

    /// The generation of the mark of `(entity, group)` in `tenant` as of `version`, the partition's record of its
    /// registration at `entity`'s home (term-dictionary, "A reference to another partition's entity registers at its
    /// home"), or `None` when the partition holds no mark for it.
    ///
    /// # Errors
    ///
    /// The store's error; [`QuadError::corrupted`] for a value that is no 8-byte generation of at least 1.
    pub fn mark(
        &self,
        tenant: u64,
        entity: SystemId,
        group: BucketGroup,
        version: u64,
    ) -> Result<Option<NonZeroU64>, QuadError> {
        let key = QuadKey::Mark {
            tenant,
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

    /// The partition's own subjects holding `predicate` to `object` in `graph` of `tenant` as of `version`, sorted:
    /// its reverse list of `object`, which names its own subjects alone (partitions, "A partition owns every entry its
    /// subjects' writes derive"). Read only after `object`'s object-predicates list names `predicate` (partitions,
    /// "Absence is proven by a node's predicate lists"): a list the predicate list does not name is not read.
    ///
    /// # Errors
    ///
    /// The store's error; corruption of a list.
    pub fn subjects(
        &self,
        tenant: u64,
        graph: Graph,
        predicate: SystemId,
        object: &EncodedTerm,
        version: u64,
    ) -> Result<Vec<SystemId>, QuadError> {
        if !self.points_at(tenant, graph, predicate, object, version)? {
            return Ok(Vec::new());
        }
        let key = QuadKey::Reverse {
            tenant,
            graph,
            predicate,
            object: object.clone(),
        }
        .to_bytes();
        let mut subjects = ids(self.objects(&key, version)?);
        subjects.sort_unstable();
        Ok(subjects)
    }

    /// The predicates `subject` holds in `graph` of `tenant` as of `version`: its subject-predicates list, sorted.
    ///
    /// # Errors
    ///
    /// The store's error; corruption of a list.
    pub fn subject_predicates(
        &self,
        tenant: u64,
        graph: Graph,
        subject: SystemId,
        version: u64,
    ) -> Result<Vec<SystemId>, QuadError> {
        let key = QuadKey::SubjectPredicates {
            tenant,
            graph,
            subject,
        }
        .to_bytes();
        let mut predicates = ids(self.objects(&key, version)?);
        predicates.sort_unstable();
        Ok(predicates)
    }

    /// The predicates the partition's own subjects hold to `object` in `graph` of `tenant` as of `version`: its
    /// object-predicates list of `object`, sorted.
    ///
    /// # Errors
    ///
    /// The store's error; corruption of a list.
    pub fn object_predicates(
        &self,
        tenant: u64,
        graph: Graph,
        object: &EncodedTerm,
        version: u64,
    ) -> Result<Vec<SystemId>, QuadError> {
        let key = object_predicates_key(tenant, graph, object);
        let mut predicates = ids(self.objects(&key, version)?);
        predicates.sort_unstable();
        Ok(predicates)
    }

    /// How many of the partition's own subjects hold `predicate` to `object` in `graph` of `tenant` as of
    /// `version`: its share of `object`'s reverse count, which a lookup sums over the partitions `object`'s referrer
    /// set names (index-maintenance, "Count indexes").
    ///
    /// # Errors
    ///
    /// The store's error; corruption of a list.
    pub fn reverse_count(
        &self,
        tenant: u64,
        graph: Graph,
        predicate: SystemId,
        object: &EncodedTerm,
        version: u64,
    ) -> Result<u64, QuadError> {
        let subjects = self.subjects(tenant, graph, predicate, object, version)?;
        Ok(u64::try_from(subjects.len()).unwrap_or(u64::MAX))
    }

    /// The read kernel for a plain read: the lists `scan` names, read as of its version, in key order, at most its limit of
    /// them, appended to `out`. A list holding nothing at the version gives no row. The key a next scan begins at, just past
    /// the last row, as its `from`; or `None` when the range is exhausted.
    ///
    /// # Errors
    ///
    /// As [`QuadStore::page`].
    pub fn scan(
        &self,
        scan: &KernelScan,
        out: &mut Vec<KernelRow>,
    ) -> Result<Option<Vec<u8>>, QuadError> {
        let page = self.page(scan, KernelRow::gather(out))?;
        Ok(page.into_end().resume_from())
    }

    /// A page of the read kernel for a plain read, the walk ([`walk_lists`]) over the store's lists, a source adding no key: the
    /// lists `scan` names, read as of its version, in key order, streamed to `sink` object by object, each with its key and
    /// statement IDs. It ends at the range's end, after the scan's limit of rows, where `sink`
    /// breaks on an object it does not take, which the next page, resuming as the end says, reads again, or after the keys giving
    /// nothing the scan allows. A sink that breaks having taken nothing leaves a paused end with no last key.
    ///
    /// # Errors
    ///
    /// The store's error; corruption of a list; [`QuadError::corrupted`] for a keyspace holding no lists.
    pub fn page<F>(&self, scan: &KernelScan, sink: F) -> Result<Page, QuadError>
    where
        F: FnMut(&[u8], &EncodedTerm, &[SystemId]) -> ControlFlow<()>,
    {
        scan.check_keyspace()?;
        let (start, end) = scan.bounds();
        let version = scan.version();
        let snapshot = self.store().snapshot(version)?;
        let mut walk = PageWalk::new(scan, version, sink);
        let walked = walk_lists(
            &snapshot,
            &Scan::range(start, end),
            self.lists(),
            version,
            scan.walk_bounds(),
            |key, lists| walk.visit(key, lists),
        )?;
        Ok(walk.end(walked))
    }

    /// Where a list of `keyspace` resumes for a reader naming the object `encoding` in a Scan's `after`, for [`KernelScan::resuming`]:
    /// at that object's posting, read itself when the list holds it.
    ///
    /// # Errors
    ///
    /// [`TermError`] for bytes that are not one whole object encoding.
    pub fn resume_at(
        &self,
        keyspace: Keyspace,
        encoding: &[u8],
    ) -> Result<PostingIdentity, TermError> {
        object_identity(keyspace, encoding, self.fingerprint())
    }

    /// A page of the referrer sets or the statements: each key `scan` names and its stored value as of the scan's version, in key
    /// order, streamed to `sink`, which breaks on a value it does not take. It ends as [`QuadStore::page`] does; a value is taken
    /// whole or not at all, so a pause lies between keys.
    ///
    /// # Errors
    ///
    /// The store's error; [`QuadError::corrupted`] for another keyspace.
    pub fn values_page<F>(&self, scan: &KernelScan, sink: F) -> Result<Page, QuadError>
    where
        F: FnMut(&[u8], &[u8]) -> ControlFlow<()>,
    {
        self.values_page_over(scan, &BTreeMap::new(), sink)
    }

    /// A page of stored values as [`QuadStore::values_page`] reads it, `own` over what the store holds: each key's value there, or
    /// no entry for `None`, whatever the store holds at the version. What a transaction reads of the statements with its own
    /// records over them.
    ///
    /// # Errors
    ///
    /// The store's error; [`QuadError::corrupted`] for another keyspace.
    pub fn values_page_over<F>(
        &self,
        scan: &KernelScan,
        own: &BTreeMap<Vec<u8>, Option<Vec<u8>>>,
        mut sink: F,
    ) -> Result<Page, QuadError>
    where
        F: FnMut(&[u8], &[u8]) -> ControlFlow<()>,
    {
        scan.check_values()?;
        let (start, end) = scan.bounds();
        let snapshot = self.store().snapshot(scan.version())?;
        let mut held = snapshot
            .scan(&Scan::range(start.clone(), end.clone()))
            .peekable();
        let mut written = own
            .range(start..)
            .take_while(|(key, _)| end.as_ref().is_none_or(|end| *key < end))
            .peekable();
        let (mut rows, mut last) = (0usize, None::<Vec<u8>>);
        // Ends: each turn takes the next entry of `held` and `written`, both finite, or returns.
        while let Some((key, value)) = next_value(&mut held, &mut written)? {
            let Some(value) = value else {
                continue;
            };
            if let Some(last) = last.take_if(|_| rows == scan.limit().get()) {
                return Ok(Page::new(PageEnd::Stopped { last }, rows));
            }
            if sink(&key, &value).is_break() {
                return Ok(Page::new(paused_values(last, &key), rows));
            }
            rows += 1;
            last = Some(key);
        }
        Ok(Page::new(PageEnd::Exhausted, rows))
    }

    /// Whether `object`'s object-predicates list in `graph` of `tenant` names `predicate` as of `version`.
    fn points_at(
        &self,
        tenant: u64,
        graph: Graph,
        predicate: SystemId,
        object: &EncodedTerm,
        version: u64,
    ) -> Result<bool, QuadError> {
        let key = object_predicates_key(tenant, graph, object);
        let list = self.lists().read(&key, version)?;
        Ok(list_holds(&list, version, predicate)?)
    }
}

/// A key of a page of stored values and its value, `None` for no entry.
type StoredValue = (Vec<u8>, Option<Vec<u8>>);

/// The next key of a page of stored values and its value: what `held`, a snapshot's scan, gives or `written`, the page's own
/// values, says over it, `None` for no entry. `None` when both have ended.
fn next_value<'a, H, W>(
    held: &mut Peekable<H>,
    written: &mut Peekable<W>,
) -> Result<Option<StoredValue>, QuadError>
where
    H: Iterator<Item = Result<Item, StoreError>>,
    W: Iterator<Item = (&'a Vec<u8>, &'a Option<Vec<u8>>)>,
{
    let stored = match held.peek() {
        Some(Ok(item)) => Some(item.key()),
        Some(Err(failed)) => return Err(failed.clone().into()),
        None => None,
    };
    let order = match (stored, written.peek()) {
        (None, None) => return Ok(None),
        (Some(_), None) => Ordering::Less,
        (None, Some(_)) => Ordering::Greater,
        (Some(stored), Some((key, _))) => stored.cmp(key.as_slice()),
    };
    if order == Ordering::Less {
        return Ok(held
            .next()
            .transpose()?
            .map(|item| (item.key().to_vec(), Some(item.value().to_vec()))));
    }
    if order == Ordering::Equal {
        let _shadowed: Option<Result<Item, StoreError>> = held.next();
    }
    Ok(written
        .next()
        .map(|(key, value)| (key.clone(), value.clone())))
}

/// How a page of stored values paused at `next` ended, `last` the last key it gave.
fn paused_values(last: Option<Vec<u8>>, next: &[u8]) -> PageEnd {
    match last {
        Some(last) => {
            let mut from = last.clone();
            from.push(0);
            PageEnd::Paused {
                last: Some(last),
                from,
                after: None,
            }
        }
        None => PageEnd::Paused {
            last: None,
            from: next.to_vec(),
            after: None,
        },
    }
}

/// The key of `object`'s object-predicates list in `graph` of `tenant`.
fn object_predicates_key(tenant: u64, graph: Graph, object: &EncodedTerm) -> Vec<u8> {
    QuadKey::ObjectPredicates {
        tenant,
        graph,
        object: object.clone(),
    }
    .to_bytes()
}

/// The IDs a list of IDs holds, each object's.
fn ids(objects: Vec<(EncodedTerm, Vec<SystemId>)>) -> Vec<SystemId> {
    objects.into_iter().filter_map(|(o, _)| o.as_id()).collect()
}
