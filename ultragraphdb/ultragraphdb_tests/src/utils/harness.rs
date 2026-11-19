/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! What each process test shares: where a process's settings come from, the config service and its double, and waiting on a
//! process.

use std::io::{Read, Write as _};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::ExitStatus;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Duration;

use compio::runtime::Runtime;
use service_utils::{ServiceStartConfig, ServiceUtil, WaitStrategy};
use ultragraphdb_common_role::RoleClient;
use ultragraphdb_protocol_transport::Address;
use ultragraphdb_protocol_wire::{BodyView, Members, MembersView, RoleKind};
use ultragraphdb_tool_bdd::{Context, TestError, TestResult, loopback};

use crate::utils::http_status::http_status;
use crate::utils::one_host::{
    CLUSTER, CONFIG_OFFSET, DOUBLE_OFFSET, client_address, compute_observability,
    compute_role_address, frontend_observability, frontend_on_one_host, render_settings,
    storage_observability, storage_on_one_host, storage_role_address, system_role_address,
};

/// How long a role has to answer its first probe, or to exit. A start takes milliseconds; the margin is for a loaded machine.
pub const START_SECONDS: u64 = 20;

/// The role secret every role of a scenario holds.
pub const SECRET: &str = "the cluster's role secret";

// `Relaxed` suffices: the counter only keeps names apart, and the changes to one atomic fall in one order.
static DIRECTORIES: AtomicU32 = AtomicU32::new(0);

/// The variable naming the process a role ends with: the test process that starts it.
pub const PARENT_PID: &str = "ULTRAGRAPHDB_PARENT_PID";

/// The address the config service listens at, in a scenario starting it.
///
/// # Errors
///
/// When the target sets no port base, as `ultragraphdb_tool_bdd::port` says.
pub fn config_address() -> TestResult<String> {
    loopback(CONFIG_OFFSET)
}

/// The address of the config double, which serves a scenario's settings for an instance as they stand: a partial or broken
/// target, which the service itself would rightly refuse to hold.
///
/// # Errors
///
/// As [`config_address`].
pub fn double_address() -> TestResult<String> {
    loopback(DOUBLE_OFFSET)
}

/// The revision the double says it serves.
pub const DOUBLE_REVISION: u64 = 7;

/// The key a role reads settings with.
pub const READ_KEY: &str = "a read key of at least thirty two bytes long";

/// The key that changes settings, which no scenario here uses.
pub const ADMIN_KEY: &str = "an admin key of at least thirty two bytes long";

/// The binary of the config service.
pub const CONFIG_BINARY: &str = "ultragraphdb_config";

/// The target every scenario's roles belong to.
pub const TARGET: &str = "ci";

/// The file in the test's own directory named `name`.
pub fn scratch(name: &str) -> PathBuf {
    std::env::var_os("TEST_TMPDIR")
        .map_or_else(std::env::temp_dir, PathBuf::from)
        .join(format!("{name}-{}", std::process::id()))
}

/// The file holding the read key, written whole on each call.
pub fn key_file() -> PathBuf {
    let file = scratch("read.key");
    // Absent, a role says it cannot read its key, which fails the scenario by name.
    drop(replace(&file, READ_KEY));
    file
}

/// Writes `contents` to `file`, a scratch file, whole: into a scratch file of its own, which then takes `file`'s name. A
/// process reading `file` meanwhile, such as a config service started a moment before, finds what it held or `contents`,
/// never the empty file a write in place leaves between its truncation and its write.
fn replace(file: &Path, contents: &str) -> std::io::Result<()> {
    let staged = scratch(&format!(
        "staged-{}",
        DIRECTORIES.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::write(&staged, contents)?;
    std::fs::rename(staged, file)
}

/// The five variables of instance `instance`, asking the service at `address`.
pub fn bootstrap(address: &str, instance: &str) -> Vec<(String, String)> {
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
pub fn process(
    program: &'static str,
    variables: Vec<(String, String)>,
    log: &Path,
    wait_strategy: WaitStrategy,
) -> ServiceStartConfig {
    // The role ends with the test process, which Bazel may kill before any drop stops the role, so a killed run leaves no port
    // held. A scenario's own value comes after, and the last of a name is what a process holds.
    let parent = (PARENT_PID.to_string(), std::process::id().to_string());
    ServiceStartConfig::builder()
        .program(program)
        .env_vars([vec![parent], variables].concat())
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
pub fn served_by_double(program: &str, text: &str) -> String {
    let at = DIRECTORIES.fetch_add(1, Ordering::Relaxed);
    let instance = format!("{}_{at}", role_of(program));
    // Absent, the double answers 404, and the role says so by name.
    drop(std::fs::write(double_directory().join(&instance), text));
    instance
}

/// What starts the binary `program` on `settings`, served by the double, its log in the file `log`, waiting as
/// `wait_strategy` says.
///
/// # Errors
///
/// As [`double_address`].
pub fn role(
    program: &'static str,
    settings: &[(String, String)],
    log: &Path,
    wait_strategy: WaitStrategy,
) -> TestResult<ServiceStartConfig> {
    let instance = served_by_double(program, &render_settings(settings));
    Ok(process(
        program,
        bootstrap(&double_address()?, &instance),
        log,
        wait_strategy,
    ))
}

/// What starts the binary `program` as `instance`, asking the config service.
///
/// # Errors
///
/// As [`config_address`].
pub fn role_of_service(
    program: &'static str,
    instance: &str,
    log: &Path,
    wait_strategy: WaitStrategy,
) -> TestResult<ServiceStartConfig> {
    Ok(process(
        program,
        bootstrap(&config_address()?, instance),
        log,
        wait_strategy,
    ))
}

/// Writes target `TARGET` of `instances`, each a key and its settings, and a central file declaring them: the service's own
/// settings file, whose `[config]` table ends in `security`.
///
/// # Errors
///
/// When the target sets no port base, or a file of the folder cannot be written.
pub fn config_folder(
    instances: &[(&str, &[(String, String)])],
    security: &str,
) -> TestResult<PathBuf> {
    config_folder_at(&config_address()?, instances, security)
}

/// As [`config_folder`], for a config service listening at `address`.
///
/// # Errors
///
/// When a file of the folder cannot be written.
pub fn config_folder_at(
    address: &str,
    instances: &[(&str, &[(String, String)])],
    security: &str,
) -> TestResult<PathBuf> {
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
            "[config]\naddress = \"tcp://{address}\"\n{security}\ndata_dir = \"{}\"\nread_key_file = \"{}\"\nadmin_key_file = \"{}\"\ntargets_file = \"{}\"\n",
            folder.join("data").display(),
            key_file().display(),
            folder.join("admin.key").display(),
            folder.join("targets.toml").display(),
        ),
    )?;
    Ok(own)
}

/// What starts the config service on its settings file `own`.
pub fn config_process(own: &Path, wait_strategy: WaitStrategy) -> ServiceStartConfig {
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
///
/// # Errors
///
/// When the target sets no port base, the folder cannot be written, or the service does not start and answer `/health` within
/// `START_SECONDS`.
pub fn start_config_service(
    util: &ServiceUtil,
    instances: &[(&str, &[(String, String)])],
) -> TestResult<u32> {
    start_config_service_at(util, &config_address()?, instances)
}

/// As [`start_config_service`], the service listening at `address`.
///
/// # Errors
///
/// When the folder cannot be written, or the service does not start and answer `/health` within `START_SECONDS`.
pub fn start_config_service_at(
    util: &ServiceUtil,
    address: &str,
    instances: &[(&str, &[(String, String)])],
) -> TestResult<u32> {
    let own = config_folder_at(address, instances, "plaintext = true")?;
    let wait = answering(address, "/health");
    Ok(util.start_service_from_config(config_process(&own, wait))?)
}

/// A wait for `address` to answer `path` with success.
pub fn answering(address: &str, path: &str) -> WaitStrategy {
    WaitStrategy::WaitForHttpHealthCheck(format!("http://{address}{path}"), START_SECONDS)
}

/// `settings` with the setting `name` holding `value`, in place of what it held.
pub fn with(mut settings: Vec<(String, String)>, name: &str, value: &str) -> Vec<(String, String)> {
    settings.retain(|(held, _)| held != name);
    settings.push((name.to_string(), value.to_string()));
    settings
}

/// A data directory no other start of this run names.
pub fn fresh_data() -> String {
    scratch(&format!(
        "data-{}",
        DIRECTORIES.fetch_add(1, Ordering::Relaxed)
    ))
    .display()
    .to_string()
}

/// A storage process's settings, hosting the system group, with a role secret and data directories of its own: its
/// partitions' and the group's.
///
/// # Errors
///
/// When the role secret file cannot be written.
pub fn storage_holding() -> TestResult<Vec<(String, String)>> {
    Ok(with(
        with(
            holding(storage_on_one_host()?, SECRET)?,
            "storage.data_dir",
            &fresh_data(),
        ),
        "system.data_dir",
        &fresh_data(),
    ))
}

/// The frontend's settings, with a role secret; it holds no data.
///
/// # Errors
///
/// When the role secret file cannot be written.
pub fn frontend_holding() -> TestResult<Vec<(String, String)>> {
    holding(frontend_on_one_host()?, SECRET)
}

/// `settings` with a role secret file holding `secret`.
///
/// # Errors
///
/// When the role secret file cannot be written.
pub fn holding(settings: Vec<(String, String)>, secret: &str) -> TestResult<Vec<(String, String)>> {
    let file = scratch(&format!("role-secret-{}", secret.len()));
    replace(&file, secret)?;
    Ok(with(
        settings,
        "cluster.role_secret_file",
        &file.display().to_string(),
    ))
}

/// Waits for `address` to answer `path` with success, for a role whose readiness comes after its start.
///
/// # Errors
///
/// When `address` has not answered `path` with success within `START_SECONDS`.
#[expect(
    clippy::disallowed_methods,
    reason = "a test's wait for a process is bounded by the wall clock: a probe that connects and hears nothing takes its own time, which a count of sleeps does not hold"
)]
pub fn until_answering(address: &str, path: &str) -> TestResult {
    let started = std::time::Instant::now();
    let mut last = String::from("no probe ran");
    while started.elapsed() < Duration::from_secs(START_SECONDS) {
        // A role not yet listening is a role not yet answering.
        match http_status(address, path) {
            Ok(200) => return Ok(()),
            Ok(status) => last = format!("status {status}"),
            Err(error) => last = error.to_string(),
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    Err(TestError::new(format!(
        "{address}{path} never answered within {START_SECONDS} s; the last probe: {last}"
    )))
}

/// What the system group at `address` lists: each member's kind, partition and address.
///
/// # Errors
///
/// When the runtime cannot start, the system group refuses the dial or the request, or its answer is no Members or names a
/// member without its address.
pub fn members_at(address: &str) -> TestResult<Vec<(RoleKind, u32, String)>> {
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
                    // A `u32` index into a `usize`, lossless on every target the tests build for.
                    .address(member.address() as usize)
                    .context("its address")?;
                Ok((member.kind(), member.partition(), held.to_string()))
            })
            .collect()
    })
}

/// Answers one request of the double: the file named for the instance the request's path ends in, or 404.
fn answer_double(mut stream: TcpStream) -> std::io::Result<()> {
    let mut held = Vec::new();
    let mut chunk = [0; 1024];
    // Reads until the request's head ends or the peer closes; a test's request is one short head (Ultra rule 2).
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

/// Every address a process of these tests declares, and what declares it: the config double's last.
fn declared() -> TestResult<[(String, &'static str); 9]> {
    Ok([
        (config_address()?, "the config service"),
        (client_address()?, "the frontend's client endpoint"),
        (system_role_address()?, "the system group's role endpoint"),
        (frontend_observability()?, "the frontend's observability"),
        (compute_role_address()?, "compute's role endpoint"),
        (compute_observability()?, "compute's observability"),
        (storage_role_address()?, "storage's role endpoint"),
        (storage_observability()?, "storage's observability"),
        (double_address()?, "the config double"),
    ])
}

/// That no process holds an address these tests declare, before any is started: a process an earlier run left behind is
/// named by the port it holds, where a role started now would only exit naming its own address. The first address taken
/// fails, naming its port and what declares it.
fn ports_free() -> TestResult {
    first_taken(&declared()?).map_or(Ok(()), |taken| {
        Err(TestError::new(format!(
            "{taken}: a process of an earlier run may still hold it"
        )))
    })
}

/// Waits up to [`START_SECONDS`] until no process holds an address a role of these tests declares, nor one of `own`, the
/// suite's own, each named with what declares it. A role a scenario killed is reaped before the kernel tears down the
/// io_uring ring it ran on, and an accept in flight on that ring holds its listener until then, so the next scenario's role
/// retries that bind, and a step dialling the address once `/health` answers finds nothing yet listening. The double's address is
/// left out: the double holds it for the whole run.
///
/// # Errors
///
/// When the target sets no port base, or an address is still held at the end, naming its port and what declares it.
pub fn until_ports_free(own: &[(String, &str)]) -> TestResult {
    let [held @ .., _double] = declared()?;
    let addresses: Vec<(String, &str)> = held.into_iter().chain(own.iter().cloned()).collect();
    for _ in 0..START_SECONDS * 20 {
        if first_taken(&addresses).is_none() {
            return Ok(());
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    first_taken(&addresses).map_or(Ok(()), |taken| {
        Err(TestError::new(format!(
            "{taken}, {START_SECONDS} s after the scenario before ended its processes"
        )))
    })
}

/// The first of `addresses` a process holds, as its port, what declares it, the address and the bind's error.
fn first_taken(addresses: &[(String, &str)]) -> Option<String> {
    addresses.iter().find_map(|(address, declared)| {
        TcpListener::bind(address).err().map(|error| {
            let port = address.rsplit(':').next().unwrap_or(address.as_str());
            format!("port {port}, {declared} at {address}, is taken ({error})")
        })
    })
}

/// Starts the double, for the life of the test process.
///
/// # Errors
///
/// When the target sets no port base, an address the tests declare is taken, the double's directory cannot be made, or its
/// address cannot be bound.
#[expect(
    clippy::disallowed_methods,
    reason = "the double answers beside the scenarios, on a thread of the test's own"
)]
pub fn start_double() -> TestResult {
    ports_free()?;
    std::fs::create_dir_all(double_directory())?;
    let listener = TcpListener::bind(double_address()?)?;
    drop(std::thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            // A connection failing costs that request alone, and its role says so.
            drop(answer_double(stream));
        }
    }));
    Ok(())
}

/// The exit `exited` is the code `code`, or the failure shows `log`.
///
/// # Errors
///
/// When no exit was held.
pub fn exit_is(exited: Option<ExitStatus>, log: &str, code: i32) -> TestResult {
    let status = exited.context("an exit")?;
    assert_eq!(status.code(), Some(code), "{log}");
    Ok(())
}
