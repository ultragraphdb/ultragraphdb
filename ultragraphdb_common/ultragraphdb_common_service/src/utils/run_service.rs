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

use compio::BufResult;
use compio::io::{AsyncRead, AsyncWriteExt};
use compio::net::{TcpListener, TcpStream};
use compio::runtime::{Runtime, spawn};
use ultragraphdb_common_service_config::ServiceConfig;
use ultragraphdb_common_telemetry::{LogFormat, TelemetryConfig, init};
use ultragraphdb_error_role::RoleError;

use crate::types::cpu_binding::CpuBinding;
use crate::types::health_request::HealthRequest;
use crate::types::readiness::Readiness;
use crate::types::role_kind::RoleKind;
use crate::types::role_start::RoleStart;
use crate::utils::cpu_list::allowed_in_status;
use crate::utils::http_responder::{REQUEST_MAX, parse_request, response};

/// How often the role looks at whether a signal arrived: the flag is all a handler may safely set.
const SIGNAL_POLL: Duration = Duration::from_millis(20);

/// How long a connection has to finish its request, so a silent one holds nothing.
const REQUEST_TIME: Duration = Duration::from_secs(5);

const SIGNAL_INT: i32 = 2;
const SIGNAL_TERM: i32 = 15;

/// Runs the role `kind` with the arguments `args`, the program's name left out, until SIGTERM or SIGINT or until
/// `role`, its work, fails. A `role` that ends without failing has started what it serves, which runs on.
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
    R: FnOnce(RoleStart, Readiness) -> F,
    F: Future<Output = Result<(), RoleError>> + 'static,
{
    let start = RoleStart::read(args, variables)?;
    let _telemetry = init(&TelemetryConfig::new("info", LogFormat::Plain))
        .map_err(|error| RoleError::unavailable(format!("telemetry: {error}")))?;
    let stop = stop_flag()?;
    let (runtime, asked) = role_runtime(start.config())?;
    let cpu = binding(
        start.config().runtime().pin_cores(),
        asked,
        thread_allowed(),
    );
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
    R: FnOnce(RoleStart, Readiness) -> F,
    F: Future<Output = Result<(), RoleError>> + 'static,
{
    let readiness = Readiness::default();
    let address = start.observability();
    let listener = TcpListener::bind(address)
        .await
        .map_err(|error| RoleError::unavailable(format!("listening at {address}: {error}")))?;
    tracing::info!(role = %kind, %address, "{cpu}");
    spawn(accept(listener, readiness.clone())).detach();
    // Shared with the one task on this runtime that may fill it.
    let failed = Rc::new(Cell::new(None));
    let working = role(start.clone(), readiness);
    let outcome = Rc::clone(&failed);
    spawn(async move { outcome.set(working.await.err()) }).detach();
    while !stop.load(Ordering::Acquire) {
        if let Some(error) = failed.take() {
            return Err(error);
        }
        compio::time::sleep(SIGNAL_POLL).await;
    }
    tracing::info!(role = %kind, "stopping");
    Ok(())
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
fn stop_flag() -> Result<Arc<AtomicBool>, RoleError> {
    // Shared with the signal handler, which may do nothing else (Ultra rule 9).
    let stop = Arc::new(AtomicBool::new(false));
    for signal in [SIGNAL_TERM, SIGNAL_INT] {
        let _id = signal_hook::flag::register(signal, Arc::clone(&stop))
            .map_err(|error| RoleError::unavailable(format!("signal {signal}: {error}")))?;
    }
    Ok(stop)
}

/// The role's runtime, bound to the first CPU it is permitted when the settings ask and the platform can say what it
/// holds. A platform without `/proc` gets no request at all: macOS would take it as no more than a cache hint.
fn role_runtime(config: &ServiceConfig) -> Result<(Runtime, Option<usize>), RoleError> {
    let asked = (config.runtime().pin_cores() == Some(true))
        .then(thread_allowed)
        .flatten()
        .and_then(|allowed| allowed.first().copied());
    let mut builder = Runtime::builder();
    if let Some(cpu) = asked {
        let _builder = builder.thread_affinity(HashSet::from([cpu]));
    }
    let runtime = builder
        .build()
        .map_err(|error| RoleError::unavailable(format!("the runtime: {error}")))?;
    Ok((runtime, asked))
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
    loop {
        match listener.accept().await {
            Ok((stream, _peer)) => spawn(answer(stream, readiness.clone())).detach(),
            Err(error) => tracing::warn!(%error, "a health connection was not accepted"),
        }
    }
}

/// Reads one request and writes its response. A connection that fails or goes silent is dropped: its probe sees that.
async fn answer(mut stream: TcpStream, readiness: Readiness) {
    let exchange = async {
        let request = read_request(&mut stream).await;
        let text = response(request, readiness.is_ready());
        stream.write_all(text.as_bytes().to_vec()).await.0
    };
    if let Ok(Err(error)) = compio::time::timeout(REQUEST_TIME, exchange).await {
        tracing::debug!(%error, "a health response was not written");
    }
}

/// The request arriving on `stream`, read until it is complete, past its bound, or the peer stops sending.
async fn read_request(stream: &mut TcpStream) -> HealthRequest {
    let mut received = Vec::with_capacity(REQUEST_MAX);
    // Each read takes at least a byte, so the bound on the bytes bounds the loop.
    for _ in 0..REQUEST_MAX {
        let chunk = Vec::with_capacity(REQUEST_MAX - received.len().min(REQUEST_MAX - 1));
        let BufResult(read, chunk) = stream.read(chunk).await;
        match read {
            Ok(count) if count > 0 => received.extend_from_slice(&chunk),
            Ok(_) | Err(_) => return HealthRequest::BadRequest,
        }
        let request = parse_request(&received);
        if request != HealthRequest::Incomplete {
            return request;
        }
    }
    HealthRequest::BadRequest
}
