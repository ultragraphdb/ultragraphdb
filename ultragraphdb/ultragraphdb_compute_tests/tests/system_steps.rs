/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! Compute's part of system-group, "A lost system group is waited for until the deadline", over the built binaries: a storage
//! process hosting the system group by its `[system]` table, and compute dialling it at `cluster.system_address` (design D2).

use std::time::Duration;

use cucumber::{then, when};
use ultragraphdb_protocol_wire::RoleKind;
use ultragraphdb_tests::{
    COMPUTE_BINARY, SECRET, START_SECONDS, STORAGE_BINARY, answering, compute_observability,
    compute_on_one_host, exit_is, fresh_data, holding, members_at, role, scratch, storage_holding,
    storage_observability, with,
};
use ultragraphdb_tool_bdd::{TestError, TestResult, loopback};

use crate::{Steps, services, signal};

/// Where the hosting storage process's system group listens, as an offset above the target's port base: an address of its
/// own beside its partitions'.
const SYSTEM_ROLE_OFFSET: u16 = 4;

/// The deadline of the compute process whose group restarts: compute asks the group whether it lists compute every half of
/// it, a minute here, so only its connection's close lists it again within the [`START_SECONDS`] the scenario waits.
const LONG_DEADLINE_MS: u64 = 120_000;

/// `settings` dialling the system group at `system`.
fn dialling(settings: Vec<(String, String)>, system: &str) -> Vec<(String, String)> {
    with(settings, "cluster.system_address", system)
}

/// A storage process of one partition hosting the system group at `system`, its logs in a directory of their own.
fn hosting(system: &str) -> TestResult<Vec<(String, String)>> {
    let data = fresh_data();
    let tables = [
        ("cluster.partition_count", "1"),
        ("system.role_address", system),
        ("system.data_dir", data.as_str()),
        ("sequencer.epoch_ms_min", "1"),
        ("sequencer.epoch_ms_max", "20"),
    ];
    Ok(tables.into_iter().fold(
        dialling(storage_holding()?, system),
        |settings, (name, value)| with(settings, name, value),
    ))
}

#[when("the hosting storage process is killed and does not return within `join_deadline_ms`")]
fn hosting_storage_killed(w: &mut Steps) -> TestResult {
    let util = services()?;
    let system = format!("tcp://{}", loopback(SYSTEM_ROLE_OFFSET)?);
    let storage = util.start_service_from_config(role(
        STORAGE_BINARY,
        &hosting(&system)?,
        &scratch("storage-hosting"),
        answering(&storage_observability()?, "/ready"),
    )?)?;
    let log = scratch("compute-lost-group.log");
    let compute = util.start_service_from_config(role(
        COMPUTE_BINARY,
        &with(
            dialling(holding(compute_on_one_host()?, SECRET)?, &system),
            "cluster.join_deadline_ms",
            "800",
        ),
        &log,
        answering(&compute_observability()?, "/ready"),
    )?)?;
    signal(storage, "KILL")?;
    w.exited = Some(util.wait_for_exit(compute, Duration::from_secs(START_SECONDS))?);
    w.log = std::fs::read_to_string(&log)?;
    w.roles = Some(util);
    Ok(())
}

/// Whether the system group at `system` lists a compute process within [`START_SECONDS`].
fn compute_listed(system: &str) -> TestResult<bool> {
    for _ in 0..START_SECONDS * 20 {
        if members_at(system)?
            .iter()
            .any(|(kind, _, _)| *kind == RoleKind::Compute)
        {
            return Ok(true);
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    Ok(false)
}

#[when("the hosting storage process is killed and started again within `join_deadline_ms`")]
fn hosting_storage_restarted(w: &mut Steps) -> TestResult {
    let util = services()?;
    let system = format!("tcp://{}", loopback(SYSTEM_ROLE_OFFSET)?);
    let settings = hosting(&system)?;
    let storage_log = scratch("storage-restarted");
    let storage_ready = storage_observability()?;
    let hosting_storage = || {
        role(
            STORAGE_BINARY,
            &settings,
            &storage_log,
            answering(&storage_ready, "/ready"),
        )
    };
    let storage = util.start_service_from_config(hosting_storage()?)?;
    let log = scratch("compute-restarted-group.log");
    let _compute: u32 = util.start_service_from_config(role(
        COMPUTE_BINARY,
        &with(
            dialling(holding(compute_on_one_host()?, SECRET)?, &system),
            "cluster.join_deadline_ms",
            &LONG_DEADLINE_MS.to_string(),
        ),
        &log,
        answering(&compute_observability()?, "/ready"),
    )?)?;
    assert!(compute_listed(&system)?, "compute joined at its start");
    signal(storage, "KILL")?;
    let _killed = util.wait_for_exit(storage, Duration::from_secs(START_SECONDS))?;
    let _again: u32 = util.start_service_from_config(hosting_storage()?)?;
    w.outcome = Some(compute_listed(&system)?);
    w.log = std::fs::read_to_string(&log)?;
    w.roles = Some(util);
    Ok(())
}

#[then(
    "compute dials the group as its connection closes and joins it again on the connection the group answers, so the group's Members lists compute again as soon as the group is back"
)]
fn joined_again_at_once(w: &mut Steps) -> TestResult {
    let (waited_ms, next_ask_ms) = (START_SECONDS * 1_000, LONG_DEADLINE_MS / 2);
    if waited_ms >= next_ask_ms {
        return Err(TestError::new(format!(
            "the step waits {waited_ms} ms, past compute's next ask at {next_ask_ms} ms"
        )));
    }
    assert_eq!(w.outcome, Some(true), "{}", w.log);
    w.roles = None;
    Ok(())
}

#[then(
    "compute shuts down through its ordinary path and exits with code 69 naming the system group's address"
)]
fn ended_naming_the_group(w: &mut Steps) -> TestResult {
    exit_is(w.exited, &w.log, 69)?;
    assert!(w.log.contains(&loopback(SYSTEM_ROLE_OFFSET)?), "{}", w.log);
    // The ordinary path is the one SIGTERM takes, which logs the role's stop before it drains its reads.
    assert!(w.log.contains("stopping role=compute"), "{}", w.log);
    w.roles = None;
    Ok(())
}
