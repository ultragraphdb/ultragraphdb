/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! Vectors as a transaction writes them: in the forward list alone. A vector written to a predicate declared with a
//! vector index is refused when its dimension is not the declared one; the vector indexer, not the transaction, inserts
//! it into the HNSW lists once its round is applied, or marks its node dead once it is deleted. While the predicate's
//! vector index is built, writes record the nodes whose vector they change, which the build's swap inserts.

use ultragraphdb_core_mapping::PredicateDeclaration;
use ultragraphdb_core_terms::{EncodedTerm, TermView};
use ultragraphdb_store_posting::CachedList;
use ultragraphdb_traits_store::{ListCache, Store};

use crate::errors::changeset_error::ChangesetError;
use crate::types::changeset_write::ChangesetWrite;
use crate::types::changeset_write::changeset_write_triples::Resolved;

impl<S, C> ChangesetWrite<'_, S, C>
where
    S: Store,
    C: ListCache<Value = CachedList>,
{
    /// Refuses a vector written to a predicate with a vector index whose dimension is not the one its declaration
    /// states.
    pub(crate) fn check_dimension(
        &self,
        r: &Resolved,
        declaration: &PredicateDeclaration,
    ) -> Result<(), ChangesetError> {
        let Some(declared) = declaration.vector_index().and_then(|spec| spec.dimension()) else {
            return Ok(());
        };
        let Ok(TermView::Vector(written)) = r.object.view() else {
            return Ok(());
        };
        let declared = usize::from(declared.get());
        if written.len() == declared {
            return Ok(());
        }
        Err(ChangesetError::Dimension(
            declaration.predicate().as_str(),
            declared,
            written.len(),
        ))
    }

    /// Records that the subject's vector `object` of `predicate` in `graph` was written or removed, when the
    /// predicate's vector index is being built.
    pub(crate) fn vector_changed(
        &mut self,
        graph: u64,
        subject: u64,
        predicate: u64,
        object: &EncodedTerm,
    ) {
        if self.vector_built == Some(predicate) && object.is_vector() {
            self.touched.insert((graph, subject));
        }
    }
}
