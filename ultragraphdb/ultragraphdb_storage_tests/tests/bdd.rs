/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The storage binary as a process, bound to `features/`, with no container. One scenario runs at a time: the binaries listen
//! at their declared ports.

mod played_home;
mod read_kernel_steps;

use compio::runtime::Runtime;
use cucumber::{World, then, when};
use service_utils::{ServiceUtil, WaitStrategy};
use sha2::{Digest, Sha256};
use std::path::Path;
use std::process::ExitStatus;
use std::sync::atomic::AtomicBool;
use std::time::Duration;
use ultragraphdb_cluster_placement::{NodeId, PlacementMap};
use ultragraphdb_common_role::RoleClient;
use ultragraphdb_common_service::{Bootstrap, RoleKind as ServiceKind, fetch_settings};
use ultragraphdb_core_ids::{BucketGroup, Epoch};
use ultragraphdb_protocol_transport::Address;
use ultragraphdb_protocol_wire::{
    Bind, BindKind, BodyView, Command, Commit, CommitView, ErrorReplyView, Reference,
    ReferenceRegistration, ReferenceView, RoleKind, TermTable, Write, WriteChange,
};
use ultragraphdb_store_quads::iri_bucket;
use ultragraphdb_tests::{
    ADMIN_KEY, CLUSTER, CONFIG_ADDRESS, CONFIG_BINARY, DOUBLE_ADDRESS, DOUBLE_REVISION,
    FRONTEND_BINARY, FRONTEND_OBSERVABILITY, FRONTEND_ROLE_ADDRESS, READ_KEY, SECRET,
    START_SECONDS, STORAGE_BINARY, STORAGE_OBSERVABILITY, STORAGE_ROLE_ADDRESS, TARGET, answering,
    bootstrap, config_folder, config_process, exit_is, fresh_data, frontend_holding, holding,
    http_body, members_at, process, render_settings, role, role_of_service, scratch,
    served_by_double, start_config_service, start_double, storage_holding, storage_on_one_host,
    until_answering, with,
};
use ultragraphdb_tool_bdd::{Context, TestError, TestResult};
use ultragraphdb_transport_http_client::{HttpClient, HttpReply, HttpUrl};
use ultragraphdb_transport_tls::TestPki;

/// Where the sandbox holds the binaries, relative to the test's working directory: `copy_file` writes them to this package.
const ROOT_PATH: &str = "ultragraphdb/ultragraphdb_storage_tests";

fn services() -> TestResult<ServiceUtil> {
    Ok(ServiceUtil::new(
        ROOT_PATH,
        vec![CONFIG_BINARY, FRONTEND_BINARY, STORAGE_BINARY],
    )?)
}

#[derive(Debug, Default, World)]
struct Steps {
    registered: Option<ReferenceRegistration>,
    code: Option<u8>,
    config_pid: u32,
    exited: Option<ExitStatus>,
    frontend: String,
    graph: String,
    kernel: read_kernel_steps::KernelSteps,
    log: String,
    pid: u32,
    roles: Option<ServiceUtil>,
    status: u16,
    version: u64,
}

/// How long a role refused by its peer retries, in a scenario that waits it out.
const SHORT_DEADLINE_MS: &str = "800";

/// The address a loadable role's admin listener holds.
const ADMIN_ADDRESS: &str = "127.0.0.1:18090";

const NAMESPACE: u64 = 1;
const PREDICATE: &str = "http://ex.org/p";

/// Runs the storage binary on the settings `text` to its exit.
fn storage_on(w: &mut Steps, text: &str) -> TestResult {
    let instance = served_by_double(STORAGE_BINARY, text);
    run_to_exit(w, bootstrap(DOUBLE_ADDRESS, &instance))
}

/// Runs the storage binary with `variables` alone in its environment, to its exit.
fn run_to_exit(w: &mut Steps, variables: Vec<(String, String)>) -> TestResult {
    let log = scratch("storage-refused.log");
    let util = services()?;
    let pid = util.start_service_from_config(process(
        STORAGE_BINARY,
        variables,
        &log,
        WaitStrategy::NoWait,
    ))?;
    w.exited = Some(util.wait_for_exit(pid, Duration::from_secs(START_SECONDS))?);
    w.log = std::fs::read_to_string(&log)?;
    Ok(())
}

#[when("a storage process started with a different role secret connects to the frontend")]
fn wrong_secret(w: &mut Steps) -> TestResult {
    let util = services()?;
    let log = scratch(FRONTEND_BINARY);
    let wait = answering(FRONTEND_OBSERVABILITY, "/health");
    let settings = frontend_holding()?;
    let _frontend: u32 =
        util.start_service_from_config(role(FRONTEND_BINARY, &settings, &log, wait))?;
    let refused = with(
        with(
            holding(storage_on_one_host(), "another secret")?,
            "storage.data_dir",
            &fresh_data(),
        ),
        "cluster.join_deadline_ms",
        SHORT_DEADLINE_MS,
    );
    let refused_log = scratch("storage-refused.log");
    let pid = util.start_service_from_config(role(
        STORAGE_BINARY,
        &refused,
        &refused_log,
        WaitStrategy::NoWait,
    ))?;
    w.exited = Some(util.wait_for_exit(pid, Duration::from_secs(START_SECONDS))?);
    w.log = std::fs::read_to_string(&refused_log)?;
    w.frontend = format!("tcp://{}", FRONTEND_ROLE_ADDRESS);
    w.roles = Some(util);
    Ok(())
}

#[then(
    "its authentication is refused, it sends no role command, and it retries until its deadline"
)]
fn refused_and_retried(w: &mut Steps) -> TestResult {
    exit_is(w.exited, &w.log, 69)?;
    assert!(w.log.contains("authentication failed"), "{}", w.log);
    let attempts = w
        .log
        .split("after ")
        .nth(1)
        .and_then(|rest| rest.split(' ').next())
        .and_then(|count| count.parse::<u32>().ok())
        .context("the attempts it made")?;
    assert!(attempts > 1, "{}", w.log);
    assert_eq!(members_at(&w.frontend)?, Vec::new(), "no role joined");
    w.roles = None;
    Ok(())
}

#[when("a role starts with `role_secret_file` naming a file that does not exist")]
fn missing_secret(w: &mut Steps) -> TestResult {
    let absent = scratch("never-written");
    let settings = with(
        with(storage_on_one_host(), "storage.data_dir", &fresh_data()),
        "cluster.role_secret_file",
        &absent.display().to_string(),
    );
    let log = scratch("storage-no-secret.log");
    let util = services()?;
    let pid = util.start_service_from_config(role(
        STORAGE_BINARY,
        &settings,
        &log,
        WaitStrategy::NoWait,
    ))?;
    w.exited = Some(util.wait_for_exit(pid, Duration::from_secs(START_SECONDS))?);
    w.log = std::fs::read_to_string(&log)?;
    w.graph = absent.display().to_string();
    Ok(())
}

#[then("it exits with code 78 and an error naming the file")]
fn names_the_file(w: &mut Steps) -> TestResult {
    exit_is(w.exited, &w.log, 78)?;
    assert!(w.log.contains(&w.graph), "{}", w.log);
    Ok(())
}

/// A subject of `partitions` partitions that `partition` homes.
fn subject_of(partition: u32, partitions: u32, namespace: u64) -> TestResult<String> {
    let map = PlacementMap::even(partitions, NodeId::new(1))?;
    (0..100_000)
        .map(|at| format!("http://ex.org/s{at}"))
        .find(|iri| {
            iri_bucket(namespace, iri)
                .is_some_and(|bucket| map.partition_of(bucket, Epoch::ZERO).get() == partition)
        })
        .context("a subject of the partition")
}

/// A Write of `subject` `<p>` `object`, the object an IRI.
fn edge_write(subject: &str, object: &str) -> Write {
    let mut table = TermTable::new();
    let s = table.iri(subject);
    let p = table.iri(PREDICATE);
    let o = table.iri(object);
    Write::new(
        table,
        vec![WriteChange::Insert {
            graph: None,
            subject: s,
            predicate: p,
            object: o,
        }],
    )
}

/// Commits `write` in a transaction bound to `partition` at `address`, through no session: its commit version.
async fn commit_at(address: &str, partition: u32, write: &Write) -> TestResult<u64> {
    commit_through(address, partition, write, Commit::new([0; 16], 0, 0)).await
}

/// Commits `write` in a transaction bound to `partition` at `address` by the Commit `commit`: its commit version.
async fn commit_through(
    address: &str,
    partition: u32,
    write: &Write,
    commit: Commit,
) -> TestResult<u64> {
    let target = Address::parse(address)?;
    let mut client = RoleClient::dial(&target, CLUSTER, "frontend", SECRET).await?;
    let bind = Bind::new(partition, BindKind::Transaction, NAMESPACE, 0, 1, [7; 16]);
    drop(client.request(&bind).await?);
    let stream = client.last_stream();
    client.send_on(stream, write).await?;
    client.send_on(stream, &commit).await?;
    let answer = client.next_frame().await?;
    let view = answer.view()?;
    if view.command() == Command::Error {
        let refusal = ErrorReplyView::from_frame(&view)?;
        return Err(TestError::new(format!(
            "partition {partition} refused: {}",
            refusal.message()
        )));
    }
    Ok(CommitView::from_frame(&view)?.commit_version())
}

/// The settings of a storage process hosting `count` partitions from `first`, at TCP ports from `port`.
fn storage_of(first: u32, count: u32, port: u32) -> TestResult<Vec<(String, String)>> {
    let observed = format!("tcp://127.0.0.1:{}", 18_100 + first);
    let mut settings = with(storage_holding()?, "observability.address", &observed);
    settings = with(settings, "storage.partition_first", &first.to_string());
    settings = with(settings, "storage.partition_count", &count.to_string());
    Ok(with(
        settings,
        "storage.role_address",
        &format!("tcp://127.0.0.1:{port}"),
    ))
}

/// A frontend over 4 partitions and two storage processes hosting 1 and 2, and 3 and 4, all held by the config service.
fn two_processes(w: &mut Steps) -> TestResult {
    start_processes(w, &[1, 3])
}

/// As [`two_processes`], the second storage process left unstarted for the test to play the partitions it hosts.
fn one_process_of_two(w: &mut Steps) -> TestResult {
    start_processes(w, &[1])
}

/// The config service holding a frontend over 4 partitions and two storage processes hosting 1 and 2, and 3 and 4; then the
/// frontend and the storage processes whose first partitions are `firsts`.
fn start_processes(w: &mut Steps, firsts: &[u32]) -> TestResult {
    let util = services()?;
    let settings = with(frontend_holding()?, "cluster.partition_count", "4");
    let (first, second) = (storage_of(1, 2, 17_200)?, storage_of(3, 2, 17_300)?);
    let _service: u32 = start_config_service(
        &util,
        &[
            ("frontend", &settings),
            ("storage_1", &first),
            ("storage_3", &second),
        ],
    )?;
    let wait = answering(FRONTEND_OBSERVABILITY, "/health");
    let _frontend: u32 = util.start_service_from_config(role_of_service(
        FRONTEND_BINARY,
        "frontend",
        &scratch(FRONTEND_BINARY),
        wait,
    ))?;
    for at in firsts {
        let ready = answering(&format!("127.0.0.1:{}", 18_100 + at), "/ready");
        let start = role_of_service(
            STORAGE_BINARY,
            &format!("storage_{at}"),
            &scratch(&format!("storage-{at}")),
            ready,
        );
        let _storage: u32 = util.start_service_from_config(start)?;
    }
    w.roles = Some(util);
    w.frontend = format!("tcp://{}", FRONTEND_ROLE_ADDRESS);
    Ok(())
}

#[when("one storage process claims partitions 1 and 2 and another claims 3 and 4")]
fn claims(w: &mut Steps) -> TestResult {
    two_processes(w)
}

#[then(
    "both join, Members names each partition's address, and a write reaches its partition on either process"
)]
fn on_either(w: &mut Steps) -> TestResult {
    let members = members_at(&w.frontend)?;
    let expected: Vec<(RoleKind, u32, String)> =
        [(1, 17_200), (2, 17_201), (3, 17_300), (4, 17_301)]
            .map(|(partition, port)| {
                (
                    RoleKind::Storage,
                    partition,
                    format!("tcp://127.0.0.1:{port}"),
                )
            })
            .to_vec();
    assert_eq!(members, expected);
    Runtime::new()?.block_on(async {
        for (partition, _, address) in members
            .iter()
            .map(|(_, partition, address)| (*partition, (), address))
        {
            let subject = subject_of(partition, 4, NAMESPACE)?;
            let write = edge_write(&subject, &subject);
            let version = commit_at(address, partition, &write).await?;
            assert!(version > 0, "partition {partition}");
            // A whole transaction through a session passes as one through none does.
            let session = Commit::new(u128::from(partition).to_be_bytes(), 1, 0);
            let through = commit_through(address, partition, &write, session).await?;
            assert!(through > version, "partition {partition}");
        }
        Ok::<(), TestError>(())
    })?;
    w.roles = None;
    Ok(())
}

#[when(
    "a partition of one storage process registers a reference to an entity homed in another storage process"
)]
fn registers(w: &mut Steps) -> TestResult {
    two_processes(w)?;
    let subject = subject_of(3, 4, NAMESPACE)?;
    let group = BucketGroup::of(iri_bucket(NAMESPACE, &subject).context("a bucket")?);
    let target = Address::parse("tcp://127.0.0.1:17300")?;
    let mut reference = Reference::request().in_namespace(NAMESPACE);
    let _index = reference.register_iri(3, Some(group), &subject, 0);
    let registered = Runtime::new()?.block_on(async {
        let mut client = RoleClient::dial(&target, CLUSTER, "storage", SECRET).await?;
        // A repeat names the older generation, so it changes nothing: it only asks until a close has reached the home.
        for _ in 0..START_SECONDS * 20 {
            let reply = client.request(&reference).await?;
            let view = reply.view()?;
            let answer = ReferenceView::from_frame(&view)?;
            assert!(answer.is_reply());
            let registration = answer.registration(0).context("the registration's reply")?;
            if registration.close() > 0 {
                return Ok(registration);
            }
            compio::time::sleep(Duration::from_millis(50)).await;
        }
        Err(TestError::new("no close reached the home"))
    })?;
    w.registered = Some(registered);
    Ok(())
}

#[then(
    "it sends a registration role command to that process, and receives the entity's ID, the member's generation and the home's latest delivered close in the reply"
)]
fn registered_reply(w: &mut Steps) -> TestResult {
    let registered = w.registered.context("a reply")?;
    assert_eq!(registered.outcome(), 0);
    assert!(registered.entity().is_some(), "the entity's ID");
    assert_eq!(registered.generation(), 1, "a member's first generation");
    assert!(registered.close() > 0, "the home's latest delivered close");
    w.roles = None;
    Ok(())
}

#[when("a write in one storage process names an entity homed in the other")]
fn crossing(w: &mut Steps) -> TestResult {
    two_processes(w)?;
    let subject = subject_of(1, 4, NAMESPACE)?;
    let object = subject_of(3, 4, NAMESPACE)?;
    w.version = Runtime::new()?.block_on(commit_at(
        "tcp://127.0.0.1:17200",
        1,
        &edge_write(&subject, &object),
    ))?;
    Ok(())
}

#[then("the home registers it by a Reference, and the write commits")]
fn registered(w: &mut Steps) {
    assert!(w.version > 0);
    w.roles = None;
}

/// One partition's storage, started with `settings`, waiting on `/ready`; its pid.
fn start_storage(util: &ServiceUtil, settings: &[(String, String)], log: &Path) -> TestResult<u32> {
    let ready = answering(STORAGE_OBSERVABILITY, "/ready");
    Ok(util.start_service_from_config(role(STORAGE_BINARY, settings, log, ready))?)
}

/// What runs a storage process with commits acknowledged, its settings and its pid.
type Committed = (ServiceUtil, Vec<(String, String)>, u32);

/// A frontend and one partition's storage, with commits acknowledged: what restarts it, its log and its pid.
fn committed_storage(w: &mut Steps) -> TestResult<Committed> {
    let util = services()?;
    let wait = answering(FRONTEND_OBSERVABILITY, "/health");
    let _frontend: u32 = util.start_service_from_config(role(
        FRONTEND_BINARY,
        &frontend_holding()?,
        &scratch(FRONTEND_BINARY),
        wait,
    ))?;
    let settings = storage_holding()?;
    let pid = start_storage(&util, &settings, &scratch("storage-first"))?;
    let subject = subject_of(1, 1, NAMESPACE)?;
    let address = format!("tcp://{}", STORAGE_ROLE_ADDRESS);
    Runtime::new()?.block_on(async {
        for at in 0..8 {
            let object = format!("http://ex.org/o{at}");
            w.version = commit_at(&address, 1, &edge_write(&subject, &object)).await?;
        }
        Ok::<(), TestError>(())
    })?;
    Ok((util, settings, pid))
}

/// The rounds the log at `log` says its recovery replayed, its last recovery's.
fn rounds_replayed(log: &str) -> TestResult<u64> {
    log.lines()
        .rev()
        .filter(|line| line.contains("recovered"))
        .filter_map(|line| line.split("rounds=").nth(1))
        .filter_map(|rest| rest.split_whitespace().next())
        .find_map(|count| count.parse().ok())
        .context("a recovery in the log")
}

#[when("storage receives SIGTERM after acknowledging commits")]
fn flushing(w: &mut Steps) -> TestResult {
    let (util, settings, pid) = committed_storage(w)?;
    w.exited = Some(util.stop_service(pid)?);
    let again = scratch("storage-again.log");
    let _restarted: u32 = start_storage(&util, &settings, &again)?;
    w.log = std::fs::read_to_string(&again)?;
    w.roles = Some(util);
    Ok(())
}

#[then(
    "every acknowledged commit is applied to its store before it exits, and a restart replays nothing"
)]
fn nothing_to_replay(w: &mut Steps) -> TestResult {
    exit_is(w.exited, &w.log, 0)?;
    assert_eq!(rounds_replayed(&w.log)?, 0, "{}", w.log);
    w.roles = None;
    Ok(())
}

#[when("a storage process killed with rounds in its log restarts")]
fn killed(w: &mut Steps) -> TestResult {
    let (util, settings, pid) = committed_storage(w)?;
    let killed = std::process::Command::new("kill")
        .args(["-9", &pid.to_string()])
        .status()?;
    assert!(killed.success());
    w.exited = Some(util.wait_for_exit(pid, Duration::from_secs(START_SECONDS))?);
    let again = scratch("storage-recovered.log");
    let health = answering(STORAGE_OBSERVABILITY, "/health");
    let _restarted: u32 =
        util.start_service_from_config(role(STORAGE_BINARY, &settings, &again, health))?;
    until_answering(STORAGE_OBSERVABILITY, "/ready")?;
    w.log = std::fs::read_to_string(&again)?;
    w.roles = Some(util);
    Ok(())
}

#[then("`/health` answers at once and `/ready` answers only after the replay")]
fn replayed_first(w: &mut Steps) -> TestResult {
    assert!(rounds_replayed(&w.log)? > 0, "{}", w.log);
    let recovered = w.log.find("recovered").context("a recovery")?;
    let ready = w.log.find("ready").context("the role ready")?;
    assert!(recovered < ready, "{}", w.log);
    w.roles = None;
    Ok(())
}

#[when(
    "the storage binary receives settings whose `[storage]` table holds the key `partiton_count`"
)]
fn unknown_setting(w: &mut Steps) -> TestResult {
    storage_on(w, "[storage]\npartiton_count = 4\n")
}

#[then("it exits with code 78 and an error naming `partiton_count`")]
fn names_the_key(w: &mut Steps) -> TestResult {
    exit_is(w.exited, &w.log, 78)?;
    assert!(w.log.contains("partiton_count"), "{}", w.log);
    Ok(())
}

#[when("the storage binary receives a setting out of its bounds")]
fn out_of_bounds(w: &mut Steps) -> TestResult {
    storage_on(w, "[storage]\npartition_first = -1\n")
}

#[then("it exits with code 78 and an error naming the setting and the bound")]
fn names_the_bound(w: &mut Steps) -> TestResult {
    exit_is(w.exited, &w.log, 78)?;
    assert!(w.log.contains("storage.partition_first"), "{}", w.log);
    assert!(w.log.contains(&u32::MAX.to_string()), "{}", w.log);
    Ok(())
}

#[when("a role starts with `pin_cores = true`")]
fn pinned(w: &mut Steps) -> TestResult {
    let mut variables = storage_holding()?;
    variables.push(("runtime.pin_cores".to_string(), "true".to_string()));
    let util = services()?;
    let log = scratch("storage-pinned.log");
    let wait = answering(STORAGE_OBSERVABILITY, "/health");
    let _pid: u32 = util.start_service_from_config(role(STORAGE_BINARY, &variables, &log, wait))?;
    w.log = std::fs::read_to_string(&log)?;
    Ok(())
}

/// Only Linux can read a binding back here: macOS binds nothing, and a container may refuse. Each says so, and serves.
#[then("it logs what became of its binding, and serves requests")]
fn logs_the_binding(w: &mut Steps) {
    let expected: &[&str] = if cfg!(target_os = "linux") {
        &["pinned to CPU", "unpinned: asked for CPU"]
    } else {
        &["unpinned: this platform cannot bind"]
    };
    assert!(
        expected.iter().any(|line| w.log.contains(line)),
        "{}",
        w.log
    );
}

#[when("the storage binary receives settings with no `storage.data_dir`")]
fn no_data_dir(w: &mut Steps) -> TestResult {
    let text = render_settings(&holding(storage_on_one_host(), SECRET)?);
    storage_on(w, &text)
}

#[then("it exits with code 78 and an error naming `storage.data_dir`")]
fn names_data_dir(w: &mut Steps) -> TestResult {
    exit_is(w.exited, &w.log, 78)?;
    assert!(w.log.contains("storage.data_dir"), "{}", w.log);
    Ok(())
}

#[when("a storage process starts and no config service answers for its patience")]
fn service_down(w: &mut Steps) -> TestResult {
    let port = std::net::TcpListener::bind("127.0.0.1:0")?
        .local_addr()?
        .port();
    let read = Bootstrap::read(
        ServiceKind::Storage,
        bootstrap(&format!("127.0.0.1:{port}"), "storage_1"),
    )?;
    let patience = Duration::from_millis(1);
    let refused = Runtime::new()?
        .block_on(fetch_settings(&read, patience, &AtomicBool::new(false)))
        .err()
        .context("a refusal")?;
    w.code = Some(refused.exit_code());
    w.log = refused.to_string();
    w.graph = port.to_string();
    Ok(())
}

#[then("it exits with code 69 and an error naming the URL, the target and the instance")]
fn names_the_service(w: &mut Steps) {
    assert_eq!(w.code, Some(69), "{}", w.log);
    for named in [w.graph.as_str(), TARGET, "storage_1"] {
        assert!(w.log.contains(named), "{named} in: {}", w.log);
    }
}

#[when("a storage process starts and the config service begins answering a moment later")]
fn service_late(w: &mut Steps) -> TestResult {
    let util = services()?;
    let settings = storage_holding()?;
    let frontend_settings = frontend_holding()?;
    let waiting = role_of_service(
        STORAGE_BINARY,
        "storage",
        &scratch("storage-waiting.log"),
        WaitStrategy::NoWait,
    );
    let _waiting: u32 = util.start_service_from_config(waiting)?;
    std::thread::sleep(Duration::from_millis(600));
    let _service: u32 = start_config_service(
        &util,
        &[("frontend", &frontend_settings), ("storage", &settings)],
    )?;
    w.roles = Some(util);
    Ok(())
}

#[then("the process takes its settings and starts")]
fn took_settings(w: &mut Steps) -> TestResult {
    until_answering(STORAGE_OBSERVABILITY, "/health")?;
    let health = http_body(STORAGE_OBSERVABILITY, "/health")?;
    assert!(health.contains("instance storage,"), "{health}");
    assert!(health.contains("revision 1"), "{health}");
    w.roles = None;
    Ok(())
}

#[when("a storage process starts with a key the service refuses")]
fn wrong_key(w: &mut Steps) -> TestResult {
    let util = services()?;
    let settings = storage_holding()?;
    let frontend_settings = frontend_holding()?;
    let _service: u32 = start_config_service(
        &util,
        &[("frontend", &frontend_settings), ("storage", &settings)],
    )?;
    let file = scratch("wrong.key");
    std::fs::write(&file, "a key the service never issued to anyone")?;
    let variables = with(
        bootstrap(CONFIG_ADDRESS, "storage"),
        "ULTRAGRAPHDB_CONFIG_KEY_FILE",
        &file.display().to_string(),
    );
    let log = scratch("storage-wrong-key.log");
    let pid = util.start_service_from_config(process(
        STORAGE_BINARY,
        variables,
        &log,
        WaitStrategy::NoWait,
    ))?;
    w.exited = Some(util.wait_for_exit(pid, Duration::from_secs(START_SECONDS))?);
    w.log = std::fs::read_to_string(&log)?;
    Ok(())
}

#[then("it exits with code 78 at once, naming the URL")]
fn names_the_url(w: &mut Steps) -> TestResult {
    exit_is(w.exited, &w.log, 78)?;
    assert!(w.log.contains(CONFIG_ADDRESS), "{}", w.log);
    assert!(w.log.contains("401"), "{}", w.log);
    Ok(())
}

#[when("the storage binary starts with `ULTRAGRAPHDB_CONFIG_INSTANCE=compute_1`")]
fn another_role(w: &mut Steps) -> TestResult {
    run_to_exit(w, bootstrap(DOUBLE_ADDRESS, "compute_1"))
}

#[then("it exits with code 78 naming the instance and its role, without asking the service")]
fn names_the_instance(w: &mut Steps) -> TestResult {
    exit_is(w.exited, &w.log, 78)?;
    assert!(w.log.contains("compute_1"), "{}", w.log);
    assert!(w.log.contains("storage"), "{}", w.log);
    assert!(!w.log.contains("status"), "{}", w.log);
    Ok(())
}

#[when("a role starts with `ULTRAGRAPHDB_STORAGE_PARTITION_COUNT=2` in its environment")]
fn stray_variable(w: &mut Steps) -> TestResult {
    let variables = with(
        bootstrap(DOUBLE_ADDRESS, "storage_1"),
        "ULTRAGRAPHDB_STORAGE_PARTITION_COUNT",
        "2",
    );
    run_to_exit(w, variables)
}

#[then("it exits with code 78 naming the variable")]
fn names_the_variable(w: &mut Steps) -> TestResult {
    exit_is(w.exited, &w.log, 78)?;
    assert!(
        w.log.contains("ULTRAGRAPHDB_STORAGE_PARTITION_COUNT"),
        "{}",
        w.log
    );
    Ok(())
}

#[when("a new storage process starts beside the running instances, the settings unchanged")]
fn new_instance(w: &mut Steps) -> TestResult {
    two_processes(w)
}

#[then("it receives the active revision the running instances report")]
fn same_revision(w: &mut Steps) -> TestResult {
    let first = http_body("127.0.0.1:18101", "/health")?;
    let second = http_body("127.0.0.1:18103", "/health")?;
    assert!(first.contains("instance storage_1, revision 1"), "{first}");
    assert!(
        second.contains("instance storage_3, revision 1"),
        "{second}"
    );
    w.roles = None;
    Ok(())
}

#[when("a role runs a revision of a target as an instance")]
fn running_revision(w: &mut Steps) -> TestResult {
    let util = services()?;
    let instance = served_by_double(STORAGE_BINARY, &render_settings(&storage_holding()?));
    let start = process(
        STORAGE_BINARY,
        bootstrap(DOUBLE_ADDRESS, &instance),
        &scratch("storage-health.log"),
        answering(STORAGE_OBSERVABILITY, "/health"),
    );
    let _pid: u32 = util.start_service_from_config(start)?;
    w.graph = instance;
    w.roles = Some(util);
    Ok(())
}

#[then("its `/health` body names the target, the instance and the revision")]
fn health_names(w: &mut Steps) -> TestResult {
    let health = http_body(STORAGE_OBSERVABILITY, "/health")?;
    let expected = format!(
        "target {TARGET}, instance {}, revision {DOUBLE_REVISION}",
        w.graph
    );
    assert!(health.contains(&expected), "{expected} in: {health}");
    w.roles = None;
    Ok(())
}

#[when("a role asks a config service that speaks TLS, trusting its authority")]
fn over_tls(w: &mut Steps) -> TestResult {
    let util = services()?;
    let pki = TestPki::new()?;
    let (cert, key, ca) = (scratch("tls.crt"), scratch("tls.key"), scratch("ca.crt"));
    std::fs::write(&cert, pki.server_chain_pem())?;
    std::fs::write(&key, pki.server_key_pem())?;
    std::fs::write(&ca, pki.ca_pem())?;
    let own = config_folder(
        &[
            ("frontend", &frontend_holding()?),
            ("storage", &storage_holding()?),
        ],
        &format!(
            "tls_cert_file = \"{}\"\ntls_key_file = \"{}\"",
            cert.display(),
            key.display()
        ),
    )?;
    let _service: u32 =
        util.start_service_from_config(config_process(&own, WaitStrategy::NoWait))?;
    let variables = with(
        with(
            bootstrap(CONFIG_ADDRESS, "storage"),
            "ULTRAGRAPHDB_CONFIG_URL",
            &format!("https://{CONFIG_ADDRESS}"),
        ),
        "ULTRAGRAPHDB_CONFIG_CA_FILE",
        &ca.display().to_string(),
    );
    let _role: u32 = util.start_service_from_config(process(
        STORAGE_BINARY,
        variables,
        &scratch("storage-tls.log"),
        answering(STORAGE_OBSERVABILITY, "/health"),
    ))?;
    w.roles = Some(util);
    Ok(())
}

#[then("it takes its settings over the verified connection")]
fn took_over_tls(w: &mut Steps) -> TestResult {
    let health = http_body(STORAGE_OBSERVABILITY, "/health")?;
    assert!(health.contains("revision 1"), "{health}");
    w.roles = None;
    Ok(())
}

/// `key` asks `method` of `path` at `address`, in clear text.
fn ask(address: &str, key: &str, method: &str, path: &str, body: &[u8]) -> TestResult<HttpReply> {
    let url = HttpUrl::parse(&format!("http://{address}"))?;
    let client = HttpClient::new(url, None, key.to_string())?;
    Ok(Runtime::new()?.block_on(client.request(method, path, body))?)
}

/// `settings` of a storage process with an admin listener authorising the admin key, by its digest alone.
fn with_admin(settings: Vec<(String, String)>) -> Vec<(String, String)> {
    let digest: String = Sha256::digest(ADMIN_KEY.as_bytes())
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect();
    let plain = with(settings, "admin.plaintext", "true");
    let addressed = with(plain, "admin.address", &format!("tcp://{ADMIN_ADDRESS}"));
    with(addressed, "admin.key_sha256", &digest)
}

/// Starts a frontend and one partition's storage with an admin listener, through the service holding revision 1, acknowledges
/// commits, and adds revision 2 unactivated. What it leaves running stays in `w`, with storage's pid.
fn loadable(w: &mut Steps) -> TestResult {
    let util = services()?;
    let frontend_settings = frontend_holding()?;
    let storage_settings = with_admin(storage_holding()?);
    w.config_pid = start_config_service(
        &util,
        &[
            ("frontend", &frontend_settings),
            ("storage", &storage_settings),
        ],
    )?;
    let _frontend: u32 = util.start_service_from_config(role_of_service(
        FRONTEND_BINARY,
        "frontend",
        &scratch(FRONTEND_BINARY),
        answering(FRONTEND_OBSERVABILITY, "/health"),
    ))?;
    w.pid = util.start_service_from_config(role_of_service(
        STORAGE_BINARY,
        "storage",
        &scratch("storage-loaded.log"),
        answering(STORAGE_OBSERVABILITY, "/ready"),
    ))?;
    let subject = subject_of(1, 1, NAMESPACE)?;
    let address = format!("tcp://{}", STORAGE_ROLE_ADDRESS);
    Runtime::new()?.block_on(async {
        for at in 0..8 {
            let object = format!("http://ex.org/o{at}");
            w.version = commit_at(&address, 1, &edge_write(&subject, &object)).await?;
        }
        Ok::<(), TestError>(())
    })?;
    let mut body = Vec::new();
    for (key, text) in [
        ("frontend", render_settings(&frontend_settings)),
        (
            "storage",
            format!("{}# revision 2\n", render_settings(&storage_settings)),
        ),
    ] {
        body.extend_from_slice(format!("{key} {}\n{text}\n", text.len()).as_bytes());
    }
    let path = format!("/v1/targets/{TARGET}/revisions?base=1");
    let added = ask(CONFIG_ADDRESS, ADMIN_KEY, "POST", &path, &body)?;
    assert_eq!(
        added.status(),
        201,
        "{:?}",
        String::from_utf8_lossy(added.body())
    );
    w.roles = Some(util);
    Ok(())
}

/// Asks storage to load `revision` with `key`, holding the status and the answer.
fn load(w: &mut Steps, key: &str, revision: &str) -> TestResult {
    let reply = ask(
        ADMIN_ADDRESS,
        key,
        "POST",
        "/admin/load-config",
        revision.as_bytes(),
    )?;
    w.status = reply.status();
    w.log = String::from_utf8_lossy(reply.body()).into_owned();
    Ok(())
}

/// Waits for storage's `/health` to name `revision`.
fn until_revision(revision: u64) -> TestResult {
    let expected = format!("revision {revision}\n");
    for _ in 0..START_SECONDS * 20 {
        if http_body(STORAGE_OBSERVABILITY, "/health").is_ok_and(|body| body.ends_with(&expected)) {
            return Ok(());
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    Err(TestError::new(format!(
        "storage never reported revision {revision}"
    )))
}

#[when("an administrator asks a storage process to load a revision the service holds")]
fn loads(w: &mut Steps) -> TestResult {
    loadable(w)?;
    load(w, ADMIN_KEY, "2")
}

#[then(
    "it answers 202, flushes as at SIGTERM, keeps its process id, and its `/health` names the revision"
)]
fn loaded(w: &mut Steps) -> TestResult {
    assert_eq!(w.status, 202, "{}", w.log);
    until_revision(2)?;
    until_answering(STORAGE_OBSERVABILITY, "/ready")?;
    let alive = std::process::Command::new("kill")
        .args(["-0", &w.pid.to_string()])
        .status()?;
    assert!(alive.success(), "the process it was started as still runs");
    let log = std::fs::read_to_string(scratch("storage-loaded.log"))?;
    assert_eq!(rounds_replayed(&log)?, 0, "{log}");
    assert_eq!(log.matches("recovered").count(), 2, "{log}");
    w.roles = None;
    Ok(())
}

#[when("a request to load a revision carries the read key")]
fn load_with_read_key(w: &mut Steps) -> TestResult {
    loadable(w)?;
    load(w, READ_KEY, "2")
}

#[then("it is refused with status 403, and the role runs on")]
fn load_refused(w: &mut Steps) -> TestResult {
    assert_eq!(w.status, 403, "{}", w.log);
    until_revision(1)?;
    w.roles = None;
    Ok(())
}

#[when(
    "a role is asked to load a revision the service does not hold, or while the service is down"
)]
fn load_unfetchable(w: &mut Steps) -> TestResult {
    loadable(w)?;
    load(w, ADMIN_KEY, "9")?;
    assert_eq!(w.status, 422, "{}", w.log);
    assert!(
        w.log.contains("revision 9") && w.log.contains("404"),
        "{}",
        w.log
    );
    let util = w.roles.as_ref().context("the roles")?;
    let _stopped = util.stop_service(w.config_pid)?;
    load(w, ADMIN_KEY, "2")?;
    assert_eq!(w.status, 503, "{}", w.log);
    assert!(
        w.log.contains("revision 2") && w.log.contains("config service"),
        "{}",
        w.log
    );
    Ok(())
}

#[then("the request is refused naming the revision and why, and the role runs on at its revision")]
fn load_failed(w: &mut Steps) -> TestResult {
    until_revision(1)?;
    w.roles = None;
    Ok(())
}

#[when("a role loaded a revision while another is active, and then crashes and restarts")]
fn load_crashes(w: &mut Steps) -> TestResult {
    loadable(w)?;
    load(w, ADMIN_KEY, "2")?;
    assert_eq!(w.status, 202, "{}", w.log);
    until_revision(2)?;
    let killed = std::process::Command::new("kill")
        .args(["-9", &w.pid.to_string()])
        .status()?;
    assert!(killed.success());
    let util = w.roles.as_ref().context("the roles")?;
    w.exited = Some(util.wait_for_exit(w.pid, Duration::from_secs(START_SECONDS))?);
    let _restarted: u32 = util.start_service_from_config(role_of_service(
        STORAGE_BINARY,
        "storage",
        &scratch("storage-after-crash.log"),
        answering(STORAGE_OBSERVABILITY, "/health"),
    ))?;
    Ok(())
}

#[then("it starts at the active revision, and its `/health` says so")]
fn active_after_crash(w: &mut Steps) -> TestResult {
    until_revision(1)?;
    w.roles = None;
    Ok(())
}

#[when("an active revision names a setting this release does not hold, and an instance restarts")]
fn named_too_early(w: &mut Steps) -> TestResult {
    let text = format!(
        "{}new_in_a_later_release = 1\n",
        render_settings(&[("storage.partition_count".to_string(), "1".to_string())])
    );
    storage_on(w, &text)
}

#[then("it exits with code 78 naming the setting")]
fn named_back(w: &mut Steps) -> TestResult {
    exit_is(w.exited, &w.log, 78)?;
    assert!(w.log.contains("new_in_a_later_release"), "{}", w.log);
    Ok(())
}

fn main() -> TestResult {
    start_double()?;
    futures::executor::block_on(
        Steps::cucumber()
            .max_concurrent_scenarios(1)
            .fail_on_skipped()
            .run_and_exit(ultragraphdb_tool_bdd::features_dir()?),
    );
    Ok(())
}
