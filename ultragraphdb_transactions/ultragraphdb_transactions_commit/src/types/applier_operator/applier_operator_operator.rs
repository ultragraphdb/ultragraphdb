/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_protocol_wire::OperationResult;
use ultragraphdb_store_apply::{ChangesetError, ChangesetErrorEnum};
use ultragraphdb_store_posting::CachedList;
use ultragraphdb_traits_store::{Executor, ListCache, Store, VersionlessEntry};
use ultragraphdb_transactions_txn::{Operated, Prepared, TxnError, TxnErrorEnum};

use crate::errors::commit_error::CommitError;
use crate::traits::operator::Operator;
use crate::types::applier_operator::ApplierOperator;

impl<S, C, E> Operator<Prepared> for ApplierOperator<'_, S, C, E>
where
    S: Store + Send + Sync + 'static,
    C: ListCache<Value = CachedList> + Send + Sync + 'static,
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
        let mut found = Vec::new();
        let operated = proposal
            .operated(applier, overlay, &dictionary, &mut found)
            .map_err(refusal)?;
        results.extend(
            found
                .into_iter()
                .map(|(uid, created)| OperationResult::new(uid, created)),
        );
        Ok(operated)
    }

    fn staged_record<'s>(&self, staged: &'s Operated) -> &'s [u8] {
        staged.record()
    }

    fn adopt(&mut self, proposal: &mut Prepared, staged: Operated) {
        proposal.adopt(staged);
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
            CommitError::OperationFailed(found)
        }
        _ => CommitError::Refused(error.to_string()),
    }
}
