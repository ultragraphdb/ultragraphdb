/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The role binaries' scenarios, bound to `features/role_binaries/`. One scenario runs at a time: the binaries listen at
//! their declared ports.

use std::io::{Read, Write as _};
use std::net::{TcpListener, TcpStream};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::ExitStatus;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::time::Duration;

use compio::runtime::Runtime;
use cucumber::{World, then, when};
use service_utils::{ServiceStartConfig, ServiceUtil, WaitStrategy};
use sha2::{Digest, Sha256};
use ultragraphdb_client_compio::{Client, ClientConfig, TcpConnector};
use ultragraphdb_cluster_placement::{NodeId, PlacementMap};
use ultragraphdb_common_role::RoleClient;
use ultragraphdb_common_service::{Bootstrap, RoleKind as ServiceKind, fetch_settings};
use ultragraphdb_core_ids::Epoch;
use ultragraphdb_protocol_scram::{ScramLogin, system_nonce};
use ultragraphdb_protocol_transport::Address;
use ultragraphdb_protocol_wire::{
    Bind, BindKind, BodyView, Command, Commit, CommitView, EpochCloseView, ErrorReplyView, Lease,
    LeaseView, Members, MembersView, Placement, PlacementView, RoleKind, Subscribe, TermTable,
    Write, WriteChange,
};
use ultragraphdb_store_quads::iri_bucket;
use ultragraphdb_tests::{
    CLIENT_ADDRESS, CLUSTER, COMPUTE_BINARY, COMPUTE_OBSERVABILITY, COMPUTE_ROLE_ADDRESS,
    FRONTEND_BINARY, FRONTEND_OBSERVABILITY, FRONTEND_ROLE_ADDRESS, STORAGE_BINARY,
    STORAGE_OBSERVABILITY, STORAGE_ROLE_ADDRESS, compute_on_one_host, frontend_on_one_host,
    http_body, http_status, render_settings, storage_on_one_host,
};
use ultragraphdb_tool_bdd::{Context, TestError, TestResult};
use ultragraphdb_transport_http_client::{HttpClient, HttpReply, HttpUrl};
use ultragraphdb_transport_tls::TestPki;

/// Where the sandbox holds the binaries, relative to the test's working directory: `copy_file` writes them to this package.
const ROOT_PATH: &str = "ultragraphdb/ultragraphdb_tests";

/// How long a role has to answer its first probe, or to exit. A start takes milliseconds; the margin is for a loaded machine.
const START_SECONDS: u64 = 20;

/// The role secret every role of a scenario holds.
const SECRET: &str = "the cluster's role secret";

/// How long a role refused by its peer retries, in a scenario that waits it out.
const SHORT_DEADLINE_MS: &str = "800";

static DIRECTORIES: AtomicU32 = AtomicU32::new(0);

/// The address the config service listens at, in a scenario starting it.
const CONFIG_ADDRESS: &str = "127.0.0.1:17090";

/// The address of the config double, which serves a scenario's settings for an instance as they stand: a partial or broken
/// target, which the service itself would rightly refuse to hold.
const DOUBLE_ADDRESS: &str = "127.0.0.1:17091";

/// The revision the double says it serves.
const DOUBLE_REVISION: u64 = 7;

/// The key a role reads settings with.
const READ_KEY: &str = "a read key of at least thirty two bytes long";

/// The key that changes settings, which no scenario here uses.
const ADMIN_KEY: &str = "an admin key of at least thirty two bytes long";

/// The address a loadable role's admin listener holds.
const ADMIN_ADDRESS: &str = "127.0.0.1:18090";

/// The binary of the config service.
const CONFIG_BINARY: &str = "ultragraphdb_config";

/// The target every scenario's roles belong to.
const TARGET: &str = "ci";

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
    code: Option<u8>,
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
    pid: u32,
    config_pid: u32,
    status: u16,
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
        vec![
            CONFIG_BINARY,
            FRONTEND_BINARY,
            COMPUTE_BINARY,
            STORAGE_BINARY,
        ],
    )?)
}

/// The file holding the read key, written once per run.
fn key_file() -> PathBuf {
    let file = scratch("read.key");
    // Absent, a role says it cannot read its key, which fails the scenario by name.
    drop(std::fs::write(&file, READ_KEY));
    file
}

/// The five variables of instance `instance`, asking the service at `address`.
fn bootstrap(address: &str, instance: &str) -> Vec<(String, String)> {
    [
        ("ULTRAGRAPHDB_CONFIG_URL", format!("http://{address}")),
        ("ULTRAGRAPHDB_CONFIG_TARGET", TARGET.to_string()),
        ("ULTRAGRAPHDB_CONFIG_INSTANCE", instance.to_string()),
        (
            "ULTRAGRAPHDB_CONFIG_KEY_FILE",
            key_file().display().to_string(),
        ),
    ]
    .map(|(name, value)| (name.to_string(), value))
    .to_vec()
}

/// What starts the binary `program` with `variables` alone in its environment, its log in the file `log`, waiting as
/// `wait_strategy` says.
fn process(
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

/// The role a binary plays, its name without the prefix.
fn role_of(program: &str) -> &str {
    program.strip_prefix("ultragraphdb_").unwrap_or(program)
}

/// The directory the double serves: one file per instance, named for it.
fn double_directory() -> PathBuf {
    scratch("config-double")
}

/// Has the double serve `text` as a fresh instance of `program`'s role, whose name is returned.
fn served_by_double(program: &str, text: &str) -> String {
    let at = DIRECTORIES.fetch_add(1, Ordering::Relaxed);
    let instance = format!("{}_{at}", role_of(program));
    // Absent, the double answers 404, and the role says so by name.
    drop(std::fs::write(double_directory().join(&instance), text));
    instance
}

/// What starts the binary `program` on `settings`, served by the double, its log in the file `log`, waiting as
/// `wait_strategy` says.
fn role(
    program: &'static str,
    settings: &[(String, String)],
    log: &Path,
    wait_strategy: WaitStrategy,
) -> ServiceStartConfig {
    let instance = served_by_double(program, &render_settings(settings));
    process(
        program,
        bootstrap(DOUBLE_ADDRESS, &instance),
        log,
        wait_strategy,
    )
}

/// What starts the binary `program` as `instance`, asking the config service.
fn role_of_service(
    program: &'static str,
    instance: &str,
    log: &Path,
    wait_strategy: WaitStrategy,
) -> ServiceStartConfig {
    process(
        program,
        bootstrap(CONFIG_ADDRESS, instance),
        log,
        wait_strategy,
    )
}

/// Writes target `TARGET` of `instances`, each a key and its settings, and a central file declaring them: the service's own
/// settings file, whose `[config]` table ends in `security`.
fn config_folder(instances: &[(&str, &[(String, String)])], security: &str) -> TestResult<PathBuf> {
    let folder = scratch(&format!(
        "config-{}",
        DIRECTORIES.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::create_dir_all(&folder)?;
    let mut central = format!("[{TARGET}]\n");
    for (key, settings) in instances {
        let file = format!("{key}.toml");
        std::fs::write(folder.join(&file), render_settings(settings))?;
        central.push_str(&format!("{key} = \"{file}\"\n"));
    }
    std::fs::write(folder.join("targets.toml"), central)?;
    std::fs::write(folder.join("admin.key"), ADMIN_KEY)?;
    let own = folder.join("service.toml");
    std::fs::write(
        &own,
        format!(
            "[config]\naddress = \"tcp://{CONFIG_ADDRESS}\"\n{security}\ndata_dir = \"{}\"\nread_key_file = \"{}\"\nadmin_key_file = \"{}\"\ntargets_file = \"{}\"\n",
            folder.join("data").display(),
            key_file().display(),
            folder.join("admin.key").display(),
            folder.join("targets.toml").display(),
        ),
    )?;
    Ok(own)
}

/// What starts the config service on its settings file `own`.
fn config_process(own: &Path, wait_strategy: WaitStrategy) -> ServiceStartConfig {
    // The arguments live as long as the test, as `ServiceStartConfig` asks: a test process is short.
    let path: &'static str = own.display().to_string().leak();
    ServiceStartConfig::builder()
        .program(CONFIG_BINARY)
        .program_args(vec!["--config", path])
        .env_vars(Vec::new())
        .clear_env(true)
        .stderr_file(scratch("config-service.log"))
        .wait_strategy(wait_strategy)
        .build()
}

/// Starts the config service over clear text, holding `instances`, and waits until it answers: its pid.
fn start_config_service(
    util: &ServiceUtil,
    instances: &[(&str, &[(String, String)])],
) -> TestResult<u32> {
    let own = config_folder(instances, "plaintext = true")?;
    let wait = answering(CONFIG_ADDRESS, "/health");
    Ok(util.start_service_from_config(config_process(&own, wait))?)
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
        holding(storage_on_one_host(), SECRET)?,
        "storage.data_dir",
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
        holding(frontend_on_one_host(), SECRET)?,
        "frontend.data_dir",
        &directory.display().to_string(),
    ))
}

/// `settings` with a role secret file holding `secret`.
fn holding(settings: Vec<(String, String)>, secret: &str) -> TestResult<Vec<(String, String)>> {
    let file = scratch(&format!("role-secret-{}", secret.len()));
    std::fs::write(&file, secret)?;
    Ok(with(
        settings,
        "cluster.role_secret_file",
        &file.display().to_string(),
    ))
}

/// Waits for `address` to answer `path` with success, for a role whose readiness comes after its start.
fn until_answering(address: &str, path: &str) -> TestResult {
    for _ in 0..START_SECONDS * 20 {
        // A role not yet listening is a role not yet answering.
        if http_status(address, path).is_ok_and(|status| status == 200) {
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
        let mut client = RoleClient::dial(&target, CLUSTER, "compute", SECRET).await?;
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
    frontend_settings: &[(String, String)],
    storage_settings: &[(String, String)],
    compute_settings: &[(String, String)],
) -> TestResult {
    let util = services()?;
    let _service: u32 = start_config_service(
        &util,
        &[
            ("frontend", frontend_settings),
            ("storage", storage_settings),
            ("compute", compute_settings),
        ],
    )?;
    let log = scratch(FRONTEND_BINARY);
    let wait = answering(FRONTEND_OBSERVABILITY, "/health");
    let _pid: u32 =
        util.start_service_from_config(role_of_service(FRONTEND_BINARY, "frontend", &log, wait))?;
    assert_eq!(
        http_status(FRONTEND_OBSERVABILITY, "/ready")?,
        503,
        "the frontend is ready only once a partition answers it"
    );
    for (program, instance, address) in [
        (STORAGE_BINARY, "storage", STORAGE_OBSERVABILITY),
        (COMPUTE_BINARY, "compute", COMPUTE_OBSERVABILITY),
    ] {
        let ready = answering(address, "/ready");
        let joined = role_of_service(program, instance, &scratch(program), ready);
        let _joined: u32 = util.start_service_from_config(joined)?;
    }
    w.roles = Some(util);
    Ok(())
}

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

fn exited_with(w: &Steps, code: i32) -> TestResult {
    let status = w.exited.context("an exit")?;
    assert_eq!(status.code(), Some(code), "{}", w.log);
    Ok(())
}

#[when("the frontend starts, then storage, then compute")]
fn start_all(w: &mut Steps) -> TestResult {
    w.frontend = format!("tcp://{}", FRONTEND_ROLE_ADDRESS);
    w.storage = format!("tcp://{}", STORAGE_ROLE_ADDRESS);
    w.compute = format!("tcp://{}", COMPUTE_ROLE_ADDRESS);
    start_in_order(
        w,
        &frontend_holding()?,
        &storage_holding()?,
        &holding(compute_on_one_host(), SECRET)?,
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
    let joining = "cluster.frontend_address";
    let frontend_settings = with(frontend_holding()?, "frontend.role_address", &f);
    let storage_settings = with(
        with(storage_holding()?, joining, &f),
        "storage.role_address",
        &st,
    );
    let compute_settings = with(
        with(compute_on_one_host(), joining, &f),
        "compute.role_address",
        &c,
    );
    (w.frontend, w.storage, w.compute) = (f, format!("unix://{}", partition_socket.display()), c);
    let [f_path, _, c_path] = paths;
    w.sockets = vec![f_path, partition_socket, c_path];
    start_in_order(
        w,
        &frontend_settings,
        &storage_settings,
        &holding(compute_settings, SECRET)?,
    )
}

#[then(
    "every role's `/ready` answers, the frontend's last, and the frontend's members name storage and compute"
)]
fn all_ready(w: &mut Steps) -> TestResult {
    assert_eq!(http_status(STORAGE_OBSERVABILITY, "/ready")?, 200);
    assert_eq!(http_status(COMPUTE_OBSERVABILITY, "/ready")?, 200);
    until_answering(FRONTEND_OBSERVABILITY, "/ready")?;
    assert_eq!(http_status(FRONTEND_OBSERVABILITY, "/other")?, 404);
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
        let address = CLIENT_ADDRESS.parse()?;
        let config = ClientConfig::new(CLUSTER, "ultragraphdb-test");
        let mut client = Client::new(TcpConnector::new(address), config, login, 16);
        let refused = client.connect().await.err().context("a refusal")?;
        Ok(refused.to_string())
    })
}

#[when("a client authenticates on the client endpoint with the role secret")]
fn client_with_secret(w: &mut Steps) -> TestResult {
    let util = services()?;
    let log = scratch(FRONTEND_BINARY);
    let wait = answering(FRONTEND_OBSERVABILITY, "/health");
    let settings = frontend_holding()?;
    let _pid: u32 = util.start_service_from_config(role(FRONTEND_BINARY, &settings, &log, wait))?;
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
    exited_with(w, 78)?;
    assert!(w.log.contains(&w.graph), "{}", w.log);
    Ok(())
}

/// Starts the frontend alone, waiting on its `/health`.
fn frontend_alone(w: &mut Steps) -> TestResult {
    let util = services()?;
    let log = scratch(FRONTEND_BINARY);
    let wait = answering(FRONTEND_OBSERVABILITY, "/health");
    let _pid: u32 =
        util.start_service_from_config(role(FRONTEND_BINARY, &frontend_holding()?, &log, wait))?;
    w.roles = Some(util);
    Ok(())
}

/// The frontend's role address, dialled as storage holding the cluster's secret.
async fn as_storage() -> TestResult<RoleClient> {
    let target = Address::parse(&format!("tcp://{}", FRONTEND_ROLE_ADDRESS))?;
    Ok(RoleClient::dial(&target, CLUSTER, "storage", SECRET).await?)
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
    let mut client = RoleClient::dial(&target, CLUSTER, "frontend", SECRET).await?;
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
    for at in [1, 3] {
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
    exited_with(w, 0)
}

#[when(
    "the storage binary receives settings whose `[storage]` table holds the key `partiton_count`"
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

#[when("the storage binary receives a setting out of its bounds")]
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

#[when("the storage binary receives settings with no `storage.data_dir`")]
fn no_data_dir(w: &mut Steps) -> TestResult {
    let text = render_settings(&holding(storage_on_one_host(), SECRET)?);
    storage_on(w, &text)
}

#[then("it exits with code 78 and an error naming `storage.data_dir`")]
fn names_data_dir(w: &mut Steps) -> TestResult {
    exited_with(w, 78)?;
    assert!(w.log.contains("storage.data_dir"), "{}", w.log);
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
    exited_with(w, 78)?;
    assert!(w.log.contains("compute.role_address"), "{}", w.log);
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
    exited_with(w, 78)?;
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
    exited_with(w, 78)?;
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
    exited_with(w, 78)?;
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
    exited_with(w, 78)?;
    assert!(w.log.contains("new_in_a_later_release"), "{}", w.log);
    Ok(())
}

/// Answers one request of the double: the file named for the instance the request's path ends in, or 404.
fn answer_double(mut stream: TcpStream) -> std::io::Result<()> {
    let mut held = Vec::new();
    let mut chunk = [0; 1024];
    while !held.windows(4).any(|end| end == b"\r\n\r\n") {
        let read = stream.read(&mut chunk)?;
        if read == 0 {
            return Ok(());
        }
        held.extend_from_slice(chunk.get(..read).unwrap_or_default());
    }
    let head = String::from_utf8_lossy(&held);
    let instance = head
        .split_whitespace()
        .nth(1)
        .and_then(|path| path.rsplit('/').next())
        .unwrap_or_default();
    let answer = match std::fs::read(double_directory().join(instance)) {
        Ok(body) => [
            format!(
                "HTTP/1.1 200 OK\r\nX-Revision: {DOUBLE_REVISION}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                body.len()
            )
            .into_bytes(),
            body,
        ]
        .concat(),
        Err(_) => b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".to_vec(),
    };
    stream.write_all(&answer)
}

/// Starts the double, for the life of the test process.
#[expect(
    clippy::disallowed_methods,
    reason = "the double answers beside the scenarios, on a thread of the test's own"
)]
fn start_double() -> TestResult {
    std::fs::create_dir_all(double_directory())?;
    let listener = TcpListener::bind(DOUBLE_ADDRESS)?;
    drop(std::thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            // A connection failing costs that request alone, and its role says so.
            drop(answer_double(stream));
        }
    }));
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
