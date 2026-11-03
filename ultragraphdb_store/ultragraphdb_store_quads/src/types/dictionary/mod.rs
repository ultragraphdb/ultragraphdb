/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

mod dictionary_mint;

use std::collections::BTreeMap;
use std::sync::{Mutex, MutexGuard};

use ultragraphdb_core_ids::{CanonicalKey, Namespace, SystemId, TypePrefix};
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
/// node's is drawn among. A pending entry stays until a commit that wrote it is settled, or a refused write that took
/// it is released, so an IRI has one ID from its creation on, whoever commits it.
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
    /// By namespace, then by term: the IDs created and not yet written by a settled commit.
    pending: BTreeMap<u64, BTreeMap<String, SystemId>>,
    /// How many pending entries there are, in every namespace.
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

    /// The ID of `iri` in `namespace` with where it came from: pending, held in `store`, or created, its bucket
    /// computed from the namespace and the IRI and its sequence a `Resource` one. An IRI under the skolem base is
    /// refused unless it is pending or held.
    pub(crate) fn resolve<S: Store>(
        &self,
        store: &S,
        namespace: u64,
        iri: &str,
    ) -> Result<(SystemId, TermOrigin), QuadError> {
        let mut state = self.lock();
        if let Some(id) = state.pending_id(namespace, iri) {
            return Ok((id, TermOrigin::Pending));
        }
        if let Some(id) = held_id(store, namespace, iri)? {
            return Ok((id, TermOrigin::Held));
        }
        if is_skolem(namespace, iri) {
            return Err(QuadError::unknown_skolem(iri));
        }
        let key = CanonicalKey::iri(iri)?;
        self.check_room(&state)?;
        let id = SystemId::resource(
            Namespace::new(namespace),
            &key,
            state.sequences.next(TypePrefix::RESOURCE)?,
        );
        state.add_pending(namespace, iri, id);
        Ok((id, TermOrigin::Created))
    }

    /// The ID `term` has, pending or held in `store`, creating none.
    pub(crate) fn lookup<S: Store>(
        &self,
        store: &S,
        namespace: u64,
        term: &str,
    ) -> Result<Option<SystemId>, QuadError> {
        let state = self.lock();
        match state.pending_id(namespace, term) {
            Some(id) => Ok(Some(id)),
            None => held_id(store, namespace, term),
        }
    }

    /// Forgets the pending entries of `written`, each a term and the ID it took, now that the store holds them.
    pub(crate) fn settle<'t>(
        &self,
        namespace: u64,
        written: impl Iterator<Item = (&'t str, SystemId)>,
    ) {
        let mut state = self.lock();
        for (term, id) in written {
            state.remove_pending(namespace, term, id);
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
    /// The pending ID of `term` in `namespace`.
    fn pending_id(&self, namespace: u64, term: &str) -> Option<SystemId> {
        self.pending
            .get(&namespace)
            .and_then(|terms| terms.get(term))
            .copied()
    }

    /// Records `term`'s new ID `id` as pending.
    fn add_pending(&mut self, namespace: u64, term: &str, id: SystemId) {
        let previous = self
            .pending
            .entry(namespace)
            .or_default()
            .insert(term.to_string(), id);
        debug_assert!(previous.is_none(), "a term is created once");
        self.pending_len += 1;
    }

    /// Forgets `term`'s pending entry when it is `id`.
    fn remove_pending(&mut self, namespace: u64, term: &str, id: SystemId) {
        let Some(terms) = self.pending.get_mut(&namespace) else {
            return;
        };
        if terms.get(term) == Some(&id) {
            let _removed: Option<SystemId> = terms.remove(term);
            debug_assert!(self.pending_len > 0, "a pending entry was counted");
            self.pending_len = self.pending_len.saturating_sub(1);
        }
        if terms.is_empty() {
            let _emptied: Option<BTreeMap<String, SystemId>> = self.pending.remove(&namespace);
        }
    }
}

/// The ID the dictionary of `store` holds for `term`, an IRI or a reifier's name, in `namespace`: an entry that is no
/// valid system ID is corruption naming its key.
pub(crate) fn held_id<S: Store>(
    store: &S,
    namespace: u64,
    term: &str,
) -> Result<Option<SystemId>, QuadError> {
    let key = term_key(namespace, term);
    store
        .get_versionless(&key)?
        .map(|value| {
            SystemId::decode(&value).map_err(|e| {
                QuadError::corrupted(&key, format!("a term's entry holds no system ID: {e}"))
            })
        })
        .transpose()
}
