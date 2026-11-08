/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The config service and its CLI as processes, end to end and with no container, bound to `features/`. One scenario runs at
//! a time: the service listens at a fixed port.

use std::io::{Read, Write};
use std::net::{Shutdown, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus, Output, Stdio};
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Duration;

use compio::runtime::Runtime;
use cucumber::{World, then, when};
use service_utils::{ServiceStartConfig, ServiceUtil, WaitStrategy};
use ultragraphdb_tool_bdd::{Context, TestError, TestResult};
use ultragraphdb_transport_http_client::{HttpClient, HttpReply, HttpUrl};

/// Where the sandbox holds the binaries, relative to the test's working directory: `copy_file` writes them to this package.
const ROOT_PATH: &str = "ultragraphdb/ultragraphdb_config_tests";

const SERVICE: &str = "ultragraphdb_config";
const CLI: &str = "ultragraphdb_config_cli";

/// The address the service listens at.
const ADDRESS: &str = "127.0.0.1:17090";

/// How long the service has to answer, or to exit. A start takes milliseconds; the margin is for a loaded machine.
const START_SECONDS: u64 = 20;

const READ_KEY: &str = "a read key of at least thirty two bytes long";
const ADMIN_KEY: &str = "an admin key of at least thirty two bytes long";

const FRONTEND: &str = "[cluster]\nid = 7\nrole_secret_file = \"/run/secret\"\npartition_count = 2\n[observability]\naddress = \"tcp://127.0.0.1:18081\"\n[frontend]\nrole_address = \"tcp://10.0.0.1:7081\"\ndata_dir = \"/data\"\n[sequencer]\nepoch_ms_min = 2\nepoch_ms_max = 10\n";
const STORAGE: &str = "[cluster]\nid = 7\nrole_secret_file = \"/run/secret\"\nfrontend_address = \"tcp://frontend:7081\"\n[observability]\naddress = \"tcp://0.0.0.0:8080\"\n[storage]\npartition_count = 1\ndata_dir = \"/data\"\n";
const STORAGE_1: &str = "[storage]\npartition_first = 1\nrole_address = \"tcp://10.0.0.2:7000\"\n";
const STORAGE_2: &str = "[storage]\npartition_first = 2\nrole_address = \"tcp://10.0.0.4:7000\"\n";
const COMPUTE: &str = "[cluster]\nid = 7\nrole_secret_file = \"/run/secret\"\nfrontend_address = \"tcp://frontend:7081\"\n[observability]\naddress = \"tcp://0.0.0.0:8080\"\n[compute]\nrole_address = \"tcp://10.0.0.3:7082\"\n";

static FOLDERS: AtomicU32 = AtomicU32::new(0);

#[derive(Debug, Default, World)]
struct Steps {
    util: Option<ServiceUtil>,
    folder: PathBuf,
    exited: Option<ExitStatus>,
    log: String,
    reply: Option<HttpReply>,
    output: Option<Output>,
    graph: String,
    /// What a start's environment holds.
    environment: Vec<(String, String)>,
}

/// A valid target's files, as they are named on disk with the key each is declared by.
fn valid_files() -> Vec<(&'static str, &'static str, String)> {
    vec![
        ("frontend", "ci_frontend.toml", FRONTEND.to_string()),
        ("storage", "ci_storage.toml", STORAGE.to_string()),
        ("storage_1", "ci_storage_1.toml", STORAGE_1.to_string()),
        ("storage_2", "ci_storage_2.toml", STORAGE_2.to_string()),
        ("compute_1", "ci_compute_1.toml", COMPUTE.to_string()),
    ]
}

/// A fresh folder holding the service's settings and keys, and a central file declaring the valid target.
fn prepare(w: &mut Steps) -> TestResult {
    let root = std::env::var_os("TEST_TMPDIR").map_or_else(std::env::temp_dir, PathBuf::from);
    w.folder = root.join(format!(
        "config-tests-{}-{}",
        std::process::id(),
        FOLDERS.fetch_add(1, Ordering::Relaxed)
    ));
    let settings = w.folder.join("settings");
    std::fs::create_dir_all(&settings)?;
    std::fs::write(w.folder.join("read.key"), READ_KEY)?;
    std::fs::write(w.folder.join("admin.key"), ADMIN_KEY)?;
    let mut central = "[ci]\n".to_string();
    for (key, file, text) in valid_files() {
        central.push_str(&format!("{key} = \"{file}\"\n"));
        std::fs::write(settings.join(file), text)?;
    }
    std::fs::write(settings.join("targets.toml"), central)?;
    let own = format!(
        "[config]\naddress = \"tcp://{ADDRESS}\"\nplaintext = true\ndata_dir = \"{}\"\nread_key_file = \"{}\"\nadmin_key_file = \"{}\"\ntargets_file = \"{}\"\n",
        w.folder.join("data").display(),
        w.folder.join("read.key").display(),
        w.folder.join("admin.key").display(),
        settings.join("targets.toml").display(),
    );
    std::fs::write(w.folder.join("service.toml"), own)?;
    Ok(())
}

fn service(w: &Steps) -> ServiceUtilArgs {
    ServiceUtilArgs {
        settings: w.folder.join("service.toml"),
        log: w.folder.join("service.log"),
    }
}

/// What a start names.
struct ServiceUtilArgs {
    settings: PathBuf,
    log: PathBuf,
}

/// Starts the service, waiting as `wait_strategy` says, and returns its process id.
fn start(w: &mut Steps, wait_strategy: WaitStrategy) -> TestResult<u32> {
    let args = service(w);
    // The configuration names its arguments for the process's life, and one process's are few.
    let settings: &'static str = Box::leak(args.settings.display().to_string().into_boxed_str());
    let config = ServiceStartConfig::builder()
        .program(SERVICE)
        .program_args(vec!["--config", settings])
        .env_vars(w.environment.clone())
        .clear_env(true)
        .stderr_file(&args.log)
        .wait_strategy(wait_strategy)
        .build();
    if w.util.is_none() {
        w.util = Some(ServiceUtil::new(ROOT_PATH, vec![SERVICE])?);
    }
    let util = w.util.as_ref().context("the services")?;
    Ok(util.start_service_from_config(config)?)
}

fn answering() -> WaitStrategy {
    WaitStrategy::WaitForHttpHealthCheck(format!("http://{ADDRESS}/health"), START_SECONDS)
}

/// Starts the service and waits until it answers.
fn start_serving(w: &mut Steps) -> TestResult<u32> {
    start(w, answering())
}

/// Starts the service expecting it to exit, and holds its exit and its log.
fn start_refused(w: &mut Steps) -> TestResult {
    let pid = start(w, WaitStrategy::NoWait)?;
    let util = w.util.as_ref().context("the services")?;
    w.exited = Some(util.wait_for_exit(pid, Duration::from_secs(START_SECONDS))?);
    w.log = std::fs::read_to_string(w.folder.join("service.log"))?;
    Ok(())
}

/// Asks the service, with `key`.
fn ask(key: &str, method: &str, target: &str, body: &[u8]) -> TestResult<HttpReply> {
    let url = HttpUrl::parse(&format!("http://{ADDRESS}"))?;
    let client = HttpClient::new(url, None, key.to_string())?;
    Ok(Runtime::new()?.block_on(client.request(method, target, body))?)
}

fn body_of(reply: &HttpReply) -> String {
    String::from_utf8_lossy(reply.body()).into_owned()
}

/// A revision's body of the valid files, `storage` standing in for the storage file.
fn bundle(storage: &str, compute: &str) -> Vec<u8> {
    let mut body = Vec::new();
    for (key, _, text) in valid_files() {
        let text = match key {
            "storage" => storage.to_string(),
            "compute_1" => compute.to_string(),
            _ => text,
        };
        body.extend_from_slice(format!("{key} {}\n", text.len()).as_bytes());
        body.extend_from_slice(text.as_bytes());
        body.push(b'\n');
    }
    body
}

fn add(base: u64, comment: &str) -> TestResult<HttpReply> {
    ask(
        ADMIN_KEY,
        "POST",
        &format!("/v1/targets/ci/revisions?base={base}"),
        &bundle(&format!("{STORAGE}# {comment}\n"), COMPUTE),
    )
}

fn targets_list() -> TestResult<String> {
    Ok(body_of(&ask(READ_KEY, "GET", "/v1/targets", b"")?))
}

fn exit_code(w: &Steps) -> TestResult<i32> {
    w.exited
        .as_ref()
        .and_then(ExitStatus::code)
        .context("an exit code")
}

fn says(text: &str, words: &[&str]) {
    for word in words {
        assert!(text.contains(word), "{word} in: {text}");
    }
}

fn cli(w: &mut Steps) -> TestResult {
    let binary = Path::new(ROOT_PATH).join(CLI);
    w.output = Some(
        Command::new(binary)
            .arg(w.folder.join("settings").join("targets.toml"))
            .env_clear()
            .output()?,
    );
    Ok(())
}

fn cli_text(w: &Steps) -> TestResult<(String, i32)> {
    let output = w.output.as_ref().context("the CLI's output")?;
    Ok((
        String::from_utf8_lossy(&output.stdout).into_owned(),
        output.status.code().context("an exit code")?,
    ))
}

#[when("a config service runs and another starts over the same data directory")]
fn second_process(w: &mut Steps) -> TestResult {
    prepare(w)?;
    let _first = start_serving(w)?;
    let own = std::fs::read_to_string(w.folder.join("service.toml"))?;
    std::fs::write(
        w.folder.join("service.toml"),
        own.replace(ADDRESS, "127.0.0.1:17091"),
    )?;
    w.folder
        .join("service.log")
        .exists()
        .then_some(())
        .context("the first's log")?;
    let log = w.folder.join("second.log");
    let settings: &'static str = Box::leak(
        w.folder
            .join("service.toml")
            .display()
            .to_string()
            .into_boxed_str(),
    );
    let config = ServiceStartConfig::builder()
        .program(SERVICE)
        .program_args(vec!["--config", settings])
        .env_vars(w.environment.clone())
        .clear_env(true)
        .stderr_file(&log)
        .wait_strategy(WaitStrategy::NoWait)
        .build();
    let util = w.util.as_ref().context("the services")?;
    let pid = util.start_service_from_config(config)?;
    w.exited = Some(util.wait_for_exit(pid, Duration::from_secs(START_SECONDS))?);
    w.log = std::fs::read_to_string(log)?;
    Ok(())
}

#[then("the second exits with code 69 and an error naming the directory")]
fn second_refused(w: &mut Steps) -> TestResult {
    assert_eq!(exit_code(w)?, 69, "{}", w.log);
    says(&w.log, &[&w.folder.join("data").display().to_string()]);
    Ok(())
}

#[when("the config service's dependency graph is read")]
fn graph(w: &mut Steps) -> TestResult {
    w.graph = std::fs::read_to_string(Path::new(ROOT_PATH).join("config_deps"))?;
    Ok(())
}

#[then("it names neither `ultragraphdb_store_kv` nor fjall")]
fn no_engine(w: &mut Steps) {
    assert!(
        w.graph.contains("ultragraphdb_common_config"),
        "a graph of the service"
    );
    assert!(!w.graph.contains("ultragraphdb_store_kv"));
    assert!(!w.graph.contains("fjall"));
}

#[when("the config service starts on a fresh directory with a declared file failing a check")]
fn broken_start(w: &mut Steps) -> TestResult {
    prepare(w)?;
    let settings = w.folder.join("settings");
    std::fs::write(
        settings.join("ci_compute_1.toml"),
        format!("{COMPUTE}partiton = 1\n"),
    )?;
    std::fs::write(
        settings.join("ci_storage.toml"),
        STORAGE.replace("data_dir = \"/data\"\n", ""),
    )?;
    start_refused(w)
}

#[then("it exits with code 78, lists every defect, stores nothing and serves nothing")]
fn broken_stopped(w: &mut Steps) -> TestResult {
    assert_eq!(exit_code(w)?, 78, "{}", w.log);
    says(
        &w.log,
        &[
            "compute.partiton",
            "storage.data_dir",
            "storage_1",
            "storage_2",
        ],
    );
    assert!(!w.folder.join("data").exists(), "nothing is stored");
    assert!(
        ask(READ_KEY, "GET", "/health", b"").is_err(),
        "nothing is served"
    );
    Ok(())
}

#[when("the service starts with `targets_file` naming a file that does not exist")]
fn no_central(w: &mut Steps) -> TestResult {
    prepare(w)?;
    std::fs::remove_file(w.folder.join("settings").join("targets.toml"))?;
    start_refused(w)
}

#[then("it exits with code 78 and an error naming the setting and the path it looked at")]
fn no_central_named(w: &mut Steps) -> TestResult {
    assert_eq!(exit_code(w)?, 78, "{}", w.log);
    let looked = w.folder.join("settings").join("targets.toml");
    says(
        &w.log,
        &["config.targets_file", &looked.display().to_string()],
    );
    Ok(())
}

#[when("the central file holds no target")]
fn hollow_central(w: &mut Steps) -> TestResult {
    prepare(w)?;
    std::fs::write(
        w.folder.join("settings").join("targets.toml"),
        "# nothing\n",
    )?;
    start_refused(w)
}

#[then("it exits with code 78, the error naming the central file and saying what it must declare")]
fn hollow_named(w: &mut Steps) -> TestResult {
    assert_eq!(exit_code(w)?, 78, "{}", w.log);
    says(
        &w.log,
        &["targets.toml", "declares no target", "must declare"],
    );
    Ok(())
}

#[when("target `ci` declares `storage_1 = \"ci_storage_1.toml\"` and no such file exists")]
fn absent_declared(w: &mut Steps) -> TestResult {
    prepare(w)?;
    std::fs::remove_file(w.folder.join("settings").join("ci_storage_1.toml"))?;
    start_refused(w)
}

#[then(
    "it exits with code 78, the error naming the target, the instance, the value as written and the path it resolved to"
)]
fn absent_named(w: &mut Steps) -> TestResult {
    assert_eq!(exit_code(w)?, 78, "{}", w.log);
    let resolved = w.folder.join("settings").join("ci_storage_1.toml");
    says(
        &w.log,
        &[
            "target ci",
            "instance storage_1",
            "ci_storage_1.toml",
            &resolved.display().to_string(),
        ],
    );
    Ok(())
}

#[when("two declared files are absent and a third is malformed")]
fn three_wrong(w: &mut Steps) -> TestResult {
    prepare(w)?;
    let settings = w.folder.join("settings");
    std::fs::remove_file(settings.join("ci_storage_1.toml"))?;
    std::fs::remove_file(settings.join("ci_storage_2.toml"))?;
    std::fs::write(settings.join("ci_compute_1.toml"), "[broken\n")?;
    start_refused(w)
}

#[then("the one error lists all three, and it exits with code 78")]
fn three_named(w: &mut Steps) -> TestResult {
    assert_eq!(exit_code(w)?, 78, "{}", w.log);
    says(
        &w.log,
        &[
            "ci_storage_1.toml",
            "ci_storage_2.toml",
            "ci_compute_1.toml",
        ],
    );
    Ok(())
}

#[when("a service starts declaring target `ci`, and again after a declared file changed validly")]
fn begun_twice(w: &mut Steps) -> TestResult {
    prepare(w)?;
    let pid = start_serving(w)?;
    let util = w.util.as_ref().context("the services")?;
    let _exit = util.stop_service(pid)?;
    std::fs::write(
        w.folder.join("settings").join("ci_storage.toml"),
        format!("{STORAGE}# changed\n"),
    )?;
    let _pid = start_serving(w)?;
    Ok(())
}

#[then("`ci` holds revision 1 alone, and the second start logs that the file differs")]
fn begun_once(w: &mut Steps) -> TestResult {
    says(&targets_list()?, &["ci latest 1 active 1"]);
    says(
        &std::fs::read_to_string(w.folder.join("service.log"))?,
        &["differ", "ci"],
    );
    Ok(())
}

#[when("a revision is acknowledged and activated, and the service is killed and restarted")]
fn killed(w: &mut Steps) -> TestResult {
    prepare(w)?;
    let pid = start_serving(w)?;
    assert_eq!(add(1, "second")?.status(), 201);
    let active = ask(ADMIN_KEY, "PUT", "/v1/targets/ci/active", b"2")?;
    assert_eq!(active.status(), 200, "{}", body_of(&active));
    let status = Command::new("kill")
        .args(["-9", &pid.to_string()])
        .status()?;
    if !status.success() {
        return Err(TestError::new("kill failed"));
    }
    let util = w.util.as_ref().context("the services")?;
    let _exit = util.wait_for_exit(pid, Duration::from_secs(START_SECONDS))?;
    let _pid = start_serving(w)?;
    Ok(())
}

#[then("the revision is stored, and the active revision is the one last acknowledged")]
fn held(_w: &mut Steps) -> TestResult {
    says(&targets_list()?, &["ci latest 2 active 2"]);
    let reply = ask(READ_KEY, "GET", "/v1/targets/ci/instances/storage_1", b"")?;
    assert_eq!(reply.header("X-Revision"), Some("2"));
    says(
        &body_of(&reply),
        &["partition_first = 1", "data_dir = \"/data\""],
    );
    Ok(())
}

#[when("target `ci` holds revisions 1 and 2, with 2 active, and an administrator activates 1")]
fn rollback(w: &mut Steps) -> TestResult {
    prepare(w)?;
    let _pid = start_serving(w)?;
    assert_eq!(add(1, "second")?.status(), 201);
    assert_eq!(
        ask(ADMIN_KEY, "PUT", "/v1/targets/ci/active", b"2")?.status(),
        200
    );
    assert_eq!(
        ask(ADMIN_KEY, "PUT", "/v1/targets/ci/active", b"1")?.status(),
        200
    );
    Ok(())
}

#[then("an instance's read names revision 1 in `X-Revision`, and revision 2 is still stored")]
fn rolled_back(_w: &mut Steps) -> TestResult {
    let active = ask(READ_KEY, "GET", "/v1/targets/ci/instances/storage_2", b"")?;
    assert_eq!(active.header("X-Revision"), Some("1"));
    let older = ask(
        READ_KEY,
        "GET",
        "/v1/targets/ci/revisions/2/instances/storage_2",
        b"",
    )?;
    assert_eq!(older.status(), 200);
    assert_eq!(older.header("X-Revision"), Some("2"));
    let files = ask(ADMIN_KEY, "GET", "/v1/targets/ci/revisions/2", b"")?;
    assert_eq!(
        files.body(),
        bundle(&format!("{STORAGE}# second\n"), COMPUTE)
    );
    Ok(())
}

#[when("two administrators edit from revision 1, and the first adds revision 2")]
fn stale(w: &mut Steps) -> TestResult {
    prepare(w)?;
    let _pid = start_serving(w)?;
    assert_eq!(add(1, "first")?.status(), 201);
    w.reply = Some(add(1, "second")?);
    Ok(())
}

#[then("the second's revision is refused with status 409 naming revision 2, and nothing is stored")]
fn refused_stale(w: &mut Steps) -> TestResult {
    let reply = w.reply.as_ref().context("a reply")?;
    assert_eq!(reply.status(), 409);
    says(&body_of(reply), &["latest revision is 2"]);
    says(&targets_list()?, &["ci latest 2"]);
    Ok(())
}

#[when(
    "an administrator adds a revision whose `storage_1` lacks `data_dir` and whose `compute_1` names `partiton`"
)]
fn broken_revision(w: &mut Steps) -> TestResult {
    prepare(w)?;
    let _pid = start_serving(w)?;
    let body = bundle(
        &STORAGE.replace("data_dir = \"/data\"\n", ""),
        &format!("{COMPUTE}partiton = 1\n"),
    );
    w.reply = Some(ask(
        ADMIN_KEY,
        "POST",
        "/v1/targets/ci/revisions?base=1",
        &body,
    )?);
    Ok(())
}

#[then(
    "it is refused with status 422 listing both defects, each naming its instance and setting, and nothing is stored"
)]
fn refused_broken(w: &mut Steps) -> TestResult {
    let reply = w.reply.as_ref().context("a reply")?;
    assert_eq!(reply.status(), 422);
    says(
        &body_of(reply),
        &[
            "compute_1",
            "compute.partiton",
            "storage_1",
            "storage.data_dir",
        ],
    );
    says(&targets_list()?, &["ci latest 1"]);
    Ok(())
}

#[when("a request reaches a target's instance with no bearer key")]
fn no_key(w: &mut Steps) -> TestResult {
    prepare(w)?;
    let _pid = start_serving(w)?;
    w.reply = Some(ask("", "GET", "/v1/targets/ci/instances/storage_1", b"")?);
    Ok(())
}

#[then("it is refused with status 401, and its body names no setting")]
fn refused_no_key(w: &mut Steps) -> TestResult {
    let reply = w.reply.as_ref().context("a reply")?;
    assert_eq!(reply.status(), 401);
    let text = body_of(reply);
    assert!(
        !text.contains("data_dir") && !text.contains("cluster"),
        "{text}"
    );
    Ok(())
}

#[when("a request adds a revision with the read key")]
fn read_key_writes(w: &mut Steps) -> TestResult {
    prepare(w)?;
    let _pid = start_serving(w)?;
    let body = bundle(STORAGE, COMPUTE);
    w.reply = Some(ask(
        READ_KEY,
        "POST",
        "/v1/targets/ci/revisions?base=1",
        &body,
    )?);
    Ok(())
}

#[then("it is refused with status 403, and nothing is stored")]
fn refused_read_key(w: &mut Steps) -> TestResult {
    assert_eq!(w.reply.as_ref().context("a reply")?.status(), 403);
    says(&targets_list()?, &["ci latest 1"]);
    Ok(())
}

#[when("a request names an instance the active revision does not hold")]
fn unknown_instance(w: &mut Steps) -> TestResult {
    prepare(w)?;
    let _pid = start_serving(w)?;
    w.reply = Some(ask(
        READ_KEY,
        "GET",
        "/v1/targets/ci/instances/storage_9",
        b"",
    )?);
    Ok(())
}

#[then("it is refused with status 404 naming the target, the revision and the instance")]
fn refused_instance(w: &mut Steps) -> TestResult {
    let reply = w.reply.as_ref().context("a reply")?;
    assert_eq!(reply.status(), 404);
    says(&body_of(reply), &["target ci", "revision 1", "storage_9"]);
    Ok(())
}

#[when("a target is retired and a role of it starts")]
fn retired(w: &mut Steps) -> TestResult {
    prepare(w)?;
    let _pid = start_serving(w)?;
    assert_eq!(
        ask(ADMIN_KEY, "DELETE", "/v1/targets/ci", b"")?.status(),
        200
    );
    w.reply = Some(ask(
        READ_KEY,
        "GET",
        "/v1/targets/ci/instances/storage_1",
        b"",
    )?);
    Ok(())
}

#[then(
    "the request is refused with status 404 saying the target is retired, and the target's revisions are still stored"
)]
fn refused_retired(w: &mut Steps) -> TestResult {
    let reply = w.reply.as_ref().context("a reply")?;
    assert_eq!(reply.status(), 404);
    says(&body_of(reply), &["retired"]);
    assert_eq!(
        ask(ADMIN_KEY, "GET", "/v1/targets/ci/revisions/1", b"")?.status(),
        200
    );
    Ok(())
}

#[when("the service starts with an admin key of 8 bytes")]
fn short_key(w: &mut Steps) -> TestResult {
    prepare(w)?;
    std::fs::write(w.folder.join("admin.key"), "12345678")?;
    start_refused(w)
}

#[then("it exits with code 78 naming the key's file and the 32 bytes it requires")]
fn short_key_named(w: &mut Steps) -> TestResult {
    assert_eq!(exit_code(w)?, 78, "{}", w.log);
    says(
        &w.log,
        &[&w.folder.join("admin.key").display().to_string(), "32"],
    );
    Ok(())
}

#[when("a role asks for an instance of a target whose only revision has not been activated")]
fn no_active(w: &mut Steps) -> TestResult {
    prepare(w)?;
    let _pid = start_serving(w)?;
    let begun = ask(
        ADMIN_KEY,
        "POST",
        "/v1/targets/staging/revisions?base=0",
        &bundle(STORAGE, COMPUTE),
    )?;
    assert_eq!(begun.status(), 201, "{}", body_of(&begun));
    w.reply = Some(ask(
        READ_KEY,
        "GET",
        "/v1/targets/staging/instances/storage_1",
        b"",
    )?);
    Ok(())
}

#[then("it is refused with status 404 saying the target has no active revision")]
fn refused_no_active(w: &mut Steps) -> TestResult {
    let reply = w.reply.as_ref().context("a reply")?;
    assert_eq!(reply.status(), 404);
    says(&body_of(reply), &["no active revision"]);
    Ok(())
}

#[when("a request's declared body is larger than the bound")]
fn oversized(w: &mut Steps) -> TestResult {
    prepare(w)?;
    let _pid = start_serving(w)?;
    let body = vec![b'x'; 512 * 1024 + 1];
    w.reply = Some(ask(
        ADMIN_KEY,
        "POST",
        "/v1/targets/ci/revisions?base=1",
        &body,
    )?);
    Ok(())
}

#[then("it is refused with status 413 before its body is read, and nothing is stored")]
fn refused_oversized(w: &mut Steps) -> TestResult {
    assert_eq!(w.reply.as_ref().context("a reply")?.status(), 413);
    says(&targets_list()?, &["ci latest 1"]);
    Ok(())
}

/// The client a script is: a socket that writes on, here after it has read the refusal.
#[when("a client goes on sending a body after the refusal of its declared size has arrived")]
fn still_sending(w: &mut Steps) -> TestResult {
    prepare(w)?;
    let _pid = start_serving(w)?;
    let piece = vec![b'x'; 64 * 1024];
    let declared = 512 * 1024 + 1;
    let head = format!(
        "POST /v1/targets/ci/revisions?base=1 HTTP/1.1\r\nHost: {ADDRESS}\r\nAuthorization: Bearer {ADMIN_KEY}\r\nContent-Length: {declared}\r\nConnection: close\r\n\r\n"
    );
    let mut stream = TcpStream::connect(ADDRESS)?;
    stream.set_read_timeout(Some(Duration::from_secs(START_SECONDS)))?;
    stream.write_all(&[head.as_bytes(), &piece].concat())?;
    let mut answer = vec![0; 1024];
    let first = stream.read(&mut answer)?;
    answer.truncate(first);
    // Long enough for a close behind the refusal to have reached this socket, as a reset, by the next write or the one after.
    for turn in 0..4 {
        std::thread::sleep(Duration::from_millis(10));
        stream
            .write_all(&piece)
            .map_err(|error| TestError::new(format!("write {turn} after the refusal: {error}")))?;
    }
    stream.shutdown(Shutdown::Write)?;
    let _rest: usize = stream.read_to_end(&mut answer)?;
    w.log = String::from_utf8(answer)?;
    Ok(())
}

#[then(
    "each of its writes is taken, it reads status 413 and then the end of the connection, and nothing is stored"
)]
fn refusal_arrived(w: &mut Steps) -> TestResult {
    assert!(w.log.starts_with("HTTP/1.1 413"), "{}", w.log);
    says(&targets_list()?, &["ci latest 1"]);
    Ok(())
}

#[when("a request asks for `/health` and `/ready` with no key")]
fn health(w: &mut Steps) -> TestResult {
    prepare(w)?;
    let _pid = start_serving(w)?;
    Ok(())
}

#[then("both answer 200")]
fn healthy(_w: &mut Steps) -> TestResult {
    assert_eq!(ask("", "GET", "/health", b"")?.status(), 200);
    assert_eq!(ask("", "GET", "/ready", b"")?.status(), 200);
    Ok(())
}

#[when("an administrator runs `ultragraphdb_config_cli` over settings the service would refuse")]
fn cli_fails(w: &mut Steps) -> TestResult {
    prepare(w)?;
    std::fs::write(
        w.folder.join("settings").join("ci_compute_1.toml"),
        format!("{COMPUTE}partiton = 1\n"),
    )?;
    cli(w)?;
    start_refused(w)
}

#[then("it prints the same defects the service would, and exits with code 78")]
fn cli_failed(w: &mut Steps) -> TestResult {
    let (text, code) = cli_text(w)?;
    assert_eq!(code, 78, "{text}");
    says(&text, &["compute.partiton", "compute_1"]);
    assert_eq!(exit_code(w)?, 78);
    let line = text
        .lines()
        .find(|line| line.contains("partiton"))
        .context("the defect")?;
    says(&w.log, &[line.trim_start_matches("error: ")]);
    Ok(())
}

#[when("an administrator runs it over valid settings")]
fn cli_valid(w: &mut Steps) -> TestResult {
    prepare(w)?;
    cli(w)
}

#[then(
    "it lists each target with its instances, says it did not compare against stored revisions, and exits with code 0"
)]
fn cli_passed(w: &mut Steps) -> TestResult {
    let (text, code) = cli_text(w)?;
    assert_eq!(code, 0, "{text}");
    says(
        &text,
        &[
            "target ci: compute_1, frontend, storage_1, storage_2",
            "not compared",
        ],
    );
    Ok(())
}

#[when("the directory holds `ci_storge_1.toml`, which no target declares")]
fn cli_stray(w: &mut Steps) -> TestResult {
    prepare(w)?;
    std::fs::write(w.folder.join("settings").join("ci_storge_1.toml"), "")?;
    cli(w)
}

#[then("the check warns naming the file, and its exit code is unchanged")]
fn cli_warned(w: &mut Steps) -> TestResult {
    let (text, code) = cli_text(w)?;
    assert_eq!(code, 0, "{text}");
    says(&text, &["warning", "ci_storge_1.toml"]);
    Ok(())
}

#[when("the config service starts with `--config` naming a file that does not exist")]
fn no_settings_file(w: &mut Steps) -> TestResult {
    prepare(w)?;
    std::fs::remove_file(w.folder.join("service.toml"))?;
    start_refused(w)
}

#[then("it exits with code 78 and an error naming the file")]
fn no_settings_file_named(w: &mut Steps) -> TestResult {
    assert_eq!(exit_code(w)?, 78, "{}", w.log);
    says(
        &w.log,
        &[&w.folder.join("service.toml").display().to_string()],
    );
    Ok(())
}

#[when("an administrator checks the `dev` and `dev-net` profiles")]
fn profiles(w: &mut Steps) -> TestResult {
    let binary = Path::new(ROOT_PATH).join(CLI);
    for profile in ["dev", "dev-net"] {
        let central = Path::new("ultragraphdb/ultragraphdb_config/profiles")
            .join(profile)
            .join("targets.toml");
        let output = Command::new(&binary).arg(central).env_clear().output()?;
        let text = String::from_utf8_lossy(&output.stdout);
        assert_eq!(output.status.code(), Some(0), "{profile}: {text}");
        w.log.push_str(&format!("{profile}: {text}"));
    }
    Ok(())
}

#[then("each passes, listing its frontend, storage and compute")]
fn profiles_pass(w: &mut Steps) {
    says(
        &w.log,
        &[
            "dev: ",
            "dev-net: ",
            "target dev: compute, frontend, storage",
            "target dev-net: compute, frontend, storage",
        ],
    );
}

#[when("an administrator activates a revision through the API and downloads target `ci`")]
fn downloads(w: &mut Steps) -> TestResult {
    prepare(w)?;
    let _pid = start_serving(w)?;
    assert_eq!(add(1, "second")?.status(), 201);
    assert_eq!(
        ask(ADMIN_KEY, "PUT", "/v1/targets/ci/active", b"2")?.status(),
        200
    );
    w.reply = Some(ask(ADMIN_KEY, "GET", "/v1/targets/ci/download", b"")?);
    Ok(())
}

#[then(
    "the answer holds that revision's files as received, names the revision and a file to save them as, and taken back it adds a revision"
)]
fn downloaded(w: &mut Steps) -> TestResult {
    let reply = w.reply.as_ref().context("a reply")?;
    assert_eq!(reply.status(), 200);
    assert_eq!(
        reply.body(),
        bundle(&format!("{STORAGE}# second\n"), COMPUTE)
    );
    assert_eq!(reply.header("X-Revision"), Some("2"));
    says(
        reply
            .header("Content-Disposition")
            .context("a file's name")?,
        &["attachment", "ci-revision-2.bundle"],
    );
    let back = ask(
        ADMIN_KEY,
        "POST",
        "/v1/targets/ci/revisions?base=2",
        reply.body(),
    )?;
    assert_eq!(back.status(), 201, "{}", body_of(&back));
    says(&targets_list()?, &["ci latest 3 active 2"]);
    Ok(())
}

#[when("a request downloads a target with the read key")]
fn download_read_key(w: &mut Steps) -> TestResult {
    prepare(w)?;
    let _pid = start_serving(w)?;
    w.reply = Some(ask(READ_KEY, "GET", "/v1/targets/ci/download", b"")?);
    Ok(())
}

#[then("it is refused with status 403")]
fn download_refused(w: &mut Steps) -> TestResult {
    assert_eq!(w.reply.as_ref().context("a reply")?.status(), 403);
    Ok(())
}

#[when("an administrator downloads a target whose only revision has not been activated")]
fn download_none_active(w: &mut Steps) -> TestResult {
    prepare(w)?;
    let _pid = start_serving(w)?;
    let begun = ask(
        ADMIN_KEY,
        "POST",
        "/v1/targets/staging/revisions?base=0",
        &bundle(STORAGE, COMPUTE),
    )?;
    assert_eq!(begun.status(), 201, "{}", body_of(&begun));
    w.reply = Some(ask(ADMIN_KEY, "GET", "/v1/targets/staging/download", b"")?);
    Ok(())
}

/// Runs the CLI's restore with `token` into `folder` of this scenario, holding its output.
fn restore(w: &mut Steps, url: &str, token: &str) -> TestResult {
    restore_with(w, url, token, &[], &[])
}

/// As [`restore`], with `more` arguments after the folder and `environment` set.
fn restore_with(
    w: &mut Steps,
    url: &str,
    token: &str,
    more: &[&str],
    environment: &[(&str, &str)],
) -> TestResult {
    let folder = w.folder.join("restored");
    w.output = Some(
        Command::new(Path::new(ROOT_PATH).join(CLI))
            .args(["restore", url, token])
            .arg(&folder)
            .args(more)
            .env_clear()
            .envs(environment.iter().copied())
            .output()?,
    );
    Ok(())
}

/// The connection a restore opens to `listener`, waited for within the start's bound.
fn accepted(listener: &std::net::TcpListener) -> TestResult<TcpStream> {
    for _ in 0..START_SECONDS * 100 {
        match listener.accept() {
            Ok((stream, _)) => return Ok(stream),
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                std::thread::sleep(Duration::from_millis(10));
            }
            Err(error) => return Err(error.into()),
        }
    }
    Err(TestError::new("the restore never connected"))
}

/// A restore of a server standing in for a config service, run while this thread answers its connections in order, one reply
/// each, a status and a body; the stand-in's URL is held in `graph`.
fn restore_from_stand_in(w: &mut Steps, replies: Vec<(u16, Vec<u8>)>) -> TestResult {
    let listener = std::net::TcpListener::bind("127.0.0.1:0")?;
    listener.set_nonblocking(true)?;
    w.graph = format!("http://{}", listener.local_addr()?);
    let restoring = Command::new(Path::new(ROOT_PATH).join(CLI))
        .args(["restore", &w.graph, ADMIN_KEY])
        .arg(w.folder.join("restored"))
        .env_clear()
        .stdout(Stdio::piped())
        .spawn()?;
    for (status, body) in replies {
        let mut stream = accepted(&listener)?;
        stream.set_nonblocking(false)?;
        // A request's head, read to its end within a bound: the restore sends no body.
        let mut head = Vec::new();
        let mut chunk = [0; 1024];
        while !head.windows(4).any(|window| window == b"\r\n\r\n") && head.len() < 16 * 1024 {
            let count = stream.read(&mut chunk)?;
            if count == 0 {
                break;
            }
            head.extend_from_slice(chunk.get(..count).context("the bytes read")?);
        }
        let reply = format!(
            "HTTP/1.1 {status} Stand-in\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            body.len()
        );
        stream.write_all(reply.as_bytes())?;
        stream.write_all(&body)?;
        stream.shutdown(Shutdown::Write)?;
    }
    w.output = Some(restoring.wait_with_output()?);
    Ok(())
}

/// The listing a stand-in answers for target `ci` at `revision`, active at `revision`.
fn listing_of(revision: &str) -> Vec<u8> {
    format!("ci latest {revision} active {revision}\n").into_bytes()
}

/// That the restore's output says `words`, with `code`, and wrote no folder.
fn restore_refused_with(w: &Steps, code: i32, words: &[&str]) -> TestResult {
    let (text, exited) = cli_text(w)?;
    assert_eq!(exited, code, "{text}");
    says(&text, words);
    assert!(!w.folder.join("restored").exists());
    Ok(())
}

#[when(
    "an administrator adds a revision of target `ci` through the API, activates none, and restores the service into a folder"
)]
fn restores(w: &mut Steps) -> TestResult {
    prepare(w)?;
    let pid = start_serving(w)?;
    assert_eq!(add(1, "edited")?.status(), 201);
    restore(w, &format!("http://{ADDRESS}"), ADMIN_KEY)?;
    let util = w.util.as_ref().context("the services")?;
    let _exit = util.stop_service(pid)?;
    Ok(())
}

#[then(
    "the folder holds the latest revision's files alone, says which is active, and a fresh service over it serves those settings"
)]
fn restored(w: &mut Steps) -> TestResult {
    let (text, code) = cli_text(w)?;
    assert_eq!(code, 0, "{text}");
    says(
        &text,
        &[
            "target ci: revision 2, the latest",
            "active revision is 1",
            "is restored",
        ],
    );
    let folder = w.folder.join("restored");
    let storage = std::fs::read_to_string(folder.join("ci").join("storage.toml"))?;
    assert_eq!(storage, format!("{STORAGE}# edited\n"));
    let own = std::fs::read_to_string(w.folder.join("service.toml"))?
        .replace(
            &w.folder
                .join("settings")
                .join("targets.toml")
                .display()
                .to_string(),
            &folder.join("targets.toml").display().to_string(),
        )
        .replace(
            &w.folder.join("data").display().to_string(),
            &w.folder.join("fresh").display().to_string(),
        );
    std::fs::write(w.folder.join("service.toml"), own)?;
    let _pid = start_serving(w)?;
    says(&targets_list()?, &["ci latest 1 active 1"]);
    let served = body_of(&ask(
        READ_KEY,
        "GET",
        "/v1/targets/ci/instances/storage_1",
        b"",
    )?);
    says(&served, &["partition_first = 1", "data_dir = \"/data\""]);
    let files = ask(ADMIN_KEY, "GET", "/v1/targets/ci/revisions/1", b"")?;
    says(&String::from_utf8_lossy(files.body()), &["# edited"]);
    Ok(())
}

#[when("a service holds a retired target and one with no active revision beside `ci`")]
fn partly_serving(w: &mut Steps) -> TestResult {
    prepare(w)?;
    let _pid = start_serving(w)?;
    let body = bundle(STORAGE, COMPUTE);
    for target in ["idle", "old"] {
        let path = format!("/v1/targets/{target}/revisions?base=0");
        assert_eq!(ask(ADMIN_KEY, "POST", &path, &body)?.status(), 201);
    }
    assert_eq!(
        ask(ADMIN_KEY, "PUT", "/v1/targets/old/active", b"1")?.status(),
        200
    );
    assert_eq!(
        ask(ADMIN_KEY, "DELETE", "/v1/targets/old", b"")?.status(),
        200
    );
    restore(w, &format!("http://{ADDRESS}"), ADMIN_KEY)
}

#[then(
    "the restore holds `ci` and the one with no active revision, and says it left out the retired one"
)]
fn left_out(w: &mut Steps) -> TestResult {
    let (text, code) = cli_text(w)?;
    assert_eq!(code, 0, "{text}");
    says(
        &text,
        &[
            "target old is retired",
            "target ci: revision 1",
            "target idle: revision 1",
        ],
    );
    let folder = w.folder.join("restored");
    assert!(folder.join("ci").is_dir());
    assert!(folder.join("idle").is_dir());
    assert!(!folder.join("old").exists());
    Ok(())
}

#[when("a restore is run with the read key")]
fn restore_read_key(w: &mut Steps) -> TestResult {
    prepare(w)?;
    let _pid = start_serving(w)?;
    restore(w, &format!("http://{ADDRESS}"), READ_KEY)
}

#[then(
    "it exits with code 78 saying the token was refused, prints no token, and no folder appears"
)]
fn restore_refused(w: &mut Steps) -> TestResult {
    let (text, code) = cli_text(w)?;
    assert_eq!(code, 78, "{text}");
    says(&text, &["refused the token"]);
    assert!(!text.contains(READ_KEY), "{text}");
    assert!(!w.folder.join("restored").exists());
    Ok(())
}

#[when("a restore names a folder holding a file")]
fn restore_occupied(w: &mut Steps) -> TestResult {
    prepare(w)?;
    let _pid = start_serving(w)?;
    let folder = w.folder.join("restored");
    std::fs::create_dir_all(&folder)?;
    std::fs::write(folder.join("mine.toml"), "# an administrator's own\n")?;
    restore(w, &format!("http://{ADDRESS}"), ADMIN_KEY)
}

#[then("it exits with code 74 saying so, and the folder is as it was")]
fn restore_left_alone(w: &mut Steps) -> TestResult {
    let (text, code) = cli_text(w)?;
    assert_eq!(code, 74, "{text}");
    says(&text, &["already holds files", "absent or empty"]);
    let entries: Vec<_> = std::fs::read_dir(w.folder.join("restored"))?
        .flatten()
        .collect();
    assert_eq!(entries.len(), 1);
    Ok(())
}

#[when("a restore names a URL where nothing answers")]
fn restore_down(w: &mut Steps) -> TestResult {
    prepare(w)?;
    let port = std::net::TcpListener::bind("127.0.0.1:0")?
        .local_addr()?
        .port();
    w.graph = format!("127.0.0.1:{port}");
    let url = format!("http://{}", w.graph);
    restore(w, &url, ADMIN_KEY)
}

#[then("it exits with code 69 naming the URL")]
fn restore_unreachable(w: &mut Steps) -> TestResult {
    let (text, code) = cli_text(w)?;
    assert_eq!(code, 69, "{text}");
    says(&text, &[&w.graph]);
    Ok(())
}

/// A listing line naming no target, as a stand-in answers it.
const NO_TARGET: &str = "no target is named here";
/// A revision that is no number.
const WORDED_REVISION: &str = "two";
/// The status and body of a stand-in failing.
const FAILED_STATUS: u16 = 500;
const FAILED_SAID: &str = "the stand-in failed";
/// A trust file that does not exist.
const NO_TRUST_FILE: &str = "/nonexistent/ultragraphdb/trust.pem";

#[when("a service's one target is retired and a restore is run")]
fn restore_all_retired(w: &mut Steps) -> TestResult {
    prepare(w)?;
    let _pid = start_serving(w)?;
    assert_eq!(
        ask(ADMIN_KEY, "DELETE", "/v1/targets/ci", b"")?.status(),
        200
    );
    restore(w, &format!("http://{ADDRESS}"), ADMIN_KEY)
}

#[then(
    "it exits with code 78 saying the service holds no target but retired ones, and no folder appears"
)]
fn restore_nothing_held(w: &mut Steps) -> TestResult {
    restore_refused_with(w, 78, &["holds no target but retired ones", ADDRESS])
}

#[when("a restore reads a listing holding a line that names no target")]
fn restore_listing_wrong(w: &mut Steps) -> TestResult {
    prepare(w)?;
    restore_from_stand_in(w, vec![(200, format!("{NO_TARGET}\n").into_bytes())])
}

#[then("it exits with code 78 naming the URL and the line, and no folder appears")]
fn restore_listing_refused(w: &mut Steps) -> TestResult {
    let url = w.graph.clone();
    restore_refused_with(w, 78, &[&url, NO_TARGET])
}

#[when("a restore reads a listing naming revision `two` of target `ci`")]
fn restore_revision_worded(w: &mut Steps) -> TestResult {
    prepare(w)?;
    restore_from_stand_in(w, vec![(200, listing_of(WORDED_REVISION))])
}

#[then("it exits with code 78 naming `two` as the revision, and no folder appears")]
fn restore_revision_refused(w: &mut Steps) -> TestResult {
    restore_refused_with(w, 78, &[&format!("{WORDED_REVISION} as a revision")])
}

#[when("a restore's listing is answered with status 500")]
fn restore_failed_status(w: &mut Steps) -> TestResult {
    prepare(w)?;
    restore_from_stand_in(w, vec![(FAILED_STATUS, FAILED_SAID.as_bytes().to_vec())])
}

#[then(
    "it exits with code 78 naming the URL, the status and what the server said, and no folder appears"
)]
fn restore_status_refused(w: &mut Steps) -> TestResult {
    let url = w.graph.clone();
    restore_refused_with(
        w,
        78,
        &[&url, &format!("status {FAILED_STATUS}"), FAILED_SAID],
    )
}

#[when("a restore reads a revision of target `ci` whose `storage` lacks `data_dir`")]
fn restore_refused_settings(w: &mut Steps) -> TestResult {
    prepare(w)?;
    let revision = bundle(&STORAGE.replace("data_dir = \"/data\"\n", ""), COMPUTE);
    restore_from_stand_in(w, vec![(200, listing_of("1")), (200, revision)])
}

#[then("it exits with code 78 naming the central file it wrote and `storage.data_dir`")]
fn restore_settings_refused(w: &mut Steps) -> TestResult {
    let (text, code) = cli_text(w)?;
    assert_eq!(code, 78, "{text}");
    let central = w.folder.join("restored").join("targets.toml");
    says(
        &text,
        &[
            &central.display().to_string(),
            "this release refuses it",
            "storage.data_dir",
        ],
    );
    assert!(central.is_file());
    Ok(())
}

#[when("a restore names an `https` URL and `--ca-file` naming no file")]
fn restore_trust_named(w: &mut Steps) -> TestResult {
    prepare(w)?;
    restore_with(
        w,
        &format!("https://{ADDRESS}"),
        ADMIN_KEY,
        &["--ca-file", NO_TRUST_FILE],
        &[],
    )
}

#[when("a restore names an `https` URL with `SSL_CERT_FILE` naming no file")]
fn restore_trust_system(w: &mut Steps) -> TestResult {
    prepare(w)?;
    restore_with(
        w,
        &format!("https://{ADDRESS}"),
        ADMIN_KEY,
        &[],
        &[("SSL_CERT_FILE", NO_TRUST_FILE)],
    )
}

#[then("it exits with code 78 naming the trust file, and no folder appears")]
fn restore_trust_refused(w: &mut Steps) -> TestResult {
    restore_refused_with(w, 78, &[NO_TRUST_FILE])
}

#[when("an administrator runs `ultragraphdb_config_cli` with no argument")]
fn cli_bare(w: &mut Steps) -> TestResult {
    w.output = Some(
        Command::new(Path::new(ROOT_PATH).join(CLI))
            .env_clear()
            .output()?,
    );
    Ok(())
}

#[then("it prints both forms of its command line and exits with code 64")]
fn cli_usage(w: &mut Steps) -> TestResult {
    let (text, code) = cli_text(w)?;
    assert_eq!(code, 64, "{text}");
    says(
        &text,
        &[
            "usage:",
            &format!("{CLI} <central file>"),
            &format!("{CLI} restore <url>"),
        ],
    );
    Ok(())
}

#[when(
    "a config service starts with `ULTRAGRAPHDB_PARENT_PID` naming a process that is not its parent"
)]
fn parent_gone(w: &mut Steps) -> TestResult {
    prepare(w)?;
    // No process holds the largest identity, so it is no parent of the service's.
    w.environment
        .push(("ULTRAGRAPHDB_PARENT_PID".to_string(), u32::MAX.to_string()));
    start_refused(w)
}

#[then("it stops as on SIGTERM and exits with code 0")]
fn stopped_with_its_parent(w: &mut Steps) -> TestResult {
    let status = w.exited.context("an exit")?;
    assert_eq!(status.code(), Some(0), "{}", w.log);
    Ok(())
}

#[when(
    "a config service starts at an address another process holds, which lets it go within the bound"
)]
fn port_held_a_moment(w: &mut Steps) -> TestResult {
    prepare(w)?;
    let held = std::net::TcpListener::bind(ADDRESS)?;
    let _pid = start(w, WaitStrategy::NoWait)?;
    let log = service(w).log;
    let mut retried = false;
    for _ in 0..START_SECONDS * 100 {
        retried = std::fs::read_to_string(&log).is_ok_and(|text| text.contains("in use"));
        if retried {
            break;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    drop(held);
    assert!(
        retried,
        "a retry in the log, {START_SECONDS} s on: {:?}",
        std::fs::read_to_string(&log)
    );
    Ok(())
}

#[then("it logs each retry naming the address, binds it, and answers `/health`")]
fn took_the_port(w: &mut Steps) -> TestResult {
    let mut status = None;
    for _ in 0..START_SECONDS * 20 {
        status = ask(READ_KEY, "GET", "/health", b"")
            .ok()
            .map(|reply| reply.status());
        if status == Some(200) {
            break;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    assert_eq!(status, Some(200), "{ADDRESS}/health");
    let log = std::fs::read_to_string(service(w).log)?;
    assert!(
        log.lines()
            .any(|line| line.contains("in use") && line.contains(ADDRESS)),
        "{log}"
    );
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
