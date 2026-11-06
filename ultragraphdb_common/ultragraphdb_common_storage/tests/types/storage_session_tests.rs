/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::time::Duration;

use compio::runtime::{Runtime, spawn};
use futures::StreamExt;
use futures::channel::mpsc::{UnboundedReceiver, unbounded};
use ultragraphdb_common_role::{
    Answer, ROLE_ITERATIONS, RoleClient, RoleSecret, Server, listen, serve_connection,
};
use ultragraphdb_common_storage::{CoreMessage, StorageSession};
use ultragraphdb_protocol_transport::Endpoint;
use ultragraphdb_protocol_wire::{
    BodyBuilder, BodyView, Closed, Command, ErrorCode, ErrorReply, ErrorReplyView, FLAG_LAST,
    Header,
};
use ultragraphdb_tool_bdd::{Context, TestResult};

const CLUSTER: u64 = 7;
const PASSWORD: &str = "s3cret";
const DUE: Duration = Duration::from_secs(5);

/// Plays a core: each request's stream is ended by a pushed Error of `Refused`, and the request itself, which crossed that Error,
/// is refused with `Protocol`, as a real core refuses a request on a stream whose transaction has ended.
async fn core_ending_each_stream(mut inbox: UnboundedReceiver<CoreMessage>) {
    while let Some(message) = inbox.next().await {
        if let CoreMessage::Request {
            stream,
            done,
            pushes,
            ..
        } = message
        {
            let ended = ErrorReply::new(ErrorCode::Refused, "why the transaction ended");
            let header = Header::new(CLUSTER, Command::Error)
                .with_stream(stream)
                .with_flags(FLAG_LAST);
            if let Ok(frame) = ended.frame(header) {
                drop(pushes.unbounded_send((stream, frame)));
            }
            let crossed = ErrorReply::new(ErrorCode::Protocol, "a stream no Bind opened");
            drop(done.send(Answer::Refuse(crossed)));
        }
    }
}

/// A request crossing the pushed Error that ended its stream takes no refusal of its own: the peer reads why the stream ended.
#[test]
fn test_a_refusal_crossing_its_streams_end_gives_way_to_it() -> TestResult {
    Runtime::new()?.block_on(async {
        // A port of this host that was free a moment ago.
        let port = std::net::TcpListener::bind("127.0.0.1:0")?
            .local_addr()?
            .port();
        let listener = listen(&format!("tcp://127.0.0.1:{port}")).await?;
        let address = listener.local_address()?;
        let file = std::env::temp_dir().join(format!("session-secret-{}", std::process::id()));
        std::fs::write(&file, PASSWORD)?;
        let held = RoleSecret::read(&file.display().to_string())?.credentials()?;
        let server =
            Server::new(CLUSTER, "test", Endpoint::Partition)?.with_iterations(ROLE_ITERATIONS);
        let (core, inbox) = unbounded();
        spawn(core_ending_each_stream(inbox)).detach();
        spawn(async move {
            if let Ok(carrier) = listener.accept().await {
                let session = StorageSession::new(1, core, 1);
                serve_connection(carrier, &server, held, session).await;
            }
        })
        .detach();
        let mut client = RoleClient::dial(&address, CLUSTER, "compute", PASSWORD).await?;
        for _ in 0..2 {
            client.send_only(&Closed::request(&[1])).await?;
            let frame = compio::time::timeout(DUE, client.next_frame())
                .await
                .context("the Error ending the stream")??;
            let view = frame.view()?;
            assert_eq!(view.header().stream(), client.last_stream());
            assert_eq!(
                ErrorReplyView::from_frame(&view)?.code(),
                ErrorCode::Refused
            );
        }
        Ok(())
    })
}
