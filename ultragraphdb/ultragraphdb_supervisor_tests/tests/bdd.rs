/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The supervisor as a process, bound to `features/`, with no container: the shipped `dev` profile with its paths and ports
//! moved into the test's own, run by the real binaries. One scenario runs at a time: the binaries listen at their declared ports.
//! The `image_bdd` target runs the scenarios tagged `@image` and `@docker` in Docker instead, through `images`.

mod images;

use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus};
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Duration;

use compio::runtime::Runtime;
use cucumber::{World, then, when};
use service_utils::{ServiceStartConfig, ServiceUtil, WaitStrategy};
use sha2::{Digest, Sha256};
use ultragraphdb_tests::{
    ADMIN_KEY, CLIENT_ADDRESS, COMPUTE_BINARY, COMPUTE_OBSERVABILITY, CONFIG_ADDRESS,
    CONFIG_BINARY, FRONTEND_BINARY, FRONTEND_OBSERVABILITY, PARENT_PID, START_SECONDS,
    STORAGE_BINARY, STORAGE_OBSERVABILITY, exit_is, http_status, scratch, until_answering,
};
use ultragraphdb_tool_bdd::{Context, TestError, TestResult};
use ultragraphdb_transport_http_client::{HttpClient, HttpUrl};

/// Where the sandbox holds the binaries, relative to the test's working directory: `copy_file` writes them to this package.
const ROOT_PATH: &str = "ultragraphdb/ultragraphdb_supervisor_tests";

/// Where the sandbox holds the shipped profiles.
const PROFILES: &str = "ultragraphdb/ultragraphdb_config/profiles";

const SUPERVISOR_BINARY: &str = "ultragraphdb_supervisor";

/// Where the sandbox holds a copy of the supervisor with no other binary beside it.
const ALONE_PATH: &str = "ultragraphdb/ultragraphdb_supervisor_tests/alone";

/// The address storage's admin listener holds, in the scenario asking it to load.
const ADMIN_ADDRESS: &str = "127.0.0.1:18090";

/// What the `dev` profile's central file declares.
const PROFILE_FILES: [&str; 4] = [
    "targets.toml",
    "frontend.toml",
    "storage.toml",
    "compute.toml",
];

/// What the supervisor logs once the frontend's `/ready`, the last it waits on, has answered.
const EVERY_ROLE_READY: &str = "every role is ready";

/// The secrets the first start draws, relative to the data directory.
const SECRETS: [&str; 3] = ["config/read.key", "config/admin.key", "config/role.secret"];

static STARTS: AtomicU32 = AtomicU32::new(0);

#[derive(Debug, Default, World)]
struct Steps {
    exited: Option<ExitStatus>,
    log: String,
    log_file: PathBuf,
    environment: Vec<(String, String)>,
    root: PathBuf,
    running: Option<(ServiceUtil, u32)>,
    sockets: PathBuf,
    containers: images::Containers,
    /// The files a refusal is to name.
    files: Vec<String>,
}

/// A scenario that failed midway still stops what it started, so the next finds its ports free.
impl Drop for Steps {
    fn drop(&mut self) {
        if let Some((util, pid)) = self.running.take() {
            // The stop's outcome was the failed scenario's to report.
            drop(util.stop_service(pid));
        }
        if !self.sockets.as_os_str().is_empty() {
            // What stays in /tmp costs nothing.
            drop(std::fs::remove_dir_all(&self.sockets));
        }
    }
}

/// `text` of the shipped `dev` profile, with its paths and ports the test's own.
fn moved(text: &str, root: &Path, sockets: &Path) -> String {
    text.replace("/data", &root.join("data").display().to_string())
        .replace("/run/ultragraph", &sockets.display().to_string())
        .replace(
            "/etc/ultragraphdb/profiles/dev",
            &root.join("profile").display().to_string(),
        )
        .replace("127.0.0.1:8081", FRONTEND_OBSERVABILITY)
        .replace("127.0.0.1:8082", COMPUTE_OBSERVABILITY)
        .replace("127.0.0.1:8083", STORAGE_OBSERVABILITY)
        .replace("127.0.0.1:8090", CONFIG_ADDRESS)
        .replace("0.0.0.0:7080", CLIENT_ADDRESS)
}

/// Writes the `dev` profile into a directory of the scenario's own, and notes it in `w`.
fn write_profile(w: &mut Steps) -> TestResult {
    let at = STARTS.fetch_add(1, Ordering::Relaxed);
    w.root = scratch(&format!("supervised-{at}"));
    // A socket's path has a short limit, which the sandbox's own directory passes.
    w.sockets = PathBuf::from(format!("/tmp/ugsv-{}-{at}", std::process::id()));
    std::fs::create_dir_all(w.root.join("profile"))?;
    std::fs::create_dir_all(&w.sockets)?;
    for file in PROFILE_FILES {
        let text = std::fs::read_to_string(Path::new(PROFILES).join("dev").join(file))?;
        std::fs::write(
            w.root.join("profile").join(file),
            moved(&text, &w.root, &w.sockets),
        )?;
    }
    let own = std::fs::read_to_string(Path::new(PROFILES).join("dev.toml"))?;
    std::fs::write(w.root.join("dev.toml"), moved(&own, &w.root, &w.sockets))?;
    Ok(())
}

fn services() -> TestResult<ServiceUtil> {
    Ok(ServiceUtil::new(
        ROOT_PATH,
        vec![
            SUPERVISOR_BINARY,
            CONFIG_BINARY,
            FRONTEND_BINARY,
            COMPUTE_BINARY,
            STORAGE_BINARY,
        ],
    )?)
}

/// Starts the supervisor on `args`, its log and the log of everything it starts in a new file.
fn start_supervisor(w: &mut Steps, args: Vec<String>) -> TestResult {
    let at = STARTS.fetch_add(1, Ordering::Relaxed);
    w.log_file = scratch(&format!("supervisor-{at}.log"));
    // The arguments live as long as the test, as `ServiceStartConfig` asks: a test process is short.
    let args: &'static [String] = args.leak();
    // The supervisor ends with the test process, which Bazel may kill before any drop stops it, and everything it started ends
    // with it. A scenario's own value comes after, and the last of a name is what a process holds.
    let parent = (PARENT_PID.to_string(), std::process::id().to_string());
    let config = ServiceStartConfig::builder()
        .program(SUPERVISOR_BINARY)
        .program_args(args.iter().map(String::as_str).collect())
        .env_vars([vec![parent], w.environment.clone()].concat())
        .clear_env(true)
        .stderr_file(&w.log_file)
        .wait_strategy(WaitStrategy::NoWait)
        .build();
    let util = services()?;
    let pid = util.start_service_from_config(config)?;
    w.running = Some((util, pid));
    Ok(())
}

fn on_profile(w: &Steps) -> Vec<String> {
    vec![
        "--config".to_string(),
        w.root.join("dev.toml").display().to_string(),
    ]
}

fn read_log(w: &mut Steps) -> TestResult {
    w.log = std::fs::read_to_string(&w.log_file)?;
    Ok(())
}

/// Starts the supervisor on the scenario's profile, and waits until it says every role is ready: the frontend's `/ready` alone
/// answers before compute has started, since a partition answering it is what decides it.
fn start_to_ready(w: &mut Steps) -> TestResult {
    start_supervisor(w, on_profile(w))?;
    for _ in 0..START_SECONDS * 20 {
        read_log(w)?;
        if w.log.contains(EVERY_ROLE_READY) {
            return Ok(());
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    Err(TestError::new(format!(
        "the supervisor never said every role is ready:\n{}",
        w.log
    )))
}

/// Waits for the supervisor to exit by itself, and holds how, with its log.
fn until_exit(w: &mut Steps) -> TestResult {
    let (util, pid) = w.running.take().context("a running supervisor")?;
    w.exited = Some(util.wait_for_exit(pid, Duration::from_secs(START_SECONDS))?);
    read_log(w)
}

/// `lines` stand in the log in this order.
fn in_order(log: &str, lines: &[&str]) -> TestResult {
    let mut rest = log;
    for line in lines {
        let at = rest
            .find(line)
            .ok_or_else(|| TestError::new(format!("{line} after those before it, in\n{log}")))?;
        rest = rest.get(at + line.len()..).context("the rest of the log")?;
    }
    Ok(())
}

/// Sends `signal` to `pid`, as an operator would.
fn signal(pid: u32, signal: &str) -> TestResult {
    let sent = Command::new("kill")
        .arg(format!("-{signal}"))
        .arg(pid.to_string())
        .output()?;
    assert!(sent.status.success(), "{sent:?}");
    Ok(())
}

#[when("the supervisor starts the config service and the three roles")]
fn starts_all(w: &mut Steps) -> TestResult {
    write_profile(w)?;
    start_to_ready(w)
}

#[then(
    "the config service answers first, every role's `/ready` answers, the frontend's last, and no role waited on one that was waiting on it"
)]
fn start_order(w: &mut Steps) -> TestResult {
    for address in [
        STORAGE_OBSERVABILITY,
        COMPUTE_OBSERVABILITY,
        FRONTEND_OBSERVABILITY,
    ] {
        assert_eq!(http_status(address, "/ready")?, 200, "{address}");
    }
    assert_eq!(http_status(CONFIG_ADDRESS, "/ready")?, 200);
    in_order(
        &w.log,
        &[
            "answering instance=config path=/ready",
            "started instance=frontend",
            "answering instance=frontend path=/health",
            "started instance=storage",
            "answering instance=storage path=/ready",
            "started instance=compute",
            "answering instance=compute path=/ready",
            "answering instance=frontend path=/ready",
            EVERY_ROLE_READY,
        ],
    )
}

#[when("the storage process exits with code 74 while the others run")]
fn storage_fails(w: &mut Steps) -> TestResult {
    write_profile(w)?;
    let digest: String = Sha256::digest(ADMIN_KEY)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect();
    let settings = w.root.join("profile").join("storage.toml");
    let text = std::fs::read_to_string(&settings)?;
    std::fs::write(
        &settings,
        format!(
            "{text}[admin]\naddress = \"tcp://{ADMIN_ADDRESS}\"\nplaintext = true\nkey_sha256 = \"{digest}\"\n"
        ),
    )?;
    let config = w.root.join("data").join("config");
    std::fs::create_dir_all(&config)?;
    std::fs::write(config.join("admin.key"), ADMIN_KEY)?;
    start_to_ready(w)?;
    // A log that cannot be opened is what a store's failure is to a role: it restarts by a load, into the code 74 of a real
    // store, while the frontend and compute run on.
    let log = w
        .root
        .join("data")
        .join("storage")
        .join("p1")
        .join("r1")
        .join("wal");
    std::fs::rename(&log, log.with_extension("moved"))?;
    std::fs::write(&log, "not a log")?;
    let client = HttpClient::new(
        HttpUrl::parse(&format!("http://{ADMIN_ADDRESS}"))?,
        None,
        ADMIN_KEY.to_string(),
    )?;
    let answer = Runtime::new()?.block_on(client.request("POST", "/admin/load-config", b"1"))?;
    assert_eq!(answer.status(), 202);
    until_exit(w)
}

#[then("the supervisor stops the frontend and compute and exits with code 74")]
fn stopped_on_74(w: &mut Steps) -> TestResult {
    exit_is(w.exited, &w.log, 74)?;
    // The process that ended was reaped, so its ID may be another's by now: it is sent no signal.
    assert!(!w.log.contains("a signal found no process"), "{}", w.log);
    in_order(
        &w.log,
        &[
            "the partition 1 log",
            "ended instance=storage code=74",
            "stopping role=frontend",
            "stopped instance=frontend code=0",
            "stopping role=compute",
            "stopped instance=compute code=0",
            "stopped instance=config code=0",
        ],
    )
}

#[when("the compute process is killed by SIGKILL while the others run")]
fn compute_killed(w: &mut Steps) -> TestResult {
    write_profile(w)?;
    start_to_ready(w)?;
    signal(pid_of(w, "compute")?, "KILL")?;
    until_exit(w)
}

#[then("the supervisor stops the frontend and storage and exits with code 137")]
fn stopped_on_137(w: &mut Steps) -> TestResult {
    exit_is(w.exited, &w.log, 137)?;
    in_order(
        &w.log,
        &[
            "ended instance=compute code=137",
            "stopping role=frontend",
            "stopped instance=frontend code=0",
            "stopping role=storage",
            "stopped instance=storage code=0",
            "stopped instance=config code=0",
        ],
    )
}

#[when("the supervisor receives SIGTERM")]
fn terminated(w: &mut Steps) -> TestResult {
    write_profile(w)?;
    start_to_ready(w)?;
    let (util, pid) = w.running.take().context("a running supervisor")?;
    w.exited = Some(util.stop_service(pid)?);
    read_log(w)
}

/// The supervisor, ready, told `name`.
fn ready_then_told(w: &mut Steps, name: &str) -> TestResult {
    write_profile(w)?;
    start_to_ready(w)?;
    let (_, pid) = w.running.as_ref().context("a running supervisor")?;
    signal(*pid, name)?;
    until_exit(w)
}

#[when("the supervisor receives SIGINT")]
fn interrupted(w: &mut Steps) -> TestResult {
    ready_then_told(w, "INT")
}

#[when("the supervisor receives SIGHUP")]
fn hung_up(w: &mut Steps) -> TestResult {
    ready_then_told(w, "HUP")
}

#[when("the supervisor receives SIGQUIT")]
fn quit(w: &mut Steps) -> TestResult {
    ready_then_told(w, "QUIT")
}

#[then(
    "the frontend stops accepting work first, compute drains next, storage flushes last, and the supervisor exits 0"
)]
fn stopped_in_order(w: &mut Steps) -> TestResult {
    exit_is(w.exited, &w.log, 0)?;
    in_order(
        &w.log,
        &[
            "stopping role=frontend",
            "stopped accepting",
            "stopped instance=frontend code=0",
            "stopping role=compute",
            "stopped instance=compute code=0",
            "stopping role=storage",
            "stopped instance=storage code=0",
            "stopped instance=config code=0",
        ],
    )
}

/// The pid the log says `instance` was started with.
fn pid_of(w: &mut Steps, instance: &str) -> TestResult<u32> {
    read_log(w)?;
    let (_, rest) = w
        .log
        .split_once(&format!("started instance={instance} pid="))
        .context("the instance's pid in the log")?;
    Ok(rest.split_whitespace().next().context("the pid")?.parse()?)
}

#[when("the storage process is told to stop by someone other than the supervisor")]
fn storage_told(w: &mut Steps) -> TestResult {
    write_profile(w)?;
    start_to_ready(w)?;
    signal(pid_of(w, "storage")?, "TERM")?;
    until_exit(w)
}

#[then("the supervisor stops the frontend and compute and exits with code 0")]
fn stopped_on_0(w: &mut Steps) -> TestResult {
    exit_is(w.exited, &w.log, 0)?;
    in_order(
        &w.log,
        &[
            "ended instance=storage code=0",
            "stopped instance=frontend code=0",
            "stopped instance=compute code=0",
            "stopped instance=config code=0",
        ],
    )
}

#[when("the supervisor receives SIGTERM while the frontend waits for its partition")]
fn terminated_in_the_start(w: &mut Steps) -> TestResult {
    write_profile(w)?;
    // The frontend refuses every Lease of storage's block size, so storage leases nothing and the start waits on it until its
    // deadline.
    let settings = w.root.join("profile").join("frontend.toml");
    let text = std::fs::read_to_string(&settings)?;
    std::fs::write(
        &settings,
        text.replace("[metadata]\n", "[metadata]\nblock_size_max = 1\n"),
    )?;
    start_supervisor(w, on_profile(w))?;
    until_answering(FRONTEND_OBSERVABILITY, "/health")?;
    until_answering(STORAGE_OBSERVABILITY, "/health")?;
    let (_, pid) = w.running.as_ref().context("a running supervisor")?;
    signal(*pid, "TERM")?;
    until_exit(w)
}

#[then("it starts nothing more, stops what it started, and exits 0")]
fn stopped_what_it_started(w: &mut Steps) -> TestResult {
    exit_is(w.exited, &w.log, 0)?;
    assert!(!w.log.contains("started instance=compute"), "{}", w.log);
    in_order(
        &w.log,
        &[
            "stopped instance=frontend code=0",
            "stopped instance=storage code=0",
            "stopped instance=config code=0",
        ],
    )
}

#[when("the supervisor starts with `ULTRAGRAPHDB_STORAGE_PARTITION_COUNT=2` in its environment")]
fn stray_variable(w: &mut Steps) -> TestResult {
    write_profile(w)?;
    w.environment = vec![(
        "ULTRAGRAPHDB_STORAGE_PARTITION_COUNT".to_string(),
        "2".to_string(),
    )];
    start_to_ready(w)
}

#[then("every role reaches readiness, since a role would refuse the variable")]
fn ready_past_the_variable(_w: &mut Steps) -> TestResult {
    for address in [
        STORAGE_OBSERVABILITY,
        COMPUTE_OBSERVABILITY,
        FRONTEND_OBSERVABILITY,
    ] {
        assert_eq!(http_status(address, "/ready")?, 200, "{address}");
    }
    Ok(())
}

#[when("the supervisor starts with no other binary beside it")]
fn alone(w: &mut Steps) -> TestResult {
    write_profile(w)?;
    w.log_file = scratch("supervisor-alone.log");
    let own: &'static str = w.root.join("dev.toml").display().to_string().leak();
    let config = ServiceStartConfig::builder()
        .program(SUPERVISOR_BINARY)
        .program_args(vec!["--config", own])
        .env_vars(Vec::new())
        .clear_env(true)
        .stderr_file(&w.log_file)
        .wait_strategy(WaitStrategy::NoWait)
        .build();
    let util = ServiceUtil::new(ALONE_PATH, vec![SUPERVISOR_BINARY])?;
    let pid = util.start_service_from_config(config)?;
    w.running = Some((util, pid));
    until_exit(w)
}

#[then("it exits with code 69 and an error naming the binary it looked for")]
fn names_the_binary(w: &mut Steps) -> TestResult {
    exit_is(w.exited, &w.log, 69)?;
    assert!(w.log.contains(CONFIG_BINARY), "{}", w.log);
    assert!(w.log.contains("beside it"), "{}", w.log);
    Ok(())
}

#[when("the development container has started")]
fn container_started(w: &mut Steps) -> TestResult {
    write_profile(w)?;
    start_to_ready(w)
}

#[then(
    "`/data` holds `meta/`, `seq/` and `storage/p1/r1/`, the paths a cluster node uses for the same roles"
)]
fn data_layout(w: &mut Steps) {
    let data = w.root.join("data");
    for path in ["meta", "seq", "storage/p1/r1"] {
        assert!(data.join(path).is_dir(), "{path}");
    }
}

/// What the secrets of the scenario's data directory hold.
fn secrets(w: &Steps) -> TestResult<Vec<Vec<u8>>> {
    SECRETS
        .iter()
        .map(|file| Ok(std::fs::read(w.root.join("data").join(file))?))
        .collect()
}

#[when("the development container starts, stops and starts again over one data directory")]
fn restarted(w: &mut Steps) -> TestResult {
    write_profile(w)?;
    start_to_ready(w)?;
    let drawn = secrets(w)?;
    let (util, pid) = w.running.take().context("a running supervisor")?;
    exit_is(Some(util.stop_service(pid)?), "the first stop", 0)?;
    start_to_ready(w)?;
    assert_eq!(secrets(w)?, drawn, "the restart kept what was drawn");
    Ok(())
}

#[then(
    "the config service's two keys and the role secret stand in its data directory at mode 0600, each at least 32 bytes and differing, and the restart kept them and reached readiness"
)]
fn secrets_drawn(w: &mut Steps) -> TestResult {
    let held = secrets(w)?;
    for (file, bytes) in SECRETS.iter().zip(&held) {
        assert!(bytes.len() >= 32, "{file}: {} bytes", bytes.len());
        let mode = std::fs::metadata(w.root.join("data").join(file))?
            .permissions()
            .mode();
        assert_eq!(mode & 0o777, 0o600, "{file}");
    }
    let [read, admin, role] = held.as_slice() else {
        return Err(TestError::new("three secrets"));
    };
    assert_ne!(read, admin);
    assert_ne!(read, role);
    assert_ne!(admin, role);
    assert_eq!(http_status(FRONTEND_OBSERVABILITY, "/ready")?, 200);
    Ok(())
}

/// Where the development profile keeps the bootstrap administrator's password, under the data directory.
const BOOTSTRAP_SECRET: &str = "config/bootstrap.secret";

#[when(
    "the development container has started, the bootstrap administrator's password file is removed, and the container starts again over the same data directory"
)]
fn bootstrap_removed(w: &mut Steps) -> TestResult {
    write_profile(w)?;
    start_to_ready(w)?;
    let password = w.root.join("data").join(BOOTSTRAP_SECRET);
    assert!(password.is_file(), "the first start drew the password");
    let (util, pid) = w.running.take().context("a running supervisor")?;
    exit_is(Some(util.stop_service(pid)?), "the first stop", 0)?;
    std::fs::remove_file(&password)?;
    start_to_ready(w)
}

#[then("no file stands at the password's path, and the restart reached readiness")]
fn bootstrap_stays_removed(w: &mut Steps) -> TestResult {
    let password = w.root.join("data").join(BOOTSTRAP_SECRET);
    assert!(!password.exists(), "{} was drawn again", password.display());
    assert_eq!(http_status(FRONTEND_OBSERVABILITY, "/ready")?, 200);
    Ok(())
}

#[when("the supervisor cannot finish writing the admin key it draws")]
fn draw_cut_short(w: &mut Steps) -> TestResult {
    write_profile(w)?;
    let config = w.root.join("data").join("config");
    // A directory at the key's temporary name is a write failing where a full volume would fail it. Another key's temporary
    // name holding a killed draw's bytes is what a power loss leaves.
    std::fs::create_dir_all(config.join("admin.key.drawing"))?;
    std::fs::write(config.join("read.key.drawing"), "cut")?;
    start_supervisor(w, on_profile(w))?;
    until_exit(w)
}

#[then(
    "it exits with code 74 with no file at the key's path, and the next start draws it and reaches readiness"
)]
fn draws_again(w: &mut Steps) -> TestResult {
    exit_is(w.exited, &w.log, 74)?;
    let config = w.root.join("data").join("config");
    assert!(!config.join("admin.key").exists(), "{}", w.log);
    assert!(std::fs::read(config.join("read.key"))?.len() >= 32);
    std::fs::remove_dir(config.join("admin.key.drawing"))?;
    start_to_ready(w)?;
    assert!(std::fs::read(config.join("admin.key"))?.len() >= 32);
    Ok(())
}

#[when("the supervisor starts with `--config` naming a file that does not exist")]
fn no_file(w: &mut Steps) -> TestResult {
    w.root = scratch("never-written.toml");
    start_supervisor(
        w,
        vec!["--config".to_string(), w.root.display().to_string()],
    )?;
    until_exit(w)
}

#[then("it exits with code 78 and an error naming the file")]
fn names_the_file(w: &mut Steps) -> TestResult {
    exit_is(w.exited, &w.log, 78)?;
    let named = w.root.display().to_string();
    assert!(w.log.contains(&named), "{}", w.log);
    Ok(())
}

#[when("the supervisor starts with no arguments")]
fn no_arguments(w: &mut Steps) -> TestResult {
    start_supervisor(w, Vec::new())?;
    until_exit(w)
}

#[then("it exits with code 78 and an error saying it takes `--config <file>`")]
fn says_its_arguments(w: &mut Steps) -> TestResult {
    exit_is(w.exited, &w.log, 78)?;
    assert!(w.log.contains("--config <file>"), "{}", w.log);
    Ok(())
}

#[when("the supervisor starts on a config service's settings asking for TLS")]
fn tls_asked(w: &mut Steps) -> TestResult {
    write_profile(w)?;
    let own = w.root.join("dev.toml");
    let text = std::fs::read_to_string(&own)?;
    std::fs::write(
        &own,
        text.replace(
            "plaintext = true",
            "tls_cert_file = \"/etc/tls/cert.pem\"\ntls_key_file = \"/etc/tls/key.pem\"",
        ),
    )?;
    start_supervisor(w, on_profile(w))?;
    until_exit(w)
}

#[then("it exits with code 78 and an error naming `config.plaintext` and saying to set it")]
fn names_plaintext(w: &mut Steps) -> TestResult {
    exit_is(w.exited, &w.log, 78)?;
    assert!(w.log.contains("config.plaintext"), "{}", w.log);
    assert!(w.log.contains("plaintext = true"), "{}", w.log);
    Ok(())
}

#[when("the supervisor starts on a central file declaring a storage file with no `data_dir`")]
fn broken_file(w: &mut Steps) -> TestResult {
    write_profile(w)?;
    let settings = w.root.join("profile").join("storage.toml");
    let text = std::fs::read_to_string(&settings)?;
    let kept: Vec<&str> = text
        .lines()
        .filter(|line| !line.starts_with("data_dir"))
        .collect();
    std::fs::write(&settings, kept.join("\n") + "\n")?;
    start_supervisor(w, on_profile(w))?;
    until_exit(w)
}

#[then("it exits with code 78, the error naming `storage.data_dir`, and started no role")]
fn names_data_dir(w: &mut Steps) -> TestResult {
    exit_is(w.exited, &w.log, 78)?;
    assert!(w.log.contains("storage.data_dir"), "{}", w.log);
    assert!(!w.log.contains("started instance=frontend"), "{}", w.log);
    Ok(())
}

#[when(
    "the supervisor starts on a target whose storage file names another `role_secret_file` than its frontend's"
)]
fn two_role_secrets(w: &mut Steps) -> TestResult {
    write_profile(w)?;
    let settings = w.root.join("profile").join("storage.toml");
    let text = std::fs::read_to_string(&settings)?;
    let named = text
        .lines()
        .find(|line| line.starts_with("role_secret_file"))
        .context("storage's role secret file")?
        .to_string();
    let other = named.replacen("role.secret", "role-secret", 1);
    std::fs::write(&settings, text.replacen(&named, &other, 1))?;
    w.files = vec![
        quoted(&named).context("the frontend's file")?,
        quoted(&other).context("storage's file")?,
    ];
    start_supervisor(w, on_profile(w))?;
    until_exit(w)
}

/// The quoted value of a setting's line.
fn quoted(line: &str) -> Option<String> {
    line.split('"').nth(1).map(str::to_string)
}

#[then("it exits with code 78, the error naming both files, and started no role")]
fn names_both_secrets(w: &mut Steps) -> TestResult {
    exit_is(w.exited, &w.log, 78)?;
    for file in &w.files {
        assert!(w.log.contains(file.as_str()), "{file}: {}", w.log);
    }
    assert!(!w.log.contains("started instance=frontend"), "{}", w.log);
    Ok(())
}

/// Declares a second target in the central file, starts on it, and holds the refusal; returns what the file said before.
fn declare_two(w: &mut Steps) -> TestResult<String> {
    write_profile(w)?;
    let central = w.root.join("profile").join("targets.toml");
    let text = std::fs::read_to_string(&central)?;
    std::fs::write(
        &central,
        format!(
            "{text}[other]\nfrontend = \"frontend.toml\"\nstorage = \"storage.toml\"\ncompute = \"compute.toml\"\n"
        ),
    )?;
    start_supervisor(w, on_profile(w))?;
    until_exit(w)?;
    Ok(text)
}

#[when("the supervisor starts on a central file declaring two targets")]
fn two_targets(w: &mut Steps) -> TestResult {
    let _before = declare_two(w)?;
    Ok(())
}

/// Starts the config service alone, as a refusal says to, and asks it to `method` `path`, under the key the first start drew.
fn on_the_service_alone(w: &Steps, method: &str, path: &str) -> TestResult {
    let args: &'static [String] = on_profile(w).leak();
    let config = ServiceStartConfig::builder()
        .program(CONFIG_BINARY)
        .program_args(args.iter().map(String::as_str).collect())
        .env_vars(Vec::new())
        .clear_env(true)
        .stderr_file(scratch("config-alone.log"))
        .wait_strategy(WaitStrategy::NoWait)
        .build();
    let util = services()?;
    let pid = util.start_service_from_config(config)?;
    until_answering(CONFIG_ADDRESS, "/ready")?;
    let key = std::fs::read_to_string(w.root.join("data").join("config").join("admin.key"))?;
    let client = HttpClient::new(
        HttpUrl::parse(&format!("http://{CONFIG_ADDRESS}"))?,
        None,
        key,
    )?;
    let answer = Runtime::new()?.block_on(client.request(method, path, b""))?;
    assert_eq!(answer.status(), 200, "{method} {path}");
    exit_is(Some(util.stop_service(pid)?), "the service alone", 0)
}

#[when("the store holds two active targets and the central file has gone back to one")]
fn store_keeps_two(w: &mut Steps) -> TestResult {
    let before = declare_two(w)?;
    exit_is(w.exited, &w.log, 78)?;
    std::fs::write(w.root.join("profile").join("targets.toml"), before)?;
    start_supervisor(w, on_profile(w))?;
    until_exit(w)
}

#[then("the refusal names the store and retiring the target it says reaches readiness")]
fn retiring_cures(w: &mut Steps) -> TestResult {
    exit_is(w.exited, &w.log, 78)?;
    let data = w.root.join("data").join("config").display().to_string();
    for said in [
        data.as_str(),
        "changes nothing",
        "DELETE /v1/targets/<name>",
    ] {
        assert!(w.log.contains(said), "{said}: {}", w.log);
    }
    on_the_service_alone(w, "DELETE", "/v1/targets/other")?;
    start_to_ready(w)
}

#[when("the store's one target was retired through the API")]
fn store_retired(w: &mut Steps) -> TestResult {
    write_profile(w)?;
    start_to_ready(w)?;
    let (util, pid) = w.running.take().context("a running supervisor")?;
    exit_is(Some(util.stop_service(pid)?), "the first stop", 0)?;
    on_the_service_alone(w, "DELETE", "/v1/targets/dev")?;
    start_supervisor(w, on_profile(w))?;
    until_exit(w)
}

#[then(
    "it exits with code 78, the error naming the target and how to activate it, and started no role"
)]
fn says_to_activate(w: &mut Steps) -> TestResult {
    exit_is(w.exited, &w.log, 78)?;
    for said in ["[dev]", "PUT /v1/targets/<name>/active"] {
        assert!(w.log.contains(said), "{said}: {}", w.log);
    }
    assert!(!w.log.contains("started instance=frontend"), "{}", w.log);
    Ok(())
}

#[when("the supervisor starts on a profile whose socket directory does not exist")]
fn no_socket_directory(w: &mut Steps) -> TestResult {
    write_profile(w)?;
    std::fs::remove_dir(&w.sockets)?;
    start_supervisor(w, on_profile(w))?;
    until_exit(w)
}

#[then("it exits with code 78, the error naming the directory and saying what it must be")]
fn names_the_directory(w: &mut Steps) -> TestResult {
    exit_is(w.exited, &w.log, 78)?;
    let named = w.sockets.display().to_string();
    assert!(w.log.contains(&named), "{}", w.log);
    assert!(w.log.contains("must exist and be writable"), "{}", w.log);
    Ok(())
}

#[then(
    "it exits with code 78, the error naming both targets and saying it runs one, and started no role"
)]
fn names_both_targets(w: &mut Steps) -> TestResult {
    exit_is(w.exited, &w.log, 78)?;
    for said in [
        "dev",
        "other",
        "runs one target",
        "store",
        "DELETE /v1/targets/<name>",
    ] {
        assert!(w.log.contains(said), "{said}: {}", w.log);
    }
    assert!(!w.log.contains("started instance=frontend"), "{}", w.log);
    Ok(())
}

#[when("each of the four binaries is asked for its help")]
fn asks_help(w: &mut Steps) -> TestResult {
    w.log.clear();
    for binary in [
        SUPERVISOR_BINARY,
        FRONTEND_BINARY,
        COMPUTE_BINARY,
        STORAGE_BINARY,
    ] {
        let asked = Command::new(Path::new(ROOT_PATH).join(binary))
            .arg("--help")
            .env_clear()
            .output()?;
        let text = String::from_utf8_lossy(&asked.stdout);
        assert_eq!(asked.status.code(), Some(0), "{binary}: {text}");
        for code in ["0", "69", "70", "74", "78"] {
            let row = format!("  {code} ");
            assert!(
                text.lines().any(|line| line.starts_with(&row)),
                "{binary} lists no {code}: {text}"
            );
        }
        w.log.push_str(&format!("{binary}: {text}\n"));
    }
    Ok(())
}

#[then(
    "each exits 0 and lists the codes 0, 69, 70, 74 and 78, and the supervisor's the code of a role a signal killed"
)]
fn help_names_codes(w: &mut Steps) -> TestResult {
    let (supervisor, _) = w
        .log
        .split_once(FRONTEND_BINARY)
        .context("the supervisor's help first")?;
    assert!(supervisor.contains("128 plus"), "{supervisor}");
    Ok(())
}

/// In Docker, the development image under the runtime's own profile; on this host, the supervised roles.
#[when("a role starts under Docker's default seccomp profile")]
fn under_default_seccomp(w: &mut Steps) -> TestResult {
    if images::in_docker() {
        return images::start_development(&mut w.containers, images::Seccomp::Default);
    }
    write_profile(w)?;
    start_to_ready(w)?;
    read_log(w)
}

/// In Docker, every role on the polling driver and a write and a read through the frontend. On this host io_uring exists on
/// Linux alone, so everywhere else this is the refusal itself, and on Linux with io_uring allowed the same role says it runs
/// on io_uring.
#[then("it starts on the polling driver, logs that it does, and serves requests")]
fn on_polling(w: &mut Steps) -> TestResult {
    if images::in_docker() {
        images::on_driver(&w.containers, "polling", "io_uring")?;
        let rows = images::write_and_read(&w.containers)?;
        assert!(!rows.is_empty(), "{}", w.containers.log);
        return Ok(());
    }
    let polling = w.log.contains("on the polling driver");
    let allowed = cfg!(target_os = "linux") && w.log.contains("on the io_uring driver");
    assert!(polling || allowed, "{}", w.log);
    assert_eq!(http_status(STORAGE_OBSERVABILITY, "/ready")?, 200);
    Ok(())
}

#[when("the storage image is built")]
fn storage_image_built(w: &mut Steps) -> TestResult {
    images::storage_image(&mut w.containers)
}

#[then(
    "it holds the storage binary as its entrypoint on the glibc base, for amd64 and arm64, runs as user 65532, and holds no shell and no other role's binary"
)]
fn storage_image_alone(w: &mut Steps) -> TestResult {
    images::storage_image_holds_its_binary(&w.containers)
}

#[when("the development image starts with its default profile")]
fn development_started(w: &mut Steps) -> TestResult {
    images::start_development(&mut w.containers, images::Seccomp::Default)
}

#[then(
    "the frontend and compute connect through `unix:///run/ultragraph/<role>.sock`, each partition through `unix:///run/ultragraph/storage-p<partition>.sock`, and a write and a read through the frontend succeed"
)]
fn development_unix(w: &mut Steps) -> TestResult {
    images::development_over_unix(&w.containers)
}

#[when("the three roles run in three containers configured with `tcp://` addresses")]
fn three_hosts(w: &mut Steps) -> TestResult {
    images::start_three_hosts(&mut w.containers)
}

#[then(
    "the same write and read succeed, and the read's rows are byte-identical to the Unix-socket run's"
)]
fn same_rows(w: &mut Steps) -> TestResult {
    images::same_rows_over_both(&mut w.containers)
}

#[when("a role starts under the shipped seccomp profile")]
fn under_shipped_seccomp(w: &mut Steps) -> TestResult {
    images::start_development(&mut w.containers, images::Seccomp::Shipped)
}

#[then("it starts on io_uring and logs that it does")]
fn on_io_uring(w: &mut Steps) -> TestResult {
    images::on_driver(&w.containers, "io_uring", "polling")
}

/// Whether the process `pid` still runs, as `kill -0` says.
fn runs(pid: u32) -> TestResult<bool> {
    let asked = Command::new("kill")
        .args(["-0", &pid.to_string()])
        .output()?;
    Ok(asked.status.success())
}

/// Waits for each of `started` to end by itself, each named with its process's identity. Past the bound whatever still runs is
/// killed, so the next scenario finds its ports free, and named as the failure.
fn until_ended(started: &[(&str, u32)]) -> TestResult {
    for _ in 0..START_SECONDS * 20 {
        let mut left = false;
        for (_, pid) in started {
            left = left || runs(*pid)?;
        }
        if !left {
            return Ok(());
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    let mut running = Vec::new();
    for (instance, pid) in started {
        if runs(*pid)? {
            signal(*pid, "KILL")?;
            running.push(*instance);
        }
    }
    Err(TestError::new(format!(
        "{running:?} still ran {START_SECONDS} s after the supervisor was killed"
    )))
}

#[when("the supervisor is killed by SIGKILL while every role runs")]
fn supervisor_killed(w: &mut Steps) -> TestResult {
    write_profile(w)?;
    start_to_ready(w)?;
    let mut started = Vec::new();
    for instance in ["config", "frontend", "storage", "compute"] {
        started.push((instance, pid_of(w, instance)?));
    }
    let (_, pid) = w.running.as_ref().context("a running supervisor")?;
    signal(*pid, "KILL")?;
    until_exit(w)?;
    until_ended(&started)?;
    read_log(w)
}

#[then("the config service and the three roles each stop as on SIGTERM and free their ports")]
fn none_orphaned(w: &mut Steps) -> TestResult {
    for role in ["frontend", "storage", "compute"] {
        assert!(
            w.log.contains(&format!("stopping role={role}")),
            "{role} stopped by no stop of its own:\n{}",
            w.log
        );
    }
    let service_stopped = w
        .log
        .lines()
        .any(|line| line.trim_end().ends_with(" stopping"));
    assert!(
        service_stopped,
        "the config service stopped by no stop of its own:\n{}",
        w.log
    );
    for address in [
        FRONTEND_OBSERVABILITY,
        COMPUTE_OBSERVABILITY,
        STORAGE_OBSERVABILITY,
        CONFIG_ADDRESS,
        CLIENT_ADDRESS,
    ] {
        drop(std::net::TcpListener::bind(address).context(address)?);
    }
    Ok(())
}

#[when(
    "the supervisor starts with `ULTRAGRAPHDB_PARENT_PID` naming a process that is not its parent"
)]
fn supervisor_parent_gone(w: &mut Steps) -> TestResult {
    write_profile(w)?;
    // No process holds the largest identity, so it is no parent of the supervisor's.
    w.environment
        .push((PARENT_PID.to_string(), u32::MAX.to_string()));
    start_supervisor(w, on_profile(w))?;
    until_exit(w)
}

#[then("it stops what it started and exits with code 0")]
fn stopped_with_its_parent(w: &mut Steps) -> TestResult {
    exit_is(w.exited, &w.log, 0)?;
    assert!(!w.log.contains(EVERY_ROLE_READY), "{}", w.log);
    Ok(())
}

#[when("the development image starts, the supervisor at PID 1")]
fn development_at_pid_1(w: &mut Steps) -> TestResult {
    images::start_development(&mut w.containers, images::Seccomp::Default)
}

#[then(
    "the supervisor names process 1 to each role it starts, and every role reaches readiness and stays there"
)]
fn role_under_pid_1(w: &mut Steps) -> TestResult {
    images::roles_run_on_under_pid_1(&w.containers)
}

#[when(
    "a client commits through the development container until `docker kill` ends it, and the container starts again"
)]
fn development_killed(w: &mut Steps) -> TestResult {
    images::commit_through_a_kill(&mut w.containers)
}

#[then("every commit acknowledged before the kill reads back through the restarted container")]
fn acknowledged_held(w: &mut Steps) -> TestResult {
    images::every_acknowledged_commit_reads_back(&w.containers)
}

/// `bdd` runs every feature but those tagged `@image`; `image_bdd` runs those and the ones tagged `@docker`, in Docker, and
/// says so plainly when no daemon answers, as scrith's container tests skip.
fn main() -> TestResult {
    let in_docker = images::in_docker();
    if in_docker && !images::docker_answers() {
        println!("SKIPPED: no Docker daemon answered `docker version`, so no image scenario ran");
        return Ok(());
    }
    futures::executor::block_on(
        Steps::cucumber()
            .max_concurrent_scenarios(1)
            .fail_on_skipped()
            .filter_run_and_exit(
                ultragraphdb_tool_bdd::features_dir()?,
                move |feature, _, _| {
                    let tagged = |name: &str| feature.tags.iter().any(|tag| tag == name);
                    if in_docker {
                        tagged("image") || tagged("docker")
                    } else {
                        !tagged("image")
                    }
                },
            ),
    );
    Ok(())
}
