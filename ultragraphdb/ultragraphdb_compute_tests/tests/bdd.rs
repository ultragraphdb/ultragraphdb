/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The compute binary as a process, bound to `features/`, with no container. One scenario runs at a time: the binaries listen
//! at their declared ports.

mod access_steps;
mod batch_steps;
mod cluster;
mod level_steps;
mod lookup_steps;
mod played_frontend;
mod played_partition;
mod read_steps;
mod resume_steps;
mod robust_steps;
mod scene;
mod tap;
mod transaction_steps;

use cucumber::{World, then, when};
use service_utils::{ServiceUtil, WaitStrategy};
use std::path::Path;
use std::process::{Command, ExitStatus};
use std::sync::{Arc, OnceLock};
use std::time::Duration;
use ultragraphdb_common_role::RoleClient;
use ultragraphdb_protocol_transport::Address;
use ultragraphdb_protocol_wire::{Scan, ScanAt, ScanRange};
use ultragraphdb_tests::{
    CLUSTER, COMPUTE_BINARY, COMPUTE_OBSERVABILITY, COMPUTE_ROLE_ADDRESS, DOUBLE_ADDRESS,
    FRONTEND_BINARY, FRONTEND_OBSERVABILITY, SECRET, START_SECONDS, STORAGE_BINARY,
    STORAGE_OBSERVABILITY, STORAGE_ROLE_ADDRESS, answering, bootstrap, compute_on_one_host,
    exit_is, frontend_holding, holding, http_status, process, render_settings, role, scratch,
    served_by_double, start_double, storage_holding, until_answering, with,
};
use ultragraphdb_tool_bdd::{Context, TestError, TestResult};

use crate::cluster::{ClusterInfo, start_cluster};
use crate::played_frontend::{EpochClock, Played, play};
use crate::played_partition::{PlayedTables, play_partition};
use crate::scene::{ReadAnswer, Scene};
use crate::tap::{ScanSeen, TapLog};

/// The partitions a test plays.
const PARTITIONS_PLAYED: u32 = 3;

/// The `scan_stall_ms` the compute process over the played partitions keeps: short, so a partition that hung fails its read soon.
pub const PLAYED_SCAN_STALL_MS: &str = "500";

/// Where the sandbox holds the binaries, relative to the test's working directory: `copy_file` writes them to this package.
const ROOT_PATH: &str = "ultragraphdb/ultragraphdb_compute_tests";

fn services() -> TestResult<ServiceUtil> {
    Ok(ServiceUtil::new(
        ROOT_PATH,
        vec![FRONTEND_BINARY, COMPUTE_BINARY, STORAGE_BINARY],
    )?)
}

/// The cluster of four partitions most scenarios read, a tenant of its own each.
static FOUR: OnceLock<ClusterInfo> = OnceLock::new();
/// The cluster of one partition, which owns every tenant's whole run.
static SOLO: OnceLock<ClusterInfo> = OnceLock::new();
/// The cluster retaining what it reads for a moment and stalling a Scan soon.
static BRIEF: OnceLock<ClusterInfo> = OnceLock::new();
/// The cluster whose partitions the test plays.
static PLAYED: OnceLock<ClusterInfo> = OnceLock::new();
/// The tables of the played partitions.
static TABLES: OnceLock<Vec<Arc<PlayedTables>>> = OnceLock::new();

fn held(cluster: &'static OnceLock<ClusterInfo>) -> &'static ClusterInfo {
    match cluster.get() {
        Some(info) => info,
        None => std::process::abort(),
    }
}

/// The cluster of four partitions.
fn four() -> &'static ClusterInfo {
    held(&FOUR)
}

/// The cluster of one partition.
fn solo() -> &'static ClusterInfo {
    held(&SOLO)
}

/// The cluster of a short retention and a short stall.
fn brief() -> &'static ClusterInfo {
    held(&BRIEF)
}

/// The cluster of played partitions.
fn played() -> &'static ClusterInfo {
    held(&PLAYED)
}

/// The tables of played partition `partition`.
fn played_tables(partition: u32) -> TestResult<&'static PlayedTables> {
    TABLES
        .get()
        .and_then(|tables| tables.get(partition as usize - 1))
        .map(Arc::as_ref)
        .context("the played partition")
}

#[derive(Debug, Default, World)]
struct Steps {
    exited: Option<ExitStatus>,
    graph: String,
    log: String,
    roles: Option<ServiceUtil>,
    scene: Option<Scene>,
    answers: Vec<ReadAnswer>,
    scans: Vec<ScanSeen>,
    connections: Vec<Vec<usize>>,
    iris: Vec<String>,
    texts: Vec<String>,
    keys: Vec<Vec<u8>>,
    graph_keys: Vec<Vec<u8>>,
    numbers: Vec<u64>,
    hash: u64,
    kind: Option<u64>,
    outcome: Option<bool>,
}

impl Steps {
    /// The scenario's scene.
    fn scene(&self) -> TestResult<&Scene> {
        self.scene.as_ref().context("the scenario's scene")
    }

    /// The read a scenario made first.
    fn answer(&self) -> TestResult<&ReadAnswer> {
        self.answers.first().context("an answer")
    }
}

#[when("a role is running and receives SIGTERM")]
fn terminate(w: &mut Steps) -> TestResult {
    let util = services()?;
    let log = scratch("compute-stopped.log");
    let wait = answering(COMPUTE_OBSERVABILITY, "/health");
    let settings = holding(compute_on_one_host(), SECRET)?;
    let pid = util.start_service_from_config(role(COMPUTE_BINARY, &settings, &log, wait))?;
    w.exited = Some(util.stop_service(pid)?);
    w.log = std::fs::read_to_string(&log)?;
    Ok(())
}

#[then("it exits with code 0")]
fn exits_clean(w: &mut Steps) -> TestResult {
    exit_is(w.exited, &w.log, 0)
}

#[then("it names neither `ultragraphdb_store_kv` nor fjall")]
fn no_engine(w: &mut Steps) -> TestResult {
    if !w.graph.contains("ultragraphdb_common_service") {
        return Err(TestError::new(format!(
            "the graph is not the binary's: {}",
            w.graph
        )));
    }
    for engine in ["ultragraphdb_store_kv", "fjall"] {
        assert!(!w.graph.contains(engine), "{engine} is linked");
    }
    Ok(())
}

#[when("the compute binary receives settings with no `compute.role_address`")]
fn no_compute_address(w: &mut Steps) -> TestResult {
    let mut settings = holding(compute_on_one_host(), SECRET)?;
    settings.retain(|(name, _)| name != "compute.role_address");
    let log = scratch("compute-no-address.log");
    let util = services()?;
    let instance = served_by_double(COMPUTE_BINARY, &render_settings(&settings));
    let pid = util.start_service_from_config(process(
        COMPUTE_BINARY,
        bootstrap(DOUBLE_ADDRESS, &instance),
        &log,
        WaitStrategy::NoWait,
    ))?;
    w.exited = Some(util.wait_for_exit(pid, Duration::from_secs(START_SECONDS))?);
    w.log = std::fs::read_to_string(&log)?;
    Ok(())
}

#[then("it exits with code 78 and an error naming `compute.role_address`")]
fn names_compute_address(w: &mut Steps) -> TestResult {
    exit_is(w.exited, &w.log, 78)?;
    assert!(w.log.contains("compute.role_address"), "{}", w.log);
    Ok(())
}

#[when("the compute binary's dependency graph is listed")]
fn compute_graph(w: &mut Steps) -> TestResult {
    w.graph = std::fs::read_to_string(Path::new(ROOT_PATH).join("compute_deps"))?;
    Ok(())
}

/// Starts compute as a process joining a frontend, served by the double.
fn compute_joining(util: &ServiceUtil) -> TestResult {
    let _frontend: u32 = util.start_service_from_config(role(
        FRONTEND_BINARY,
        &frontend_holding()?,
        &scratch(FRONTEND_BINARY),
        answering(FRONTEND_OBSERVABILITY, "/health"),
    ))?;
    let _compute: u32 = util.start_service_from_config(role(
        COMPUTE_BINARY,
        &holding(compute_on_one_host(), SECRET)?,
        &scratch(COMPUTE_BINARY),
        answering(COMPUTE_OBSERVABILITY, "/health"),
    ))?;
    Ok(())
}

#[when("compute has joined a frontend no partition has joined")]
fn compute_alone(w: &mut Steps) -> TestResult {
    let util = services()?;
    compute_joining(&util)?;
    w.roles = Some(util);
    Ok(())
}

#[then("its `/health` answers, its `/ready` does not, and it turns ready once a partition joins")]
fn ready_after_a_partition(w: &mut Steps) -> TestResult {
    assert_eq!(http_status(COMPUTE_OBSERVABILITY, "/health")?, 200);
    assert_eq!(http_status(COMPUTE_OBSERVABILITY, "/ready")?, 503);
    let util = w.roles.as_ref().context("the roles")?;
    let _storage: u32 = util.start_service_from_config(role(
        STORAGE_BINARY,
        &storage_holding()?,
        &scratch(STORAGE_BINARY),
        answering(STORAGE_OBSERVABILITY, "/ready"),
    ))?;
    until_answering(COMPUTE_OBSERVABILITY, "/ready")?;
    w.roles = None;
    Ok(())
}

#[when("a role asks compute for a command it does not host")]
fn asks_compute(w: &mut Steps) -> TestResult {
    let util = services()?;
    compute_joining(&util)?;
    w.log = crate::scene::run(async {
        let target = Address::parse(&format!("tcp://{COMPUTE_ROLE_ADDRESS}"))?;
        let mut client = RoleClient::dial(&target, CLUSTER, "frontend", SECRET).await?;
        let mut scan = Scan::new(ScanAt::Latest, 0, 0, 1, [0; 16], 0, 0);
        let first = [0x1B_u8; 9];
        let past = [first.as_slice(), &[0]].concat();
        scan.push_range(ScanRange::new(1, &first, &past));
        let refused = client.request(&scan).await.err();
        let refused = refused.context("a refusal")?;
        Ok::<String, TestError>(refused.to_string())
    })?;
    w.roles = Some(util);
    Ok(())
}

#[then("the connection authenticates and the command is refused as not served")]
fn not_served(w: &mut Steps) {
    assert!(w.log.contains("524"), "{}", w.log);
    w.roles = None;
}

#[when("a role asks compute with a different role secret")]
fn wrong_secret_at_compute(w: &mut Steps) -> TestResult {
    let util = services()?;
    compute_joining(&util)?;
    w.log = crate::scene::run(async {
        let target = Address::parse(&format!("tcp://{COMPUTE_ROLE_ADDRESS}"))?;
        let refused = RoleClient::dial(&target, CLUSTER, "frontend", "another secret")
            .await
            .err()
            .context("a refusal")?;
        Ok::<String, TestError>(refused.to_string())
    })?;
    w.roles = Some(util);
    Ok(())
}

#[then("its authentication is refused")]
fn refused_at_compute(w: &mut Steps) {
    assert!(w.log.contains("authentication failed"), "{}", w.log);
    w.roles = None;
}

/// Sends `signal` to `pid`, as an operator would.
fn signal(pid: u32, signal: &str) -> TestResult {
    let sent = Command::new("kill")
        .arg(format!("-{signal}"))
        .arg(pid.to_string())
        .output()?;
    assert!(sent.status.success(), "{sent:?}");
    Ok(())
}

/// Starts the three roles with compute's deadline short, sends storage `to_storage`, and holds how compute exits; the storage
/// process is returned to its caller, for what it still needs.
fn lose_storage(w: &mut Steps, to_storage: &str) -> TestResult<u32> {
    let util = services()?;
    let _frontend: u32 = util.start_service_from_config(role(
        FRONTEND_BINARY,
        &frontend_holding()?,
        &scratch(FRONTEND_BINARY),
        answering(FRONTEND_OBSERVABILITY, "/health"),
    ))?;
    let storage = util.start_service_from_config(role(
        STORAGE_BINARY,
        &storage_holding()?,
        &scratch(STORAGE_BINARY),
        answering(STORAGE_OBSERVABILITY, "/ready"),
    ))?;
    let log = scratch("compute-lost-storage.log");
    let compute = util.start_service_from_config(role(
        COMPUTE_BINARY,
        &with(
            holding(compute_on_one_host(), SECRET)?,
            "cluster.join_deadline_ms",
            "800",
        ),
        &log,
        answering(COMPUTE_OBSERVABILITY, "/ready"),
    ))?;
    signal(storage, to_storage)?;
    w.exited = Some(util.wait_for_exit(compute, Duration::from_secs(START_SECONDS))?);
    w.log = std::fs::read_to_string(&log)?;
    w.roles = Some(util);
    Ok(storage)
}

#[when("compute loses its storage process, which does not return within compute's deadline")]
fn loses_storage(w: &mut Steps) -> TestResult {
    let _gone = lose_storage(w, "KILL")?;
    Ok(())
}

/// A stopped process closes nothing and its kernel still accepts, as a hung one does: only a question with a limit finds it.
#[when(
    "compute's storage process stops answering with its connections open, and does not return within compute's deadline"
)]
fn storage_hangs(w: &mut Steps) -> TestResult {
    let stopped = lose_storage(w, "STOP")?;
    signal(stopped, "KILL")
}

#[then("compute shuts down through its ordinary path and exits with code 69")]
fn lost_storage(w: &mut Steps) -> TestResult {
    exit_is(w.exited, &w.log, 69)?;
    assert!(w.log.contains(STORAGE_ROLE_ADDRESS), "{}", w.log);
    w.roles = None;
    Ok(())
}

/// A compute process over `PARTITIONS_PLAYED` partitions the test plays.
fn start_played_cluster(util: &ServiceUtil, base: u32) -> TestResult<(ClusterInfo, u32)> {
    let clock = EpochClock::new();
    let mut tables = Vec::new();
    let mut listed = Vec::new();
    for partition in 1..=PARTITIONS_PLAYED {
        let port = base + 10 + partition - 1;
        tables.push(play_partition(port)?);
        listed.push((partition, format!("tcp://127.0.0.1:{port}")));
    }
    drop(TABLES.set(tables));
    play(
        base + 1,
        Played {
            partitions: PARTITIONS_PLAYED,
            clock: Arc::clone(&clock),
            listed,
        },
    )?;
    let compute = crate::cluster::start_compute(
        util,
        base,
        &[("storage.scan_stall_ms", PLAYED_SCAN_STALL_MS)],
    )?;
    let info = ClusterInfo {
        partitions: PARTITIONS_PLAYED,
        base,
        clock,
        log: Arc::new(TapLog::default()),
    };
    Ok((info, compute))
}

/// Starts the clusters every read scenario shares: their processes' pids.
fn start_clusters(util: &ServiceUtil) -> TestResult<Vec<u32>> {
    let mut pids = Vec::new();
    let (of_four, started_four) = start_cluster(util, 17_400, 4, &[])?;
    drop(FOUR.set(of_four));
    pids.extend(started_four);
    let (of_one, started_one) = start_cluster(util, 17_500, 1, &[])?;
    drop(SOLO.set(of_one));
    pids.extend(started_one);
    let (of_brief, started_brief) = start_cluster(
        util,
        17_600,
        1,
        &[
            ("storage.read_retain_ms", "50"),
            ("storage.scan_stall_ms", "300"),
        ],
    )?;
    drop(BRIEF.set(of_brief));
    pids.extend(started_brief);
    let (of_played, compute) = start_played_cluster(util, 17_700)?;
    drop(PLAYED.set(of_played));
    pids.push(compute);
    Ok(pids)
}

fn main() -> TestResult {
    start_double()?;
    let util = services()?;
    let pids = start_clusters(&util)?;
    let summary = futures::executor::block_on(
        Steps::cucumber()
            .max_concurrent_scenarios(1)
            .fail_on_skipped()
            .run(ultragraphdb_tool_bdd::features_dir()?),
    );
    for pid in pids {
        drop(util.stop_service(pid));
    }
    if cucumber::writer::Stats::execution_has_failed(&summary)
        || cucumber::writer::Stats::hook_errors(&summary) > 0
    {
        std::process::exit(1);
    }
    Ok(())
}
