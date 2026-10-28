/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use oxrdf::{GraphNameRef, NamedNodeRef, NamedOrBlankNodeRef, Term};
use ultragraphdb_store_posting::CachedList;
use ultragraphdb_traits_store::{ListCache, Store};

use crate::errors::changeset_error::ChangesetError;
use crate::types::changeset_write::ChangesetWrite;

impl<S, C> ChangesetWrite<'_, S, C>
where
    S: Store,
    C: ListCache<Value = CachedList>,
{
    /// The objects of `subject`'s `predicate` in `graph` as the write sees them, its own writes on top of its
    /// snapshot, decoded into `out`, which is cleared first: a read of the subject's list whole, recorded. A graph,
    /// subject or predicate the store does not hold is recorded absent and holds nothing.
    pub fn objects_into(
        &mut self,
        graph: GraphNameRef<'_>,
        subject: NamedOrBlankNodeRef<'_>,
        predicate: NamedNodeRef<'_>,
        out: &mut Vec<Term>,
    ) -> Result<(), ChangesetError> {
        out.clear();
        let Some(graph) = self.find_graph(graph)? else {
            return Ok(());
        };
        let Some(subject) = self
            .find_term(predicate, subject.into())?
            .and_then(|e| e.as_uid())
        else {
            return Ok(());
        };
        let Some(predicate) = self.find_iri(predicate)? else {
            return Ok(());
        };
        let objects = self.write.objects(graph, subject, predicate)?;
        out.reserve(objects.len());
        for (object, _) in &objects {
            out.push(self.decode(object)?);
        }
        Ok(())
    }
}
