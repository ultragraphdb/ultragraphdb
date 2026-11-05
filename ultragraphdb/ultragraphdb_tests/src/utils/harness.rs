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
use ultragraphdb_tool_bdd::{Context, TestError, TestResult};

use crate::utils::http_status::http_status;
use crate::utils::one_host::{CLUSTER, frontend_on_one_host, render_settings, storage_on_one_host};

/// How long a role has to answer its first probe, or to exit. A start takes milliseconds; the margin is for a loaded machine.
pub const START_SECONDS: u64 = 20;

/// The role secret every role of a scenario holds.
pub const SECRET: &str = "the cluster's role secret";

static DIRECTORIES: AtomicU32 = AtomicU32::new(0);

/// The address the config service listens at, in a scenario starting it.
pub const CONFIG_ADDRESS: &str = "127.0.0.1:17090";

/// The address of the config double, which serves a scenario's settings for an instance as they stand: a partial or broken
/// target, which the service itself would rightly refuse to hold.
pub const DOUBLE_ADDRESS: &str = "127.0.0.1:17091";

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

/// The file holding the read key, written once per run.
pub fn key_file() -> PathBuf {
    let file = scratch("read.key");
    // Absent, a role says it cannot read its key, which fails the scenario by name.
    drop(std::fs::write(&file, READ_KEY));
    file
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
pub fn served_by_double(program: &str, text: &str) -> String {
    let at = DIRECTORIES.fetch_add(1, Ordering::Relaxed);
    let instance = format!("{}_{at}", role_of(program));
    // Absent, the double answers 404, and the role says so by name.
    drop(std::fs::write(double_directory().join(&instance), text));
    instance
}

/// What starts the binary `program` on `settings`, served by the double, its log in the file `log`, waiting as
/// `wait_strategy` says.
pub fn role(
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
pub fn role_of_service(
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
pub fn config_folder(
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
pub fn start_config_service(
    util: &ServiceUtil,
    instances: &[(&str, &[(String, String)])],
) -> TestResult<u32> {
    let own = config_folder(instances, "plaintext = true")?;
    let wait = answering(CONFIG_ADDRESS, "/health");
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

/// A storage process's settings, with a role secret and a data directory of its own.
pub fn storage_holding() -> TestResult<Vec<(String, String)>> {
    Ok(with(
        holding(storage_on_one_host(), SECRET)?,
        "storage.data_dir",
        &fresh_data(),
    ))
}

/// The frontend's settings, with a role secret and a data directory of its own.
pub fn frontend_holding() -> TestResult<Vec<(String, String)>> {
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
pub fn holding(settings: Vec<(String, String)>, secret: &str) -> TestResult<Vec<(String, String)>> {
    let file = scratch(&format!("role-secret-{}", secret.len()));
    std::fs::write(&file, secret)?;
    Ok(with(
        settings,
        "cluster.role_secret_file",
        &file.display().to_string(),
    ))
}

/// Waits for `address` to answer `path` with success, for a role whose readiness comes after its start.
pub fn until_answering(address: &str, path: &str) -> TestResult {
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
pub fn start_double() -> TestResult {
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
