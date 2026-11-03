/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The derivation of a write's keys from its logical writes (write-ahead-log, "A round's record is logical"): one
//! function, [`ChangesetWrite::derive`], which a changeset's first apply runs on its net effect at
//! [`ChangesetWrite::finish`] and replay runs on a logged record, so every list, index, count and statistics entry
//! the two write is derived alike.

use oxrdf::Term;
use ultragraphdb_core_ids::SystemId;
use ultragraphdb_core_keys::QuadKey;
use ultragraphdb_core_mapping::PredicateDeclaration;
use ultragraphdb_core_terms::EncodedTerm;
use ultragraphdb_error_store::StoreError;
use ultragraphdb_store_posting::{CachedList, DEL};
use ultragraphdb_store_quads::{StatementRecord, posting_object};
use ultragraphdb_traits_blocks::BlockSource;
use ultragraphdb_traits_store::{ListCache, Store};

use crate::errors::changeset_error::ChangesetError;
use crate::types::changeset_write::ChangesetWrite;
use crate::types::changeset_write::changeset_write_triples::Resolved;
use crate::types::logical_quad::LogicalQuad;
use crate::types::logical_writes::LogicalWrites;

impl<S, C, B> ChangesetWrite<'_, S, C, B>
where
    S: Store,
    C: ListCache<Value = CachedList>,
    B: BlockSource,
{
    /// Derives the keys of `writes` on the write's snapshot: each removed statement removed, then each inserted
    /// statement added with its statement IDs, or its IDs set when the subject holds it, every list, index, count and
    /// statistics entry maintained as it goes; and each statement record written or removed.
    pub(crate) fn derive(&mut self, writes: &LogicalWrites) -> Result<(), ChangesetError> {
        self.deriving = true;
        self.initial()?;
        for quad in writes.removed() {
            let declaration = self.declaration_of(quad.predicate())?;
            let _removed: bool = self.remove(
                quad.graph(),
                quad.subject(),
                quad.predicate(),
                &declaration,
                quad.object(),
            )?;
        }
        let inserted = writes.inserted();
        let mut start = 0;
        // Each turn takes the run of entries naming one statement, at least one, so the walk ends with the entries.
        while let Some(first) = inserted.get(start) {
            let run = inserted
                .get(start..)
                .unwrap_or_default()
                .iter()
                .take_while(|q| same_statement(q, first))
                .count();
            let ids: Vec<SystemId> = inserted
                .get(start..start + run)
                .unwrap_or_default()
                .iter()
                .filter_map(LogicalQuad::statement)
                .collect();
            self.replay_insert(first, &ids)?;
            start += run.max(1);
        }
        for (graph, id, record) in writes.records() {
            let record = match record {
                Some(bytes) => Some(StatementRecord::decode(&id.to_be_bytes(), bytes)?),
                None => None,
            };
            let _previous: Option<Option<StatementRecord>> =
                self.records.insert((*graph, *id), record);
        }
        Ok(())
    }

    /// The write's statements by their net effect on its snapshot: for each object a forward list's delta names,
    /// removed when the snapshot holds it and the delta removes it, and inserted, once per statement ID, when the delta
    /// sets it and the snapshot does not hold it with those IDs. A change the write undid leaves nothing. The write's
    /// statement records are its own and not among them.
    pub(crate) fn net_writes(&mut self) -> Result<LogicalWrites, ChangesetError> {
        let (mut inserted, mut removed) = (Vec::new(), Vec::new());
        for (key, delta) in self.write.request().deltas() {
            let Ok((
                QuadKey::Forward {
                    graph,
                    predicate,
                    subject,
                    ..
                },
                None,
            )) = QuadKey::parse(&key)
            else {
                continue;
            };
            let held = self.write.snapshot_objects(graph, subject, predicate)?;
            for posting in delta.postings() {
                let object = posting_object(&key, posting)?;
                let before = held.iter().find(|(o, _)| *o == object).map(|(_, ids)| ids);
                let quad = |statement| {
                    LogicalQuad::new(graph, subject, predicate, object.clone(), statement)
                };
                match (posting.op(), before, posting.statement_ids()) {
                    (DEL, Some(_), _) => removed.push(quad(None)),
                    (DEL, None, _) => {}
                    (_, Some(ids), now) if ids.as_slice() == now => {}
                    (_, _, []) => inserted.push(quad(None)),
                    (_, _, now) => inserted.extend(now.iter().map(|id| quad(Some(*id)))),
                }
            }
        }
        Ok(LogicalWrites::new(inserted, removed, Vec::new()))
    }

    /// Adds the statement of `quad` with `ids`, or sets its IDs when the subject holds it already: a test of the one
    /// statement, as an insert reads it.
    fn replay_insert(
        &mut self,
        quad: &LogicalQuad,
        ids: &[SystemId],
    ) -> Result<(), ChangesetError> {
        let (graph, subject, predicate) = (quad.graph(), quad.subject(), quad.predicate());
        let held = self
            .write
            .holds(graph, subject, predicate, quad.object())?
            .is_some();
        if held {
            self.write
                .set_statement_ids(graph, subject, predicate, quad.object(), ids);
            return Ok(());
        }
        let declaration = self.declaration_of(predicate)?;
        let object = self.decode(quad.object())?;
        let resolved = Resolved {
            graph,
            subject,
            predicate,
            object: quad.object().clone(),
        };
        self.add(&resolved, &declaration, object.as_ref(), ids)
    }

    /// The declaration the predicate of ID `predicate` is written under, read through its IRI.
    fn declaration_of(
        &mut self,
        predicate: SystemId,
    ) -> Result<PredicateDeclaration, ChangesetError> {
        match self.decode(&EncodedTerm::id(predicate))? {
            Term::NamedNode(iri) => self.declaration(predicate, iri.as_ref()),
            Term::BlankNode(_) | Term::Literal(_) | Term::Triple(_) => Err(StoreError::corrupted(
                predicate.to_be_bytes().to_vec(),
                "a logged statement's predicate names no IRI".to_string(),
            )
            .into()),
        }
    }
}

/// Whether `a` and `b` name one statement: its graph, subject, predicate and object.
fn same_statement(a: &LogicalQuad, b: &LogicalQuad) -> bool {
    a.graph() == b.graph()
        && a.subject() == b.subject()
        && a.predicate() == b.predicate()
        && a.object() == b.object()
}
