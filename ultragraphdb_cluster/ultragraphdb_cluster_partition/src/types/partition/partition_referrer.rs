/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use oxrdf::vocab::rdf;
use oxrdf::{
    GraphName, GraphNameRef, NamedNode, NamedNodeRef, NamedOrBlankNode, NamedOrBlankNodeRef, Term,
    TermRef,
};
use ultragraphdb_cluster_placement::PartitionId;
use ultragraphdb_core_ids::{BucketGroup, Epoch};
use ultragraphdb_core_mapping::{ChangeRef, Changes, StatementView};
use ultragraphdb_store_apply::Operation;
use ultragraphdb_store_posting::CachedList;
use ultragraphdb_store_quads::iri_bucket;
use ultragraphdb_traits_blocks::BlockSource;
use ultragraphdb_traits_store::{Executor, ListCache, Store};
use ultragraphdb_transactions_txn::Transaction;

use crate::errors::partition_error::PartitionError;
use crate::types::answer::Answer;
use crate::types::partition::Partition;
use crate::types::reference::Reference;

/// The partition as a referrer (term-dictionary, "A reference to another partition's entity registers at its home").
impl<S, C, E, B> Partition<S, C, E, B>
where
    S: Store + Send + Sync + 'static,
    C: ListCache<Value = CachedList> + Send + Sync + 'static,
    B: BlockSource + Send + Sync + 'static,
    E: Executor,
{
    /// The references of `changeset`, which `txn` is about to write, that register at another partition first, at
    /// `epoch`, appended to `out`, sorted by home so they are batched per home within the request, each once: an inserted
    /// statement's IRI or skolem-IRI object homed elsewhere, from its subject's bucket group, when the partition holds no
    /// ID for it or no mark of `(entity, group)`, the mark read into `txn`'s read set; and any other IRI a change names
    /// homed elsewhere that the partition holds no ID for, naming no group: a predicate, a graph, a deleted statement's
    /// object, a property's predicate or value. A reference the partition already holds with its mark registers nothing.
    ///
    /// # Errors
    ///
    /// The transaction's errors reading a mark or minting a new blank subject; the store's error.
    pub fn references<W: Changes>(
        &self,
        txn: &mut Transaction<'_, S, C, E, B>,
        changeset: &W,
        epoch: Epoch,
        now_us: u64,
        out: &mut Vec<Reference>,
    ) -> Result<(), PartitionError> {
        let first = out.len();
        let tenant = changeset.tenant();
        // The first write of a tenant installs its catalog, which names `rdf:type`.
        self.unheld(tenant, rdf::TYPE.as_str(), epoch, out)?;
        for change in changeset.changes() {
            self.change_references(txn, tenant, &change, epoch, now_us, out)?;
        }
        batch(first, out);
        Ok(())
    }

    /// Refuses `changes`, a first request or a one-frame write, whose first subject named by an IRI or a skolem IRI lies outside
    /// the partition's ranges at `epoch`: the sender's map is stale (`PROTOCOL.md` §11.12). A later subject outside them is the
    /// transaction's to refuse, cross-partition.
    ///
    /// # Errors
    ///
    /// [`PartitionError::wrong_partition`] naming the bucket and its owner.
    pub fn check_first_subject<W: Changes>(
        &self,
        changes: &W,
        epoch: Epoch,
    ) -> Result<(), PartitionError> {
        let first = changes.changes().find_map(|change| match change {
            ChangeRef::Insert { subject, .. }
            | ChangeRef::InsertEdge { subject, .. }
            | ChangeRef::Delete { subject, .. }
            | ChangeRef::DeletePattern { subject, .. }
            | ChangeRef::SetProperty {
                statement: StatementView::Triple { subject, .. },
                ..
            }
            | ChangeRef::DeleteProperty {
                statement: StatementView::Triple { subject, .. },
                ..
            } => match subject {
                NamedOrBlankNodeRef::NamedNode(iri) => iri_bucket(changes.tenant(), iri.as_str()),
                NamedOrBlankNodeRef::BlankNode(_) => None,
            },
            ChangeRef::DeleteStatement { .. }
            | ChangeRef::SetProperty { .. }
            | ChangeRef::DeleteProperty { .. }
            | ChangeRef::Declare(_) => None,
        });
        first.map_or(Ok(()), |bucket| self.route(bucket, epoch))
    }

    /// As [`Partition::check_first_subject`], for a commit-time `operation` of `tenant` naming a subject.
    ///
    /// # Errors
    ///
    /// [`PartitionError::wrong_partition`] naming the bucket and its owner.
    pub fn check_operation_subject(
        &self,
        tenant: u64,
        operation: &Operation,
        epoch: Epoch,
    ) -> Result<(), PartitionError> {
        let subject = match operation {
            Operation::CompareAndSet { subject, .. } | Operation::CounterAdd { subject, .. } => {
                subject
            }
            Operation::InsertIfAbsent { .. } | Operation::UpsertAndLink { .. } => return Ok(()),
        };
        let NamedOrBlankNode::NamedNode(iri) = subject else {
            return Ok(());
        };
        iri_bucket(tenant, iri.as_str()).map_or(Ok(()), |bucket| self.route(bucket, epoch))
    }

    /// The references of one change, unsorted.
    fn change_references(
        &self,
        txn: &mut Transaction<'_, S, C, E, B>,
        tenant: u64,
        change: &ChangeRef<'_>,
        epoch: Epoch,
        now_us: u64,
        out: &mut Vec<Reference>,
    ) -> Result<(), PartitionError> {
        match change {
            ChangeRef::Insert {
                graph,
                subject,
                predicate,
                object,
            } => self.statement_references(
                txn,
                tenant,
                (*graph, *subject, *predicate, *object),
                epoch,
                now_us,
                out,
            )?,
            ChangeRef::InsertEdge {
                graph,
                subject,
                predicate,
                object,
                properties,
            } => {
                self.statement_references(
                    txn,
                    tenant,
                    (*graph, *subject, *predicate, *object),
                    epoch,
                    now_us,
                    out,
                )?;
                for (key, value) in properties {
                    self.unheld(tenant, key.as_str(), epoch, out)?;
                    self.unheld_term(tenant, *value, epoch, out)?;
                }
            }
            ChangeRef::Delete {
                graph,
                predicate,
                object,
                ..
            } => {
                self.unheld_graph(tenant, *graph, epoch, out)?;
                self.unheld(tenant, predicate.as_str(), epoch, out)?;
                self.unheld_term(tenant, *object, epoch, out)?;
            }
            ChangeRef::DeletePattern {
                graph, predicate, ..
            } => {
                self.unheld_graph(tenant, *graph, epoch, out)?;
                self.unheld(tenant, predicate.as_str(), epoch, out)?;
            }
            ChangeRef::SetProperty {
                predicate, value, ..
            } => {
                self.unheld(tenant, predicate.as_str(), epoch, out)?;
                self.unheld_term(tenant, *value, epoch, out)?;
            }
            ChangeRef::DeleteProperty { predicate, .. } => {
                self.unheld(tenant, predicate.as_str(), epoch, out)?;
            }
            ChangeRef::DeleteStatement { graph, .. } => {
                self.unheld_graph(tenant, *graph, epoch, out)?;
            }
            ChangeRef::Declare(_) => {}
        }
        Ok(())
    }

    /// The references a read of `predicate` in `graph` of `tenant` registers first, at `epoch`, appended to `out` as
    /// [`Partition::references`] appends them: a read gets or creates what it names, so a term no statement holds is
    /// recorded absent by its ID, and a predicate or graph homed elsewhere takes its ID from its home.
    ///
    /// # Errors
    ///
    /// The store's error.
    pub fn read_references(
        &self,
        tenant: u64,
        graph: &GraphName,
        predicate: &NamedNode,
        epoch: Epoch,
        out: &mut Vec<Reference>,
    ) -> Result<(), PartitionError> {
        let first = out.len();
        self.unheld_graph(tenant, graph.as_ref(), epoch, out)?;
        self.unheld(tenant, predicate.as_str(), epoch, out)?;
        batch(first, out);
        Ok(())
    }

    /// The references `operation`, a commit-time operation of `txn`, registers first, at `epoch`, appended to `out` as
    /// [`Partition::references`] appends them. The node an insert-if-absent or upsert-and-link claims takes its bucket
    /// at the commit step, so no group can register for it: its value or a link naming a node homed elsewhere is refused.
    ///
    /// # Errors
    ///
    /// [`PartitionError::cross_partition`] naming the partition of such a value or link; the transaction's errors reading
    /// a mark; the store's error.
    pub fn operation_references(
        &self,
        txn: &mut Transaction<'_, S, C, E, B>,
        tenant: u64,
        operation: &Operation,
        epoch: Epoch,
        now_us: u64,
        out: &mut Vec<Reference>,
    ) -> Result<(), PartitionError> {
        let first = out.len();
        self.unheld(tenant, rdf::TYPE.as_str(), epoch, out)?;
        match operation {
            Operation::InsertIfAbsent {
                graph,
                predicate,
                value,
                properties,
            } => {
                self.claim_references(tenant, (graph, predicate, value, properties), epoch, out)?
            }
            Operation::UpsertAndLink {
                graph,
                predicate,
                value,
                properties,
                links,
            } => {
                self.claim_references(tenant, (graph, predicate, value, properties), epoch, out)?;
                for link in links {
                    self.unheld(tenant, link.predicate().as_str(), epoch, out)?;
                    if let NamedOrBlankNode::NamedNode(other) = link.other() {
                        self.refuse_elsewhere(tenant, other.as_str(), epoch)?;
                    }
                }
            }
            Operation::CompareAndSet {
                graph,
                subject,
                predicate,
                expected,
                new,
            } => {
                self.unheld_term(tenant, expected.as_ref(), epoch, out)?;
                self.statement_references(
                    txn,
                    tenant,
                    (
                        graph.as_ref(),
                        subject.as_ref(),
                        predicate.as_ref(),
                        new.as_ref(),
                    ),
                    epoch,
                    now_us,
                    out,
                )?;
            }
            Operation::CounterAdd {
                graph, predicate, ..
            } => {
                self.unheld_graph(tenant, graph.as_ref(), epoch, out)?;
                self.unheld(tenant, predicate.as_str(), epoch, out)?;
            }
        }
        batch(first, out);
        Ok(())
    }

    /// The references of a claim's graph, predicate and properties; a value naming a node homed elsewhere is refused.
    fn claim_references(
        &self,
        tenant: u64,
        (graph, predicate, value, properties): (
            &GraphName,
            &NamedNode,
            &Term,
            &[(NamedNode, Term)],
        ),
        epoch: Epoch,
        out: &mut Vec<Reference>,
    ) -> Result<(), PartitionError> {
        self.unheld_graph(tenant, graph.as_ref(), epoch, out)?;
        self.unheld(tenant, predicate.as_str(), epoch, out)?;
        if let Term::NamedNode(value) = value {
            self.refuse_elsewhere(tenant, value.as_str(), epoch)?;
        }
        for (key, property) in properties {
            self.unheld(tenant, key.as_str(), epoch, out)?;
            self.unheld_term(tenant, property.as_ref(), epoch, out)?;
        }
        Ok(())
    }

    /// Refuses `iri` as cross-partition when it is homed elsewhere.
    fn refuse_elsewhere(&self, tenant: u64, iri: &str, epoch: Epoch) -> Result<(), PartitionError> {
        match self.home_elsewhere(tenant, iri, epoch) {
            Some(home) => Err(PartitionError::cross_partition(self.id.get(), home.get())),
            None => Ok(()),
        }
    }

    /// The references of an inserted statement: its graph and predicate by [`Partition::unheld`]; its object, an IRI
    /// homed elsewhere, from its subject's bucket group.
    fn statement_references(
        &self,
        txn: &mut Transaction<'_, S, C, E, B>,
        tenant: u64,
        (graph, subject, predicate, object): (
            GraphNameRef<'_>,
            NamedOrBlankNodeRef<'_>,
            NamedNodeRef<'_>,
            TermRef<'_>,
        ),
        epoch: Epoch,
        now_us: u64,
        out: &mut Vec<Reference>,
    ) -> Result<(), PartitionError> {
        self.unheld_graph(tenant, graph, epoch, out)?;
        self.unheld(tenant, predicate.as_str(), epoch, out)?;
        let TermRef::NamedNode(object) = object else {
            return self.unheld_term(tenant, object, epoch, out);
        };
        let Some(home) = self.home_elsewhere(tenant, object.as_str(), epoch) else {
            return Ok(());
        };
        let bucket = match subject {
            NamedOrBlankNodeRef::NamedNode(iri) => iri_bucket(tenant, iri.as_str()),
            NamedOrBlankNodeRef::BlankNode(_) => Some(txn.node_id(subject, now_us)?.bucket()),
        };
        let Some(bucket) = bucket else {
            return Ok(());
        };
        let group = BucketGroup::of(bucket);
        let held = self.applier.quads().id_of(tenant, object.as_str())?;
        if let Some(id) = held
            && txn.read_mark(id, group, now_us)?.is_some()
        {
            return Ok(());
        }
        out.push(Reference::new(
            home,
            tenant,
            object.as_str().to_string(),
            held,
            Some(group),
            0,
        ));
        Ok(())
    }

    /// A reference naming no group to `iri` when it is homed elsewhere and the partition holds no ID for it.
    fn unheld(
        &self,
        tenant: u64,
        iri: &str,
        epoch: Epoch,
        out: &mut Vec<Reference>,
    ) -> Result<(), PartitionError> {
        if let Some(home) = self.home_elsewhere(tenant, iri, epoch)
            && self.applier.quads().id_of(tenant, iri)?.is_none()
        {
            out.push(Reference::new(home, tenant, iri.to_string(), None, None, 0));
        }
        Ok(())
    }

    /// As [`Partition::unheld`] for a named graph.
    fn unheld_graph(
        &self,
        tenant: u64,
        graph: GraphNameRef<'_>,
        epoch: Epoch,
        out: &mut Vec<Reference>,
    ) -> Result<(), PartitionError> {
        match graph {
            GraphNameRef::NamedNode(iri) => self.unheld(tenant, iri.as_str(), epoch, out),
            GraphNameRef::BlankNode(_) | GraphNameRef::DefaultGraph => Ok(()),
        }
    }

    /// As [`Partition::unheld`] for a term that is an IRI; a literal or a blank node's label is the partition's own.
    fn unheld_term(
        &self,
        tenant: u64,
        term: TermRef<'_>,
        epoch: Epoch,
        out: &mut Vec<Reference>,
    ) -> Result<(), PartitionError> {
        match term {
            TermRef::NamedNode(iri) => self.unheld(tenant, iri.as_str(), epoch, out),
            // A triple term is refused where the write encodes it: it holds no statement here.
            TermRef::BlankNode(_) | TermRef::Literal(_) | TermRef::Triple(_) => Ok(()),
        }
    }

    /// The partition owning `iri`'s bucket at `epoch` when it is another one.
    fn home_elsewhere(&self, tenant: u64, iri: &str, epoch: Epoch) -> Option<PartitionId> {
        let home = self.placement.partition_of(iri_bucket(tenant, iri)?, epoch);
        (home != self.id).then_some(home)
    }

    /// Takes `answer`, the home's answer to `reference`, into `txn` at `now_us`: an IRI the partition held no ID for
    /// adopted by the ID the home gave, and the member's mark at the answered generation, so the transaction's commit
    /// writes the mark and the copied dictionary entries and is held until the close the home named is delivered here.
    ///
    /// # Errors
    ///
    /// The transaction's errors.
    pub fn adopt(
        &self,
        txn: &mut Transaction<'_, S, C, E, B>,
        reference: &Reference,
        answer: &Answer,
        now_us: u64,
    ) -> Result<(), PartitionError> {
        if reference.entity().is_none() {
            txn.adopt(reference.iri(), answer.entity(), answer.close(), now_us)?;
        }
        if let (Some(group), Some(generation)) = (reference.group(), answer.generation()) {
            txn.mark(answer.entity(), group, generation, answer.close(), now_us)?;
        }
        Ok(())
    }
}

/// Sorts the references from `first` on by home and removes repeats, so they are batched per home within the request.
fn batch(first: usize, out: &mut Vec<Reference>) {
    let mut added = out.split_off(first);
    added.sort_by(|a, b| (a.home(), a.iri(), a.group()).cmp(&(b.home(), b.iri(), b.group())));
    added.dedup();
    out.append(&mut added);
}
