/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::sync::Arc;

use crate::types::breach::Breach;
use crate::types::contents::Contents;
use crate::types::invariant::Invariant;
use crate::types::store_model::model_state::ModelOp;
use crate::types::store_model::{Outcome, Status, StoreModel};
use crate::utils::hex::hex;

impl StoreModel {
    /// The states a recovery may end in. A restart keeps every pending write and may keep the one in flight; a power
    /// loss keeps a prefix of them, the one in flight last.
    pub(super) fn outcomes(&mut self) -> Vec<Outcome> {
        let status = std::mem::take(&mut self.status);
        self.snapshots.clear();
        self.readers.clear();
        let replay = |ops: &[ModelOp]| {
            let mut state = (*self.base).clone();
            ops.iter().for_each(|op| state.apply(op));
            state
        };
        match status {
            Status::Open
            | Status::Down {
                power_loss: false,
                in_flight: None,
            } => {
                let state = replay(&self.pending);
                vec![Outcome {
                    state,
                    base: self.base.clone(),
                    pending: self.pending.clone(),
                    survivors: None,
                }]
            }
            Status::Poisoned { in_flight: op }
            | Status::Down {
                power_loss: false,
                in_flight: Some(op),
            } => {
                let mut with = self.pending.clone();
                with.push(op);
                [self.pending.clone(), with]
                    .into_iter()
                    .map(|pending| Outcome {
                        state: replay(&pending),
                        base: self.base.clone(),
                        pending,
                        survivors: None,
                    })
                    .collect()
            }
            Status::Down {
                power_loss: true,
                in_flight,
            } => {
                let mut ops = self.pending.clone();
                ops.extend(in_flight);
                (0..=ops.len())
                    .map(|k| {
                        let state = replay(&ops[..k]);
                        Outcome {
                            base: Arc::new(state.clone()),
                            state,
                            pending: Vec::new(),
                            survivors: Some(k),
                        }
                    })
                    .collect()
            }
        }
    }

    /// Adopts the outcome the store's `contents` match, or names the invariant no outcome satisfies.
    pub(super) fn resolve_recovery(
        &mut self,
        outcomes: Vec<Outcome>,
        contents: &Contents,
    ) -> Result<(), Breach> {
        let found = outcomes.iter().position(|o| {
            contents
                .difference(&o.state.contents(), &self.tainted)
                .is_none()
        });
        let Some(at) = found else {
            return Err(self.diagnose(&outcomes, contents));
        };
        let Some(outcome) = outcomes.into_iter().nth(at) else {
            return Err(Breach::new(
                Invariant::ContentsMatchModel,
                "the matched recovery outcome vanished",
            ));
        };
        self.current = Arc::new(outcome.state);
        self.base = outcome.base;
        self.pending = outcome.pending;
        self.drops.clear();
        self.recent.clear();
        Ok(())
    }

    /// Why no outcome matches: a batch present in part, the applied timestamp, a synced write lost, or a set of
    /// survivors that is not a prefix.
    fn diagnose(&self, outcomes: &[Outcome], contents: &Contents) -> Breach {
        let ops = outcomes
            .last()
            .map(|o| o.pending.clone())
            .unwrap_or_default();
        if let Some(detail) = ops.iter().find_map(|op| partial_batch(op, contents)) {
            return Breach::new(Invariant::BatchWhole, detail);
        }
        let differences: Vec<String> = outcomes
            .iter()
            .filter_map(|o| contents.difference(&o.state.contents(), &self.tainted))
            .collect();
        let first = differences.first().cloned().unwrap_or_default();
        if outcomes
            .iter()
            .all(|o| o.state.applied_ts != contents.applied_ts())
        {
            return Breach::new(Invariant::AppliedTsIsHighest, first);
        }
        let base = self.base.contents();
        let lost = base.keys().iter().find(|(key, versions)| {
            !self.tainted.contains(*key)
                && versions.iter().any(|v| !contents.versions(key).contains(v))
        });
        if let Some((key, _)) = lost
            && outcomes.iter().all(|o| o.survivors.is_some())
        {
            return Breach::new(
                Invariant::SyncedBatchSurvives,
                format!(
                    "a synced version of key {} is missing after recovery: {first}",
                    hex(key)
                ),
            );
        }
        if outcomes.iter().all(|o| o.survivors.is_some()) {
            return Breach::new(Invariant::SurvivorsArePrefix, first);
        }
        Breach::new(Invariant::ContentsMatchModel, first)
    }
}

/// A description of how `op` survives in part in `contents`, if it does: some of its entries hold their version
/// with their value and others hold no such version.
pub(super) fn partial_batch(op: &ModelOp, contents: &Contents) -> Option<String> {
    let ModelOp::Batch { entries, .. } = op else {
        return None;
    };
    let present = |e: &&ultragraphdb_traits_store::Entry| {
        contents
            .versions(e.key())
            .iter()
            .any(|v| v.version() == e.version() && v.is_deletion() == e.is_deletion())
    };
    let (held, missing): (Vec<_>, Vec<_>) = entries.iter().partition(present);
    (!held.is_empty() && !missing.is_empty()).then(|| {
        format!(
            "a batch survives with {} of its {} entries; key {} at {} is missing",
            held.len(),
            entries.len(),
            hex(missing[0].key()),
            missing[0].version()
        )
    })
}
