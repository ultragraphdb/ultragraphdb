/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::cell::Cell;
use std::collections::{BTreeSet, HashSet};
use std::future::Future;
use std::process::ExitCode;
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use compio::net::TcpListener;
use compio::runtime::{Runtime, spawn};
use ultragraphdb_common_service_config::ServiceConfig;
use ultragraphdb_common_telemetry::{LogFormat, TelemetryConfig, init};
use ultragraphdb_error_role::RoleError;
use ultragraphdb_transport_http_server::{HttpLimits, serve_http};

use crate::types::cpu_binding::CpuBinding;
use crate::types::health_service::HealthService;
use crate::types::readiness::Readiness;
use crate::types::role_kind::RoleKind;
use crate::types::role_start::RoleStart;
use crate::types::stopping::Stopping;
use crate::utils::cpu_list::allowed_in_status;

/// How often the role looks at whether a signal arrived: the flag is all a handler may safely set.
const SIGNAL_POLL: Duration = Duration::from_millis(20);

/// How long a connection has to finish its request, so a silent one holds nothing.
const REQUEST_TIME: Duration = Duration::from_secs(5);

/// How many looks a stopping role gets: 60 s of `SIGNAL_POLL`.
const SHUTDOWN_POLLS: u32 = 3_000;

const SIGNAL_INT: i32 = 2;
const SIGNAL_TERM: i32 = 15;

/// Runs the role `kind` with the arguments `args`, the program's name left out, until SIGTERM or SIGINT or until
/// `role`, its work, fails. A `role` that ends without failing has started what it serves, which runs on. Past a
/// signal, [`Stopping`] says so, and a `role` still working has until its limit to flush and end.
///
/// # Errors
///
/// [`RoleError::configuration`] for a refused configuration; [`RoleError::unavailable`] when the runtime, the
/// handlers or the observability listener cannot be set up; whatever `role` fails with.
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
    let start = RoleStart::read(args, variables)?;
    let _telemetry = init(&TelemetryConfig::new("info", LogFormat::Plain))
        .map_err(|error| RoleError::unavailable(format!("telemetry: {error}")))?;
    let stop = stop_flag()?;
    let (runtime, cpu) = core_runtime(start.config(), 0)?;
    runtime.block_on(serve_service(kind, &start, cpu, &stop, role))
}

/// Serves `/health` and `/ready` for the role `kind`, and runs `role`, which sets its readiness, until `stop` is set or
/// `role` fails.
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
) -> Result<(), RoleError>
where
    R: FnOnce(RoleStart, Readiness, Stopping) -> F,
    F: Future<Output = Result<(), RoleError>> + 'static,
{
    let readiness = Readiness::default();
    let address = start.observability();
    let listener = TcpListener::bind(address)
        .await
        .map_err(|error| RoleError::unavailable(format!("listening at {address}: {error}")))?;
    tracing::info!(role = %kind, %address, "{cpu}");
    spawn(accept(listener, readiness.clone())).detach();
    // Shared with the one task on this runtime that fills them.
    let ended = Rc::new(Cell::new(None));
    let stopping = Stopping::default();
    let working = role(start.clone(), readiness, stopping.clone());
    let outcome = Rc::clone(&ended);
    spawn(async move { outcome.set(Some(working.await)) }).detach();
    let mut served = false;
    while !stop.load(Ordering::Acquire) {
        match ended.take() {
            Some(Err(error)) => return Err(error),
            Some(Ok(())) => served = true,
            None => {}
        }
        compio::time::sleep(SIGNAL_POLL).await;
    }
    tracing::info!(role = %kind, "stopping");
    stopping.set();
    if served {
        return Ok(());
    }
    // A role still working flushes what it holds before it ends; one that does not end in time is left to the exit.
    for _ in 0..SHUTDOWN_POLLS {
        match ended.take() {
            Some(finished) => return finished,
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
    Ok((runtime, binding(pin_cores, asked, thread_allowed())))
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
async fn accept(listener: TcpListener, readiness: Readiness) {
    let limits = HttpLimits::new(0, REQUEST_TIME);
    let service = HealthService::new(readiness);
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
