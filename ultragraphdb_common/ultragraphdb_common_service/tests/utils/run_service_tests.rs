/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::collections::BTreeSet;
use std::process::{Command, ExitCode, Stdio};
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use compio::BufResult;
use compio::driver::DriverType;
use compio::io::{AsyncReadExt, AsyncWrite, AsyncWriteExt};
use compio::net::TcpStream;
use compio::runtime::{JoinHandle, Runtime, spawn, spawn_blocking};
use ultragraphdb_common_service::{
    Bootstrap, CpuBinding, PARENT_PID, Readiness, RoleIdentity, RoleKind, RoleStart, ServiceEnd,
    Stopping, allowed_in_status, binding, core_runtime, driver_name, exit_status, fetch_settings,
    role_help, run_service, serve_service, stop_flag, stop_with_parent,
};
use ultragraphdb_common_service_config::ServiceConfig;
use ultragraphdb_error_role::RoleError;
use ultragraphdb_tool_bdd::{Context, TestError, TestResult};
use ultragraphdb_transport_http_server::{
    HttpLimits, HttpRequest, HttpResponse, HttpService, serve_http,
};

const HEALTH: &str = "GET /health HTTP/1.1\r\n\r\n";
const READY: &str = "GET /ready HTTP/1.1\r\n\r\n";

/// How long a test waits on a service it has told to end, or on anything else it is owed at once: far past what either takes.
const WAIT: Duration = Duration::from_secs(5);

/// `future`'s output, or a failure naming `what` when it takes past [`WAIT`].
async fn within<T>(what: &str, future: impl Future<Output = T>) -> TestResult<T> {
    compio::time::timeout(WAIT, future)
        .await
        .map_err(|_| TestError::new(format!("{what} within {WAIT:?}")))
}

/// A refused start stops the role before it binds or installs anything: no environment, or any argument.
#[test]
fn test_a_refused_start_returns_its_error() -> TestResult {
    for (args, named) in [
        (vec![], "ULTRAGRAPHDB_CONFIG_URL"),
        (vec!["--config".to_string()], "--config"),
    ] {
        let refused = run_service(
            RoleKind::Storage,
            &args,
            [],
            |_start, _readiness, _stopping| async { Ok(()) },
        )
        .err()
        .context("a refusal")?;
        assert_eq!(refused.exit_code(), RoleError::EX_CONFIG);
        assert!(refused.to_string().contains(named), "{refused}");
    }
    Ok(())
}

/// `--help` ends the role before it reads its environment, which holds nothing here.
#[test]
fn test_help_starts_nothing() -> TestResult {
    run_service(
        RoleKind::Storage,
        &["--help".to_string()],
        [],
        |_start, _readiness, _stopping| async { Err(RoleError::software("it started")) },
    )?;
    Ok(())
}

#[test]
fn test_a_role_s_help_names_it_its_variables_and_the_codes() {
    let help = role_help(RoleKind::Compute);
    for said in [
        "ultragraphdb_compute",
        "ULTRAGRAPHDB_CONFIG_URL",
        RoleError::EXIT_CODES,
    ] {
        assert!(help.contains(said), "{said}: {help}");
    }
}

/// The log names a driver as the role-binaries spec does.
#[test]
fn test_a_driver_has_its_name() {
    assert_eq!(driver_name(DriverType::Poll), "polling");
    assert_eq!(driver_name(DriverType::IoUring), "io_uring");
    assert_eq!(driver_name(DriverType::IOCP), "iocp");
}

#[test]
fn test_an_outcome_chooses_the_exit_status() {
    assert_eq!(exit_status(Ok(())), ExitCode::SUCCESS);
    let refused = RoleError::configuration("x");
    let code = ExitCode::from(refused.exit_code());
    assert_eq!(exit_status(Err(refused)), code);
}

#[test]
fn test_a_binding_is_read_back_only_when_asked_and_known() {
    let held = BTreeSet::from([3]);
    assert_eq!(
        binding(None, Some(3), Some(held.clone())),
        CpuBinding::NotAsked
    );
    assert_eq!(
        binding(Some(false), Some(3), Some(held.clone())),
        CpuBinding::NotAsked
    );
    assert_eq!(
        binding(Some(true), None, Some(held.clone())),
        CpuBinding::Unavailable
    );
    assert_eq!(binding(Some(true), Some(3), None), CpuBinding::Unavailable);
    assert_eq!(
        binding(Some(true), Some(3), Some(held.clone())),
        CpuBinding::ReadBack { asked: 3, held }
    );
}

/// A start listening at a free port of this host.
fn start_on_free_port() -> TestResult<RoleStart> {
    let port = std::net::TcpListener::bind("127.0.0.1:0")?
        .local_addr()?
        .port();
    let address = format!("tcp://127.0.0.1:{port}");
    let text = format!("[observability]\naddress = \"{address}\"\n");
    Ok(RoleStart::from_settings(
        text.as_bytes(),
        RoleIdentity::new("ci", "storage", 1),
    )?)
}

/// A role whose work sets its readiness to `ready`, says it listens when `listens` says so, and ends, leaving the service
/// serving.
fn role_ready(
    ready: bool,
    listens: bool,
) -> impl FnOnce(RoleStart, Readiness, Stopping) -> std::future::Ready<Result<(), RoleError>> {
    move |_start, readiness, _stopping| {
        readiness.set(ready);
        if listens {
            readiness.set_listening();
        }
        std::future::ready(Ok(()))
    }
}

/// The role `kind`, ready as `ready` says and listening as `listens` says, serving `start` on this runtime until the flag it returns is set.
fn serving(
    kind: RoleKind,
    start: &RoleStart,
    ready: bool,
    listens: bool,
) -> (Arc<AtomicBool>, JoinHandle<Result<ServiceEnd, RoleError>>) {
    let stop = Arc::new(AtomicBool::new(false));
    let (owned, flag) = (start.clone(), Arc::clone(&stop));
    let task = spawn(async move {
        serve_service(
            kind,
            &owned,
            CpuBinding::NotAsked,
            &flag,
            role_ready(ready, listens),
        )
        .await
    });
    (stop, task)
}

/// Stops the role and checks it ended cleanly.
async fn stopped(stop: &AtomicBool, task: JoinHandle<Result<ServiceEnd, RoleError>>) -> TestResult {
    stop.store(true, Ordering::Release);
    let outcome = within("the service's end", task)
        .await?
        .map_err(|_| TestError::new("the role panicked"))?;
    assert_eq!(outcome?, ServiceEnd::Stopped);
    Ok(())
}

/// Connects to the role, retrying while its listener is not up yet.
async fn connect(start: &RoleStart) -> TestResult<TcpStream> {
    for _ in 0..200 {
        if let Ok(stream) = TcpStream::connect(start.observability()).await {
            return Ok(stream);
        }
        compio::time::sleep(Duration::from_millis(5)).await;
    }
    Err(TestError::new("the listener never came up"))
}

/// All the role answers on `stream`, until it closes.
async fn answer(stream: &mut TcpStream) -> TestResult<String> {
    let BufResult(read, bytes) = stream.read_to_end(Vec::new()).await;
    let _count: usize = read?;
    Ok(String::from_utf8(bytes)?)
}

/// What the role answers `parts`, written in turn with a yield between them.
async fn exchange(start: &RoleStart, parts: &[&str]) -> TestResult<String> {
    let mut stream = connect(start).await?;
    for part in parts {
        let BufResult(written, _) = stream.write_all(part.as_bytes().to_vec()).await;
        written?;
        compio::time::sleep(Duration::from_millis(5)).await;
    }
    answer(&mut stream).await
}

fn status(text: &str) -> Option<&str> {
    text.split(' ').nth(1)
}

/// The listener end to end, on one runtime: what each role answers, a request arriving in pieces, and stopping.
#[test]
fn test_a_role_answers_until_it_is_stopped() -> TestResult {
    let runtime = Runtime::new()?;
    runtime.block_on(async {
        // A frontend's `/health` waits for the role to say it has bound its addresses; every other role's answers at once.
        for (kind, ready, listens, healthy, answers) in [
            (RoleKind::Storage, true, false, "200", "200"),
            (RoleKind::Frontend, false, false, "503", "503"),
            (RoleKind::Frontend, false, true, "200", "503"),
            (RoleKind::Compute, false, false, "200", "503"),
        ] {
            let start = start_on_free_port()?;
            let (stop, task) = serving(kind, &start, ready, listens);
            let health = exchange(&start, &[HEALTH]).await?;
            assert_eq!(status(&health), Some(healthy), "{kind}");
            let answered = exchange(&start, &[READY]).await?;
            assert_eq!(status(&answered), Some(answers), "{kind}");
            let (first, last) = HEALTH.split_at(7);
            let pieces = exchange(&start, &[first, last]).await?;
            assert_eq!(status(&pieces), Some(healthy), "a request in two writes");
            assert!(!task.is_finished(), "a role serves until it is stopped");
            stopped(&stop, task).await?;
        }
        Ok(())
    })
}

/// A peer that stops sending before its request ends gets a 400, and takes nothing from the next.
#[test]
fn test_a_truncated_request_is_answered_and_closed() -> TestResult {
    let runtime = Runtime::new()?;
    runtime.block_on(async {
        let start = start_on_free_port()?;
        let (stop, task) = serving(RoleKind::Storage, &start, true, false);
        let mut stream = connect(&start).await?;
        let BufResult(written, _) = stream.write_all(b"GET /hea".to_vec()).await;
        written?;
        stream.shutdown().await?;
        let refused = answer(&mut stream).await?;
        assert_eq!(status(&refused), Some("400"));
        let health = exchange(&start, &[HEALTH]).await?;
        assert_eq!(status(&health), Some("200"));
        stopped(&stop, task).await
    })
}

/// An address taken stops the role as unavailable, with the code that says so.
#[test]
fn test_an_address_taken_is_unavailable() -> TestResult {
    let held = std::net::TcpListener::bind("127.0.0.1:0")?;
    let address = format!("tcp://{}", held.local_addr()?);
    let text = format!("[observability]\naddress = \"{address}\"\n");
    let start = RoleStart::from_settings(text.as_bytes(), RoleIdentity::new("ci", "storage", 1))?;
    let runtime = Runtime::new()?;
    let stop = AtomicBool::new(false);
    let serving = serve_service(
        RoleKind::Storage,
        &start,
        CpuBinding::NotAsked,
        &stop,
        role_ready(true, false),
    );
    // Past the 5 s of retries a port in use is given.
    let refused = runtime
        .block_on(compio::time::timeout(2 * WAIT, serving))
        .map_err(|_| TestError::new("a refusal past the retries"))?
        .err()
        .context("a refusal")?;
    assert_eq!(refused.exit_code(), RoleError::EX_UNAVAILABLE);
    Ok(())
}

/// A role's failure ends the service with that failure, its code the role's.
#[test]
fn test_a_failing_role_ends_the_service_with_its_error() -> TestResult {
    let start = start_on_free_port()?;
    let runtime = Runtime::new()?;
    let stop = AtomicBool::new(false);
    let refused = runtime
        .block_on(serve_service(
            RoleKind::Compute,
            &start,
            CpuBinding::NotAsked,
            &stop,
            |_start, _readiness, _stopping| {
                std::future::ready(Err(RoleError::configuration("no secret")))
            },
        ))
        .err()
        .context("the role's failure")?;
    assert_eq!(refused.exit_code(), RoleError::EX_CONFIG);
    assert!(refused.to_string().contains("no secret"), "{refused}");
    Ok(())
}

/// A role still working is told to stop, and the service waits for it to flush and end.
#[test]
fn test_a_role_working_at_the_stop_ends_before_the_service() -> TestResult {
    let start = start_on_free_port()?;
    let runtime = Runtime::new()?;
    let stop = Arc::new(AtomicBool::new(false));
    let flushed = Arc::new(AtomicBool::new(false));
    let (flag, done) = (Arc::clone(&stop), Arc::clone(&flushed));
    runtime.block_on(async {
        let role = move |_start, _readiness, stopping: Stopping| async move {
            while !stopping.is_stopping() {
                compio::time::sleep(Duration::from_millis(2)).await;
            }
            done.store(true, Ordering::Release);
            Ok(())
        };
        let service = spawn(async move {
            serve_service(RoleKind::Storage, &start, CpuBinding::NotAsked, &flag, role).await
        });
        compio::time::sleep(Duration::from_millis(50)).await;
        stop.store(true, Ordering::Release);
        let ended = within("the service's end", service)
            .await?
            .map_err(|_| TestError::new("the service panicked"))?;
        assert_eq!(ended?, ServiceEnd::Stopped);
        assert!(
            flushed.load(Ordering::Acquire),
            "the role flushed before the service ended"
        );
        Ok(())
    })
}

/// SIGTERM and SIGINT each set the stop flag, and neither ends the process: a role stops by its own steps.
#[test]
fn test_a_signal_sets_the_stop_flag() -> TestResult {
    let stop = stop_flag()?;
    for signal in [signal_hook::consts::SIGTERM, signal_hook::consts::SIGINT] {
        stop.store(false, Ordering::Release);
        signal_hook::low_level::raise(signal)?;
        assert!(stop.load(Ordering::Acquire), "signal {signal}");
    }
    Ok(())
}

/// A runtime pins its thread only when the settings ask, to the first CPU the thread may run on, and says what it holds as
/// the operating system reads it back; where the system says nothing, as on macOS, the binding is unavailable.
#[test]
fn test_a_runtime_pins_only_when_asked() -> TestResult {
    let permitted = std::fs::read_to_string("/proc/thread-self/status")
        .ok()
        .and_then(|status| allowed_in_status(&status));
    let expected = match permitted.and_then(|cpus| cpus.first().copied()) {
        Some(first) => CpuBinding::ReadBack {
            asked: first,
            held: BTreeSet::from([first]),
        },
        None => CpuBinding::Unavailable,
    };
    let pinned = ServiceConfig::parse(b"[runtime]\npin_cores = true\n")?;
    assert_eq!(core_runtime(&pinned, 0)?.1, expected);
    for text in [&b"[runtime]\npin_cores = false\n"[..], b""] {
        let unpinned = ServiceConfig::parse(text)?;
        assert_eq!(core_runtime(&unpinned, 0)?.1, CpuBinding::NotAsked);
    }
    Ok(())
}

/// The admin key, and its SHA-256 digest by `shasum -a 256`.
const ADMIN_KEY: &str = "0123456789abcdef0123456789abcdef";
const ADMIN_DIGEST: &str = "3eb1bd439947eb762998e566ccc2e099c791118b2f40579cc4f7da2b5061b7f9";

/// A config service answering every request with `settings` at revision 3.
#[derive(Clone)]
struct Settings(Rc<str>);

impl HttpService for Settings {
    async fn call(&self, _request: HttpRequest) -> HttpResponse {
        HttpResponse::new(200)
            .with_header("X-Revision", "3")
            .with_body(self.0.as_bytes().to_vec())
    }
}

/// A free port of this host.
fn free_port() -> TestResult<u16> {
    Ok(std::net::TcpListener::bind("127.0.0.1:0")?
        .local_addr()?
        .port())
}

/// A config service on this runtime serving settings whose admin listener is at `admin`, and what a storage role's
/// variables say for it.
async fn config_service(admin: u16) -> TestResult<Bootstrap> {
    let settings = format!(
        "[observability]\naddress = \"tcp://127.0.0.1:{}\"\n[admin]\naddress = \"tcp://127.0.0.1:{admin}\"\nplaintext = true\nkey_sha256 = \"{ADMIN_DIGEST}\"\n",
        free_port()?
    );
    let listener = compio::net::TcpListener::bind("127.0.0.1:0").await?;
    let port = listener.local_addr()?.port();
    let service = Settings(Rc::from(settings));
    spawn(async move {
        while let Ok((stream, _)) = listener.accept().await {
            serve_http(stream, &HttpLimits::new(0, WAIT), &service).await;
        }
    })
    .detach();
    let root = std::env::var_os("TEST_TMPDIR").map_or_else(std::env::temp_dir, Into::into);
    let key = std::path::Path::new(&root).join(format!("load-{}", std::process::id()));
    std::fs::write(&key, "a read key of at least thirty two bytes")?;
    Ok(Bootstrap::read(
        RoleKind::Storage,
        [
            (
                "ULTRAGRAPHDB_CONFIG_URL",
                format!("http://127.0.0.1:{port}"),
            ),
            ("ULTRAGRAPHDB_CONFIG_TARGET", "ci".to_string()),
            ("ULTRAGRAPHDB_CONFIG_INSTANCE", "storage_1".to_string()),
            ("ULTRAGRAPHDB_CONFIG_KEY_FILE", key.display().to_string()),
        ]
        .map(|(name, value)| (name.to_string(), value)),
    )?)
}

/// The status the admin listener at `port` answers a load of `body` presenting `key` with.
async fn load(port: u16, key: Option<&str>, body: &str) -> TestResult<String> {
    let mut stream = None;
    for _ in 0..200 {
        if let Ok(connected) = TcpStream::connect(("127.0.0.1", port)).await {
            stream = Some(connected);
            break;
        }
        compio::time::sleep(Duration::from_millis(5)).await;
    }
    let mut stream = stream.context("the admin listener")?;
    let bearer = key.map_or(String::new(), |key| {
        format!("Authorization: Bearer {key}\r\n")
    });
    let request = format!(
        "POST /admin/load-config HTTP/1.1\r\n{bearer}Content-Length: {}\r\n\r\n{body}",
        body.len()
    );
    let BufResult(written, _) = stream.write_all(request.into_bytes()).await;
    written?;
    let reply = within("the admin's answer", answer(&mut stream)).await??;
    Ok(status(&reply).context("a status")?.to_string())
}

/// A role whose settings name an admin listener serves it; a load there, with the admin key alone, ends the service at the
/// revision it names, with no signal.
#[test]
fn test_a_load_ends_the_service_at_its_revision() -> TestResult {
    Runtime::new()?.block_on(async {
        let admin = free_port()?;
        let bootstrap = config_service(admin).await?;
        let start = fetch_settings(&bootstrap, WAIT, &AtomicBool::new(false))
            .await?
            .context("a start")?;
        let stop = Arc::new(AtomicBool::new(false));
        let flag = Arc::clone(&stop);
        let task = spawn(async move {
            let role = role_ready(true, false);
            serve_service(RoleKind::Storage, &start, CpuBinding::NotAsked, &flag, role).await
        });
        assert_eq!(
            load(admin, Some("a wrong key of thirty two bytes!!"), "3").await?,
            "403"
        );
        assert_eq!(load(admin, Some(ADMIN_KEY), "3").await?, "202");
        let ended = within("the service's end", task)
            .await?
            .map_err(|_| TestError::new("the service panicked"))?;
        assert_eq!(ended?, ServiceEnd::Load(3));
        assert!(!stop.load(Ordering::Acquire), "no signal ended it");
        Ok(())
    })
}

/// Set in a child run of this binary to the parent `child_runs_a_role_named_a_parent` names its role: `gone` for a process
/// that is not its parent, `kept` for the parent it has, `variable` for the one its own environment names.
const PARENT_CHILD: &str = "UGDB_RUN_SERVICE_PARENT_CHILD";

/// How long a role named the parent it has is watched staying up.
const KEPT: Duration = Duration::from_secs(1);

/// In a child run, a storage role named a parent by `ULTRAGRAPHDB_PARENT_PID`, `gone` or `kept`, against a config service
/// at a closed port, and prints what became of it within its bound: `outcome=ended=ok`, `outcome=ended=<the error>` or
/// `outcome=running`. Each child is a process of its own, since a role installs its telemetry once a process.
#[test]
fn child_runs_a_role_named_a_parent() -> TestResult {
    let Some(named) = std::env::var_os(PARENT_CHILD) else {
        return Ok(());
    };
    if named == "variable" {
        println!("outcome={}", stopped_with_parent()?);
        return Ok(());
    }
    let (parent, bound) = if named == "kept" {
        (std::os::unix::process::parent_id(), KEPT)
    } else {
        (u32::MAX, WAIT)
    };
    let closed = free_port()?;
    let root = std::env::var_os("TEST_TMPDIR").map_or_else(std::env::temp_dir, Into::into);
    let key = std::path::Path::new(&root).join(format!("parent-{}", std::process::id()));
    std::fs::write(&key, "a read key of at least thirty two bytes")?;
    let variables = [
        (
            "ULTRAGRAPHDB_CONFIG_URL",
            format!("http://127.0.0.1:{closed}"),
        ),
        ("ULTRAGRAPHDB_CONFIG_TARGET", "ci".to_string()),
        ("ULTRAGRAPHDB_CONFIG_INSTANCE", "storage_1".to_string()),
        ("ULTRAGRAPHDB_CONFIG_KEY_FILE", key.display().to_string()),
        ("ULTRAGRAPHDB_PARENT_PID", parent.to_string()),
    ]
    .map(|(name, value)| (name.to_string(), value));
    let outcome = Runtime::new()?.block_on(async move {
        let running = spawn_blocking(move || {
            run_service(
                RoleKind::Storage,
                &[],
                variables,
                |_start, _readiness, _stopping| async { Ok(()) },
            )
        });
        match compio::time::timeout(bound, running).await {
            Ok(Ok(Ok(()))) => "ended=ok".to_string(),
            Ok(Ok(Err(error))) => format!("ended={error}"),
            Ok(Err(_)) => "ended=a panic".to_string(),
            Err(_) => "running".to_string(),
        }
    });
    println!("outcome={outcome}");
    Ok(())
}

/// Whether a flag [`stop_with_parent`] watches, for the process this one's environment names in [`PARENT_PID`], is set
/// within [`WAIT`]: `stopped` or `running`.
fn stopped_with_parent() -> TestResult<&'static str> {
    let stop = Arc::new(AtomicBool::new(false));
    stop_with_parent(&stop)?;
    for _ in 0..WAIT.as_millis() / 10 {
        if stop.load(Ordering::Acquire) {
            return Ok("stopped");
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    Ok("running")
}

/// What a child run of `child_runs_a_role_named_a_parent` printed for the parent `named`, its own environment naming in
/// [`PARENT_PID`] a process that is not its parent, the child waited on for at most 10 s.
fn child_outcome(named: &str) -> TestResult<String> {
    let mut child = Command::new(std::env::current_exe()?)
        .args([
            "utils::run_service_tests::child_runs_a_role_named_a_parent",
            "--exact",
            "--nocapture",
            "--test-threads=1",
        ])
        .env(PARENT_CHILD, named)
        .env(PARENT_PID, u32::MAX.to_string())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()?;
    let mut polls = 0_u32;
    while child.try_wait()?.is_none() {
        polls += 1;
        if polls > 200 {
            child.kill()?;
            return Err(TestError::new(format!("the {named} child within 10 s")));
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    let stdout = String::from_utf8(child.wait_with_output()?.stdout)?;
    stdout
        .lines()
        .find_map(|line| {
            line.split_once("outcome=")
                .map(|(_, outcome)| outcome.to_string())
        })
        .context(&format!("the {named} child prints its outcome in {stdout}"))
}

/// A role started with `ULTRAGRAPHDB_PARENT_PID` stops as on SIGTERM once that process is not its parent, and runs on while
/// it is (config-service spec). The config service is away, so the watch alone ends the start before its 60 s patience.
/// The config service and the supervisor, which read no bootstrap, take the parent from their environment.
#[test]
fn test_a_role_ends_with_the_parent_it_names() -> TestResult {
    assert_eq!(child_outcome("gone")?, "ended=ok");
    assert_eq!(child_outcome("kept")?, "running");
    assert_eq!(child_outcome("variable")?, "stopped");
    Ok(())
}

/// An admin listener the settings name without its key, with a key that is no digest, or over TLS files that do not load
/// stops the start as a configuration error, before anything listens.
#[test]
fn test_an_admin_listener_s_refused_settings_stop_the_start() -> TestResult {
    let root = std::env::var_os("TEST_TMPDIR").map_or_else(std::env::temp_dir, Into::into);
    let key = std::path::Path::new(&root).join(format!("admin-refused-{}", std::process::id()));
    std::fs::write(&key, "a read key of at least thirty two bytes")?;
    let bootstrap = Bootstrap::read(
        RoleKind::Storage,
        [
            ("ULTRAGRAPHDB_CONFIG_URL", "http://127.0.0.1:1".to_string()),
            ("ULTRAGRAPHDB_CONFIG_TARGET", "ci".to_string()),
            ("ULTRAGRAPHDB_CONFIG_INSTANCE", "storage_1".to_string()),
            ("ULTRAGRAPHDB_CONFIG_KEY_FILE", key.display().to_string()),
        ]
        .map(|(name, value)| (name.to_string(), value)),
    )?;
    let absent = std::path::Path::new(&root).join("absent");
    let runtime = Runtime::new()?;
    for (admin, named) in [
        ("plaintext = true\n".to_string(), "key_sha256"),
        (
            "plaintext = true\nkey_sha256 = \"no digest\"\n".to_string(),
            "admin.key_sha256",
        ),
        (
            format!(
                "key_sha256 = \"{ADMIN_DIGEST}\"\ntls_cert_file = \"{0}/cert.pem\"\ntls_key_file = \"{0}/key.pem\"\n",
                absent.display()
            ),
            "admin.tls_cert_file",
        ),
    ] {
        let text = format!(
            "[observability]\naddress = \"tcp://127.0.0.1:{}\"\n[admin]\naddress = \"tcp://127.0.0.1:{}\"\n{admin}",
            free_port()?,
            free_port()?
        );
        let start =
            RoleStart::from_settings(text.as_bytes(), RoleIdentity::new("ci", "storage", 1))?
                .with_bootstrap(bootstrap.clone());
        let stop = AtomicBool::new(false);
        let refused = runtime
            .block_on(compio::time::timeout(
                WAIT,
                serve_service(
                    RoleKind::Storage,
                    &start,
                    CpuBinding::NotAsked,
                    &stop,
                    role_ready(true, false),
                ),
            ))
            .map_err(|_| TestError::new("a refusal within the wait"))?
            .err()
            .context("a refusal")?;
        assert_eq!(refused.exit_code(), RoleError::EX_CONFIG, "{refused}");
        assert!(refused.to_string().contains(named), "{refused}");
    }
    Ok(())
}

/// A start whose settings name no admin listener serves none: the role runs as it would without a bootstrap.
#[test]
fn test_a_start_naming_no_admin_listener_serves_none() -> TestResult {
    let root = std::env::var_os("TEST_TMPDIR").map_or_else(std::env::temp_dir, Into::into);
    let key = std::path::Path::new(&root).join(format!("no-admin-{}", std::process::id()));
    std::fs::write(&key, "a read key of at least thirty two bytes")?;
    let bootstrap = Bootstrap::read(
        RoleKind::Storage,
        [
            ("ULTRAGRAPHDB_CONFIG_URL", "http://127.0.0.1:1".to_string()),
            ("ULTRAGRAPHDB_CONFIG_TARGET", "ci".to_string()),
            ("ULTRAGRAPHDB_CONFIG_INSTANCE", "storage_1".to_string()),
            ("ULTRAGRAPHDB_CONFIG_KEY_FILE", key.display().to_string()),
        ]
        .map(|(name, value)| (name.to_string(), value)),
    )?;
    let start = start_on_free_port()?.with_bootstrap(bootstrap);
    let stop = AtomicBool::new(false);
    let refused = Runtime::new()?
        .block_on(serve_service(
            RoleKind::Storage,
            &start,
            CpuBinding::NotAsked,
            &stop,
            |_start, _readiness, _stopping| {
                std::future::ready(Err(RoleError::configuration("no secret")))
            },
        ))
        .err()
        .context("the role's failure")?;
    assert!(refused.to_string().contains("no secret"), "{refused}");
    Ok(())
}

/// A process this one's environment names no parent for watches none: nothing sets the flag.
#[test]
fn test_no_parent_named_is_no_parent_watched() -> TestResult {
    if std::env::var_os(PARENT_PID).is_some() {
        // Run under a supervisor that names one: the child runs above cover that case.
        return Ok(());
    }
    let stop = Arc::new(AtomicBool::new(false));
    stop_with_parent(&stop)?;
    std::thread::sleep(Duration::from_millis(50));
    assert!(!stop.load(Ordering::Acquire));
    Ok(())
}
