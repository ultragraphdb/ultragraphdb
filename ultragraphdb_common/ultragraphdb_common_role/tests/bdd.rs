/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The wire protocol's scenarios of a role connection's handshake, and the system group's scenario of a role's start on
//! its settings (`features/system-group/`).

use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::mpsc;
use std::time::Duration;

use compio::runtime::{Runtime, spawn_blocking};
use cucumber::{World, then, when};
use ultragraphdb_common_role::{
    Answer, ROLE_ITERATIONS, RoleClient, RoleCredentials, RoleSecret, Server, Session, listen,
    serve_connection,
};
use ultragraphdb_common_service::{RoleIdentity, RoleStart};
use ultragraphdb_error_role::RoleError;
use ultragraphdb_protocol_transport::{Address, Endpoint, ReceivedFrame};
use ultragraphdb_protocol_wire::{ErrorCode, ErrorReply, MAX_FRAME_SIZE, ROLE_MAX_STREAMS};
use ultragraphdb_tool_bdd::{Context, TestError, TestResult};

const CLUSTER: u64 = 7;
const PASSWORD: &str = "s3cret";

/// How long a step waits on the server: far past what a handshake on this host takes.
const WAIT: Duration = Duration::from_secs(5);

/// What the connection's two sides announced, and a role's start on its settings.
#[derive(Debug, Default, World)]
struct Steps {
    welcome: Option<(u32, u32)>,
    hello: u32,
    start: Option<Result<RoleStart, RoleError>>,
}

/// Holds what the peer's Hello stated, and serves nothing.
#[derive(Debug)]
struct Recording {
    hello: Arc<AtomicU32>,
}

impl Session for Recording {
    async fn answer(&mut self, _stream: u64, _frame: ReceivedFrame) -> Answer {
        Answer::Refuse(ErrorReply::new(ErrorCode::Unavailable, "no"))
    }

    fn welcomed(&mut self, max_frame: u32) {
        self.hello.store(max_frame, Ordering::Release);
    }
}

/// Serves one connection at `text` on a thread and a runtime of its own, so a server that never yields cannot hold this
/// step's bounded wait: the address it holds.
async fn serve_apart(
    text: String,
    held: RoleCredentials,
    hello: Arc<AtomicU32>,
) -> TestResult<Address> {
    let (bound, address) = mpsc::channel();
    spawn_blocking(move || {
        let Ok(runtime) = Runtime::new() else {
            return;
        };
        runtime.block_on(async move {
            let Ok(server) = Server::new(CLUSTER, "test", Endpoint::Partition)
                .map(|server| server.with_iterations(ROLE_ITERATIONS))
            else {
                return;
            };
            let Ok(listener) = listen(&text).await else {
                return;
            };
            drop(bound.send(listener.local_address().ok()));
            if let Ok(carrier) = listener.accept().await {
                serve_connection(carrier, &server, held, Recording { hello }).await;
            }
        });
    })
    .detach();
    // Polled, not waited on: the blocking task starts only as this runtime runs.
    for _ in 0..1_000 {
        if let Ok(sent) = address.try_recv() {
            return sent.context("the server's address");
        }
        compio::time::sleep(Duration::from_millis(5)).await;
    }
    Err(TestError::new("the server listening within 5 s"))
}

#[when("a role connects to a role endpoint")]
fn a_role_connects(w: &mut Steps) -> TestResult {
    let file = std::env::temp_dir().join(format!("role-limits-{}", std::process::id()));
    std::fs::write(&file, PASSWORD)?;
    let held = RoleSecret::read(&file.display().to_string())?.credentials()?;
    Runtime::new()?.block_on(async {
        let port = std::net::TcpListener::bind("127.0.0.1:0")?
            .local_addr()?
            .port();
        let hello = Arc::new(AtomicU32::new(0));
        let address =
            serve_apart(format!("tcp://127.0.0.1:{port}"), held, Arc::clone(&hello)).await?;
        let client = compio::time::timeout(
            WAIT,
            RoleClient::dial(&address, CLUSTER, "compute", PASSWORD),
        )
        .await
        .map_err(|_| TestError::new(format!("a handshake within {WAIT:?}")))??;
        let welcome = client.welcome();
        w.welcome = Some((welcome.max_frame(), welcome.max_streams()));
        w.hello = hello.load(Ordering::Acquire);
        TestResult::Ok(())
    })
}

#[then(
    "the Welcome names `max_frame` 1,048,576 and `max_streams` 4,294,967,295, and the role's Hello named `max_frame` 1,048,576"
)]
fn its_limits(w: &mut Steps) -> TestResult {
    let (max_frame, max_streams) = w.welcome.context("a Welcome")?;
    assert_eq!(max_frame as usize, MAX_FRAME_SIZE);
    assert_eq!(max_streams, ROLE_MAX_STREAMS);
    assert_eq!(w.hello as usize, MAX_FRAME_SIZE);
    Ok(())
}

#[when("a storage instance's `[system]` table names `role_address` and no `data_dir`")]
fn a_system_table_without_its_directory(w: &mut Steps) {
    let text = "[observability]\naddress = \"tcp://127.0.0.1:1\"\n\
                [storage]\nrole_address = \"tcp://storage-0:7200\"\ndata_dir = \"/data/storage\"\n\
                [system]\nrole_address = \"tcp://storage-0:7100\"\n";
    w.start = Some(RoleStart::from_settings(
        text.as_bytes(),
        RoleIdentity::new("ci", "storage", 1),
    ));
}

/// The start every role binary makes on its settings refuses them with the error whose code the process exits with;
/// the storage binary's exit itself is bound over the built binary (group 4 of the change).
#[then("the process exits with code 78 and an error naming `system.data_dir`")]
fn exits_naming_the_directory(w: &mut Steps) -> TestResult {
    let refused = match w.start.take().context("a start")? {
        Ok(start) => return Err(TestError::new(format!("started as {start:?}"))),
        Err(error) => error,
    };
    assert_eq!(refused.exit_code(), 78);
    assert!(refused.to_string().contains("system.data_dir"), "{refused}");
    Ok(())
}

fn main() -> TestResult {
    futures::executor::block_on(
        Steps::cucumber()
            .fail_on_skipped()
            .run_and_exit(ultragraphdb_tool_bdd::features_dir()?),
    );
    Ok(())
}
