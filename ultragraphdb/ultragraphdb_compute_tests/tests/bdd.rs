/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The compute binary as a process, bound to `features/`, with no container: its client endpoint over storage processes and
//! the system group one hosts; and the read features' reads, sent to its client endpoint over real or played storage
//! partitions and a played system group. One scenario runs at a time: the binaries listen at their declared ports.

mod access_steps;
mod books_steps;
mod client;
mod cluster;
mod endpoint_read_steps;
mod endpoint_steps;
mod level_steps;
mod lookup_steps;
mod played_partition;
mod played_system;
mod processes;
mod read_steps;
mod resume_steps;
mod reverse_version_steps;
mod robust_steps;
mod scene;
mod system_steps;
mod tap;
mod transaction_steps;

use compio::runtime::Runtime;
use cucumber::{World, then, when};
use futures::FutureExt;
use service_utils::{ServiceUtil, WaitStrategy};
use std::path::Path;
use std::process::ExitStatus;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;
use ultragraphdb_client_compio::{Client, ClientConfig, TcpConnector};
use ultragraphdb_common_service::BIND_RETRIES;
use ultragraphdb_protocol_scram::{ScramLogin, system_nonce};
use ultragraphdb_tests::{
    CLUSTER, COMPUTE_BINARY, PARENT_PID, SECRET, START_SECONDS, STORAGE_BINARY, answering,
    bootstrap, client_address, compute_holding, compute_observability, double_address, exit_is,
    http_status, process, render_settings, role, scratch, served_by_double, start_double,
    storage_holding, storage_observability, until_answering, until_ports_free, with,
};
use ultragraphdb_tool_bdd::{Context, TestError, TestResult};

use crate::cluster::{ClusterInfo, cluster_base, start_cluster};
use crate::played_partition::{PlayedTables, play_partition};
use crate::played_system::{EpochClock, Played, play};
use crate::scene::{ReadAnswer, Scene};
use crate::tap::{ScanSeen, TapLog};

/// The partitions a test plays.
const PARTITIONS_PLAYED: u32 = 3;

/// The `scan_stall_ms` the compute process over the played partitions keeps: short, so a partition that hung fails its read soon.
pub const PLAYED_SCAN_STALL_MS: &str = "500";

/// Where the sandbox holds the binaries, relative to the test's working directory: `copy_file` writes them to this package.
const ROOT_PATH: &str = "ultragraphdb/ultragraphdb_compute_tests";

/// An address in TEST-NET-3, which no host's interface holds (RFC 5737).
const NO_INTERFACE: &str = "203.0.113.1:18099";

fn services() -> TestResult<ServiceUtil> {
    Ok(ServiceUtil::new(
        ROOT_PATH,
        vec![COMPUTE_BINARY, STORAGE_BINARY],
    )?)
}

/// The processes of the clusters below, which live for the whole test process and `main` stops at its end.
static SHARED: OnceLock<ServiceUtil> = OnceLock::new();
/// The pids of the clusters started so far. A `Mutex`, as a static is shared: the scenarios run one at a time, so it is
/// held for one push at a time.
static STARTED: Mutex<Vec<u32>> = Mutex::new(Vec::new());
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

/// The cluster `cluster` holds, `start` starting it on its first use, so a target whose scenarios read no shared cluster
/// starts none.
fn shared(
    cluster: &'static OnceLock<ClusterInfo>,
    start: impl FnOnce(&ServiceUtil) -> TestResult<(ClusterInfo, Vec<u32>)>,
) -> TestResult<&'static ClusterInfo> {
    if let Some(info) = cluster.get() {
        return Ok(info);
    }
    let (info, pids) = start(SHARED.get().context("the shared clusters' processes")?)?;
    STARTED
        .lock()
        .map_err(|_| TestError::new("the shared clusters' pids"))?
        .extend(pids);
    Ok(cluster.get_or_init(|| info))
}

/// The cluster of four partitions.
fn four() -> TestResult<&'static ClusterInfo> {
    shared(&FOUR, |util| start_cluster(util, cluster_base(0)?, 4, &[]))
}

/// The cluster of one partition.
fn solo() -> TestResult<&'static ClusterInfo> {
    shared(&SOLO, |util| start_cluster(util, cluster_base(1)?, 1, &[]))
}

/// The cluster of a short retention and a short stall.
fn brief() -> TestResult<&'static ClusterInfo> {
    shared(&BRIEF, |util| {
        start_cluster(
            util,
            cluster_base(2)?,
            1,
            &[
                ("storage.read_retain_ms", "50"),
                ("storage.scan_stall_ms", "300"),
            ],
        )
    })
}

/// The cluster of played partitions.
fn played() -> TestResult<&'static ClusterInfo> {
    shared(&PLAYED, |util| start_played_cluster(util, cluster_base(3)?))
}

/// The tables of played partition `partition`.
fn played_tables(partition: u32) -> TestResult<&'static PlayedTables> {
    let _started = played()?;
    TABLES
        .get()
        .and_then(|tables| tables.get(partition as usize - 1))
        .map(Arc::as_ref)
        .context("the played partition")
}

/// The addresses a role of these scenarios holds beyond those every suite declares: the second storage process's.
fn own_addresses() -> TestResult<[(String, &'static str); 2]> {
    Ok([
        (
            processes::second_role_address()?,
            "the second storage process's partition",
        ),
        (
            processes::second_observability()?,
            "the second storage process's observability",
        ),
    ])
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
    iris: Vec<String>,
    texts: Vec<String>,
    keys: Vec<Vec<u8>>,
    numbers: Vec<u64>,
    kind: Option<u64>,
    refused: Option<String>,
    answered: endpoint_read_steps::Answered,
    ids: Vec<Vec<u8>>,
    status: u16,
    /// The processes of a scenario over the system group, which it starts, kills and starts again.
    processes: processes::Roles,
    /// What a scenario's When saw, for its Then to judge.
    seen: Vec<String>,
    /// The roles a lost system group ended, each by its binary's name: how it exited, and its log.
    dependants: Vec<(&'static str, Option<ExitStatus>, String)>,
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
    let wait = answering(&compute_observability()?, "/health");
    let settings = compute_holding()?;
    let pid = util.start_service_from_config(role(COMPUTE_BINARY, &settings, &log, wait)?)?;
    w.exited = Some(util.stop_service(pid)?);
    w.log = std::fs::read_to_string(&log)?;
    Ok(())
}

#[then("it exits with code 0")]
fn exits_clean(w: &mut Steps) -> TestResult {
    exit_is(w.exited, &w.log, 0)
}

#[when("the compute binary's dependency graph is listed")]
fn compute_graph(w: &mut Steps) -> TestResult {
    w.graph = std::fs::read_to_string(Path::new(ROOT_PATH).join("compute_deps"))?;
    Ok(())
}

/// Compute alone: no storage process, so nothing hosts the system group, and the address compute dials it at is free first.
#[when("compute has started while no process listens at `cluster.system_address`")]
fn compute_alone(w: &mut Steps) -> TestResult {
    until_ports_free(&[])?;
    let util = services()?;
    let _compute: u32 = util.start_service_from_config(role(
        COMPUTE_BINARY,
        &compute_holding()?,
        &scratch(COMPUTE_BINARY),
        answering(&compute_observability()?, "/health"),
    )?)?;
    w.roles = Some(util);
    Ok(())
}

#[then("its `/health` answers, its `/ready` does not, and it turns ready once a partition joins")]
fn ready_after_a_partition(w: &mut Steps) -> TestResult {
    let observed = compute_observability()?;
    assert_eq!(http_status(&observed, "/health")?, 200);
    assert_eq!(http_status(&observed, "/ready")?, 503);
    let util = w.roles.as_ref().context("the roles")?;
    let _storage: u32 = util.start_service_from_config(role(
        STORAGE_BINARY,
        &storage_holding()?,
        &scratch(STORAGE_BINARY),
        answering(&storage_observability()?, "/ready"),
    )?)?;
    until_answering(&observed, "/ready")?;
    w.roles = None;
    Ok(())
}

/// Starts compute over two storage processes, the first hosting the system group and partition 1 and the second partition 2,
/// with compute's deadline short; sends the second `to_storage`, and holds how compute exits. A hung or killed
/// process hosting the group too would be the group's loss, which compute names by the group's address (system-group, "A
/// lost system group is waited for until the deadline"). The lost storage process is returned to its caller, for what it
/// still needs.
fn lose_storage(w: &mut Steps, to_storage: &str) -> TestResult<u32> {
    let util = services()?;
    let _hosting: u32 = util.start_service_from_config(role(
        STORAGE_BINARY,
        &with(storage_holding()?, "cluster.partition_count", "2"),
        &scratch(STORAGE_BINARY),
        answering(&storage_observability()?, "/ready"),
    )?)?;
    let storage = util.start_service_from_config(role(
        STORAGE_BINARY,
        &processes::second_of()?,
        &scratch("storage-partition-2"),
        answering(&processes::second_observability()?, "/ready"),
    )?)?;
    let log = scratch("compute-lost-storage.log");
    let compute = util.start_service_from_config(role(
        COMPUTE_BINARY,
        &with(compute_holding()?, "cluster.join_deadline_ms", "800"),
        &log,
        answering(&compute_observability()?, "/ready"),
    )?)?;
    processes::signal(storage, to_storage)?;
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
    processes::signal(stopped, "KILL")
}

#[then("compute shuts down through its ordinary path and exits with code 69")]
fn lost_storage(w: &mut Steps) -> TestResult {
    exit_is(w.exited, &w.log, 69)?;
    assert!(
        w.log.contains(&processes::second_role_address()?),
        "{}",
        w.log
    );
    // The ordinary path is the one SIGTERM takes, which logs the role's stop before it drains its reads.
    assert!(w.log.contains("stopping role=compute"), "{}", w.log);
    w.roles = None;
    Ok(())
}

/// A compute process over `PARTITIONS_PLAYED` partitions the test plays, dialling their played group: its pid.
fn start_played_cluster(util: &ServiceUtil, base: u32) -> TestResult<(ClusterInfo, Vec<u32>)> {
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
    let compute = cluster::start_compute(
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
    Ok((info, vec![compute]))
}

/// A client's refusal as it reads it, logging in as `name` with `password`.
fn client_refusal(name: &str, password: &str) -> TestResult<String> {
    Runtime::new()?.block_on(async {
        let login = ScramLogin::new(name, password, system_nonce)?;
        let address = client_address()?.parse()?;
        let config = ClientConfig::new(CLUSTER, "ultragraphdb-test");
        let mut client = Client::new(TcpConnector::new(address), config, login, 16);
        let refused = client.connect().await.err().context("a refusal")?;
        Ok(refused.to_string())
    })
}

#[when("a client authenticates on the client endpoint with the role secret")]
fn client_with_secret(w: &mut Steps) -> TestResult {
    // Compute relays an exchange to the system group, so the storage process hosting it starts first.
    endpoint_steps::started(w, &compute_holding()?)?;
    w.log = client_refusal("storage", SECRET)?;
    Ok(())
}

#[then("authentication is refused with the same error as a wrong password")]
fn same_as_wrong_password(w: &mut Steps) -> TestResult {
    assert!(w.log.contains("authentication failed"), "{}", w.log);
    assert_eq!(w.log, client_refusal("storage", "a wrong password")?);
    w.roles = None;
    Ok(())
}

#[then(
    "it names neither `ultragraphdb_store_kv`, fjall, `ultragraphdb_common_system`, `ultragraphdb_cluster_metadata`, `ultragraphdb_cluster_sequencer` nor `ultragraphdb_transactions_log`"
)]
fn no_engine_and_no_book(w: &mut Steps) -> TestResult {
    if !w.graph.contains("ultragraphdb_common_service") {
        return Err(TestError::new(format!(
            "the graph is not the binary's: {}",
            w.graph
        )));
    }
    for engine in ["ultragraphdb_store_kv", "fjall"] {
        assert!(!w.graph.contains(engine), "{engine} is linked");
    }
    // A crate's target is named for it, so a label naming one ends in `:<crate>`.
    for book in [
        "ultragraphdb_common_system",
        "ultragraphdb_cluster_metadata",
        "ultragraphdb_cluster_sequencer",
        "ultragraphdb_transactions_log",
    ] {
        let named: Vec<&str> = w
            .graph
            .lines()
            .filter(|label| label.contains(&format!("/{book}:")))
            .collect();
        assert!(named.is_empty(), "{book} is in the graph: {named:?}");
    }
    Ok(())
}

#[when("the compute binary receives settings with no `cluster.system_address`")]
fn no_system_address(w: &mut Steps) -> TestResult {
    let mut settings = compute_holding()?;
    let before = settings.len();
    settings.retain(|(name, _)| name != "cluster.system_address");
    assert_eq!(
        settings.len() + 1,
        before,
        "compute's settings named the address once"
    );
    let log = scratch("compute-no-address.log");
    let util = services()?;
    let pid = util.start_service_from_config(role(
        COMPUTE_BINARY,
        &settings,
        &log,
        WaitStrategy::NoWait,
    )?)?;
    w.exited = Some(util.wait_for_exit(pid, Duration::from_secs(START_SECONDS))?);
    w.log = std::fs::read_to_string(&log)?;
    Ok(())
}

#[then("it exits with code 78 and an error naming `cluster.system_address`")]
fn names_system_address(w: &mut Steps) -> TestResult {
    exit_is(w.exited, &w.log, 78)?;
    assert!(w.log.contains("cluster.system_address"), "{}", w.log);
    Ok(())
}

#[when("a role starts with `ULTRAGRAPHDB_PARENT_PID` naming a process that is not its parent")]
fn parent_gone(w: &mut Steps) -> TestResult {
    let log = scratch("compute-parent.log");
    let instance = served_by_double(COMPUTE_BINARY, &render_settings(&compute_holding()?));
    // No process holds the largest identity, so it is no parent of the role's.
    let named = with(
        bootstrap(&double_address()?, &instance),
        PARENT_PID,
        &u32::MAX.to_string(),
    );
    let util = services()?;
    let pid =
        util.start_service_from_config(process(COMPUTE_BINARY, named, &log, WaitStrategy::NoWait))?;
    w.exited = Some(util.wait_for_exit(pid, Duration::from_secs(START_SECONDS))?);
    w.log = std::fs::read_to_string(&log)?;
    Ok(())
}

#[then("it stops as on SIGTERM and exits with code 0")]
fn stopped_with_its_parent(w: &mut Steps) -> TestResult {
    exit_is(w.exited, &w.log, 0)
}

/// Starts compute on `settings`, served by the double, its log in `log`.
fn start_compute(
    w: &mut Steps,
    settings: &[(String, String)],
    log: &std::path::Path,
) -> TestResult<u32> {
    let util = services()?;
    let pid =
        util.start_service_from_config(role(COMPUTE_BINARY, settings, log, WaitStrategy::NoWait)?)?;
    w.roles = Some(util);
    Ok(pid)
}

/// Compute's `pid` of `w`, its exit and log held once it ends.
fn until_compute_exits(w: &mut Steps, pid: u32, log: &std::path::Path) -> TestResult {
    let util = w.roles.as_ref().context("compute")?;
    w.exited = Some(util.wait_for_exit(pid, Duration::from_secs(START_SECONDS))?);
    w.log = std::fs::read_to_string(log)?;
    Ok(())
}

/// How many of `log`'s lines are a retry of `address`.
fn retries_of(log: &str, address: &str) -> usize {
    log.lines()
        .filter(|line| line.contains("in use") && line.contains(address))
        .count()
}

#[when("a role starts at a TCP address another process holds, which lets it go within the bound")]
fn port_held_a_moment(w: &mut Steps) -> TestResult {
    let log = scratch("compute-port-a-moment.log");
    let observability = compute_observability()?;
    let held = std::net::TcpListener::bind(&observability)?;
    let _pid = start_compute(w, &compute_holding()?, &log)?;
    let mut retried = false;
    for _ in 0..START_SECONDS * 100 {
        w.log = std::fs::read_to_string(&log).unwrap_or_default();
        retried = retries_of(&w.log, &observability) > 0;
        if retried {
            break;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    drop(held);
    assert!(
        retried,
        "a retry in the log, {START_SECONDS} s on: {}",
        w.log
    );
    until_answering(&observability, "/health")?;
    w.log = std::fs::read_to_string(&log)?;
    Ok(())
}

#[then("the role logs each retry naming the address, binds it, and answers `/health`")]
fn took_the_port(w: &mut Steps) -> TestResult {
    let observability = compute_observability()?;
    assert_eq!(http_status(&observability, "/health")?, 200);
    assert!(retries_of(&w.log, &observability) > 0, "{}", w.log);
    Ok(())
}

#[when(
    "compute starts while another process holds its client address, which lets it go within the bound"
)]
fn client_port_held_a_moment(w: &mut Steps) -> TestResult {
    let log = scratch("compute-client-port-a-moment.log");
    // Compute's one listener beside its observability: it listens at no role address.
    let client = client_address()?;
    let held = std::net::TcpListener::bind(&client)?;
    let _pid = start_compute(w, &compute_holding()?, &log)?;
    let mut retried = false;
    for _ in 0..START_SECONDS * 100 {
        w.log = std::fs::read_to_string(&log).unwrap_or_default();
        retried = retries_of(&w.log, &client) > 0;
        if retried {
            break;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(
        retried,
        "a retry of the client address in the log, {START_SECONDS} s on: {}",
        w.log
    );
    // The observability address is bound, the client address is not.
    let observability = compute_observability()?;
    w.status = http_status(&observability, "/health")?;
    drop(held);
    until_answering(&observability, "/health")?;
    w.log = std::fs::read_to_string(&log)?;
    Ok(())
}

#[then(
    "`/health` answers 503 while the address is held, and once it answers 200 a client connection to the address is accepted at once"
)]
fn health_after_binding(w: &mut Steps) -> TestResult {
    assert_eq!(w.status, 503, "{}", w.log);
    let _connected = std::net::TcpStream::connect(client_address()?)?;
    w.roles = None;
    Ok(())
}

#[when("a role starts at a TCP address another process holds past the bound")]
fn port_held_past_the_bound(w: &mut Steps) -> TestResult {
    let log = scratch("compute-port-held.log");
    // Compute's client endpoint: it listens at no role address.
    let held = std::net::TcpListener::bind(client_address()?)?;
    let pid = start_compute(w, &compute_holding()?, &log)?;
    until_compute_exits(w, pid, &log)?;
    drop(held);
    Ok(())
}

#[then(
    "it exits with code 69 after a retry logged per 50 ms of the bound, the error naming the address"
)]
fn past_the_bound(w: &mut Steps) -> TestResult {
    exit_is(w.exited, &w.log, 69)?;
    assert_eq!(
        retries_of(&w.log, &client_address()?),
        BIND_RETRIES as usize + 1,
        "a line per retry, and the error: {}",
        w.log
    );
    Ok(())
}

#[when("a role starts at a TCP address no interface of its host holds")]
fn no_such_interface(w: &mut Steps) -> TestResult {
    let log = scratch("compute-no-interface.log");
    let settings = with(
        compute_holding()?,
        "observability.address",
        &format!("tcp://{NO_INTERFACE}"),
    );
    let pid = start_compute(w, &settings, &log)?;
    until_compute_exits(w, pid, &log)
}

#[then("it exits with code 69 at once, the error naming the address, with no retry logged")]
fn at_once(w: &mut Steps) -> TestResult {
    exit_is(w.exited, &w.log, 69)?;
    assert!(w.log.contains(NO_INTERFACE), "{}", w.log);
    assert_eq!(retries_of(&w.log, NO_INTERFACE), 0, "{}", w.log);
    Ok(())
}

fn main() -> TestResult {
    start_double()?;
    SHARED
        .set(services()?)
        .map_err(|_| TestError::new("the shared clusters' processes, set once"))?;
    let summary = futures::executor::block_on(
        Steps::cucumber()
            .max_concurrent_scenarios(1)
            .before(|_, _, _, _| {
                async {
                    let freed = own_addresses().and_then(|own| until_ports_free(&own));
                    assert!(freed.is_ok(), "{freed:?}");
                }
                .boxed_local()
            })
            .fail_on_skipped()
            .run(ultragraphdb_tool_bdd::features_dir()?),
    );
    let started = STARTED
        .lock()
        .map_err(|_| TestError::new("the shared clusters' pids"))?
        .clone();
    let util = SHARED.get().context("the shared clusters' processes")?;
    for pid in started {
        drop(util.stop_service(pid));
    }
    if cucumber::writer::Stats::execution_has_failed(&summary)
        || cucumber::writer::Stats::hook_errors(&summary) > 0
    {
        std::process::exit(1);
    }
    Ok(())
}
