/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::collections::BTreeSet;
use std::process::ExitCode;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use compio::BufResult;
use compio::io::{AsyncReadExt, AsyncWrite, AsyncWriteExt};
use compio::net::TcpStream;
use compio::runtime::{JoinHandle, Runtime, spawn};
use ultragraphdb_common_service::{
    CpuBinding, Readiness, RoleKind, RoleStart, binding, exit_status, run_service, serve_service,
};
use ultragraphdb_error_role::RoleError;
use ultragraphdb_tool_bdd::{Context, TestError, TestResult};

const HEALTH: &str = "GET /health HTTP/1.1\r\n\r\n";
const READY: &str = "GET /ready HTTP/1.1\r\n\r\n";

/// A configuration refused stops the role before it binds or installs anything.
#[test]
fn test_a_refused_start_returns_its_error() -> TestResult {
    let refused = run_service(RoleKind::Storage, &[], [], |_start, _readiness| async {
        Ok(())
    })
    .err()
    .context("a refusal")?;
    assert_eq!(refused.exit_code(), RoleError::EX_CONFIG);
    Ok(())
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
    Ok(RoleStart::read(
        &[],
        [("ULTRAGRAPHDB_OBSERVABILITY_ADDRESS".to_string(), address)],
    )?)
}

/// A role whose work sets its readiness to `ready` and ends, leaving the service serving.
fn role_ready(
    ready: bool,
) -> impl FnOnce(RoleStart, Readiness) -> std::future::Ready<Result<(), RoleError>> {
    move |_start, readiness| {
        readiness.set(ready);
        std::future::ready(Ok(()))
    }
}

/// The role `kind`, ready as `ready` says, serving `start` on this runtime until the flag it returns is set.
fn serving(
    kind: RoleKind,
    start: &RoleStart,
    ready: bool,
) -> (Arc<AtomicBool>, JoinHandle<Result<(), RoleError>>) {
    let stop = Arc::new(AtomicBool::new(false));
    let (owned, flag) = (start.clone(), Arc::clone(&stop));
    let task = spawn(async move {
        serve_service(kind, &owned, CpuBinding::NotAsked, &flag, role_ready(ready)).await
    });
    (stop, task)
}

/// Stops the role and checks it ended cleanly.
async fn stopped(stop: &AtomicBool, task: JoinHandle<Result<(), RoleError>>) -> TestResult {
    stop.store(true, Ordering::Release);
    let outcome = task
        .await
        .map_err(|_| TestError::new("the role panicked"))?;
    Ok(outcome?)
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
        for (kind, ready, answers) in [
            (RoleKind::Storage, true, "200"),
            (RoleKind::Frontend, false, "503"),
            (RoleKind::Compute, false, "503"),
        ] {
            let start = start_on_free_port()?;
            let (stop, task) = serving(kind, &start, ready);
            let health = exchange(&start, &[HEALTH]).await?;
            assert_eq!(status(&health), Some("200"), "{kind}");
            let answered = exchange(&start, &[READY]).await?;
            assert_eq!(status(&answered), Some(answers), "{kind}");
            let (first, last) = HEALTH.split_at(7);
            let pieces = exchange(&start, &[first, last]).await?;
            assert_eq!(status(&pieces), Some("200"), "a request in two writes");
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
        let (stop, task) = serving(RoleKind::Storage, &start, true);
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
    let start = RoleStart::read(
        &[],
        [("ULTRAGRAPHDB_OBSERVABILITY_ADDRESS".to_string(), address)],
    )?;
    let runtime = Runtime::new()?;
    let stop = AtomicBool::new(false);
    let refused = runtime
        .block_on(serve_service(
            RoleKind::Storage,
            &start,
            CpuBinding::NotAsked,
            &stop,
            role_ready(true),
        ))
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
            |_start, _readiness| std::future::ready(Err(RoleError::configuration("no secret"))),
        ))
        .err()
        .context("the role's failure")?;
    assert_eq!(refused.exit_code(), RoleError::EX_CONFIG);
    assert!(refused.to_string().contains("no secret"), "{refused}");
    Ok(())
}
