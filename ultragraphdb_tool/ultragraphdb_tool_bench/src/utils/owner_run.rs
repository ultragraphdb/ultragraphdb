/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The frontend's owner check timed as an endpoint core meets it (task 12.5b): the production owner started on its own
//! thread over its logs in the run's directory, closing epochs within the bounds given and syncing each close, the
//! bootstrap administrator created from a password file and one session of it registered; then, from this thread's
//! runtime, one check after another, each the commit's `begin_commit` from its ask to its answer, its `commit_ended`
//! sent after it as a commit's end sends it. The time is the hop to the owner's core and back and any wait for the
//! owner's turn, its sync of a close among them; the check itself is a lookup in the registry.

use std::time::Duration;

use ultragraphdb_cluster_metadata::MetadataConfig;
use ultragraphdb_cluster_sequencer::SequencerConfig;
use ultragraphdb_common_frontend::{OwnerStart, start_owner};
use ultragraphdb_common_service_config::ServiceConfig;
use ultragraphdb_protocol_wire::ErrorReply;

use crate::errors::bench_error::BenchError;
use crate::traits::clock::Clock;
use crate::types::owner_args::OwnerArgs;
use crate::types::summary::Summary;
use crate::types::wall_clock::WallClock;

/// The cluster the owner's logs name.
const CLUSTER: u64 = 1;
/// The bootstrap administrator's principal, the first the group creates.
const ADMIN: u64 = 1;
/// The session the checks name.
const SESSION: u128 = 1;

/// Runs the owner-check measurement of `args` and returns its report, one JSON line.
///
/// # Errors
///
/// [`BenchError::usage`] when the run's directory exists or the epoch bounds are refused; [`BenchError::io`] when the
/// password file cannot be written; [`BenchError::commit`] when the owner does not start, or refuses the registration
/// or a check.
pub fn run_owner(args: &OwnerArgs) -> Result<String, BenchError> {
    if args.dir().exists() {
        return Err(BenchError::usage(format!(
            "{} exists; every run takes a fresh directory",
            args.dir().display()
        )));
    }
    std::fs::create_dir_all(args.dir())?;
    let password = args.dir().join("bootstrap.secret");
    std::fs::write(&password, "the bench's administrator")?;
    let (least, most) = args.epoch_ms();
    let sequencer =
        SequencerConfig::new(least, most).map_err(|e| BenchError::usage(e.to_string()))?;
    let start = OwnerStart::new(MetadataConfig::new(), sequencer, 1)
        .with_bootstrap_file(password.display().to_string());
    let runtime = compio::runtime::Runtime::new()?;
    let checks_us = runtime.block_on(async {
        let owner = start_owner(
            ServiceConfig::default(),
            CLUSTER,
            args.dir().join("data"),
            start,
            Vec::new(),
        )
        .await
        .map_err(failed)?;
        let handle = owner.handle().clone();
        handle
            .register_session(SESSION, ADMIN)
            .await
            .map_err(|refusal| refused(&refusal))?;
        let clock = WallClock::new();
        // At most `checks` times, each pushed once.
        let mut checks_us = Vec::with_capacity(usize::try_from(args.checks()).unwrap_or(0));
        for _ in 0..args.checks() {
            let asked = clock.elapsed();
            handle
                .begin_commit(SESSION, ADMIN)
                .await
                .map_err(|refusal| refused(&refusal))?;
            checks_us.push(micros(clock.elapsed().saturating_sub(asked)));
            handle.commit_ended(SESSION);
        }
        drop(handle);
        owner.stopped().await.map_err(failed)?;
        Ok::<_, BenchError>(checks_us)
    })?;
    Ok(report(args, checks_us))
}

/// `elapsed` in whole microseconds.
fn micros(elapsed: Duration) -> u64 {
    u64::try_from(elapsed.as_micros()).unwrap_or(u64::MAX)
}

/// The owner's failure as the run's.
fn failed(error: impl std::fmt::Display) -> BenchError {
    BenchError::commit(error.to_string())
}

/// The owner's refusal as the run's.
fn refused(refusal: &ErrorReply) -> BenchError {
    BenchError::commit(format!("{:?}: {}", refusal.code(), refusal.message()))
}

/// The report of a run: the checks and their times.
fn report(args: &OwnerArgs, mut checks_us: Vec<u64>) -> String {
    let summary = Summary::of(&mut checks_us);
    let (least, most) = args.epoch_ms();
    format!(
        concat!(
            "{{\"run\":\"owner_check\",\"epoch_ms\":[{},{}],\"checks\":{},",
            "\"check_us\":{{\"p50\":{},\"p99\":{},\"max\":{},\"mean\":{:.1}}}}}"
        ),
        least,
        most,
        summary.count(),
        summary.p50(),
        summary.p99(),
        summary.max(),
        summary.mean(),
    )
}
