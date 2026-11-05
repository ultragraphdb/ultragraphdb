/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The wire protocol's scenarios of a role connection's handshake.

use std::cell::Cell;
use std::rc::Rc;

use compio::runtime::{Runtime, spawn};
use cucumber::{World, then, when};
use ultragraphdb_common_role::{
    Answer, ROLE_ITERATIONS, RoleClient, RoleSecret, Server, Session, listen, serve_connection,
};
use ultragraphdb_protocol_transport::{Endpoint, ReceivedFrame};
use ultragraphdb_protocol_wire::{ErrorCode, ErrorReply, MAX_FRAME_SIZE, ROLE_MAX_STREAMS};
use ultragraphdb_tool_bdd::{Context, TestResult};

const CLUSTER: u64 = 7;
const PASSWORD: &str = "s3cret";

/// What the connection's two sides announced.
#[derive(Debug, Default, World)]
struct Steps {
    welcome: Option<(u32, u32)>,
    hello: u32,
}

/// Holds what the peer's Hello stated, and serves nothing.
#[derive(Debug)]
struct Recording {
    hello: Rc<Cell<u32>>,
}

impl Session for Recording {
    async fn answer(&mut self, _stream: u64, _frame: ReceivedFrame) -> Answer {
        Answer::Refuse(ErrorReply::new(ErrorCode::Unavailable, "no"))
    }

    fn welcomed(&mut self, max_frame: u32) {
        self.hello.set(max_frame);
    }
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
        let listener = listen(&format!("tcp://127.0.0.1:{port}")).await?;
        let address = listener.local_address()?;
        let server =
            Server::new(CLUSTER, "test", Endpoint::Partition)?.with_iterations(ROLE_ITERATIONS);
        let hello = Rc::new(Cell::new(0));
        let session = Recording {
            hello: Rc::clone(&hello),
        };
        spawn(async move {
            if let Ok(carrier) = listener.accept().await {
                serve_connection(carrier, &server, held, session).await;
            }
        })
        .detach();
        let client = RoleClient::dial(&address, CLUSTER, "compute", PASSWORD).await?;
        let welcome = client.welcome();
        w.welcome = Some((welcome.max_frame(), welcome.max_streams()));
        w.hello = hello.get();
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

fn main() -> TestResult {
    futures::executor::block_on(
        Steps::cucumber()
            .fail_on_skipped()
            .run_and_exit(ultragraphdb_tool_bdd::features_dir()?),
    );
    Ok(())
}
