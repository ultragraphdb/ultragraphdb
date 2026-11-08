/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::cell::Cell;
use std::rc::Rc;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Duration;

use compio::runtime::{Runtime, spawn};
use ultragraphdb_common_role::{
    Answer, OpenRequests, ROLE_ITERATIONS, RoleClient, RoleCredentials, RoleSecret, Server,
    Session, finish_open, listen, serve_connection,
};
use ultragraphdb_common_service::Stopping;
use ultragraphdb_protocol_transport::{Address, Endpoint, ReceivedFrame, TransportErrorEnum};
use ultragraphdb_protocol_wire::{
    BodyBuilder, Command, ErrorCode, ErrorReply, FLAG_LAST, Header, Members,
};
use ultragraphdb_tool_bdd::{Context, TestError, TestResult};

const CLUSTER: u64 = 7;
const PASSWORD: &str = "s3cret";
const STEP: Duration = Duration::from_millis(5);
const LONG: Duration = Duration::from_secs(20);

static FILES: AtomicU32 = AtomicU32::new(0);

/// How long a test may take: far past what any here takes, so a peer that stops answering fails the test rather than holds it.
const TEST_TIME: Duration = Duration::from_secs(10);

/// Runs `test` on a runtime of its own, failing it past [`TEST_TIME`].
fn bounded(test: impl Future<Output = TestResult>) -> TestResult {
    Runtime::new()?.block_on(async {
        compio::time::timeout(TEST_TIME, test)
            .await
            .map_err(|_| TestError::new(format!("the test within {TEST_TIME:?}")))?
    })
}

fn credentials() -> TestResult<RoleCredentials> {
    let file = std::env::temp_dir().join(format!(
        "finish-open-secret-{}-{}",
        std::process::id(),
        FILES.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::write(&file, PASSWORD)?;
    Ok(RoleSecret::read(&file.display().to_string())?.credentials()?)
}

/// Answers a request with an empty Members once `release` is set, saying in `entered` that one has arrived.
#[derive(Debug, Clone, Default)]
struct Held {
    entered: Rc<Cell<bool>>,
    release: Rc<Cell<bool>>,
}

impl Session for Held {
    async fn answer(&mut self, stream: u64, _frame: ReceivedFrame) -> Answer {
        self.entered.set(true);
        while !self.release.get() {
            compio::time::sleep(STEP).await;
        }
        let header = Header::new(CLUSTER, Command::Members)
            .with_stream(stream)
            .with_flags(FLAG_LAST);
        Members::new().frame(header).map_or_else(
            |rule| Answer::Refuse(ErrorReply::protocol(&rule)),
            Answer::Reply,
        )
    }
}

/// A served role endpoint and what accepts its connections.
struct Serving {
    address: Address,
    accepting: compio::runtime::JoinHandle<()>,
    open: OpenRequests,
    stopping: Stopping,
    session: Held,
}

async fn serving() -> TestResult<Serving> {
    let port = std::net::TcpListener::bind("127.0.0.1:0")?
        .local_addr()?
        .port();
    let listener = listen(&format!("tcp://127.0.0.1:{port}")).await?;
    let address = listener.local_address()?;
    let stopping = Stopping::default();
    let open = OpenRequests::new(stopping.clone());
    let server = Server::new(CLUSTER, "test", Endpoint::Frontend)?
        .with_iterations(ROLE_ITERATIONS)
        .with_open(open.clone());
    let held = credentials()?;
    let session = Held::default();
    let shared = session.clone();
    let accepting = spawn(async move {
        while let Ok(carrier) = listener.accept().await {
            let (server, held, each) = (server.clone(), held.clone(), shared.clone());
            spawn(async move { serve_connection(carrier, &server, held, each).await }).detach();
        }
    });
    Ok(Serving {
        address,
        accepting,
        open,
        stopping,
        session,
    })
}

async fn dial(address: &Address) -> TestResult<RoleClient> {
    Ok(RoleClient::dial(address, CLUSTER, "storage", PASSWORD).await?)
}

/// Waits until `holds`, a step apart for at most 5 s, or fails naming what it waited for.
async fn until(what: &str, holds: impl Fn() -> bool) -> TestResult {
    for _ in 0..1_000 {
        if holds() {
            return Ok(());
        }
        compio::time::sleep(STEP).await;
    }
    Err(TestError::new(format!(
        "{what} did not come to hold within 5 s"
    )))
}

/// The connection's frames of its own are written before its client reads them, so what it counts for them may still be falling.
async fn until_none_open(open: &OpenRequests) -> TestResult {
    until("no request open", || open.count() == 0).await
}

/// A request open when a stop comes reaches its client, and the stop waits for it with the listener closed.
#[test]
fn test_a_stop_closes_the_listener_and_finishes_the_open_request() -> TestResult {
    bounded(async {
        let Serving {
            address,
            accepting,
            open,
            stopping,
            session,
        } = serving().await?;
        let mut client = dial(&address).await?;
        until_none_open(&open).await?;
        let replied = spawn(async move { client.request(&Members::new()).await.map(drop) });
        until("the request at its session", || session.entered.get()).await?;
        assert_eq!(open.count(), 1, "the request is open");
        stopping.set();
        let finished = Rc::new(Cell::new(None));
        let (told, counted) = (Rc::clone(&finished), open.clone());
        spawn(async move { told.set(Some(finish_open(vec![accepting], &counted, LONG).await)) })
            .detach();
        compio::time::sleep(STEP * 10).await;
        assert!(dial(&address).await.is_err(), "the listener is closed");
        assert_eq!(finished.get(), None, "the stop waits on the open request");
        session.release.set(true);
        replied.await.context("the request's task")??;
        until("the stop's end", || finished.get().is_some()).await?;
        assert_eq!(finished.get(), Some(0));
        Ok(())
    })
}

/// A request that does not finish holds a stop no longer than its limit, and is reported.
#[test]
fn test_a_stop_gives_up_at_its_limit() -> TestResult {
    bounded(async {
        let Serving {
            address,
            accepting,
            open,
            session,
            ..
        } = serving().await?;
        let mut client = dial(&address).await?;
        until_none_open(&open).await?;
        let asked = spawn(async move { client.request(&Members::new()).await.map(drop) });
        until("the request at its session", || session.entered.get()).await?;
        assert_eq!(finish_open(vec![accepting], &open, STEP * 4).await, 1);
        drop(asked);
        Ok(())
    })
}

/// A request arriving on a held connection after the stop began is refused, so a busy peer holds no stop open.
#[test]
fn test_a_request_after_the_stop_is_refused_as_unavailable() -> TestResult {
    bounded(async {
        let Serving {
            address,
            accepting: _accepting,
            stopping,
            session,
            ..
        } = serving().await?;
        let mut client = dial(&address).await?;
        stopping.set();
        let refused = client
            .request(&Members::new())
            .await
            .err()
            .context("a refusal")?;
        let TransportErrorEnum::Refused { code, message } = &refused.0 else {
            return Err(refused.into());
        };
        assert_eq!(*code, ErrorCode::Unavailable);
        assert!(message.contains("stopping"), "{message}");
        assert!(!session.entered.get(), "the session never saw it");
        Ok(())
    })
}

/// A stop gives up at its limit and no later: one request held open past a limit of 100 ms ends the wait well within a second,
/// reported as still open.
#[test]
fn test_a_stop_ends_at_its_limit_and_no_later() -> TestResult {
    bounded(async {
        let open = OpenRequests::default();
        let _held = open.enter();
        let limit = Duration::from_millis(100);
        let waiting = finish_open(Vec::<compio::runtime::JoinHandle<()>>::new(), &open, limit);
        let left = compio::time::timeout(Duration::from_secs(1), waiting)
            .await
            .map_err(|_| TestError::new("a stop past ten times its limit"))?;
        assert_eq!(left, 1);
        Ok(())
    })
}
