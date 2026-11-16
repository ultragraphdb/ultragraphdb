/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! What a `SystemSession` decides of a role connection over drawn inputs: laws L3.29 (a version read across frames), L3.33, L3.34
//! and L3.35 of `notes/laws.md`.

use std::future::Future;
use std::num::NonZeroU32;
use std::pin::pin;
use std::time::Duration;

use compio::runtime::{Runtime, spawn};
use futures::future::poll_immediate;
use hegel::TestCase;
use hegel::generators as gs;
use ultragraphdb_cluster_metadata::MetadataConfig;
use ultragraphdb_cluster_sequencer::SequencerConfig;
use ultragraphdb_common_role::RoleClient;
use ultragraphdb_common_system::{SystemHandle, SystemStart, system_run};
use ultragraphdb_error_protocol::ProtocolError;
use ultragraphdb_protocol_scram::ScramVerifier;
use ultragraphdb_protocol_transport::Address;
use ultragraphdb_protocol_wire::{
    BodyView, Catalog, CatalogFrames, Challenge, Check, CheckKind, CloseRequest, Command, Credit,
    Declare, Declared, DeclaredPredicate, Defaults, EpochCloseView, ErrorCode, ErrorReplyView,
    FLAG_LAST, Join, Lease, MAX_FRAME_FLOOR_U32, MAX_FRAME_SIZE_U32, Members, Placement, Session,
    SessionCheck, Subscribe, Verify,
};
use ultragraphdb_transactions_log::SimStorage;

use crate::utils::{Clock, SimGroup, dialled, recovered, served, start_of};

/// An epoch's length past its longest at 1 to 5 ms.
const EPOCH_US: u64 = 10_000;

/// A served group on a runtime of its own: `body` runs with its handle, its address, its role password and its clock.
fn with_served<T>(
    start: Option<SystemStart>,
    body: impl AsyncFnOnce(&SystemHandle, &Address, &str, &Clock) -> Option<T>,
) -> Option<T> {
    let runtime = Runtime::new().ok()?;
    runtime.block_on(async {
        let (storage, clock) = (SimStorage::new(9), Clock::new());
        let group: SimGroup = recovered(&storage, &start?, &clock).await?;
        let (handle, run) = system_run(group, clock.reading());
        let run = spawn(run);
        let (address, password, accepting) = served(&handle).await?;
        let answer = body(&handle, &address, &password, &clock).await;
        drop(accepting);
        drop(handle);
        run.await.ok()?.ok()?;
        answer
    })
}

/// The most closes [`closing`] makes before it gives up: a bound on a hang, past what any law here waits for.
const CLOSES_MAX: u32 = 1_000;

/// What `asked` answers, the clock moved on by an epoch each time the group has answered all it can without a close, and
/// that close delivered before `asked` is polled again: the closes it waits on, made by the test and not by real time. The
/// group answers a `LatestClose` as it takes it, keeping one that comes during a sync for the next turn, so the second of
/// two, sent once the first is answered, is answered once a turn after every ask `asked` sent has ended. `None` when the
/// group stops, or past [`CLOSES_MAX`] closes.
async fn closing<T>(
    handle: &SystemHandle,
    clock: &Clock,
    asked: impl Future<Output = T>,
) -> Option<T> {
    let mut asked = pin!(asked);
    for _ in 0..CLOSES_MAX {
        if let Some(answer) = poll_immediate(asked.as_mut()).await {
            return Some(answer);
        }
        let _taken = handle.latest_close().await.ok()?;
        let after = handle.latest_close().await.ok()?;
        if let Some(answer) = poll_immediate(asked.as_mut()).await {
            return Some(answer);
        }
        clock.advance(EPOCH_US);
        let _closed = handle.next_close(after).await.ok()?;
    }
    None
}

/// The next frame on `client` within 5 s.
async fn next(client: &mut RoleClient) -> Option<ultragraphdb_protocol_transport::ReceivedFrame> {
    compio::time::timeout(Duration::from_secs(5), client.next_frame())
        .await
        .ok()?
        .ok()
}

/// Each frame `client` receives after a Members request it sends now, through that request's reply, handed to `each` and
/// dropped, since a frame holds the client's one buffer. A connection takes its requests in order, answering or refusing each
/// before it reads the next, so what its earlier requests were answered with at once comes first; `None` when the connection
/// ends, or 5 s pass, before the reply.
async fn through_members(
    client: &mut RoleClient,
    mut each: impl FnMut(&ultragraphdb_protocol_transport::ReceivedFrame),
) -> Option<()> {
    client.send_only(&Members::new()).await.ok()?;
    let barrier = client.last_stream();
    loop {
        let frame = next(client).await?;
        each(&frame);
        if frame.view().ok()?.header().stream() == barrier {
            return Some(());
        }
    }
}

/// L3.29: a version of a drawn size up to 1 MiB, read with Catalog at a drawn `max_frame` from the floor to 1 MiB, comes back
/// across as many frames as it takes, its bytes those committed. The cap itself is the unit and BDD tests' boundary.
#[hegel::test(test_cases = 12)]
fn test_a_version_reads_back_whole_at_any_max_frame(tc: TestCase) {
    let length = tc.draw(gs::integers::<usize>().min_value(1).max_value(1 << 20));
    let max_frame = tc.draw(
        gs::integers::<u32>()
            .min_value(MAX_FRAME_FLOOR_U32)
            .max_value(MAX_FRAME_SIZE_U32),
    );
    let read = with_served(start_of(8, 50), async |handle, address, password, clock| {
        let declared = vec![Declared::Predicate(DeclaredPredicate::new(format!(
            "http://ex.org/{}",
            "v".repeat(length)
        )))];
        let (version, _) = closing(handle, clock, handle.declare_version(0, 0, declared))
            .await?
            .ok()?;
        let committed = handle.catalog(0, version).await.ok()?.bytes().to_vec();
        let mut storage = dialled(address, password, "storage", max_frame).await?;
        storage
            .send_only(&Catalog::request(0, version))
            .await
            .ok()?;
        let stream = storage.last_stream();
        let mut frames = CatalogFrames::new();
        let mut sizes = Vec::new();
        for _ in 0..64 {
            let last = {
                let frame = next(&mut storage).await?;
                sizes.push(frame.bytes().len());
                let view = frame.view().ok()?;
                frames.push(&view).ok()?;
                view.header().flags() & FLAG_LAST != 0
            };
            if last {
                break;
            }
            storage.send_on(stream, &Credit::new(1)).await.ok()?;
        }
        let read = frames.view()?.bytes().to_vec();
        Some((read == committed, sizes))
    });
    assert!(read.is_some(), "read");
    if let Some((equal, sizes)) = read {
        assert!(equal, "the bytes read are those committed");
        assert!(
            sizes.iter().all(|size| *size <= max_frame as usize),
            "{sizes:?} within {max_frame}"
        );
    }
}

/// L3.34: a frontend's Subscribe of `latest` 1 after a drawn number of closes, of which the sequencer holds a drawn number,
/// begins with the latest durable close, or the close of epoch 1 when none exists, and goes on one epoch at a time.
#[hegel::test(test_cases = 15)]
fn test_a_subscription_at_the_latest_begins_there(tc: TestCase) {
    let closes = tc.draw(gs::integers::<u64>().min_value(0).max_value(16));
    let retained = tc.draw(gs::integers::<u32>().min_value(1).max_value(16));
    let start = NonZeroU32::new(retained).and_then(|retained| {
        let sequencer = SequencerConfig::new(1, 5)
            .ok()?
            .with_closes_retained(retained);
        Some(SystemStart::new(MetadataConfig::new(), sequencer, 1))
    });
    let seen = with_served(start, async |handle, address, password, clock| {
        let mut closed = 0;
        while closed < closes {
            clock.advance(EPOCH_US);
            closed = handle.next_close(closed).await.ok()?.epoch().get();
        }
        let mut frontend = dialled(address, password, "frontend", MAX_FRAME_SIZE_U32).await?;
        frontend.send_only(&Subscribe::at_latest()).await.ok()?;
        if closes == 0 {
            clock.advance(EPOCH_US);
        }
        let first = EpochCloseView::from_frame(&next(&mut frontend).await?.view().ok()?)
            .ok()?
            .epoch();
        clock.advance(EPOCH_US);
        let _latest = handle.latest_close().await.ok()?;
        let second = EpochCloseView::from_frame(&next(&mut frontend).await?.view().ok()?)
            .ok()?
            .epoch();
        Some((first, second))
    });
    let first = closes.max(1);
    assert_eq!(seen, Some((first, first + 1)));
}

/// The roles a connection authenticates as.
const ROLES: [&str; 3] = ["frontend", "compute", "storage"];

/// Whether `PROTOCOL.md` §11.12's table of the system group's role address has `role` send `command`.
fn table_has(role: &str, command: Command) -> bool {
    let frontend = role == "frontend";
    let compute = role == "compute";
    let storage = role == "storage";
    if command == Command::Join {
        compute || storage
    } else if [Command::Members, Command::Placement, Command::Catalog].contains(&command) {
        true
    } else if [Command::Subscribe, Command::CloseRequest].contains(&command) {
        frontend || storage
    } else if [Command::Lease, Command::DefaultEntry, Command::Report].contains(&command) {
        storage
    } else {
        frontend
            && [
                Command::Challenge,
                Command::Verify,
                Command::Session,
                Command::Check,
                Command::Ended,
                Command::Declare,
                Command::Defaults,
            ]
            .contains(&command)
    }
}

/// One frame of `command` with a body its rules take, sent on `client`.
async fn send(client: &mut RoleClient, command: Command) -> Option<()> {
    let session = 7_u128.to_be_bytes();
    let sent = if command == Command::Join {
        client.send_only(&Join::compute("tcp://127.0.0.1:9")).await
    } else if command == Command::Members {
        client.send_only(&Members::new()).await
    } else if command == Command::Placement {
        client.send_only(&Placement::request(0)).await
    } else if command == Command::Catalog {
        client.send_only(&Catalog::request(0, 0)).await
    } else if command == Command::Subscribe {
        client.send_only(&Subscribe::new(0)).await
    } else if command == Command::CloseRequest {
        client.send_only(&CloseRequest::new(1, 1)).await
    } else if command == Command::Lease {
        client.send_only(&Lease::request(1, 1, 8)).await
    } else if command == Command::Challenge {
        client.send_only(&Challenge::request("alice")).await
    } else if command == Command::Verify {
        client
            .send_only(&Verify::request("alice", "n=alice,r=a", [0; 32]))
            .await
    } else if command == Command::Session {
        client.send_only(&Session::new(session, 1)).await
    } else if command == Command::Check {
        client
            .send_only(&Check::new(vec![SessionCheck::new(
                session,
                1,
                CheckKind::Asks,
            )]))
            .await
    } else if command == Command::Declare {
        let declared = vec![Declared::Predicate(DeclaredPredicate::new(
            "http://ex.org/p",
        ))];
        client
            .send_only(&Declare::declarations(9, 0, 1, declared))
            .await
    } else if command == Command::Defaults {
        client.send_only(&Defaults::request(0)).await
    } else {
        return None;
    };
    sent.ok()
}

/// L3.35: a command the endpoint table does not have the connection's role send is refused as `CommandNotServed` naming it, Error
/// 1 on stream 0 and the connection closed; one it has is not.
#[hegel::test(test_cases = 40)]
fn test_the_endpoint_table_by_role(tc: TestCase) {
    let role = tc.draw(gs::sampled_from(ROLES.to_vec()));
    let command = tc.draw(gs::sampled_from(vec![
        Command::Join,
        Command::Members,
        Command::Placement,
        Command::Catalog,
        Command::Subscribe,
        Command::CloseRequest,
        Command::Lease,
        Command::Challenge,
        Command::Verify,
        Command::Session,
        Command::Check,
        Command::Declare,
        Command::Defaults,
    ]));
    let refused = with_served(start_of(8, 50), async |_, address, password, _| {
        let mut client = dialled(address, password, role, MAX_FRAME_SIZE_U32).await?;
        send(&mut client, command).await?;
        let rule = ProtocolError::command_not_served(command.number()).to_string();
        let not_served = |frame: &ultragraphdb_protocol_transport::ReceivedFrame| {
            frame.view().is_ok_and(|view| {
                view.command() == Command::Error
                    && view.header().stream() == 0
                    && ErrorReplyView::from_frame(&view).is_ok_and(|error| {
                        error.code() == ErrorCode::Protocol && error.message().contains(&rule)
                    })
            })
        };
        if table_has(role, command) {
            // Served: a Members sent after it is answered once it was, and a refusal of it would come first.
            let mut refused = false;
            through_members(&mut client, |frame| refused |= not_served(frame)).await?;
            Some(refused)
        } else {
            // Refused: the Error is the first frame, and the connection closes; nothing more is sent to it.
            Some(not_served(&next(&mut client).await?))
        }
    });
    assert_eq!(
        refused,
        Some(!table_has(role, command)),
        "{role} sends {command:?}"
    );
}

/// L3.33: over a drawn sequence of the commands the frontend sends the group, no reply frame holds a run of 32 bytes equal to a
/// principal's stored key or server key.
#[hegel::test(test_cases = 10)]
fn test_no_reply_holds_a_key(tc: TestCase) {
    let commands: Vec<Command> = (0..tc.draw(gs::integers::<usize>().min_value(1).max_value(8)))
        .map(|_| {
            tc.draw(gs::sampled_from(vec![
                Command::Challenge,
                Command::Verify,
                Command::Members,
                Command::Placement,
                Command::Catalog,
                Command::Defaults,
                Command::Check,
                Command::Session,
            ]))
        })
        .collect();
    let replies = with_served(start_of(8, 50), async |_, address, password, _| {
        let mut frontend = dialled(address, password, "frontend", MAX_FRAME_SIZE_U32).await?;
        let mut replies = Vec::new();
        for command in &commands {
            send(&mut frontend, *command).await?;
            through_members(&mut frontend, |frame| replies.push(frame.bytes().to_vec())).await?;
        }
        Some(replies)
    });
    assert!(replies.is_some(), "answered");
    let held = ScramVerifier::derive("pw", b"a salt of alice", 4_096).ok();
    assert!(held.is_some());
    if let (Some(replies), Some(held)) = (replies, held) {
        for reply in &replies {
            for key in [held.stored_key(), held.server_key()] {
                assert!(!reply.windows(32).any(|run| run == key), "a key in a reply");
            }
        }
    }
}
