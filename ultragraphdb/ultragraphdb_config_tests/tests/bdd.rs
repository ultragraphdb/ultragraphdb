/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The config service and its CLI as processes, end to end and with no container, bound to `features/`. One scenario runs at
//! a time: the service listens at a fixed port.

use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus, Output};
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
        .env_vars(Vec::new())
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
        .env_vars(Vec::new())
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

fn main() -> TestResult {
    futures::executor::block_on(
        Steps::cucumber()
            .max_concurrent_scenarios(1)
            .fail_on_skipped()
            .run_and_exit(ultragraphdb_tool_bdd::features_dir()?),
    );
    Ok(())
}
