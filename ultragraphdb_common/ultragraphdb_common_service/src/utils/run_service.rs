/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::cell::Cell;
use std::collections::{BTreeSet, HashSet};
use std::future::Future;
use std::os::unix::process::CommandExt;
use std::process::{Command, ExitCode};
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use compio::driver::DriverType;
use compio::net::TcpListener;
use compio::runtime::{Runtime, spawn};
use ultragraphdb_common_service_config::{EndpointSecurity, ServiceConfig, ServiceConfigError};
use ultragraphdb_common_telemetry::{LogFormat, TelemetryConfig, init};
use ultragraphdb_error_role::RoleError;
use ultragraphdb_transport_http_server::{BearerKey, HttpLimits, serve_http, serve_listener};
use ultragraphdb_transport_tls::{CertificateChainFile, PrivateKeyFile, TlsAcceptor};

use crate::types::admin_service::AdminService;
use crate::types::bootstrap::{Bootstrap, PARENT_PID, REVISION, parsed};
use crate::types::cpu_binding::CpuBinding;
use crate::types::health_service::HealthService;
use crate::types::readiness::Readiness;
use crate::types::role_kind::RoleKind;
use crate::types::role_start::RoleStart;
use crate::types::service_end::ServiceEnd;
use crate::types::stopping::Stopping;
use crate::utils::cpu_list::allowed_in_status;
use crate::utils::fetch_settings::fetch_settings;
use crate::utils::listen::{bind_tcp, listen};

/// How often the role looks at whether a signal arrived: the flag is all a handler may safely set.
const SIGNAL_POLL: Duration = Duration::from_millis(20);

/// How often a role named a parent to end with looks at whether it still has it.
const PARENT_POLL: Duration = Duration::from_millis(100);

/// How long a connection has to finish its request, so a silent one holds nothing.
const REQUEST_TIME: Duration = Duration::from_secs(5);

/// How many looks a stopping role gets: 60 s of `SIGNAL_POLL`.
const SHUTDOWN_POLLS: u32 = 3_000;

/// How long a load's answer has to reach whoever asked, before the role stops.
const LOAD_REPLY_TIME: Duration = Duration::from_millis(50);

/// The most an admin request holds: a revision's number.
const ADMIN_BYTES: usize = 64;

/// How long a role asks an unreachable config service before it gives up.
const CONFIG_PATIENCE: Duration = Duration::from_secs(60);

const SIGNAL_INT: i32 = 2;
const SIGNAL_TERM: i32 = 15;

/// Runs the role `kind`, its settings fetched from the config service the environment `variables` name, until SIGTERM or
/// SIGINT or until `role`, its work, fails. It takes no arguments, `args` being the program's name left out. A `role` that ends without failing has started what it serves, which runs on. Past a
/// signal, [`Stopping`] says so, and a `role` still working has until its limit to flush and end.
///
/// # Errors
///
/// [`RoleError::configuration`] for an argument, a variable, or settings refused; [`RoleError::unavailable`] when the config
/// service does not answer within a minute, or the runtime, the handlers or the observability listener cannot be set up;
/// whatever `role` fails with.
pub fn run_service<R, F>(
    kind: RoleKind,
    args: &[String],
    variables: impl IntoIterator<Item = (String, String)>,
    role: R,
) -> Result<(), RoleError>
where
    R: FnOnce(RoleStart, Readiness, Stopping) -> F,
    F: Future<Output = Result<(), RoleError>> + 'static,
{
    if let Some(argument) = args.first() {
        if argument == "--help" {
            println!("{}", role_help(kind));
            return Ok(());
        }
        return Err(RoleError::configuration(format!(
            "a role takes no arguments but --help, not {argument}: its environment names the config service, which holds its settings"
        )));
    }
    let bootstrap = Bootstrap::read(kind, variables)?;
    let telemetry = init(&TelemetryConfig::new("info", LogFormat::Plain))
        .map_err(|error| RoleError::unavailable(format!("telemetry: {error}")))?;
    let stop = stop_flag()?;
    if let Some(parent) = bootstrap.parent() {
        watch_parent(parent, Arc::clone(&stop))?;
    }
    // Nothing listens before this: the role's addresses are among the settings it fetches.
    let fetching =
        Runtime::new().map_err(|error| RoleError::unavailable(format!("the runtime: {error}")))?;
    let fetched = fetching.block_on(fetch_settings(&bootstrap, CONFIG_PATIENCE, &stop))?;
    drop(fetching);
    let Some(start) = fetched else {
        return Ok(());
    };
    let (runtime, cpu) = core_runtime(start.config(), 0)?;
    let ended = runtime.block_on(serve_service(kind, &start, cpu, &stop, role))?;
    drop(runtime);
    drop(telemetry);
    match ended {
        ServiceEnd::Stopped => Ok(()),
        ServiceEnd::Load(revision) => Err(restart_at(revision)),
    }
}

/// What `--help` prints for the role `kind`.
#[must_use]
pub fn role_help(kind: RoleKind) -> String {
    format!(
        "ultragraphdb_{kind}

The {kind} role. It takes no arguments: its environment names the config service, which holds its settings.

  ULTRAGRAPHDB_CONFIG_URL       the service's https://host:port, or http:// for a plaintext service
  ULTRAGRAPHDB_CONFIG_TARGET    the deployment target
  ULTRAGRAPHDB_CONFIG_INSTANCE  this instance's name: {kind}, or {kind} followed by _ and a label
  ULTRAGRAPHDB_CONFIG_KEY_FILE  the file holding the read key
  ULTRAGRAPHDB_CONFIG_CA_FILE   the certificates that verify an https service
  ULTRAGRAPHDB_PARENT_PID       a process to end with: once it is not this one's parent, the role stops as on SIGTERM

Exit codes:
{}",
        RoleError::EXIT_CODES
    )
}

/// Executes this binary again in this process, so a supervisor sees no exit, starting at `revision`: what comes back is why it
/// could not. Every descriptor this process holds closes on the execution.
fn restart_at(revision: u64) -> RoleError {
    let failed = |why: &dyn std::fmt::Display| {
        RoleError::unavailable(format!("restarting at revision {revision}: {why}"))
    };
    match std::env::current_exe() {
        Ok(program) => failed(
            &Command::new(program)
                .env(REVISION, revision.to_string())
                .exec(),
        ),
        Err(error) => failed(&error),
    }
}

/// Serves `/health` and `/ready` for the role `kind`, and `/admin/load-config` when its settings name that listener, and runs
/// `role`, which sets its readiness, until `stop` is set, a load is taken, or `role` fails.
///
/// # Errors
///
/// [`RoleError::unavailable`] when the observability address cannot be listened at; whatever `role` fails with.
pub async fn serve_service<R, F>(
    kind: RoleKind,
    start: &RoleStart,
    cpu: CpuBinding,
    stop: &AtomicBool,
    role: R,
) -> Result<ServiceEnd, RoleError>
where
    R: FnOnce(RoleStart, Readiness, Stopping) -> F,
    F: Future<Output = Result<(), RoleError>> + 'static,
{
    let load = Rc::new(Cell::new(None));
    if let Some(bootstrap) = start.bootstrap() {
        serve_admin(start.config(), bootstrap, Rc::clone(&load)).await?;
    }
    let readiness = Readiness::default();
    let address = start.observability();
    let listener = TcpListener::from_std(bind_tcp(&address.to_string()).await?)
        .map_err(|error| RoleError::unavailable(format!("listening at {address}: {error}")))?;
    tracing::info!(role = %kind, %address, running = %start.identity(), "{cpu}");
    spawn(accept(
        listener,
        HealthService::new(readiness.clone(), start.identity()),
    ))
    .detach();
    // Shared with the one task on this runtime that fills them.
    let ended = Rc::new(Cell::new(None));
    let stopping = Stopping::default();
    let working = role(start.clone(), readiness, stopping.clone());
    let outcome = Rc::clone(&ended);
    spawn(async move { outcome.set(Some(working.await)) }).detach();
    let mut served = false;
    while !stop.load(Ordering::Acquire) && load.get().is_none() {
        match ended.take() {
            Some(Err(error)) => return Err(error),
            Some(Ok(())) => served = true,
            None => {}
        }
        compio::time::sleep(SIGNAL_POLL).await;
    }
    // The request that asked for the load has its answer on the wire before anything stops.
    if load.get().is_some() {
        compio::time::sleep(LOAD_REPLY_TIME).await;
    }
    let end = load.get().map_or(ServiceEnd::Stopped, ServiceEnd::Load);
    tracing::info!(role = %kind, ?end, "stopping");
    stopping.set();
    if served {
        return Ok(end);
    }
    // A role still working flushes what it holds before it ends; one that does not end in time is left to the exit.
    for _ in 0..SHUTDOWN_POLLS {
        match ended.take() {
            Some(finished) => return finished.map(|()| end),
            None => compio::time::sleep(SIGNAL_POLL).await,
        }
    }
    Err(RoleError::unavailable(
        "the role did not stop within its shutdown limit",
    ))
}

/// The code the process exits with after `outcome`, its refusal printed where an operator reads it.
pub fn exit_status(outcome: Result<(), RoleError>) -> ExitCode {
    match outcome {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("{error}");
            ExitCode::from(error.exit_code())
        }
    }
}

/// A flag SIGTERM and SIGINT set.
///
/// # Errors
///
/// [`RoleError::unavailable`] when a handler cannot be set.
pub fn stop_flag() -> Result<Arc<AtomicBool>, RoleError> {
    // Shared with the signal handler, which may do nothing else (Ultra rule 9).
    let stop = Arc::new(AtomicBool::new(false));
    for signal in [SIGNAL_TERM, SIGNAL_INT] {
        let _id = signal_hook::flag::register(signal, Arc::clone(&stop))
            .map_err(|error| RoleError::unavailable(format!("signal {signal}: {error}")))?;
    }
    Ok(stop)
}

/// Sets `stop`, as SIGTERM does, once the process this one's environment names in [`PARENT_PID`], if any, is no longer its parent.
/// What the config service and the supervisor call, which read no [`Bootstrap`].
///
/// # Errors
///
/// [`RoleError::configuration`] when the variable holds no process's identity; [`RoleError::unavailable`] when the thread watching
/// cannot start.
pub fn stop_with_parent(stop: &Arc<AtomicBool>) -> Result<(), RoleError> {
    let named = std::env::var_os(PARENT_PID).map(|value| value.to_string_lossy().into_owned());
    match parsed(named, PARENT_PID, "a process's identity")? {
        Some(parent) => watch_parent(parent, Arc::clone(stop)),
        None => Ok(()),
    }
}

/// Sets `stop`, as SIGTERM does, once `parent` is no longer this process's parent: the process that started it ended, and the
/// role went to another. The process's parent is all it reads, which macOS and Linux alike report.
///
/// # Errors
///
/// [`RoleError::unavailable`] when the thread watching cannot start.
fn watch_parent(parent: u32, stop: Arc<AtomicBool>) -> Result<(), RoleError> {
    #[expect(
        clippy::disallowed_methods,
        reason = "a role's own thread, which no simulation replays"
    )]
    let spawned = std::thread::Builder::new()
        .name("parent".to_string())
        .spawn(move || {
            while std::os::unix::process::parent_id() == parent && !stop.load(Ordering::Acquire) {
                std::thread::sleep(PARENT_POLL);
            }
            stop.store(true, Ordering::Release);
        });
    spawned
        .map(drop)
        .map_err(|error| RoleError::unavailable(format!("watching the parent process: {error}")))
}

/// A runtime for the calling thread, the role's `core`th, from 0, and what became of its binding: to the `core`th CPU it
/// is permitted when the settings ask and the platform can say what it holds. A platform without `/proc` gets no request
/// at all: macOS would take it as no more than a cache hint.
///
/// # Errors
///
/// [`RoleError::unavailable`] when the runtime cannot be built.
pub fn core_runtime(
    config: &ServiceConfig,
    core: usize,
) -> Result<(Runtime, CpuBinding), RoleError> {
    let pin_cores = config.runtime().pin_cores();
    let asked = (pin_cores == Some(true))
        .then(thread_allowed)
        .flatten()
        .and_then(|allowed| allowed.iter().nth(core).copied());
    let mut builder = Runtime::builder();
    if let Some(cpu) = asked {
        let _builder = builder.thread_affinity(HashSet::from([cpu]));
    }
    let runtime = builder
        .build()
        .map_err(|error| RoleError::unavailable(format!("the runtime: {error}")))?;
    tracing::info!(
        core,
        "a runtime on the {} driver",
        driver_name(runtime.driver_type())
    );
    Ok((runtime, binding(pin_cores, asked, thread_allowed())))
}

/// How the log names `driver`. compio takes io_uring where the kernel allows it and polls where its system calls are refused.
#[must_use]
pub fn driver_name(driver: DriverType) -> &'static str {
    match driver {
        DriverType::Poll => "polling",
        DriverType::IoUring => "io_uring",
        DriverType::IOCP => "iocp",
    }
}

/// The CPUs the calling thread may run on, or `None` where the operating system does not say.
fn thread_allowed() -> Option<BTreeSet<usize>> {
    let status = std::fs::read_to_string("/proc/thread-self/status").ok()?;
    allowed_in_status(&status)
}

/// What became of a runtime's binding, given `pin_cores`, the CPU it asked for, and what its thread holds as the operating
/// system says: compio reports no failure, so only reading it back can tell.
#[must_use]
pub fn binding(
    pin_cores: Option<bool>,
    asked: Option<usize>,
    held: Option<BTreeSet<usize>>,
) -> CpuBinding {
    if pin_cores != Some(true) {
        return CpuBinding::NotAsked;
    }
    match (asked, held) {
        (Some(asked), Some(held)) => CpuBinding::ReadBack { asked, held },
        _ => CpuBinding::Unavailable,
    }
}

/// Answers every connection on its own task; a failed accept costs that connection alone.
async fn accept(listener: TcpListener, service: HealthService) {
    let limits = HttpLimits::new(0, REQUEST_TIME);
    loop {
        match listener.accept().await {
            Ok((stream, _peer)) => {
                let service = service.clone();
                spawn(async move { serve_http(stream, &limits, &service).await }).detach();
            }
            Err(error) => tracing::warn!(%error, "a health connection was not accepted"),
        }
    }
}

/// Listens at `[admin] address`, when the settings name it, for a load.
async fn serve_admin(
    config: &ServiceConfig,
    bootstrap: &Bootstrap,
    load: Rc<Cell<Option<u64>>>,
) -> Result<(), RoleError> {
    let table = config.admin();
    let Some(address) = table.address() else {
        return Ok(());
    };
    let refused = |error: &dyn std::fmt::Display| RoleError::configuration(error.to_string());
    let digest = table
        .key_sha256()
        .ok_or_else(|| refused(&ServiceConfigError::missing("admin", "key_sha256")))?;
    let key = BearerKey::from_digest_hex(digest)
        .map_err(|error| refused(&format!("the setting admin.key_sha256: {error}")))?;
    let tls = match table.security().map_err(|error| refused(&error))? {
        EndpointSecurity::Plaintext => None,
        EndpointSecurity::Tls {
            cert_file,
            key_file,
        } => Some(
            TlsAcceptor::from_pem_files(
                &CertificateChainFile::new(cert_file),
                &PrivateKeyFile::new(key_file),
            )
            .map_err(|error| refused(&format!("the setting admin.tls_cert_file: {error}")))?,
        ),
    };
    let listener = listen(address).await?;
    let service = AdminService::new(key, bootstrap.clone(), load);
    spawn(serve_listener(
        listener,
        tls,
        HttpLimits::new(ADMIN_BYTES, REQUEST_TIME),
        service,
    ))
    .detach();
    Ok(())
}
