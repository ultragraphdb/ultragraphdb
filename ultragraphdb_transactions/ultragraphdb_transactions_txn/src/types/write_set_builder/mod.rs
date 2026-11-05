/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::collections::BTreeSet;

use ultragraphdb_core_codec::PostingList;
use ultragraphdb_core_ids::SystemId;
use ultragraphdb_core_keys::{Graph, Keyspace};
use ultragraphdb_error_invariant::InvariantError;
use ultragraphdb_protocol_wire::{
    BuildOutcome, RecordKind, RecordOperation, RecordRegistration, WriteSetRecord,
};
use ultragraphdb_store_apply::{BuildEnd, DropTarget, Executed, LogicalQuad};
use ultragraphdb_store_quads::posting_object;

use crate::errors::txn_error::TxnError;
use crate::types::read_set::ReadSet;
use crate::types::txn_limits::TxnLimits;

/// Builds an executed transaction's logical write-set record (`PROTOCOL.md` §12.2, write-ahead-log, "A round's record
/// is logical") in `ultragraphdb_protocol_wire`'s [`WriteSetRecord`]: its resolved quads with their system IDs, its
/// statement records, its drop or build as an operation, its session, request, principal and catalog version, the
/// commit record the client attached, and the conflict keys of its writes; never a key the store derives.
///
/// | Section | What the transaction's writes put there |
/// |---|---|
/// | 0 | each statement the net effect inserts: the quad, graph, subject, predicate and object encoding, once per statement ID it adds, or once with none |
/// | 1 | each statement the net effect removes, as section 0: a delete by pattern as the statements it resolved to, and an ID removed from an object left held, once per ID |
/// | 2 | each statement record written, by its graph and statement ID, a removal for a record deleted |
/// | 3 | the record's operation: a drop of a predicate or a graph by its system ID, of a tenant's data or a tenant; a build's end |
/// | 4 | each registration the transaction relied on as a referrer ([`Executed::marks`]): the entity, the subject bucket group and the generation its home's reply named |
/// | 5 | the commit record the client attached, unchanged |
/// | 6 | conflict points: the key of every write but the lists named object by object, a predicate's implicit default catalog entry and the dictionary's entries, which conflict with nothing |
/// | 7 | conflict statements: a list key and an object it changed, for each forward list changed by no more than the bound's objects |
/// | 8 | conflict ranges: a drop's key ranges, each its first key and the key past its last |
///
/// The dictionary entries the transaction creates are not in its record but in its round's (`PROTOCOL.md` §12.1); the
/// commit-time operations' results (sections 9 to 11) are added by the commit step to the record it builds again.
#[derive(Debug, Clone, Copy)]
pub struct WriteSetBuilder<'e> {
    executed: &'e Executed,
    session: u128,
    request: u64,
    principal: u64,
    catalog: u64,
    commit_record: &'e [u8],
    objects_per_list: usize,
}

impl<'e> WriteSetBuilder<'e> {
    /// The record of `executed`, committed as request `request` of session `session` (0 for none) with
    /// `commit_record`, its lists named object by object up to the default bound of [`crate::TxnLimits`].
    pub fn new(
        executed: &'e Executed,
        session: u128,
        request: u64,
        commit_record: &'e [u8],
    ) -> Self {
        Self {
            executed,
            session,
            request,
            principal: 0,
            catalog: 0,
            commit_record,
            objects_per_list: TxnLimits::default().objects_per_list(),
        }
    }

    /// The builder writing `principal`, the principal the transaction acted for; 0, without authentication, by default.
    pub fn with_principal(self, principal: u64) -> Self {
        Self { principal, ..self }
    }

    /// The builder writing `catalog`, the catalog version of the tenant the transaction executed against; 0 by
    /// default.
    pub fn with_catalog(self, catalog: u64) -> Self {
        Self { catalog, ..self }
    }

    /// The builder naming at most `objects` objects of one list one by one.
    pub fn with_objects_per_list(self, objects: usize) -> Self {
        Self {
            objects_per_list: objects,
            ..self
        }
    }

    /// The record.
    ///
    /// # Errors
    ///
    /// [`TxnError::changeset`] for a forward list's posting that names no object, or a key that does not parse:
    /// corruption the execution cannot have written.
    pub fn build(&self) -> Result<WriteSetRecord, TxnError> {
        let executed = self.executed;
        let tenant = executed.tenant();
        let mut record = match (executed.build_end(), executed.catalog_version()) {
            // A build's completion names the catalog version that started it, the one in effect while it ran (a later
            // version waits for the build), and how it ended, so replay ends it the same way (`PROTOCOL.md` §12.2).
            (Some(end), _) => {
                let outcome = match end {
                    BuildEnd::Swap(_) => BuildOutcome::Swap,
                    BuildEnd::Undo(_) => BuildOutcome::Undo,
                };
                WriteSetRecord::build(
                    tenant,
                    RecordOperation::CompleteBuild {
                        version: self.catalog,
                        outcome,
                    },
                )
            }
            // A catalog version taking effect names the version, its builds derived from it on replay.
            (None, Some(version)) => {
                let mut start =
                    WriteSetRecord::build(tenant, RecordOperation::CatalogTakesEffect(version));
                start.set_catalog(version);
                return Ok(start);
            }
            (None, None) => self.transaction_record()?,
        };
        if record.kind() == RecordKind::Build {
            record.set_catalog(self.catalog);
            return Ok(record);
        }
        let logical = executed.logical();
        let mut terms = BTreeSet::new();
        for quad in logical.inserted() {
            record.push_inserted(quad.key(), quad.statement().map(SystemId::to_be_bytes));
            terms.extend(term_keys(quad));
        }
        for quad in logical.removed() {
            record.push_removed(quad.key(), quad.statement().map(SystemId::to_be_bytes));
            terms.extend(term_keys(quad));
        }
        for (graph, id, value) in logical.records() {
            let key = [graph.to_be_bytes(), id.to_be_bytes()].concat();
            record.push_statement_record(key, value.clone());
        }
        for mark in executed.marks() {
            record.push_registration(RecordRegistration::new(
                mark.entity(),
                mark.group(),
                mark.generation(),
            ));
        }
        self.push_conflicts(&mut record);
        terms
            .into_iter()
            .for_each(|key| record.push_conflict_point(key.to_vec()));
        Ok(record)
    }

    /// Clears `out` and writes `record`'s bytes into it: their length.
    ///
    /// # Errors
    ///
    /// [`TxnError::write_set_too_large`] when the record is larger than `limit` bytes, or than `u32::MAX`; `out` is then
    /// left empty, its capacity kept.
    pub fn encode_into(
        record: &WriteSetRecord,
        out: &mut Vec<u8>,
        limit: usize,
    ) -> Result<usize, TxnError> {
        out.clear();
        record.encode_into(out);
        let limit = limit.min(u32::MAX as usize);
        if out.len() > limit {
            let size = out.len();
            out.clear();
            return Err(TxnError::write_set_too_large(size, limit));
        }
        Ok(out.len())
    }

    /// The bytes the dictionary entries `executed` creates take in its round's dictionary section, each 8 bytes of
    /// lengths, its key and its value, padded to 8: what the transaction's write set adds to its record.
    pub fn dictionary_len(executed: &Executed) -> usize {
        let dictionary = Keyspace::Dictionary.byte();
        executed
            .writes()
            .versionless()
            .iter()
            .filter(|e| e.key().first() == Some(&dictionary))
            .map(|e| (8 + e.key().len() + e.value().map_or(0, <[u8]>::len)).div_ceil(8) * 8)
            .sum()
    }

    /// Builds the record and writes its bytes into `out`, cleared first: their length.
    ///
    /// # Errors
    ///
    /// As [`WriteSetBuilder::build`]; [`TxnError::write_set_too_large`] naming their size when the record and its
    /// dictionary entries ([`WriteSetBuilder::dictionary_len`]) together are larger than `limit`, or than `u32::MAX`;
    /// `out` is then left empty.
    pub fn write_into(&self, out: &mut Vec<u8>, limit: usize) -> Result<usize, TxnError> {
        out.clear();
        let record = self.build()?;
        // The round holds the transaction's dictionary entries beside its record: both count against the bound.
        let dictionary = Self::dictionary_len(self.executed);
        let limit = limit.min(u32::MAX as usize);
        let len = Self::encode_into(&record, out, usize::MAX)?;
        let size = len.saturating_add(dictionary);
        if size > limit {
            out.clear();
            return Err(TxnError::write_set_too_large(size, limit));
        }
        Ok(len)
    }
}

impl WriteSetBuilder<'_> {
    /// The kind-1 record's fixed part and its operation: a transaction's, or a Drop's, its read version 0.
    fn transaction_record(&self) -> Result<WriteSetRecord, TxnError> {
        let executed = self.executed;
        let mut record = WriteSetRecord::transaction(executed.tenant());
        record.set_catalog(self.catalog);
        record.set_session(self.session.to_be_bytes(), self.request);
        record.set_principal(self.principal);
        record.set_commit_record(self.commit_record.to_vec());
        match executed.dropped() {
            Some(target) => record.push_operation(drop_operation(target)?),
            None => record.set_read_version(executed.read_ts()),
        }
        Ok(record)
    }

    /// Whether the delta of `key` is named object by object: a forward list, within the bound, every posting naming
    /// an object.
    fn by_objects(&self, key: &[u8], delta: &PostingList) -> bool {
        matches!(
            key.first().and_then(|b| Keyspace::from_byte(*b)),
            Some((Keyspace::Forward, false))
        ) && delta.postings().len() <= self.objects_per_list
            && delta
                .postings()
                .iter()
                .all(|p| posting_object(key, p).is_ok())
    }

    /// Pushes the conflict sections: a point for every write but the lists named object by object, a predicate's
    /// implicit default catalog entry and the dictionary's entries; a statement for each object of the lists named
    /// object by object; and a drop's ranges.
    fn push_conflicts(&self, record: &mut WriteSetRecord) {
        let executed = self.executed;
        let writes = executed.writes();
        for (key, delta) in writes.deltas() {
            if self.by_objects(key, delta) {
                for object in delta
                    .postings()
                    .iter()
                    .filter_map(|p| posting_object(key, p).ok())
                {
                    record.push_conflict_statement(key.clone(), object.as_bytes().to_vec());
                }
            } else {
                record.push_conflict_point(key.clone());
            }
        }
        // A predicate's implicit default entry is an insert-if-absent on its catalog key, no catalog change.
        let defaults = executed.defaults();
        for e in writes.entries() {
            if defaults
                .binary_search_by(|k| k.as_slice().cmp(e.key()))
                .is_err()
            {
                record.push_conflict_point(e.key().to_vec());
            }
        }
        // The dictionary is naming state: creating an entry conflicts with nothing.
        let dictionary = Keyspace::Dictionary.byte();
        for e in writes
            .versionless()
            .iter()
            .filter(|e| e.key().first() != Some(&dictionary))
        {
            record.push_conflict_point(e.key().to_vec());
        }
        for (start, end) in executed.drop_ranges() {
            record.push_conflict_range(start.clone(), end.clone());
        }
    }
}

/// The term keys of the terms `quad` names: its subject, predicate, object when an ID and graph when named.
fn term_keys(quad: &LogicalQuad) -> impl Iterator<Item = [u8; 17]> {
    let graph = match quad.graph() {
        Graph::Named(id) => Some(id),
        Graph::Default => None,
    };
    [
        Some(quad.subject()),
        Some(quad.predicate()),
        quad.object().as_id(),
        graph,
    ]
    .into_iter()
    .flatten()
    .map(ReadSet::term_key)
}

/// The record operation of the drop of `target`.
fn drop_operation(target: DropTarget) -> Result<RecordOperation, TxnError> {
    match target {
        DropTarget::Predicate(id) => Ok(RecordOperation::DropPredicate(id)),
        DropTarget::Graph(Graph::Named(id)) => Ok(RecordOperation::DropGraph(id)),
        DropTarget::Graph(Graph::Default) => Err(TxnError::changeset(
            InvariantError::violated(
                "a drop of the default graph names no graph a record operation takes",
            )
            .into(),
        )),
        DropTarget::Data => Ok(RecordOperation::DropTenantData),
        DropTarget::Tenant => Ok(RecordOperation::DropTenant),
    }
}
