/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The role binaries' scenarios, bound to `features/role_binaries/`. One scenario runs at a time: the binaries listen at
//! their declared ports.

use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::ExitStatus;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Duration;

use compio::runtime::Runtime;
use cucumber::{World, then, when};
use service_utils::{ServiceStartConfig, ServiceUtil, WaitStrategy};
use ultragraphdb_client_compio::{Client, ClientConfig, TcpConnector};
use ultragraphdb_cluster_placement::{NodeId, PlacementMap};
use ultragraphdb_common_role::RoleClient;
use ultragraphdb_compute_specs as compute;
use ultragraphdb_core_ids::Epoch;
use ultragraphdb_frontend_specs as frontend;
use ultragraphdb_protocol_scram::{ScramLogin, system_nonce};
use ultragraphdb_protocol_transport::Address;
use ultragraphdb_protocol_wire::{
    Bind, BindKind, BodyView, Command, Commit, CommitView, EpochCloseView, ErrorReplyView, Lease,
    LeaseView, Members, MembersView, Placement, PlacementView, RoleKind, Subscribe, TermTable,
    Write, WriteChange,
};
use ultragraphdb_storage_specs as storage;
use ultragraphdb_store_quads::iri_bucket;
use ultragraphdb_tests::http_status;
use ultragraphdb_tool_bdd::{Context, TestError, TestResult};

/// Where the sandbox holds the binaries, relative to the test's working directory: `copy_file` writes them to this package.
const ROOT_PATH: &str = "ultragraphdb/ultragraphdb_tests";

/// How long a role has to answer its first probe, or to exit. A start takes milliseconds; the margin is for a loaded machine.
const START_SECONDS: u64 = 20;

/// The role secret every role of a scenario holds.
const SECRET: &str = "the cluster's role secret";

/// How long a role refused by its peer retries, in a scenario that waits it out.
const SHORT_DEADLINE_MS: &str = "800";

static DIRECTORIES: AtomicU32 = AtomicU32::new(0);

const NAMESPACE: u64 = 1;
const PREDICATE: &str = "http://ex.org/p";

const GRAPHS: [&str; 2] = [
    "ultragraphdb/ultragraphdb_tests/frontend_deps",
    "ultragraphdb/ultragraphdb_tests/compute_deps",
];

#[derive(Debug, Default, World)]
struct Steps {
    roles: Option<ServiceUtil>,
    exited: Option<ExitStatus>,
    log: String,
    graph: String,
    frontend: String,
    storage: String,
    compute: String,
    sockets: Vec<PathBuf>,
    epochs: Vec<u64>,
    leased: Option<(u64, u64)>,
    ranges: usize,
    version: u64,
}

/// The file in the test's own directory named `name`.
fn scratch(name: &str) -> PathBuf {
    std::env::var_os("TEST_TMPDIR")
        .map_or_else(std::env::temp_dir, PathBuf::from)
        .join(format!("{name}-{}", std::process::id()))
}

fn services() -> TestResult<ServiceUtil> {
    Ok(ServiceUtil::new(
        ROOT_PATH,
        vec![frontend::BINARY, compute::BINARY, storage::BINARY],
    )?)
}

/// What starts the binary `program` with `variables` alone in its environment, so nothing of the test's own reaches its
/// settings, its log in the file `log`, waiting as `wait_strategy` says.
fn role(
    program: &'static str,
    variables: Vec<(String, String)>,
    log: &Path,
    wait_strategy: WaitStrategy,
) -> ServiceStartConfig {
    ServiceStartConfig::builder()
        .program(program)
        .env_vars(variables)
        .clear_env(true)
        .stderr_file(log)
        .wait_strategy(wait_strategy)
        .build()
}

/// A wait for `address` to answer `path` with success.
fn answering(address: &str, path: &str) -> WaitStrategy {
    WaitStrategy::WaitForHttpHealthCheck(format!("http://{address}{path}"), START_SECONDS)
}

/// `settings` with the setting `name` holding `value`, in place of what it held.
fn with(mut settings: Vec<(String, String)>, name: &str, value: &str) -> Vec<(String, String)> {
    settings.retain(|(held, _)| held != name);
    settings.push((name.to_string(), value.to_string()));
    settings
}

/// A data directory no other start of this run names.
fn fresh_data() -> String {
    scratch(&format!(
        "data-{}",
        DIRECTORIES.fetch_add(1, Ordering::Relaxed)
    ))
    .display()
    .to_string()
}

/// A storage process's settings, with a role secret and a data directory of its own.
fn storage_holding() -> TestResult<Vec<(String, String)>> {
    Ok(with(
        holding(storage::settings(), SECRET)?,
        "ULTRAGRAPHDB_STORAGE_DATA_DIR",
        &fresh_data(),
    ))
}

/// The frontend's settings, with a role secret and a data directory of its own.
fn frontend_holding() -> TestResult<Vec<(String, String)>> {
    let directory = scratch(&format!(
        "frontend-{}",
        DIRECTORIES.fetch_add(1, Ordering::Relaxed)
    ));
    Ok(with(
        holding(frontend::settings(), SECRET)?,
        "ULTRAGRAPHDB_FRONTEND_DATA_DIR",
        &directory.display().to_string(),
    ))
}

/// `settings` with a role secret file holding `secret`.
fn holding(settings: Vec<(String, String)>, secret: &str) -> TestResult<Vec<(String, String)>> {
    let file = scratch(&format!("role-secret-{}", secret.len()));
    std::fs::write(&file, secret)?;
    Ok(with(
        settings,
        "ULTRAGRAPHDB_CLUSTER_ROLE_SECRET_FILE",
        &file.display().to_string(),
    ))
}

/// Waits for `address` to answer `path` with success, for a role whose readiness comes after its start.
fn until_answering(address: &str, path: &str) -> TestResult {
    for _ in 0..START_SECONDS * 20 {
        if http_status(address, path)? == 200 {
            return Ok(());
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    Err(TestError::new(format!("{address}{path} never answered")))
}

/// What the frontend at `address` lists: each member's kind, partition and address.
fn members_at(address: &str) -> TestResult<Vec<(RoleKind, u32, String)>> {
    Runtime::new()?.block_on(async {
        let target = Address::parse(address)?;
        let mut client = RoleClient::dial(&target, frontend::CLUSTER, "compute", SECRET).await?;
        let reply = client.request(&Members::new()).await?;
        let view = reply.view()?;
        let members = MembersView::from_frame(&view)?;
        (0..members.len())
            .map(|index| {
                let member = members.member(index).context("a member")?;
                let held = members
                    .address(member.address() as usize)
                    .context("its address")?;
                Ok((member.kind(), member.partition(), held.to_string()))
            })
            .collect()
    })
}

/// Starts the frontend, storage and compute in the order their readiness needs, on the settings given.
fn start_in_order(
    w: &mut Steps,
    frontend_settings: Vec<(String, String)>,
    storage_settings: Vec<(String, String)>,
    compute_settings: Vec<(String, String)>,
) -> TestResult {
    let util = services()?;
    let log = scratch(frontend::BINARY);
    let wait = answering(frontend::OBSERVABILITY, "/health");
    let _pid: u32 =
        util.start_service_from_config(role(frontend::BINARY, frontend_settings, &log, wait))?;
    assert_eq!(
        http_status(frontend::OBSERVABILITY, "/ready")?,
        503,
        "the frontend is ready only once a partition answers it"
    );
    for (program, variables, address) in [
        (storage::BINARY, storage_settings, storage::OBSERVABILITY),
        (compute::BINARY, compute_settings, compute::OBSERVABILITY),
    ] {
        let ready = answering(address, "/ready");
        let joined = role(program, variables, &scratch(program), ready);
        let _joined: u32 = util.start_service_from_config(joined)?;
    }
    w.roles = Some(util);
    Ok(())
}

/// Runs the storage binary on `text` to its exit.
fn storage_on(w: &mut Steps, text: &str) -> TestResult {
    let file = scratch("settings.toml");
    std::fs::write(&file, text)?;
    // The arguments live as long as the test, as `ServiceStartConfig` asks: a test process is short.
    let path: &'static str = file.display().to_string().leak();
    let log = scratch("storage-refused.log");
    let config = ServiceStartConfig::builder()
        .program(storage::BINARY)
        .program_args(vec!["--config", path])
        .env_vars(storage::settings())
        .clear_env(true)
        .stderr_file(&log)
        .wait_strategy(WaitStrategy::NoWait)
        .build();
    let util = services()?;
    let pid = util.start_service_from_config(config)?;
    w.exited = Some(util.wait_for_exit(pid, Duration::from_secs(START_SECONDS))?);
    w.log = std::fs::read_to_string(&log)?;
    Ok(())
}

fn exited_with(w: &Steps, code: i32) -> TestResult {
    let status = w.exited.context("an exit")?;
    assert_eq!(status.code(), Some(code), "{}", w.log);
    Ok(())
}

#[when("the frontend starts, then storage, then compute")]
fn start_all(w: &mut Steps) -> TestResult {
    w.frontend = format!("tcp://{}", frontend::ROLE_ADDRESS);
    w.storage = format!("tcp://{}", storage::ROLE_ADDRESS);
    w.compute = format!("tcp://{}", compute::ROLE_ADDRESS);
    start_in_order(
        w,
        frontend_holding()?,
        storage_holding()?,
        holding(compute::settings(), SECRET)?,
    )
}

#[when("the frontend starts, then storage, then compute, each at a Unix socket")]
fn start_all_unix(w: &mut Steps) -> TestResult {
    // A socket's path has a short limit, which the sandbox's own directory passes.
    let socket =
        |name: &str| PathBuf::from(format!("/tmp/ugdb-{name}-{}.sock", std::process::id()));
    let paths = [socket("f"), socket("s"), socket("c")];
    let [f, st, c] = paths
        .each_ref()
        .map(|path| format!("unix://{}", path.display()));
    let partition_socket = PathBuf::from(format!("/tmp/ugdb-s-{}-p1.sock", std::process::id()));
    let joining = "ULTRAGRAPHDB_CLUSTER_FRONTEND_ADDRESS";
    let frontend_settings = with(
        frontend_holding()?,
        "ULTRAGRAPHDB_FRONTEND_ROLE_ADDRESS",
        &f,
    );
    let storage_settings = with(
        with(storage_holding()?, joining, &f),
        "ULTRAGRAPHDB_STORAGE_ROLE_ADDRESS",
        &st,
    );
    let compute_settings = with(
        with(compute::settings(), joining, &f),
        "ULTRAGRAPHDB_COMPUTE_ROLE_ADDRESS",
        &c,
    );
    (w.frontend, w.storage, w.compute) = (f, format!("unix://{}", partition_socket.display()), c);
    let [f_path, _, c_path] = paths;
    w.sockets = vec![f_path, partition_socket, c_path];
    start_in_order(
        w,
        frontend_settings,
        storage_settings,
        holding(compute_settings, SECRET)?,
    )
}

#[then(
    "every role's `/ready` answers, the frontend's last, and the frontend's members name storage and compute"
)]
fn all_ready(w: &mut Steps) -> TestResult {
    assert_eq!(http_status(storage::OBSERVABILITY, "/ready")?, 200);
    assert_eq!(http_status(compute::OBSERVABILITY, "/ready")?, 200);
    until_answering(frontend::OBSERVABILITY, "/ready")?;
    assert_eq!(http_status(frontend::OBSERVABILITY, "/other")?, 404);
    let expected = vec![
        (RoleKind::Compute, 0, w.compute.clone()),
        (RoleKind::Storage, 1, w.storage.clone()),
    ];
    assert_eq!(members_at(&w.frontend)?, expected);
    Ok(())
}

#[then("each role's socket has mode 0600")]
fn private_sockets(w: &mut Steps) -> TestResult {
    for socket in &w.sockets {
        let mode = std::fs::metadata(socket)?.permissions().mode() & 0o777;
        assert_eq!(mode, 0o600, "{}", socket.display());
    }
    w.roles = None;
    for socket in &w.sockets {
        std::fs::remove_file(socket)?;
    }
    Ok(())
}

#[when("a storage process started with a different role secret connects to the frontend")]
fn wrong_secret(w: &mut Steps) -> TestResult {
    let util = services()?;
    let log = scratch(frontend::BINARY);
    let wait = answering(frontend::OBSERVABILITY, "/health");
    let settings = frontend_holding()?;
    let _frontend: u32 =
        util.start_service_from_config(role(frontend::BINARY, settings, &log, wait))?;
    let refused = with(
        with(
            holding(storage::settings(), "another secret")?,
            "ULTRAGRAPHDB_STORAGE_DATA_DIR",
            &fresh_data(),
        ),
        "ULTRAGRAPHDB_CLUSTER_JOIN_DEADLINE_MS",
        SHORT_DEADLINE_MS,
    );
    let refused_log = scratch("storage-refused.log");
    let pid = util.start_service_from_config(role(
        storage::BINARY,
        refused,
        &refused_log,
        WaitStrategy::NoWait,
    ))?;
    w.exited = Some(util.wait_for_exit(pid, Duration::from_secs(START_SECONDS))?);
    w.log = std::fs::read_to_string(&refused_log)?;
    w.frontend = format!("tcp://{}", frontend::ROLE_ADDRESS);
    w.roles = Some(util);
    Ok(())
}

#[then(
    "its authentication is refused, it sends no role command, and it retries until its deadline"
)]
fn refused_and_retried(w: &mut Steps) -> TestResult {
    exited_with(w, 69)?;
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

/// A client's refusal as it reads it, logging in as `name` with `password`.
fn client_refusal(name: &str, password: &str) -> TestResult<String> {
    Runtime::new()?.block_on(async {
        let login = ScramLogin::new(name, password, system_nonce)?;
        let address = frontend::CLIENT_ADDRESS.parse()?;
        let config = ClientConfig::new(frontend::CLUSTER, "ultragraphdb-test");
        let mut client = Client::new(TcpConnector::new(address), config, login, 16);
        let refused = client.connect().await.err().context("a refusal")?;
        Ok(refused.to_string())
    })
}

#[when("a client authenticates on the client endpoint with the role secret")]
fn client_with_secret(w: &mut Steps) -> TestResult {
    let util = services()?;
    let log = scratch(frontend::BINARY);
    let wait = answering(frontend::OBSERVABILITY, "/health");
    let settings = frontend_holding()?;
    let _pid: u32 = util.start_service_from_config(role(frontend::BINARY, settings, &log, wait))?;
    w.log = client_refusal("storage", SECRET)?;
    w.roles = Some(util);
    Ok(())
}

#[then("authentication is refused with the same error as a wrong password")]
fn same_as_wrong_password(w: &mut Steps) -> TestResult {
    assert!(w.log.contains("authentication failed"), "{}", w.log);
    assert_eq!(w.log, client_refusal("storage", "a wrong password")?);
    w.roles = None;
    Ok(())
}

#[when("a role starts with `role_secret_file` naming a file that does not exist")]
fn missing_secret(w: &mut Steps) -> TestResult {
    let absent = scratch("never-written");
    let settings = with(
        with(
            storage::settings(),
            "ULTRAGRAPHDB_STORAGE_DATA_DIR",
            &fresh_data(),
        ),
        "ULTRAGRAPHDB_CLUSTER_ROLE_SECRET_FILE",
        &absent.display().to_string(),
    );
    let log = scratch("storage-no-secret.log");
    let util = services()?;
    let pid = util.start_service_from_config(role(
        storage::BINARY,
        settings,
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
    exited_with(w, 78)?;
    assert!(w.log.contains(&w.graph), "{}", w.log);
    Ok(())
}

/// Starts the frontend alone, waiting on its `/health`.
fn frontend_alone(w: &mut Steps) -> TestResult {
    let util = services()?;
    let log = scratch(frontend::BINARY);
    let wait = answering(frontend::OBSERVABILITY, "/health");
    let _pid: u32 =
        util.start_service_from_config(role(frontend::BINARY, frontend_holding()?, &log, wait))?;
    w.roles = Some(util);
    Ok(())
}

/// The frontend's role address, dialled as storage holding the cluster's secret.
async fn as_storage() -> TestResult<RoleClient> {
    let target = Address::parse(&format!("tcp://{}", frontend::ROLE_ADDRESS))?;
    Ok(RoleClient::dial(&target, frontend::CLUSTER, "storage", SECRET).await?)
}

#[when("the frontend's sequencer closes epochs 1 to 5 and storage is connected")]
fn subscribed(w: &mut Steps) -> TestResult {
    frontend_alone(w)?;
    Runtime::new()?.block_on(async {
        let mut client = as_storage().await?;
        let first = client.request(&Subscribe::new(0)).await?;
        let mut note = |frame: &ultragraphdb_protocol_transport::ReceivedFrame| -> TestResult {
            let view = frame.view()?;
            w.epochs.push(EpochCloseView::from_frame(&view)?.epoch());
            Ok(())
        };
        note(&first)?;
        drop(first);
        for _ in 1..5 {
            let next = client.next_frame().await?;
            note(&next)?;
        }
        Ok(())
    })
}

#[then("storage receives five epoch-close commands, in order")]
fn in_order(w: &mut Steps) {
    assert_eq!(w.epochs, [1, 2, 3, 4, 5]);
    w.roles = None;
}

#[when("a partition asks the frontend for a block and the placement")]
fn lease_and_placement(w: &mut Steps) -> TestResult {
    frontend_alone(w)?;
    Runtime::new()?.block_on(async {
        let mut client = as_storage().await?;
        let leased = client.request(&Lease::request(1, 1, 16)).await?;
        let view = leased.view()?;
        let block = LeaseView::from_frame(&view)?;
        w.leased = Some((block.start(), block.end()));
        drop(leased);
        let placed = client.request(&Placement::request(0)).await?;
        w.ranges = PlacementView::from_frame(&placed.view()?)?.range_count();
        Ok(())
    })
}

#[then("the frontend answers both over the role connection")]
fn answered_both(w: &mut Steps) -> TestResult {
    let (start, end) = w.leased.context("a block")?;
    assert_eq!(end - start, 16);
    assert_eq!(w.ranges, 1, "one partition holds every bucket");
    w.roles = None;
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

/// Commits `write` in a transaction bound to `partition` at `address`: its commit version.
async fn commit_at(address: &str, partition: u32, write: &Write) -> TestResult<u64> {
    let target = Address::parse(address)?;
    let mut client = RoleClient::dial(&target, frontend::CLUSTER, "frontend", SECRET).await?;
    let bind = Bind::new(partition, BindKind::Transaction, NAMESPACE, 0, 1, [7; 16]);
    drop(client.request(&bind).await?);
    let stream = client.last_stream();
    client.send_on(stream, write).await?;
    client.send_on(stream, &Commit::new([0; 16], 0, 0)).await?;
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
    let mut settings = with(
        storage_holding()?,
        "ULTRAGRAPHDB_OBSERVABILITY_ADDRESS",
        &observed,
    );
    settings = with(
        settings,
        "ULTRAGRAPHDB_STORAGE_PARTITION_FIRST",
        &first.to_string(),
    );
    settings = with(
        settings,
        "ULTRAGRAPHDB_STORAGE_PARTITION_COUNT",
        &count.to_string(),
    );
    Ok(with(
        settings,
        "ULTRAGRAPHDB_STORAGE_ROLE_ADDRESS",
        &format!("tcp://127.0.0.1:{port}"),
    ))
}

/// A frontend over 4 partitions and two storage processes hosting 1 and 2, and 3 and 4.
fn two_processes(w: &mut Steps) -> TestResult {
    let util = services()?;
    let settings = with(
        frontend_holding()?,
        "ULTRAGRAPHDB_CLUSTER_PARTITION_COUNT",
        "4",
    );
    let wait = answering(frontend::OBSERVABILITY, "/health");
    let _frontend: u32 = util.start_service_from_config(role(
        frontend::BINARY,
        settings,
        &scratch(frontend::BINARY),
        wait,
    ))?;
    for (first, port) in [(1, 17_200), (3, 17_300)] {
        let ready = answering(&format!("127.0.0.1:{}", 18_100 + first), "/ready");
        let start = role(
            storage::BINARY,
            storage_of(first, 2, port)?,
            &scratch(&format!("storage-{first}")),
            ready,
        );
        let _storage: u32 = util.start_service_from_config(start)?;
    }
    w.roles = Some(util);
    w.frontend = format!("tcp://{}", frontend::ROLE_ADDRESS);
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
        }
        Ok::<(), TestError>(())
    })?;
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
fn start_storage(
    util: &ServiceUtil,
    settings: Vec<(String, String)>,
    log: &Path,
) -> TestResult<u32> {
    let ready = answering(storage::OBSERVABILITY, "/ready");
    Ok(util.start_service_from_config(role(storage::BINARY, settings, log, ready))?)
}

/// What runs a storage process with commits acknowledged, its settings and its pid.
type Committed = (ServiceUtil, Vec<(String, String)>, u32);

/// A frontend and one partition's storage, with commits acknowledged: what restarts it, its log and its pid.
fn committed_storage(w: &mut Steps) -> TestResult<Committed> {
    let util = services()?;
    let wait = answering(frontend::OBSERVABILITY, "/health");
    let _frontend: u32 = util.start_service_from_config(role(
        frontend::BINARY,
        frontend_holding()?,
        &scratch(frontend::BINARY),
        wait,
    ))?;
    let settings = storage_holding()?;
    let pid = start_storage(&util, settings.clone(), &scratch("storage-first"))?;
    let subject = subject_of(1, 1, NAMESPACE)?;
    let address = format!("tcp://{}", storage::ROLE_ADDRESS);
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
    let _restarted: u32 = start_storage(&util, settings, &again)?;
    w.log = std::fs::read_to_string(&again)?;
    w.roles = Some(util);
    Ok(())
}

#[then(
    "every acknowledged commit is applied to its store before it exits, and a restart replays nothing"
)]
fn nothing_to_replay(w: &mut Steps) -> TestResult {
    exited_with(w, 0)?;
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
    let health = answering(storage::OBSERVABILITY, "/health");
    let _restarted: u32 =
        util.start_service_from_config(role(storage::BINARY, settings, &again, health))?;
    until_answering(storage::OBSERVABILITY, "/ready")?;
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

#[when("a role is running and receives SIGTERM")]
fn terminate(w: &mut Steps) -> TestResult {
    let util = services()?;
    let log = scratch("compute-stopped.log");
    let wait = answering(compute::OBSERVABILITY, "/health");
    let settings = holding(compute::settings(), SECRET)?;
    let pid = util.start_service_from_config(role(compute::BINARY, settings, &log, wait))?;
    w.exited = Some(util.stop_service(pid)?);
    w.log = std::fs::read_to_string(&log)?;
    Ok(())
}

#[then("it exits with code 0")]
fn exits_clean(w: &mut Steps) -> TestResult {
    exited_with(w, 0)
}

#[when(
    "the storage binary starts with a TOML file whose `[storage]` table holds the key `partiton_count`"
)]
fn unknown_setting(w: &mut Steps) -> TestResult {
    storage_on(w, "[storage]\npartiton_count = 4\n")
}

#[then("it exits with code 78 and an error naming `partiton_count`")]
fn names_the_key(w: &mut Steps) -> TestResult {
    exited_with(w, 78)?;
    assert!(w.log.contains("partiton_count"), "{}", w.log);
    Ok(())
}

#[when("the storage binary starts with a setting out of its bounds")]
fn out_of_bounds(w: &mut Steps) -> TestResult {
    storage_on(w, "[storage]\npartition_first = -1\n")
}

#[then("it exits with code 78 and an error naming the setting and the bound")]
fn names_the_bound(w: &mut Steps) -> TestResult {
    exited_with(w, 78)?;
    assert!(w.log.contains("storage.partition_first"), "{}", w.log);
    assert!(w.log.contains(&u32::MAX.to_string()), "{}", w.log);
    Ok(())
}

#[when("a role starts with `pin_cores = true`")]
fn pinned(w: &mut Steps) -> TestResult {
    let mut variables = storage_holding()?;
    variables.push((
        "ULTRAGRAPHDB_RUNTIME_PIN_CORES".to_string(),
        "true".to_string(),
    ));
    let util = services()?;
    let log = scratch("storage-pinned.log");
    let wait = answering(storage::OBSERVABILITY, "/health");
    let _pid: u32 = util.start_service_from_config(role(storage::BINARY, variables, &log, wait))?;
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

#[when("the frontend binary's dependency graph is listed")]
fn frontend_graph(w: &mut Steps) -> TestResult {
    let graph = GRAPHS.first().context("the frontend's graph")?;
    w.graph = std::fs::read_to_string(graph)?;
    Ok(())
}

#[when("the compute binary's dependency graph is listed")]
fn compute_graph(w: &mut Steps) -> TestResult {
    let graph = GRAPHS.get(1).context("compute's graph")?;
    w.graph = std::fs::read_to_string(graph)?;
    Ok(())
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

fn main() -> TestResult {
    futures::executor::block_on(
        Steps::cucumber()
            .max_concurrent_scenarios(1)
            .fail_on_skipped()
            .run_and_exit(ultragraphdb_tool_bdd::features_dir()?),
    );
    Ok(())
}
