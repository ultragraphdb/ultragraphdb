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
        self.answers.clear();
        self.temporaries.clear();
        self.temporary_snapshots.clear();
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
                    kept: self.pending.clone(),
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
                        kept: pending.clone(),
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
                            kept: ops[..k].to_vec(),
                            survivors: Some(k),
                        }
                    })
                    .collect()
            }
        }
    }

    /// Adopts the first outcome the store's `contents` match, and keeps a model of its own for each further one, or
    /// names the invariant no outcome satisfies. Outcomes the contents cannot tell apart, as a prefix drop kept and
    /// lost when it rewrote what its prefix held, differ in what a later step does, and the model follows each.
    pub(super) fn resolve_recovery(
        &mut self,
        outcomes: Vec<Outcome>,
        contents: &Contents,
    ) -> Result<(), Breach> {
        let matches = |o: &Outcome| {
            contents
                .difference(&o.state.contents(), &self.tainted)
                .is_none()
        };
        if !outcomes.iter().any(matches) {
            return Err(self.diagnose(&outcomes, contents));
        }
        let mut adopted = outcomes.into_iter().filter(matches).map(|outcome| {
            let mut model = self.clone();
            model.adopt(outcome);
            model
        });
        let Some(first) = adopted.next() else {
            return Err(Breach::new(
                Invariant::ContentsMatchModel,
                "the matched recovery outcome vanished",
            ));
        };
        let others: Vec<StoreModel> = adopted.collect();
        *self = first;
        self.others = others;
        Ok(())
    }

    fn adopt(&mut self, outcome: Outcome) {
        // A deletion the recovery lost never happened: only those the surviving state holds, or written at or below
        // its applied timestamp and since swept, still bound later writes.
        let state = &outcome.state;
        self.deletions.retain(|key, version| {
            *version <= state.applied_ts
                || state
                    .keys
                    .get(key)
                    .and_then(|versions| versions.get(version))
                    .is_some_and(|v| v.value.is_none())
        });
        self.current = Arc::new(outcome.state);
        self.base = outcome.base;
        self.pending = outcome.pending;
        self.drops.clear();
        self.recent.clear();
    }

    /// Why no outcome matches: a batch present in part, the applied timestamp, a synced write lost, or a set of
    /// survivors that is not a prefix.
    fn diagnose(&self, outcomes: &[Outcome], contents: &Contents) -> Breach {
        let ops = outcomes.last().map(|o| o.kept.clone()).unwrap_or_default();
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
    let ModelOp::Batch {
        entries,
        versionless,
        ..
    } = op
    else {
        return None;
    };
    let present = |e: &&ultragraphdb_traits_store::Entry| {
        contents
            .versions(e.key())
            .iter()
            .any(|v| v.version() == e.version() && v.is_deletion() == e.is_deletion())
    };
    // A versionless put is held while its key is, whatever a later batch wrote over it; a removal while it is not.
    let present_versionless = |e: &&ultragraphdb_traits_store::VersionlessEntry| {
        contents.versionless().contains_key(e.key()) != e.is_removal()
    };
    let (held, missing): (Vec<_>, Vec<_>) = entries.iter().partition(present);
    let (held_versionless, missing_versionless): (Vec<_>, Vec<_>) =
        versionless.iter().partition(present_versionless);
    let some_held = !held.is_empty() || !held_versionless.is_empty();
    let first_missing = missing
        .first()
        .map(|e| format!("key {} at {}", hex(e.key()), e.version()))
        .or_else(|| {
            missing_versionless
                .first()
                .map(|e| format!("versionless key {}", hex(e.key())))
        });
    first_missing.filter(|_| some_held).map(|first| {
        format!(
            "a batch survives with {} of its {} entries; {first} is missing",
            held.len() + held_versionless.len(),
            entries.len() + versionless.len(),
        )
    })
}
