/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::io::Write as _;
use std::os::unix::fs::OpenOptionsExt;
use std::os::unix::process::{CommandExt, ExitStatusExt};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use compio::runtime::Runtime;
use rustix::process::{Pid, Signal, kill_process};
use ultragraphdb_common_config_check::{Instance, TargetFiles, check_config_service, instances};
use ultragraphdb_common_service_config::{EndpointSecurity, ServiceConfig};
use ultragraphdb_common_telemetry::{LogFormat, TelemetryConfig, init};
use ultragraphdb_error_role::RoleError;
use ultragraphdb_transport_http_client::{HttpClient, HttpUrl, read_key};

/// How often the supervisor looks at its processes and its signals, and asks what it waits on.
const POLL: Duration = Duration::from_millis(20);

/// How many looks pass between two notes of what a start still waits on: 10 s of `POLL`.
const NOTE_POLLS: u32 = 500;

/// How many looks a role told to stop gets: 70 s of `POLL`, past the 60 s a role gives itself.
const STOP_POLLS: u32 = 3_500;

/// The random bytes of a secret, written as twice as many hexadecimal digits.
const SECRET_BYTES: usize = 32;

/// The mode of a secret the supervisor draws.
const SECRET_MODE: u32 = 0o600;

/// What a secret's file is called, this appended, until it is whole.
const DRAWING: &str = ".drawing";

/// The name the config service goes by in the log.
const CONFIG: &str = "config";

/// The order the processes stop in: the frontend takes no more work, compute drains, storage flushes, and the settings go last.
const STOP_ORDER: [&str; 4] = ["frontend", "compute", "storage", CONFIG];

const HELP: &str = "ultragraphdb_supervisor --config <file>

Runs the development container: the config service, the frontend, storage and compute, each its own process, started in that
order, each waited on over HTTP. It restarts nothing. SIGTERM and SIGINT stop them in the shutdown order: the frontend, compute,
storage, the config service. SIGHUP and SIGQUIT do the same, as SIGTERM, since a closed terminal reaches the supervisor alone. A
supervisor killed outright stops nothing, and what it started goes on holding its ports and directories: stop those by hand.

<file> is the config service's own settings file, over clear text, as the shipped profiles name it. The binaries stand beside this
one. The config service's two keys and the role secret are drawn at random where the settings name them, if nothing is there,
and the bootstrap administrator's password too before the cluster's first start, which reads it once.

Exit codes:
  128 plus the signal's number  a role a signal killed
";

/// What the supervisor starts from, all of it read from the config service's own settings file.
#[derive(Debug)]
struct Start {
    file: String,
    host_port: String,
    targets_file: String,
    read_key_file: String,
    admin_key_file: String,
    data_dir: String,
    directory: PathBuf,
}

/// Whether a signal to stop arrived, shared with the handlers, which may do nothing else (Ultra rule 9). `term` covers SIGHUP and
/// SIGQUIT too: each roles' process group is its own, so those reach the supervisor alone, and dying of one would orphan them.
#[derive(Debug)]
struct Signals {
    term: Arc<AtomicBool>,
    int: Arc<AtomicBool>,
}

impl Signals {
    fn new() -> Result<Self, RoleError> {
        let flag = |signals: &[Signal]| -> Result<Arc<AtomicBool>, RoleError> {
            let set = Arc::new(AtomicBool::new(false));
            for signal in signals {
                let _id = signal_hook::flag::register(signal.as_raw(), Arc::clone(&set)).map_err(
                    |error| RoleError::unavailable(format!("a signal handler: {error}")),
                )?;
            }
            Ok(set)
        };
        Ok(Self {
            term: flag(&[Signal::TERM, Signal::HUP, Signal::QUIT])?,
            int: flag(&[Signal::INT])?,
        })
    }

    fn arrived(&self) -> bool {
        self.term.load(Ordering::Acquire) || self.int.load(Ordering::Acquire)
    }

    /// What the roles are told when they stop: the signal that arrived, or SIGTERM when the supervisor stops them itself.
    fn to_forward(&self) -> Signal {
        if self.int.load(Ordering::Acquire) {
            Signal::INT
        } else {
            Signal::TERM
        }
    }
}

/// What ended a wait of the start or of the watch.
#[derive(Debug)]
enum Interrupt {
    /// SIGTERM or SIGINT arrived.
    Signal,
    /// A process ended by itself, with the code that reports it.
    Ended(u8),
    /// The supervisor refuses to go on.
    Refused(RoleError),
}

/// A process the supervisor started.
#[derive(Debug)]
struct Process {
    name: String,
    role: &'static str,
    child: Child,
}

/// The processes the supervisor holds. It holds every child's only handle, so each is reaped here.
#[derive(Debug)]
struct Supervised<'a> {
    start: &'a Start,
    signals: &'a Signals,
    processes: Vec<Process>,
}

/// Runs the supervisor named by `args`, the program's name left out: `--config <file>`, or `--help`.
///
/// # Errors
///
/// [`RoleError::configuration`] for an argument or a settings file refused; [`RoleError::unavailable`] when the handlers or the
/// runtime cannot be set up; [`RoleError::io`] when a secret cannot be written. The code a role's failure ends it with is the
/// `Ok` value, since no refusal of its own stands behind it.
pub(crate) fn run_supervisor(args: &[String]) -> Result<u8, RoleError> {
    if args.first().is_some_and(|argument| argument == "--help") {
        println!("{HELP}{}", RoleError::EXIT_CODES);
        return Ok(0);
    }
    let start = read_start(args)?;
    let _telemetry = init(&TelemetryConfig::new("info", LogFormat::Plain))
        .map_err(|error| RoleError::unavailable(format!("telemetry: {error}")))?;
    let signals = Signals::new()?;
    draw_secret(&start.read_key_file)?;
    draw_secret(&start.admin_key_file)?;
    let runtime =
        Runtime::new().map_err(|error| RoleError::unavailable(format!("the runtime: {error}")))?;
    runtime.block_on(supervise(&start, &signals))
}

fn read_start(args: &[String]) -> Result<Start, RoleError> {
    let usage = || {
        RoleError::configuration(
            "the arguments are --config <file>, the config service's own settings file, or --help",
        )
    };
    let [flag, file] = args else {
        return Err(usage());
    };
    if flag != "--config" {
        return Err(usage());
    }
    let bytes = std::fs::read(file).map_err(|error| {
        RoleError::configuration(format!(
            "{file} cannot be read: {error}. --config names the config service's own settings file"
        ))
    })?;
    let defects = check_config_service(file, &bytes);
    if !defects.is_empty() {
        let lines: Vec<String> = defects.iter().map(ToString::to_string).collect();
        return Err(RoleError::configuration(lines.join("\n")));
    }
    let config = ServiceConfig::parse(&bytes)
        .map_err(|error| RoleError::configuration(format!("{file}: {error}")))?;
    let table = config.config();
    let held = table
        .security()
        .map_err(|error| RoleError::configuration(format!("{file}: {error}")))?;
    if let EndpointSecurity::Tls { .. } = held {
        return Err(RoleError::configuration(format!(
            "{file}: setting config.plaintext: the supervisor asks the config service it starts over clear text, inside its container: set plaintext = true and remove tls_cert_file and tls_key_file; a service over TLS runs by itself, as ultragraphdb_config --config"
        )));
    }
    let setting = |value: Option<&str>, name: &str| {
        value.map(str::to_string).ok_or_else(|| {
            RoleError::configuration(format!("{file}: setting {name}: it is absent"))
        })
    };
    let address = setting(table.address(), "config.address")?;
    let host_port = tcp_host_port(&address).ok_or_else(|| {
        RoleError::configuration(format!(
            "{file}: setting config.address: it holds {address}, and the supervisor asks the service over TCP: tcp://<ip>:<port> is required"
        ))
    })?;
    let executable = std::env::current_exe()
        .map_err(|error| RoleError::unavailable(format!("where this binary stands: {error}")))?;
    let directory = executable
        .parent()
        .map(Path::to_path_buf)
        .ok_or_else(|| RoleError::unavailable("this binary stands in no directory"))?;
    Ok(Start {
        file: file.clone(),
        host_port,
        targets_file: setting(table.targets_file(), "config.targets_file")?,
        read_key_file: setting(table.read_key_file(), "config.read_key_file")?,
        admin_key_file: setting(table.admin_key_file(), "config.admin_key_file")?,
        data_dir: setting(table.data_dir(), "config.data_dir")?,
        directory,
    })
}

fn tcp_host_port(address: &str) -> Option<String> {
    address.strip_prefix("tcp://").map(str::to_string)
}

/// Whether the frontend holding `data_dir` has yet to create the cluster: its metadata group's log, `meta/` under it, is not
/// there. The bootstrap administrator's password is read at that start alone, and the operator removes it afterwards, so it is
/// drawn before that start and never again.
fn first_start(data_dir: Option<&str>) -> bool {
    data_dir.is_none_or(|directory| !Path::new(directory).join("meta").exists())
}

/// Draws the secret `file` holds when nothing is there; what is there is kept, so a restart holds what the first start drew. The
/// secret appears at `file` whole, by a rename: a draw cut short leaves its temporary name, which the next draw writes over.
fn draw_secret(file: &str) -> Result<(), RoleError> {
    let path = Path::new(file);
    if path.exists() {
        return Ok(());
    }
    let failed = |error: &dyn std::fmt::Display| {
        RoleError::io(format!(
            "the secret {file} cannot be written: {error}. Its directory must be writable by this process's user, as a volume at the data directory is"
        ))
    };
    let directory = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    std::fs::create_dir_all(directory).map_err(|error| failed(&error))?;
    let mut random = [0u8; SECRET_BYTES];
    getrandom::fill(&mut random).map_err(|error| failed(&error))?;
    let digits: String = random.iter().map(|byte| format!("{byte:02x}")).collect();
    let drawing = format!("{file}{DRAWING}");
    let mut written = std::fs::File::options()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(SECRET_MODE)
        .open(&drawing)
        .map_err(|error| failed(&error))?;
    written
        .write_all(digits.as_bytes())
        .and_then(|()| written.sync_all())
        .and_then(|()| std::fs::rename(&drawing, path))
        .and_then(|()| std::fs::File::open(directory)?.sync_all())
        .map_err(|error| failed(&error))?;
    tracing::info!(file, "drew a secret");
    Ok(())
}

/// Starts everything, watches it, stops it: the code the supervisor exits with.
async fn supervise(start: &Start, signals: &Signals) -> Result<u8, RoleError> {
    let mut supervised = Supervised {
        start,
        signals,
        processes: Vec::new(),
    };
    let interrupt = match supervised.start_all().await {
        Ok(()) => {
            tracing::info!("every role is ready");
            supervised.watch().await
        }
        Err(interrupt) => interrupt,
    };
    let (first, refused) = match interrupt {
        Interrupt::Signal => (None, None),
        Interrupt::Ended(code) => ((code != 0).then_some(code), None),
        Interrupt::Refused(error) => (None, Some(error)),
    };
    let code = supervised.stop_all(first).await;
    match refused {
        Some(error) => Err(error),
        None => Ok(code),
    }
}

impl Supervised<'_> {
    /// The start order: the config service; the frontend, once it holds its settings; storage; compute; then the frontend's
    /// readiness, which a storage partition answering it decides. No role waits on one waiting on it, since storage and compute
    /// retry their joins until their own deadlines.
    async fn start_all(&mut self) -> Result<(), Interrupt> {
        let settings = ["--config".to_string(), self.start.file.clone()];
        self.spawn(CONFIG, CONFIG, &settings, &[])?;
        let host_port = self.start.host_port.clone();
        self.until_answering(CONFIG, &host_port, "/ready").await?;
        let (target, files) = active_target(self.start)
            .await
            .map_err(Interrupt::Refused)?;
        let every = instances(&files);
        one_role_secret(&every).map_err(Interrupt::Refused)?;
        for config in every.iter().filter_map(Instance::config) {
            let secrets = [
                config.cluster().role_secret_file(),
                config
                    .metadata()
                    .bootstrap_secret_file()
                    .filter(|_| first_start(config.frontend().data_dir())),
            ];
            for file in secrets.into_iter().flatten() {
                draw_secret(file).map_err(Interrupt::Refused)?;
            }
        }
        for (role, path) in [
            ("frontend", "/health"),
            ("storage", "/ready"),
            ("compute", "/ready"),
        ] {
            for instance in every.iter().filter(|instance| instance.role() == role) {
                let observed = observability(&target, instance).map_err(Interrupt::Refused)?;
                let variables = self.role_variables(&target, instance.name());
                self.spawn(instance.name(), instance.role(), &[], &variables)?;
                self.until_answering(instance.name(), &observed, path)
                    .await?;
            }
        }
        for instance in every
            .iter()
            .filter(|instance| instance.role() == "frontend")
        {
            let observed = observability(&target, instance).map_err(Interrupt::Refused)?;
            self.until_answering(instance.name(), &observed, "/ready")
                .await?;
        }
        Ok(())
    }

    /// The bootstrap of `instance` of `target`: nothing else about a role comes from its environment.
    fn role_variables(&self, target: &str, instance: &str) -> [(&'static str, String); 4] {
        [
            (
                "ULTRAGRAPHDB_CONFIG_URL",
                format!("http://{}", self.start.host_port),
            ),
            ("ULTRAGRAPHDB_CONFIG_TARGET", target.to_string()),
            ("ULTRAGRAPHDB_CONFIG_INSTANCE", instance.to_string()),
            (
                "ULTRAGRAPHDB_CONFIG_KEY_FILE",
                self.start.read_key_file.clone(),
            ),
        ]
    }

    /// Starts the binary of `role` beside this one, in a process group of its own, so a terminal's Ctrl-C reaches the supervisor
    /// alone and the roles still stop in order. Its output goes where the supervisor's does. What this process's environment holds
    /// for a role's own bootstrap is left out, since a role refuses a variable it does not take.
    fn spawn(
        &mut self,
        name: &str,
        role: &'static str,
        args: &[String],
        variables: &[(&'static str, String)],
    ) -> Result<(), Interrupt> {
        let program = self.start.directory.join(format!("ultragraphdb_{role}"));
        let inherited = std::env::vars_os()
            .filter(|(variable, _)| !variable.as_encoded_bytes().starts_with(b"ULTRAGRAPHDB_"));
        let child = Command::new(&program)
            .args(args)
            .env_clear()
            .envs(inherited)
            .envs(variables.iter().map(|(variable, value)| (*variable, value)))
            .process_group(0)
            .spawn()
            .map_err(|error| {
                Interrupt::Refused(RoleError::unavailable(format!(
                    "{} cannot be started: {error}. The supervisor starts the binaries standing beside it",
                    program.display()
                )))
            })?;
        tracing::info!(instance = %name, pid = child.id(), "started");
        self.processes.push(Process {
            name: name.to_string(),
            role,
            child,
        });
        Ok(())
    }

    /// Why a wait ends now, if it does: a signal, or the first process found to have ended.
    fn interrupted(&mut self) -> Result<(), Interrupt> {
        if self.signals.arrived() {
            return Err(Interrupt::Signal);
        }
        for process in &mut self.processes {
            let status = process.child.try_wait().map_err(|error| {
                Interrupt::Refused(RoleError::unavailable(format!(
                    "the status of {}: {error}",
                    process.name
                )))
            })?;
            if let Some(status) = status {
                let code = code_of(status);
                tracing::error!(instance = %process.name, code, "ended");
                return Err(Interrupt::Ended(code));
            }
        }
        Ok(())
    }

    /// Waits for `address` to answer `path` with success, for as long as that takes: a role's own deadlines end a role that cannot
    /// get there, and a recovery takes the time it takes. Every 10 s the log says what is awaited.
    async fn until_answering(
        &mut self,
        name: &str,
        address: &str,
        path: &str,
    ) -> Result<(), Interrupt> {
        let url = HttpUrl::parse(&format!("http://{address}")).map_err(|error| {
            Interrupt::Refused(RoleError::configuration(format!(
                "the address {address} of {name}: {error}"
            )))
        })?;
        let client = HttpClient::new(url, None, String::new()).map_err(|error| {
            Interrupt::Refused(RoleError::configuration(format!(
                "the address {address} of {name}: {error}"
            )))
        })?;
        let mut polls = 0u32;
        loop {
            self.interrupted()?;
            let answered = client.request("GET", path, b"").await;
            if answered.is_ok_and(|reply| reply.status() == 200) {
                tracing::info!(instance = %name, path = %path, "answering");
                return Ok(());
            }
            polls = polls.wrapping_add(1);
            if polls.is_multiple_of(NOTE_POLLS) {
                tracing::info!(instance = %name, %address, path = %path, "still waiting");
            }
            compio::time::sleep(POLL).await;
        }
    }

    /// Waits until a signal arrives or a process ends. It restarts nothing.
    async fn watch(&mut self) -> Interrupt {
        loop {
            if let Err(interrupt) = self.interrupted() {
                return interrupt;
            }
            compio::time::sleep(POLL).await;
        }
    }

    /// Stops what still runs, in the shutdown order: the code to exit with, the first failure's, `first` if one came before.
    async fn stop_all(&mut self, mut first: Option<u8>) -> u8 {
        let forward = self.signals.to_forward();
        for role in STOP_ORDER {
            for process in self.processes.iter_mut().filter(|one| one.role == role) {
                tell(process, forward);
            }
            for process in self.processes.iter_mut().filter(|one| one.role == role) {
                let code = until_stopped(process, forward).await;
                tracing::info!(instance = %process.name, code, "stopped");
                if code != 0 && first.is_none() {
                    first = Some(code);
                }
            }
        }
        first.unwrap_or(0)
    }
}

/// Refused with code 78 when `every` names more than one `[cluster] role_secret_file`, naming two of them: the roles of one
/// container share one filesystem and one cluster, and a secret drawn per file would refuse every role connection.
fn one_role_secret(every: &[Instance]) -> Result<(), RoleError> {
    let mut named = every.iter().filter_map(|instance| {
        let file = instance.config()?.cluster().role_secret_file()?;
        Some((instance.name(), file))
    });
    let Some((first, file)) = named.next() else {
        return Ok(());
    };
    match named.find(|(_, other)| *other != file) {
        Some((second, other)) => Err(RoleError::configuration(format!(
            "{first} names `cluster.role_secret_file` {file} and {second} names {other}: the roles of one container share one role secret, so every instance names one file"
        ))),
        None => Ok(()),
    }
}

/// Sends `signal` to `process`. One already gone has nothing to hear: once its status is read, it is reaped, and its process
/// ID may be another process's, so it is sent nothing.
fn tell(process: &mut Process, signal: Signal) {
    if matches!(process.child.try_wait(), Ok(Some(_))) {
        return;
    }
    let pid = i32::try_from(process.child.id())
        .ok()
        .and_then(Pid::from_raw);
    if let Some(pid) = pid
        && let Err(error) = kill_process(pid, signal)
    {
        tracing::warn!(instance = %process.name, %error, "a signal found no process");
    }
}

/// Waits for `process`, told `signal`, to end, past its limit killing it: the code that reports it. A role that had no handler yet
/// when the signal came, the signal being what ended it, stopped as asked.
async fn until_stopped(process: &mut Process, signal: Signal) -> u8 {
    for _ in 0..STOP_POLLS {
        match process.child.try_wait() {
            Ok(Some(status)) => return stopped_code(status, signal),
            Ok(None) => compio::time::sleep(POLL).await,
            Err(error) => {
                tracing::error!(instance = %process.name, %error, "its status cannot be read");
                return RoleError::EX_SOFTWARE;
            }
        }
    }
    tracing::error!(instance = %process.name, "it did not stop within its limit, and is killed");
    // A process already gone is what the wait reads.
    drop(process.child.kill());
    process.child.wait().map_or(RoleError::EX_SOFTWARE, code_of)
}

fn stopped_code(status: ExitStatus, signal: Signal) -> u8 {
    if status.signal() == Some(signal.as_raw()) {
        return 0;
    }
    code_of(status)
}

/// The code `status` reports: its own, or 128 plus the number of the signal that killed it, as a shell says.
fn code_of(status: ExitStatus) -> u8 {
    const SIGNALLED: i32 = 128;
    status
        .code()
        .or_else(|| status.signal().map(|signal| SIGNALLED + signal))
        .and_then(|code| u8::try_from(code).ok())
        .unwrap_or(RoleError::EX_SOFTWARE)
}

/// Where `instance` of `target` answers `/health` and `/ready`.
fn observability(target: &str, instance: &Instance) -> Result<String, RoleError> {
    let name = instance.name();
    instance
        .config()
        .and_then(|config| config.observability().address())
        .and_then(tcp_host_port)
        .ok_or_else(|| {
            RoleError::configuration(format!(
                "target {target}, instance {name}: setting observability.address: tcp://<ip>:<port> is required, where the supervisor waits on its /ready"
            ))
        })
}

/// The one target holding an active revision, with its files at that revision: the service is the authority once it holds them.
async fn active_target(start: &Start) -> Result<(String, TargetFiles), RoleError> {
    let client = |key_file: &str| {
        let refused = |error: &dyn std::fmt::Display| {
            RoleError::configuration(format!("the key file {key_file}: {error}"))
        };
        let key = read_key(key_file).map_err(|error| refused(&error))?;
        let url = HttpUrl::parse(&format!("http://{}", start.host_port))
            .map_err(|error| refused(&error))?;
        HttpClient::new(url, None, key).map_err(|error| refused(&error))
    };
    let listed = asked(&client(&start.read_key_file)?, "/v1/targets").await?;
    let text = String::from_utf8_lossy(&listed);
    let active: Vec<&str> = text
        .lines()
        .filter(|line| !line.ends_with(" retired") && !line.ends_with(" active none"))
        .filter_map(|line| line.split(' ').next())
        .collect();
    let [target] = active.as_slice() else {
        let held: Vec<&str> = text
            .lines()
            .filter_map(|line| line.split(' ').next())
            .collect();
        return Err(not_one_target(start, &active, &held));
    };
    let path = format!("/v1/targets/{target}/download");
    let bundle = asked(&client(&start.admin_key_file)?, &path).await?;
    let files = TargetFiles::from_bundle(target, &bundle)
        .map_err(|defect| RoleError::software(defect.to_string()))?;
    Ok(((*target).to_string(), files))
}

/// Why the service's store holds no one target to run, and how to make it so: the store keeps what it once seeded, so the central
/// file no longer decides this, and the API that does is served by the service alone, which the supervisor has stopped by then.
fn not_one_target(start: &Start, active: &[&str], held: &[&str]) -> RoleError {
    let Start {
        file,
        targets_file,
        admin_key_file,
        data_dir,
        ..
    } = start;
    let cure = if active.is_empty() {
        format!(
            "holds no active revision among its targets [{}], and the supervisor runs one target. Start `ultragraphdb_config --config {file}` alone and activate one with PUT /v1/targets/<name>/active, the revision's number as its body",
            held.join(", ")
        )
    } else {
        format!(
            "holds an active revision of each of [{}], and the supervisor runs one target. A target once seeded stays, so editing {targets_file} changes nothing. Start `ultragraphdb_config --config {file}` alone and retire all but one with DELETE /v1/targets/<name>",
            active.join(", ")
        )
    };
    RoleError::configuration(format!(
        "the config service's store under {data_dir} {cure}, under the admin key in {admin_key_file}; or start over an empty {data_dir}. Run any other target with a config service of its own"
    ))
}

/// The body of what the config service answers to `GET path`.
async fn asked(client: &HttpClient, path: &str) -> Result<Vec<u8>, RoleError> {
    let reply = client
        .request("GET", path, b"")
        .await
        .map_err(|error| RoleError::unavailable(format!("asking the config service: {error}")))?;
    if reply.status() != 200 {
        return Err(RoleError::unavailable(format!(
            "the config service answers {path} with {}: {}",
            reply.status(),
            String::from_utf8_lossy(reply.body())
        )));
    }
    Ok(reply.into_body())
}
