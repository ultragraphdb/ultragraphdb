/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_core_ids::{Bucket, Namespace, SystemId, TypePrefix};
use ultragraphdb_core_mapping::Names;
use ultragraphdb_traits_blocks::BlockSource;
use ultragraphdb_traits_store::Store;

use crate::errors::quad_error::QuadError;
use crate::types::dictionary::{Dictionary, DictionaryState, held_id};
use crate::types::quad_store::SKOLEM_DRAWS_MAX;
use crate::utils::skolem::names;

impl<B: BlockSource> Dictionary<B> {
    /// A new blank node of `namespace`: its `Resource` ID and its skolem IRI, the bucket drawn among the owned buckets
    /// of the namespace's run and 104 random bits, both from the random source, drawn again while the IRI is pending
    /// or held in `store`.
    pub(crate) fn mint_blank<S: Store>(
        &self,
        store: &S,
        namespace: u64,
    ) -> Result<(SystemId, String), QuadError> {
        let names = names(namespace)?;
        let mut state = self.lock();
        self.check_room(&state)?;
        for _ in 0..SKOLEM_DRAWS_MAX {
            let bucket = self
                .owned
                .draw(Namespace::new(namespace), &mut state.random)
                .ok_or_else(|| QuadError::no_owned_bucket(namespace))?;
            if let Some(skolem) = free_skolem(&mut state, store, &names, namespace, bucket)? {
                let sequence = state.sequences.next(TypePrefix::RESOURCE)?;
                let id = SystemId::new(TypePrefix::RESOURCE, bucket, sequence);
                state.add_pending(namespace, &skolem, id);
                return Ok((id, skolem));
            }
        }
        Err(QuadError::skolem_draws(SKOLEM_DRAWS_MAX))
    }

    /// A new statement ID of `subject` in `namespace` and its reifier: a `Statement` ID in the subject's bucket, and a
    /// skolem IRI of that bucket and 104 random bits, drawn again while it is pending or held in `store`.
    pub(crate) fn mint_statement<S: Store>(
        &self,
        store: &S,
        namespace: u64,
        subject: SystemId,
    ) -> Result<(SystemId, String), QuadError> {
        let names = names(namespace)?;
        let mut state = self.lock();
        self.check_room(&state)?;
        for _ in 0..SKOLEM_DRAWS_MAX {
            if let Some(reifier) =
                free_skolem(&mut state, store, &names, namespace, subject.bucket())?
            {
                let id = SystemId::statement(subject, state.sequences.next(TypePrefix::STATEMENT)?);
                debug_assert_eq!(
                    id.bucket(),
                    subject.bucket(),
                    "a statement takes its subject's bucket"
                );
                state.add_pending(namespace, &reifier, id);
                return Ok((id, reifier));
            }
        }
        Err(QuadError::skolem_draws(SKOLEM_DRAWS_MAX))
    }
}

/// A skolem IRI of `bucket` with 104 bits drawn from the state's random source, or `None` when the dictionary already
/// holds it, pending or in `store`.
fn free_skolem<B, S: Store>(
    state: &mut DictionaryState<B>,
    store: &S,
    names: &Names,
    namespace: u64,
    bucket: Bucket,
) -> Result<Option<String>, QuadError> {
    let skolem = names.mint_skolem(bucket, &mut state.random).into_string();
    let taken = state.pending_id(namespace, &skolem).is_some()
        || held_id(store, namespace, &skolem)?.is_some();
    Ok((!taken).then_some(skolem))
}
