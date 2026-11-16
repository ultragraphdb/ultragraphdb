/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! Compute's part of system-group, "A lost system group is waited for until the deadline", over the built binaries: a storage
//! process hosting the system group by its `[system]` table, and compute dialling it at `cluster.system_address` (design D2).

use std::time::Duration;

use cucumber::{then, when};
use ultragraphdb_tests::{
    COMPUTE_BINARY, COMPUTE_OBSERVABILITY, SECRET, START_SECONDS, STORAGE_BINARY,
    STORAGE_OBSERVABILITY, answering, compute_on_one_host, exit_is, fresh_data, holding, role,
    scratch, storage_holding, with,
};
use ultragraphdb_tool_bdd::TestResult;

use crate::{Steps, services, signal};

/// Where the hosting storage process's system group listens: an address of its own beside its partitions'.
const SYSTEM_ROLE_ADDRESS: &str = "127.0.0.1:17084";

/// `settings` dialling the system group at `system` in place of the frontend.
fn dialling(mut settings: Vec<(String, String)>, system: &str) -> Vec<(String, String)> {
    settings.retain(|(name, _)| name != "cluster.frontend_address");
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
    let system = format!("tcp://{SYSTEM_ROLE_ADDRESS}");
    let storage = util.start_service_from_config(role(
        STORAGE_BINARY,
        &hosting(&system)?,
        &scratch("storage-hosting"),
        answering(STORAGE_OBSERVABILITY, "/ready"),
    ))?;
    let log = scratch("compute-lost-group.log");
    let compute = util.start_service_from_config(role(
        COMPUTE_BINARY,
        &with(
            dialling(holding(compute_on_one_host(), SECRET)?, &system),
            "cluster.join_deadline_ms",
            "800",
        ),
        &log,
        answering(COMPUTE_OBSERVABILITY, "/ready"),
    ))?;
    signal(storage, "KILL")?;
    w.exited = Some(util.wait_for_exit(compute, Duration::from_secs(START_SECONDS))?);
    w.log = std::fs::read_to_string(&log)?;
    w.roles = Some(util);
    Ok(())
}

#[then(
    "compute shuts down through its ordinary path and exits with code 69 naming the system group's address"
)]
fn ended_naming_the_group(w: &mut Steps) -> TestResult {
    exit_is(w.exited, &w.log, 69)?;
    assert!(w.log.contains(SYSTEM_ROLE_ADDRESS), "{}", w.log);
    w.roles = None;
    Ok(())
}
