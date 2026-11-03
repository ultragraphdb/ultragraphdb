/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use oxrdf::Term;
use ultragraphdb_core_ids::{Bucket, Namespace, Version};
use ultragraphdb_core_terms::{EncodedTerm, TermError};
use ultragraphdb_protocol_wire::OperationResult;
use ultragraphdb_store_apply::{ChangesetError, ChangesetErrorEnum, Operation};
use ultragraphdb_store_posting::CachedList;
use ultragraphdb_traits_blocks::BlockSource;
use ultragraphdb_traits_store::{Executor, ListCache, Store, VersionlessEntry};
use ultragraphdb_transactions_txn::{Operated, Prepared, TxnError, TxnErrorEnum};

use crate::errors::commit_error::CommitError;
use crate::traits::operator::Operator;
use crate::types::applier_operator::ApplierOperator;

impl<S, C, E, B> Operator<Prepared> for ApplierOperator<'_, S, C, E, B>
where
    S: Store + Send + Sync + 'static,
    C: ListCache<Value = CachedList> + Send + Sync + 'static,
    B: BlockSource + Send + Sync + 'static,
    E: Executor,
{
    type Staged = Operated;

    fn begin_round(&mut self) {
        self.overlay = None;
    }

    fn operate<'p, I>(
        &mut self,
        proposal: &Prepared,
        unapplied: I,
        results: &mut Vec<OperationResult>,
    ) -> Result<Operated, CommitError>
    where
        I: Iterator<Item = &'p Prepared>,
    {
        self.claim(proposal)?;
        let unapplied: Vec<&Prepared> = unapplied.collect();
        let applier = self.applier;
        let overlay = self.overlay.get_or_insert_with(|| {
            let mut overlay = applier.quads().lists().batch();
            for prepared in &unapplied {
                overlay.stack(prepared.executed().writes());
            }
            overlay
        });
        // The dictionary entries the unapplied proposals made: in no layer the operations read.
        let dictionary: Vec<&VersionlessEntry> = unapplied
            .iter()
            .flat_map(|p| p.executed().writes().versionless())
            .collect();
        proposal
            .operated(applier, overlay, &dictionary, results)
            .map_err(refusal)
    }

    fn staged_record<'s>(&self, staged: &'s Operated) -> &'s [u8] {
        staged.record()
    }

    fn staged_dictionary<'s>(
        &self,
        staged: &'s Operated,
    ) -> impl Iterator<Item = (&'s [u8], &'s [u8])> {
        staged.dictionary()
    }

    fn adopt(&mut self, proposal: &mut Prepared, staged: Operated) {
        proposal.adopt(staged);
    }

    fn discard(&mut self, proposal: &Prepared, staged: Operated) {
        proposal.release_operated(self.applier, staged);
    }

    fn admitted(&mut self, proposal: &Prepared) {
        if let Some(overlay) = self.overlay.as_mut() {
            overlay.stack(proposal.executed().writes());
        }
    }
}

/// The commit step's refusal of an operation the transaction's layer refused: a compare-and-set that did not hold
/// with the value found, anything else refused with its reason.
fn refusal(error: TxnError) -> CommitError {
    match error.0 {
        TxnErrorEnum::Changeset(ChangesetError(ChangesetErrorEnum::OperationFailed { found })) => {
            CommitError::operation_failed(found)
        }
        TxnErrorEnum::ReadAboveWatermark { .. }
        | TxnErrorEnum::PastLimit { .. }
        | TxnErrorEnum::WriteSetTooLarge { .. }
        | TxnErrorEnum::ReadSetTooLarge { .. }
        | TxnErrorEnum::ReadOnly
        | TxnErrorEnum::Namespace { .. }
        | TxnErrorEnum::Ended
        | TxnErrorEnum::Changeset(_)
        | TxnErrorEnum::Declaration => CommitError::refused(error.to_string()),
        TxnErrorEnum::CrossPartition { partition, other } => {
            CommitError::cross_partition(partition, other)
        }
    }
}

impl<S, C, E, B> ApplierOperator<'_, S, C, E, B>
where
    S: Store + Send + Sync + 'static,
    C: ListCache<Value = CachedList> + Send + Sync + 'static,
    B: BlockSource + Send + Sync + 'static,
    E: Executor,
{
    /// Refuses `proposal` cross-partition when one of its insert-if-absent or upsert-and-link operations claims a value
    /// whose bucket another partition than the operator's owns, at the transaction's read epoch.
    fn claim(&self, proposal: &Prepared) -> Result<(), CommitError> {
        let Some((partition, placement)) = self.binding else {
            return Ok(());
        };
        let epoch = Version::from_u64(proposal.read_version()).epoch();
        for operation in proposal.operations() {
            if let Some(bucket) = self.claim_bucket(proposal.namespace(), operation)? {
                let owner = placement.partition_of(bucket, epoch);
                if owner != partition {
                    return Err(CommitError::cross_partition(partition.get(), owner.get()));
                }
            }
        }
        Ok(())
    }

    /// The bucket `operation` claims its value at, `None` for an operation claiming none or a predicate or datatype
    /// the dictionary does not hold, which no unique value of the store names.
    fn claim_bucket(
        &self,
        namespace: u64,
        operation: &Operation,
    ) -> Result<Option<Bucket>, CommitError> {
        let (Operation::InsertIfAbsent {
            predicate, value, ..
        }
        | Operation::UpsertAndLink {
            predicate, value, ..
        }) = operation
        else {
            return Ok(None);
        };
        let quads = self.applier.quads();
        let held = |iri: &str| {
            quads
                .id_of(namespace, iri)
                .map_err(|e| CommitError::refused(e.to_string()))
        };
        let Some(predicate) = held(predicate.as_str())? else {
            return Ok(None);
        };
        let encoded = match value {
            Term::NamedNode(iri) => held(iri.as_str())?.map(EncodedTerm::id),
            Term::Literal(literal) => {
                let datatype = literal.datatype();
                let id = held(datatype.as_str())?;
                let encoded = EncodedTerm::literal::<TermError>(literal.as_ref(), |_| {
                    id.ok_or_else(|| {
                        TermError::malformed(Vec::new(), "a datatype the dictionary does not hold")
                    })
                });
                // A datatype the dictionary does not hold types no value the store holds, so none is claimed.
                match encoded {
                    Ok(encoded) => Some(encoded.term().clone()),
                    Err(_) => None,
                }
            }
            Term::BlankNode(_) | Term::Triple(_) => None,
        };
        Ok(encoded.map(|e| Bucket::of_value(Namespace::new(namespace), predicate, e.as_bytes())))
    }
}
