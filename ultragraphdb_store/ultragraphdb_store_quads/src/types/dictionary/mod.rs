/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

mod dictionary_mint;

use std::collections::BTreeMap;
use std::sync::{Mutex, MutexGuard};

use ultragraphdb_core_ids::{Bucket, CanonicalKey, SystemId, Tenant, TypePrefix};
use ultragraphdb_core_mapping::RandomSource;
use ultragraphdb_traits_blocks::BlockSource;
use ultragraphdb_traits_store::Store;

use crate::errors::quad_error::QuadError;
use crate::types::owned_buckets::OwnedBuckets;
use crate::types::sequences::Sequences;
use crate::types::term_origin::TermOrigin;
use crate::utils::dictionary_keys::term_key;
use crate::utils::skolem::is_skolem;

/// The term dictionary's live state beside the stored entries: the IDs created during executions whose entries no
/// commit has written yet, the sequences new IDs take, the random source names are drawn from and the buckets a blank
/// node's is drawn among. A pending entry counts the unsettled writes holding it: the write that created it and each
/// that took it while it was pending, every one of which writes its entries. It stays until a commit holding it is
/// settled, the store then holding its entries, or until every write holding it is released, refused, when no
/// committed write names its ID; so an IRI has one ID from its creation on, whoever commits it, and every committed
/// reference to an ID has its entries in its own commit or in one before it. A write dropped without a release keeps
/// its holds, so what it held stays pending until a commit holding it settles, which errs only towards keeping.
#[derive(Debug)]
pub(crate) struct Dictionary<B> {
    // Locked: executions running beside each other get or create IDs, mint names and settle commits at once, and one
    // lock makes a lookup, the allocation it may make and the pending entry it adds one step.
    state: Mutex<DictionaryState<B>>,
    owned: OwnedBuckets,
    pending_max: usize,
}

/// What the dictionary's lock guards.
#[derive(Debug)]
struct DictionaryState<B> {
    /// By tenant, then by term: the IDs created and not yet written by a settled commit, each with how many
    /// unsettled writes hold it.
    pending: BTreeMap<u64, BTreeMap<String, (SystemId, usize)>>,
    /// How many pending entries there are, in every tenant.
    pending_len: usize,
    sequences: Sequences<B>,
    random: RandomSource,
}

impl<B: BlockSource> Dictionary<B> {
    /// A dictionary leasing from `blocks`, drawing names from `random`, owning every bucket, holding at most
    /// `pending_max` pending entries.
    pub(crate) fn new(blocks: B, random: RandomSource, pending_max: usize) -> Self {
        let state = DictionaryState {
            pending: BTreeMap::new(),
            pending_len: 0,
            sequences: Sequences::new(blocks),
            random,
        };
        Self {
            state: Mutex::new(state),
            owned: OwnedBuckets::all(),
            pending_max,
        }
    }

    /// The dictionary owning `owned`.
    pub(crate) fn owning(mut self, owned: OwnedBuckets) -> Self {
        self.owned = owned;
        self
    }

    /// The dictionary holding at most `max` pending entries.
    pub(crate) fn with_pending_max(mut self, max: usize) -> Self {
        self.pending_max = max;
        self
    }

    /// The buckets the dictionary owns.
    pub(crate) fn owned(&self) -> &OwnedBuckets {
        &self.owned
    }

    /// The ID of `iri` in `tenant` with where it came from: pending, held in `store`, or created, its bucket
    /// computed from the tenant and the IRI and its sequence a `Resource` one. A pending or created ID is held by
    /// the caller's write from then on. An IRI under the skolem base is refused unless it is pending or held, and an
    /// IRI of a bucket the dictionary does not own unless it is held: its home creates its ID.
    pub(crate) fn resolve<S: Store>(
        &self,
        store: &S,
        tenant: u64,
        iri: &str,
    ) -> Result<(SystemId, TermOrigin), QuadError> {
        let mut state = self.lock();
        if let Some(id) = state.hold_pending(tenant, iri) {
            return Ok((id, TermOrigin::Pending));
        }
        if let Some(id) = held_id(store, tenant, iri)? {
            return Ok((id, TermOrigin::Held));
        }
        if is_skolem(tenant, iri) {
            return Err(QuadError::unknown_skolem(iri));
        }
        let key = CanonicalKey::iri(iri)?;
        let bucket = Bucket::derive(Tenant::new(tenant), TypePrefix::RESOURCE, &key);
        if !self.owned.owns(bucket) {
            return Err(QuadError::not_owned(iri, bucket.get()));
        }
        self.check_room(&state)?;
        let id = SystemId::new(
            TypePrefix::RESOURCE,
            bucket,
            state.sequences.next(TypePrefix::RESOURCE)?,
        );
        debug_assert_eq!(
            id,
            SystemId::resource(Tenant::new(tenant), &key, id.sequence()),
            "a Resource's bucket is derived from its key"
        );
        state.add_pending(tenant, iri, id);
        Ok((id, TermOrigin::Created))
    }

    /// The ID `term` has with where it came from, pending or held in `store`, creating none. A pending ID is held by
    /// the caller's write from then on.
    pub(crate) fn lookup<S: Store>(
        &self,
        store: &S,
        tenant: u64,
        term: &str,
    ) -> Result<Option<(SystemId, TermOrigin)>, QuadError> {
        let mut state = self.lock();
        match state.hold_pending(tenant, term) {
            Some(id) => Ok(Some((id, TermOrigin::Pending))),
            None => Ok(held_id(store, tenant, term)?.map(|id| (id, TermOrigin::Held))),
        }
    }

    /// Forgets the pending entries of `written`, each a term and the ID it took, now that the store holds them,
    /// whichever other writes still hold them: they find the entries held.
    pub(crate) fn settle<'t>(
        &self,
        tenant: u64,
        written: impl Iterator<Item = (&'t str, SystemId)>,
    ) {
        let mut state = self.lock();
        for (term, id) in written {
            state.remove_pending(tenant, term, id);
        }
    }

    /// Lets go of the pending entries of `written`, a refused write's terms with their IDs: each is forgotten once no
    /// other unsettled write holds it, so a write that took it from the pending map still finds the ID it carries the
    /// entries of. A term whose pending entry is gone, or names another ID, is left alone.
    pub(crate) fn release<'t>(
        &self,
        tenant: u64,
        written: impl Iterator<Item = (&'t str, SystemId)>,
    ) {
        let mut state = self.lock();
        for (term, id) in written {
            state.let_go(tenant, term, id);
        }
    }

    /// Refuses another pending entry once `pending_max` are held.
    fn check_room(&self, state: &DictionaryState<B>) -> Result<(), QuadError> {
        if state.pending_len < self.pending_max {
            Ok(())
        } else {
            Err(QuadError::pending_full(self.pending_max))
        }
    }

    /// The guarded state, a poisoned lock's included: every change to it is made whole before the guard drops.
    fn lock(&self) -> MutexGuard<'_, DictionaryState<B>> {
        self.state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

impl<B> DictionaryState<B> {
    /// The pending ID of `term` in `tenant`.
    fn pending_id(&self, tenant: u64, term: &str) -> Option<SystemId> {
        self.pending
            .get(&tenant)
            .and_then(|terms| terms.get(term))
            .map(|(id, _)| *id)
    }

    /// The pending ID of `term` in `tenant`, held by one more write.
    fn hold_pending(&mut self, tenant: u64, term: &str) -> Option<SystemId> {
        let (id, holders) = self.pending.get_mut(&tenant)?.get_mut(term)?;
        *holders = holders.saturating_add(1);
        Some(*id)
    }

    /// Records `term`'s new ID `id` as pending, held by the write creating it.
    fn add_pending(&mut self, tenant: u64, term: &str, id: SystemId) {
        let previous = self
            .pending
            .entry(tenant)
            .or_default()
            .insert(term.to_string(), (id, 1));
        debug_assert!(previous.is_none(), "a term is created once");
        self.pending_len += 1;
    }

    /// Forgets `term`'s pending entry when it is `id`.
    fn remove_pending(&mut self, tenant: u64, term: &str, id: SystemId) {
        self.change_pending(tenant, term, id, |_| 0);
    }

    /// Takes one holder from `term`'s pending entry when it is `id`, forgetting it once none is left.
    fn let_go(&mut self, tenant: u64, term: &str, id: SystemId) {
        self.change_pending(tenant, term, id, |holders| holders.saturating_sub(1));
    }

    /// Sets the holders of `term`'s pending entry, when it is `id`, to what `holders` makes of them, forgetting the
    /// entry at none.
    fn change_pending(
        &mut self,
        tenant: u64,
        term: &str,
        id: SystemId,
        holders: impl Fn(usize) -> usize,
    ) {
        let Some(terms) = self.pending.get_mut(&tenant) else {
            return;
        };
        if let Some((held, count)) = terms.get_mut(term)
            && *held == id
        {
            *count = holders(*count);
            if *count == 0 {
                let _removed: Option<(SystemId, usize)> = terms.remove(term);
                debug_assert!(self.pending_len > 0, "a pending entry was counted");
                self.pending_len = self.pending_len.saturating_sub(1);
            }
        }
        if terms.is_empty() {
            let _emptied: Option<BTreeMap<String, (SystemId, usize)>> =
                self.pending.remove(&tenant);
        }
    }
}

/// The ID the dictionary of `store` holds for `term`, an IRI or a reifier's name, in `tenant`: an entry that is no
/// valid system ID is corruption naming its key.
pub(crate) fn held_id<S: Store>(
    store: &S,
    tenant: u64,
    term: &str,
) -> Result<Option<SystemId>, QuadError> {
    let key = term_key(tenant, term);
    store
        .get_versionless(&key)?
        .map(|value| {
            SystemId::decode(&value).map_err(|e| {
                QuadError::corrupted(&key, format!("a term's entry holds no system ID: {e}"))
            })
        })
        .transpose()
}
