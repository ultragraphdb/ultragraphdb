/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! Recovery in the simulator: a workload of every kind of command over the write-ahead log on its simulated storage,
//! killed after every append, before and after its sync, and with every storage operation of the run failed in each
//! crash mode. A reference model applies the same commands to a plain state; after each kill the recovered state must
//! be the model's after some prefix of the commands appended that holds every one acknowledged, read at that prefix's
//! position, and the group must go on deciding from it.

use ultragraphdb_cluster_metadata::{MetadataCommand, MetadataConfig, MetadataState};
use ultragraphdb_core_ids::{Epoch, TypePrefix};
use ultragraphdb_error_cluster::ClusterError;
use ultragraphdb_tool_bdd::{TestError, TestResult};
use ultragraphdb_traits_catalog::DefaultEntry;
use ultragraphdb_transactions_log::{FaultMode, SimStorage};

use crate::utils::harness::{
    Group, block_on, crash_and_recover, fresh_with, node, ns, pid, verifier,
};

/// The workload's steps, one command each.
const STEPS: usize = 12;

/// The states after each prefix of the commands appended, the empty state first, and how many were acknowledged.
#[derive(Debug)]
struct Model {
    states: Vec<MetadataState>,
    acknowledged: usize,
}

impl Model {
    fn new() -> Self {
        Self {
            states: vec![MetadataState::new()],
            acknowledged: 0,
        }
    }

    fn last(&self) -> MetadataState {
        self.states.last().cloned().unwrap_or_default()
    }

    /// Records `command`, which the group appended.
    fn appended(&mut self, command: &MetadataCommand) -> TestResult {
        let mut next = self.last();
        next.apply(command)?;
        self.states.push(next);
        Ok(())
    }

    fn appends(&self) -> usize {
        self.states.len() - 1
    }
}

/// The settings: a registry of one session, so the workload's second registration evicts its first, session 3.
fn settings() -> TestResult<MetadataConfig> {
    Ok(MetadataConfig::new().with_sessions_max(1)?)
}

/// The command step `step` of the workload appends to a group in `state`, as the model expects it.
fn expected(state: &MetadataState, step: usize) -> TestResult<MetadataCommand> {
    let index = u64::try_from(step)?;
    Ok(match step % 6 {
        0 => MetadataCommand::OfferCatalog {
            namespace: ns(),
            predecessor: state.current_version(ns()),
            bytes: vec![u8::try_from(step)?; 3],
        },
        1 => {
            let start = state.next_sequence(TypePrefix::RESOURCE);
            MetadataCommand::LeaseBlock {
                prefix: TypePrefix::RESOURCE,
                partition: pid(1)?,
                start,
                end: start + 10 + index,
            }
        }
        2 => MetadataCommand::CreatePrincipal {
            id: u64::try_from(state.principals())? + 1,
            name: format!("p{step}"),
            verifier: verifier("a long enough password", u8::try_from(step)?)?,
            administrator: step == 2,
        },
        // The registry holds one session: the second registration evicts the first, session 3.
        3 => MetadataCommand::RegisterSession {
            session: u128::from(index),
            principal: 1,
            evicts: if state.sessions() >= 1 {
                vec![3]
            } else {
                Vec::new()
            },
        },
        4 => MetadataCommand::LearnClose {
            epoch: Epoch::new(state.last_learned().get() + 1)?,
            versions: state.pending_versions(),
            placement: state.pending_placement(),
            evictions: state.pending_evictions(),
        },
        _ if step == 5 => MetadataCommand::InitPlacement {
            partitions: 2,
            leader: node(),
        },
        _ => MetadataCommand::RecordDefault {
            namespace: ns(),
            predicate: format!("q{step}"),
            entry: vec![1],
        },
    })
}

/// Asks the group for what `command` records, through its own method; `Err` when the group refused, which a fault
/// causes.
fn perform(group: &mut Group, command: &MetadataCommand) -> Result<(), ClusterError> {
    match command {
        MetadataCommand::OfferCatalog {
            namespace,
            predecessor,
            bytes,
        } => group
            .offer_catalog(*namespace, *predecessor, bytes)
            .map(drop),
        MetadataCommand::LeaseBlock {
            prefix,
            partition,
            start,
            end,
        } => group
            .lease_block(*prefix, *partition, end - start)
            .map(drop),
        MetadataCommand::CreatePrincipal {
            name,
            verifier,
            administrator,
            ..
        } => group
            .create_principal(name, verifier.clone(), *administrator)
            .map(drop),
        MetadataCommand::RegisterSession { session, .. } => {
            group.register_session(*session, "p2").map(drop)
        }
        MetadataCommand::LearnClose {
            epoch,
            versions,
            placement,
            evictions,
        } => group
            .learn_close(*epoch, versions, *placement, evictions)
            .map(drop),
        MetadataCommand::InitPlacement { partitions, leader } => {
            group.init_placement(*partitions, *leader).map(drop)
        }
        MetadataCommand::RecordDefault {
            namespace,
            predicate,
            entry,
        } => {
            let entry = DefaultEntry::new(namespace.get(), predicate.clone(), entry.clone());
            group.record_default_entry(&entry).map(drop)
        }
        MetadataCommand::ChangePlacement { .. }
        | MetadataCommand::SetLeader { .. }
        | MetadataCommand::RemovePrincipal { .. }
        | MetadataCommand::ReportUntrimmed { .. }
        | MetadataCommand::Chunk { .. }
        | MetadataCommand::Commit { .. } => Ok(()),
    }
}

/// Step `step` of the workload through the group, and the command the model expects it to append; `Err` when the
/// group refused, which a fault causes.
fn step(
    group: &mut Group,
    model: &Model,
    step: usize,
) -> TestResult<Result<MetadataCommand, ClusterError>> {
    let command = expected(&model.last(), step)?;
    Ok(perform(group, &command).map(|()| command))
}

/// Runs the workload until `appends` commands are appended, syncing after each, the last only when `sync_last`; stops
/// at the first refusal.
fn run(group: &mut Group, model: &mut Model, appends: usize, sync_last: bool) -> TestResult {
    for index in 0..STEPS {
        let command = match step(group, model, index)? {
            Ok(command) => command,
            Err(_) => return Ok(()),
        };
        model.appended(&command)?;
        if model.appends() == appends && !sync_last {
            return Ok(());
        }
        match block_on(group.sync())? {
            Ok(durable) => model.acknowledged = usize::try_from(durable.get())?,
            Err(_) => return Ok(()),
        }
        if model.appends() == appends {
            return Ok(());
        }
    }
    Ok(())
}

/// Checks the recovered group against the model, and that it decides the next command from its state.
fn check(storage: &SimStorage, model: &Model, case: &str) -> TestResult {
    let mut group = crash_and_recover(storage, settings()?, None)?;
    let applied = usize::try_from(group.applied().get())?;
    let expected = model.states.get(applied);
    if applied < model.acknowledged || expected != Some(group.state()) {
        return Err(TestError::new(format!(
            "{case}: recovered at {applied}, appended {}, acknowledged {}",
            model.appends(),
            model.acknowledged
        )));
    }
    let next = group.state().next_sequence(TypePrefix::STATEMENT);
    let (block, position) = group.lease_block(TypePrefix::STATEMENT, pid(2)?, 1)?;
    assert_eq!(block.start().get(), next, "{case}");
    assert_eq!(usize::try_from(position.get())?, applied + 1, "{case}");
    let _durable = block_on(group.sync())??;
    Ok(())
}

#[test]
fn test_the_workload_appends_every_step() -> TestResult {
    let (_storage, mut group) = fresh_with(settings()?)?;
    let mut model = Model::new();
    run(&mut group, &mut model, STEPS, true)?;
    assert_eq!((model.appends(), model.acknowledged), (STEPS, STEPS));
    assert_eq!(Some(group.state()), model.states.last());
    // The registry evicted session 3, and the close after carried the eviction away.
    assert_eq!(group.state().sessions(), 1);
    assert_eq!(group.state().session_principal(9)?, 1);
    assert!(group.state().pending_evictions().is_empty());
    Ok(())
}

#[test]
fn test_a_kill_after_every_append_before_its_sync_loses_only_it() -> TestResult {
    for appends in 1..=STEPS {
        let (storage, mut group) = fresh_with(settings()?)?;
        let mut model = Model::new();
        run(&mut group, &mut model, appends, false)?;
        assert_eq!(model.appends(), appends);
        assert_eq!(model.acknowledged, appends - 1);
        check(
            &storage,
            &model,
            &format!("killed before the sync of append {appends}"),
        )?;
    }
    Ok(())
}

#[test]
fn test_a_kill_after_every_synced_append_keeps_it() -> TestResult {
    for appends in 1..=STEPS {
        let (storage, mut group) = fresh_with(settings()?)?;
        let mut model = Model::new();
        run(&mut group, &mut model, appends, true)?;
        assert_eq!(model.acknowledged, appends);
        check(
            &storage,
            &model,
            &format!("killed after the sync of append {appends}"),
        )?;
    }
    Ok(())
}

#[test]
fn test_a_fault_at_every_storage_operation_then_a_kill_recovers_a_prefix() -> TestResult {
    let (storage, mut group) = fresh_with(settings()?)?;
    let start = storage.trace().len();
    run(&mut group, &mut Model::new(), STEPS, true)?;
    let end = storage.trace().len();
    assert!(end > start + STEPS, "{start}..{end}");
    let modes = [
        FaultMode::Before,
        FaultMode::After,
        FaultMode::TornWrite,
        FaultMode::ShortTornWrite,
    ];
    for operation in start..end {
        for mode in modes {
            let (faulted_storage, mut faulted) = fresh_with(settings()?)?;
            faulted_storage.fail_at(operation, mode);
            let mut model = Model::new();
            run(&mut faulted, &mut model, STEPS, true)?;
            check(
                &faulted_storage,
                &model,
                &format!("{mode:?} at operation {operation}"),
            )?;
        }
    }
    Ok(())
}
