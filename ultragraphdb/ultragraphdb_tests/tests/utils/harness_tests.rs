/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! What the harness starts a process with, judged by what reads it: a role's own bootstrap, the config check, and the
//! files written. Every address asked of here is held by the test or by nothing.

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::os::unix::process::ExitStatusExt;
use std::path::{Path, PathBuf};
use std::process::ExitStatus;
use std::sync::{Mutex, MutexGuard, OnceLock};
use std::time::Duration;

use compio::runtime::spawn;
use service_utils::{ServiceStartConfig, WaitStrategy};
use ultragraphdb_common_config_check::{check_central, check_config_service};
use ultragraphdb_common_role::{
    Answer, ROLE_ITERATIONS, RoleSecret, Server, Session, listen, serve_connection,
};
use ultragraphdb_common_service::{Bootstrap, RoleKind};
use ultragraphdb_protocol_transport::{Endpoint, ReceivedFrame};
use ultragraphdb_protocol_wire::{
    BodyBuilder, Command, ErrorCode, ErrorReply, FLAG_LAST, Header, Members,
};
use ultragraphdb_tests::{
    ADMIN_KEY, CLUSTER, COMPUTE_BINARY, CONFIG_ADDRESS, CONFIG_BINARY, DOUBLE_ADDRESS,
    DOUBLE_REVISION, FRONTEND_BINARY, PARENT_PID, READ_KEY, SECRET, START_SECONDS, TARGET,
    answering, bootstrap, compute_on_one_host, config_folder, config_folder_at, config_process,
    exit_is, fresh_data, frontend_holding, holding, key_file, members_at, process, render_settings,
    role, role_of_service, scratch, served_by_double, start_double, storage_holding,
    until_answering, until_ports_free, with,
};
use ultragraphdb_tool_bdd::{Context, TestError, TestResult};

use crate::utils::http_status_tests::{off_runtime, serving, serving_ready_after};

/// Held by every test writing or reading a file the harness writes again on each call, the read key's and a role secret's,
/// each truncated and written anew: a reader meanwhile finds it short.
static WRITTEN_AGAIN: Mutex<()> = Mutex::new(());

/// The harness's rewritten files to this test alone, until the guard drops.
fn keyed() -> TestResult<MutexGuard<'static, ()>> {
    WRITTEN_AGAIN
        .lock()
        .map_err(|_| TestError::new("a test holding the key file failed"))
}

/// The setting `name` of `settings`.
fn setting<'a>(settings: &'a [(String, String)], name: &str) -> TestResult<&'a str> {
    settings
        .iter()
        .find(|(held, _)| held == name)
        .map(|(_, value)| value.as_str())
        .context(&format!("the setting {name}"))
}

/// What a role of `kind` reads from the environment `started` gives it.
fn read_as(kind: RoleKind, started: &ServiceStartConfig) -> TestResult<Bootstrap> {
    let variables = started.env_vars().clone().context("an environment")?;
    Ok(Bootstrap::read(kind, variables)?)
}

/// The `host:port` a role reading `started` asks for its settings.
fn asked_at(kind: RoleKind, started: &ServiceStartConfig) -> TestResult<String> {
    let read = read_as(kind, started)?;
    let url = read.client().url();
    Ok(format!("{}:{}", url.host(), url.port()))
}

#[test]
fn test_scratch_names_distinct_files_in_the_tests_directory() -> TestResult {
    let root = std::env::var_os("TEST_TMPDIR").map_or_else(std::env::temp_dir, PathBuf::from);
    let (one, other) = (scratch("one"), scratch("other"));
    assert_eq!(one.parent(), Some(root.as_path()));
    assert_ne!(one, other);
    let name = one
        .file_name()
        .and_then(|name| name.to_str())
        .context("a name")?;
    assert!(name.starts_with("one"), "{name}");
    Ok(())
}

#[test]
fn test_the_key_file_holds_the_read_key() -> TestResult {
    let _key = keyed()?;
    assert_eq!(std::fs::read_to_string(key_file())?, READ_KEY);
    Ok(())
}

#[test]
fn test_a_bootstrap_is_what_a_role_reads() -> TestResult {
    let _key = keyed()?;
    let variables = bootstrap("127.0.0.1:1", "storage_7");
    let read = Bootstrap::read(RoleKind::Storage, variables)?;
    assert_eq!(read.target(), TARGET);
    assert_eq!(read.instance(), "storage_7");
    assert_eq!(read.client().url().port(), 1);
    assert_eq!(read.parent(), None);
    Ok(())
}

#[test]
fn test_a_process_ends_with_the_test_and_clears_its_environment() -> TestResult {
    let _key = keyed()?;
    let log = scratch("process.log");
    let wait = answering("127.0.0.1:1", "/health");
    let started = process(
        COMPUTE_BINARY,
        bootstrap("127.0.0.1:1", "compute"),
        &log,
        wait.clone(),
    );
    assert_eq!(started.program(), COMPUTE_BINARY);
    assert!(started.clear_env());
    assert_eq!(started.stderr_file(), Some(log.as_path()));
    assert_eq!(started.wait_strategy(), &wait);
    assert_eq!(
        read_as(RoleKind::Compute, &started)?.parent(),
        Some(std::process::id())
    );
    let variables = started.env_vars().clone().context("an environment")?;
    assert!(variables.iter().any(|(name, _)| name == PARENT_PID));
    Ok(())
}

#[test]
fn test_a_role_asks_the_double_for_settings_it_serves() -> TestResult {
    let _key = keyed()?;
    std::fs::create_dir_all(scratch("config-double"))?;
    let settings = compute_on_one_host();
    let started = role(
        COMPUTE_BINARY,
        &settings,
        &scratch("role.log"),
        answering("127.0.0.1:1", "/ready"),
    );
    let read = read_as(RoleKind::Compute, &started)?;
    assert_eq!(asked_at(RoleKind::Compute, &started)?, DOUBLE_ADDRESS);
    let served = std::fs::read_to_string(scratch("config-double").join(read.instance()))?;
    assert_eq!(served, render_settings(&settings));
    Ok(())
}

#[test]
fn test_the_double_serves_each_instance_apart() -> TestResult {
    let _key = keyed()?;
    std::fs::create_dir_all(scratch("config-double"))?;
    let first = served_by_double(FRONTEND_BINARY, "first");
    let second = served_by_double(FRONTEND_BINARY, "second");
    assert_ne!(first, second);
    for (instance, text) in [(&first, "first"), (&second, "second")] {
        let started = process(
            FRONTEND_BINARY,
            bootstrap(DOUBLE_ADDRESS, instance),
            Path::new("unused.log"),
            answering("127.0.0.1:1", "/health"),
        );
        assert_eq!(
            read_as(RoleKind::Frontend, &started)?.instance(),
            instance.as_str()
        );
        assert_eq!(
            std::fs::read_to_string(scratch("config-double").join(instance))?,
            text
        );
    }
    Ok(())
}

#[test]
fn test_a_role_of_the_service_asks_the_config_service() -> TestResult {
    let _key = keyed()?;
    let started = role_of_service(
        COMPUTE_BINARY,
        "compute_2",
        &scratch("service.log"),
        answering("127.0.0.1:1", "/ready"),
    );
    assert_eq!(asked_at(RoleKind::Compute, &started)?, CONFIG_ADDRESS);
    assert_eq!(
        read_as(RoleKind::Compute, &started)?.instance(),
        "compute_2"
    );
    Ok(())
}

#[test]
fn test_answering_waits_on_the_path_asked() {
    assert_eq!(
        answering("127.0.0.1:9", "/ready"),
        WaitStrategy::WaitForHttpHealthCheck("http://127.0.0.1:9/ready".to_string(), START_SECONDS)
    );
}

#[test]
fn test_with_replaces_a_setting() {
    let settings = vec![
        ("a.b".to_string(), "1".to_string()),
        ("c.d".to_string(), "2".to_string()),
    ];
    let replaced = with(settings, "a.b", "3");
    assert_eq!(
        replaced,
        vec![
            ("c.d".to_string(), "2".to_string()),
            ("a.b".to_string(), "3".to_string()),
        ]
    );
}

#[test]
fn test_fresh_data_names_a_new_directory_each_time() {
    let (one, other) = (fresh_data(), fresh_data());
    assert_ne!(one, other);
    assert!(!one.is_empty());
}

#[test]
fn test_holding_writes_the_secret_it_names() -> TestResult {
    let _key = keyed()?;
    let settings = holding(compute_on_one_host(), "a secret of its own")?;
    let file = setting(&settings, "cluster.role_secret_file")?;
    assert_eq!(std::fs::read_to_string(file)?, "a secret of its own");
    Ok(())
}

#[test]
fn test_each_holding_role_has_a_data_directory_of_its_own() -> TestResult {
    let _key = keyed()?;
    let (frontend, storage) = (frontend_holding()?, storage_holding()?);
    let (again, storage_again) = (frontend_holding()?, storage_holding()?);
    assert_ne!(
        setting(&frontend, "frontend.data_dir")?,
        setting(&again, "frontend.data_dir")?
    );
    assert_ne!(
        setting(&storage, "storage.data_dir")?,
        setting(&storage_again, "storage.data_dir")?
    );
    for settings in [&frontend, &storage] {
        let file = setting(settings, "cluster.role_secret_file")?;
        assert_eq!(std::fs::read_to_string(file)?, SECRET);
    }
    Ok(())
}

/// An instance's key and its settings.
type Instance = (String, Vec<(String, String)>);

/// A whole cluster's settings, as the scenarios start it.
fn cluster() -> TestResult<[Instance; 3]> {
    Ok([
        ("frontend".to_string(), frontend_holding()?),
        ("storage".to_string(), storage_holding()?),
        (
            "compute".to_string(),
            holding(compute_on_one_host(), SECRET)?,
        ),
    ])
}

#[test]
fn test_a_config_folder_passes_the_config_check() -> TestResult {
    let _key = keyed()?;
    let held = cluster()?;
    let instances: Vec<(&str, &[(String, String)])> = held
        .iter()
        .map(|(key, settings)| (key.as_str(), settings.as_slice()))
        .collect();
    let own = config_folder(&instances, "plaintext = true")?;
    let folder = own.parent().context("the folder")?;
    let report = check_central(&folder.join("targets.toml"));
    assert!(report.passes(), "{:?}", report.defects());
    assert_eq!(report.targets().len(), 1);
    let defects = check_config_service("service.toml", &std::fs::read(&own)?);
    assert!(defects.is_empty(), "{defects:?}");
    assert_eq!(
        std::fs::read_to_string(folder.join("admin.key"))?,
        ADMIN_KEY
    );
    Ok(())
}

#[test]
fn test_a_config_folder_listens_where_it_is_asked() -> TestResult {
    let _key = keyed()?;
    let own = config_folder_at("127.0.0.1:1", &[], "plaintext = true")?;
    let text = std::fs::read_to_string(&own)?;
    assert!(text.contains("address = \"tcp://127.0.0.1:1\""), "{text}");
    let other = config_folder_at("127.0.0.1:1", &[], "plaintext = true")?;
    assert_ne!(own, other);
    Ok(())
}

#[test]
fn test_the_config_process_reads_its_own_file() -> TestResult {
    let own = scratch("service.toml");
    let wait = answering(CONFIG_ADDRESS, "/health");
    let started = config_process(&own, wait.clone());
    assert_eq!(started.program(), CONFIG_BINARY);
    let path = own.display().to_string();
    assert_eq!(
        started.program_args(),
        &Some(vec!["--config", path.as_str()])
    );
    assert!(started.clear_env());
    assert_eq!(started.wait_strategy(), &wait);
    Ok(())
}

#[test]
fn test_exit_is_needs_an_exit_of_that_code() -> TestResult {
    // A wait status holds the exit code in its second byte.
    let exited = ExitStatus::from_raw(3 << 8);
    exit_is(Some(exited), "exited", 3)?;
    assert!(exit_is(None, "never exited", 3).is_err());
    Ok(())
}

#[test]
fn test_until_answering_returns_once_the_path_answers() -> TestResult {
    compio::runtime::Runtime::new()?.block_on(async {
        let address = serving().await?;
        off_runtime(move || until_answering(&address, "/health")).await
    })
}

#[test]
fn test_until_answering_fails_on_a_path_that_never_answers_with_success() -> TestResult {
    compio::runtime::Runtime::new()?.block_on(async {
        let address = serving().await?;
        let failed = off_runtime(move || Ok(until_answering(&address, "/ready").is_err())).await?;
        assert!(failed, "a path answering 503 never answers with success");
        Ok(())
    })
}

#[test]
fn test_until_answering_waits_for_a_role_ready_seconds_after_its_start() -> TestResult {
    // A probe every 50 ms: 60 refused probes are 3 s of a role not yet ready, well within the START_SECONDS it may take.
    compio::runtime::Runtime::new()?.block_on(async {
        let address = serving_ready_after(60).await?;
        off_runtime(move || until_answering(&address, "/ready")).await
    })
}

/// How often a wait here probes, as the harness's own waits do.
const PROBE: Duration = Duration::from_millis(50);

/// How many probes a test waits for the double's address. A test process of a parallel run of this suite, a mutation job's,
/// holds it until its slowest test ends, about [`START_SECONDS`] after its start.
const DOUBLE_WAIT_PROBES: u64 = START_SECONDS * 3 * 20;

/// How long a test holds an address [`until_ports_free`] waits on: past the 2 s of 40 probes, well within the
/// [`START_SECONDS`] it waits.
const HELD_FOR: Duration = Duration::from_secs(4);

/// The pause between the parts of a request's head, long enough for the double to read the first part alone.
const SPLIT_PAUSE: Duration = Duration::from_millis(300);

/// The double this test process started, or why it could not: it serves for the life of the process, so it starts once.
static DOUBLE: OnceLock<TestResult> = OnceLock::new();

/// Whether nothing holds `address`, asked by binding it and letting it go.
fn address_free(address: &str) -> bool {
    TcpListener::bind(address).is_ok()
}

/// The double, started once for this test process.
fn the_double() -> TestResult {
    DOUBLE.get_or_init(start_when_free).clone()
}

/// Starts the double once no other process holds its address. A process taking it between the probe and the start is
/// waited out; a start refused while the address is free is the answer.
fn start_when_free() -> TestResult {
    for _ in 0..DOUBLE_WAIT_PROBES {
        if address_free(DOUBLE_ADDRESS) {
            let started = start_double();
            if started.is_ok() || address_free(DOUBLE_ADDRESS) {
                return started;
            }
        }
        std::thread::sleep(PROBE);
    }
    Err(TestError::new(format!("{DOUBLE_ADDRESS} stayed held")))
}

/// All the double answers to `parts`, written one after the other with a pause between, read until it closes.
fn asked_of_double(parts: &[String]) -> TestResult<String> {
    let mut stream = TcpStream::connect(DOUBLE_ADDRESS)?;
    stream.set_nodelay(true)?;
    stream.set_read_timeout(Some(Duration::from_secs(START_SECONDS)))?;
    for (index, part) in parts.iter().enumerate() {
        if index > 0 {
            std::thread::sleep(SPLIT_PAUSE);
        }
        stream.write_all(part.as_bytes())?;
    }
    let mut answer = String::new();
    let _read: usize = stream.read_to_string(&mut answer)?;
    Ok(answer)
}

/// The `GET` head asking for instance `instance`'s settings, as a role asks.
fn settings_head(instance: &str) -> String {
    format!("{instance} HTTP/1.1\r\nHost: {DOUBLE_ADDRESS}\r\n\r\n")
}

#[test]
fn test_the_double_answers_the_settings_it_serves_at_its_revision() -> TestResult {
    the_double()?;
    let instance = served_by_double(FRONTEND_BINARY, "the served settings");
    let answer = asked_of_double(&[format!(
        "GET /targets/{TARGET}/{}",
        settings_head(&instance)
    )])?;
    let (head, body) = answer.split_once("\r\n\r\n").context("an answer")?;
    assert!(head.starts_with("HTTP/1.1 200 "), "{answer}");
    let revision = format!("X-Revision: {DOUBLE_REVISION}");
    assert!(head.lines().any(|line| line == revision), "{answer}");
    assert_eq!(body, "the served settings");
    Ok(())
}

#[test]
fn test_the_double_answers_a_request_whose_head_arrives_in_parts() -> TestResult {
    the_double()?;
    let instance = served_by_double(FRONTEND_BINARY, "settings asked in parts");
    let answer = asked_of_double(&[format!("GET /targets/{TARGET}/"), settings_head(&instance)])?;
    let (head, body) = answer.split_once("\r\n\r\n").context("an answer")?;
    assert!(head.starts_with("HTTP/1.1 200 "), "{answer}");
    assert_eq!(body, "settings asked in parts");
    Ok(())
}

#[test]
fn test_the_ports_are_free_while_the_double_serves() -> TestResult {
    the_double()?;
    until_ports_free(&[])
}

#[test]
fn test_until_ports_free_waits_for_a_held_address_to_be_released() -> TestResult {
    compio::runtime::Runtime::new()?.block_on(async {
        let held = TcpListener::bind("127.0.0.1:0")?;
        let address = held.local_addr()?.to_string();
        spawn(async move {
            compio::time::sleep(HELD_FOR).await;
            drop(held);
        })
        .detach();
        let freed = off_runtime(move || {
            until_ports_free(&[(address.as_str(), "a listener the test holds")])?;
            Ok(address_free(&address))
        })
        .await?;
        assert!(freed, "the wait ended while the address was still held");
        Ok(())
    })
}

/// A second start finds the double's own address taken, at the latest, and names the first declared address it finds taken:
/// another run's process may hold one declared before it.
#[test]
fn test_the_double_starts_once_and_names_a_port_taken() -> TestResult {
    the_double()?;
    let refused = start_double().err().context("a second start")?;
    assert!(refused.reason().contains(" is taken ("), "{refused:?}");
    assert!(
        refused
            .reason()
            .ends_with(": a process of an earlier run may still hold it"),
        "{refused:?}"
    );
    Ok(())
}

#[test]
fn test_the_double_answers_an_instance_it_does_not_serve_with_404() -> TestResult {
    the_double()?;
    let instance = served_by_double(FRONTEND_BINARY, "served");
    let answer = asked_of_double(&[format!(
        "GET /targets/{TARGET}/{}",
        settings_head(&format!("{instance}-not-served"))
    )])?;
    assert!(answer.starts_with("HTTP/1.1 404 "), "{answer}");
    Ok(())
}

#[test]
fn test_the_double_lets_a_connection_closed_before_its_head_go() -> TestResult {
    the_double()?;
    let mut stream = TcpStream::connect(DOUBLE_ADDRESS)?;
    stream.set_read_timeout(Some(Duration::from_secs(START_SECONDS)))?;
    stream.shutdown(std::net::Shutdown::Write)?;
    let mut answer = Vec::new();
    let _read: usize = stream.read_to_end(&mut answer)?;
    assert!(answer.is_empty(), "no head, no answer");
    Ok(())
}

/// A wait past [`START_SECONDS`] with an address still held ends refused, naming the first address it finds taken: the
/// test's own, unless another run's process holds one declared before it.
#[test]
fn test_until_ports_free_names_an_address_held_past_its_wait() -> TestResult {
    let held = TcpListener::bind("127.0.0.1:0")?;
    let address = held.local_addr()?.to_string();
    let refused = until_ports_free(&[(address.as_str(), "a listener the test holds")])
        .err()
        .context("a wait that ends refused")?;
    assert!(refused.reason().contains(" is taken ("), "{refused:?}");
    assert!(
        refused.reason().ends_with(&format!(
            "{START_SECONDS} s after the scenario before ended its processes"
        )),
        "{refused:?}"
    );
    drop(held);
    Ok(())
}

/// Answers every Members with the members it holds.
#[derive(Debug, Clone)]
struct Listing(Members);

impl Session for Listing {
    async fn answer(&mut self, stream: u64, frame: ReceivedFrame) -> Answer {
        if frame.command() != Command::Members.number() {
            return Answer::Refuse(ErrorReply::new(ErrorCode::Unavailable, "a Members alone"));
        }
        let header = Header::new(CLUSTER, Command::Members)
            .with_stream(stream)
            .with_flags(FLAG_LAST);
        self.0.frame(header).map_or_else(
            |rule| Answer::Refuse(ErrorReply::protocol(&rule)),
            Answer::Reply,
        )
    }
}

#[test]
fn test_members_at_lists_what_the_frontend_answers() -> TestResult {
    let mut members = Members::new();
    members.push_compute("tcp://127.0.0.1:7082");
    members.push_storage(1, "tcp://127.0.0.1:7000");
    let expected: Vec<_> = (0..members.len())
        .map(|index| {
            members
                .get(index)
                .map(|(kind, partition, address)| (kind, partition, address.to_string()))
                .context("a member")
        })
        .collect::<TestResult<_>>()?;
    let secret_file = scratch("members-at-secret");
    std::fs::write(&secret_file, SECRET)?;
    let credentials = RoleSecret::read(&secret_file.display().to_string())?.credentials()?;
    let port = TcpListener::bind("127.0.0.1:0")?.local_addr()?.port();
    let address = format!("tcp://127.0.0.1:{port}");
    compio::runtime::Runtime::new()?.block_on(async {
        let listener = listen(&address).await?;
        let server =
            Server::new(CLUSTER, "frontend", Endpoint::Frontend)?.with_iterations(ROLE_ITERATIONS);
        let session = Listing(members);
        spawn(async move {
            if let Ok(carrier) = listener.accept().await {
                serve_connection(carrier, &server, credentials, session).await;
            }
        })
        .detach();
        let listed = off_runtime(move || members_at(&address)).await?;
        assert_eq!(listed, expected);
        Ok(())
    })
}
