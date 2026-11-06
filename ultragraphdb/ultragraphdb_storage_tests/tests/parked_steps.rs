/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! A role connection past a parked request (`PROTOCOL.md` §11.12): a real storage process, the frontend's side played on the
//! partition's role address, a home played where its answer must wait.

use std::time::Duration;

use cucumber::{then, when};
use ultragraphdb_cluster_placement::{NodeId, PlacementMap};
use ultragraphdb_common_role::RoleClient;
use ultragraphdb_core_ids::Epoch;
use ultragraphdb_protocol_transport::ReceivedFrame;
use ultragraphdb_protocol_wire::{
    Bind, BindKind, BindView, BodyView, Cancel, Closed, Command, Commit, DirectWrite, ErrorCode,
    ErrorReplyView, TermTable, Write, WriteChange,
};
use ultragraphdb_store_quads::iri_bucket;
use ultragraphdb_tool_bdd::{Context, TestError, TestResult};

use crate::played_home::{HomeScript, play_home};
use crate::read_kernel_steps::{PARTITION, PRINCIPAL, dial_at, id_of, run, term_scan};
use crate::{PREDICATE, Steps, TENANT, edge_write};

/// How long a frame that is due may take, past which a scenario fails naming what it waited for.
const DUE: Duration = Duration::from_secs(5);

/// How long a frame that must not come is watched for.
const SILENCE: Duration = Duration::from_millis(50);

/// What a storage process's first partition listens at.
const FIRST_ADDRESS: &str = "tcp://127.0.0.1:17200";

/// What the partition's other process listens at.
const SECOND_ADDRESS: &str = "tcp://127.0.0.1:17300";

/// The bound of the scenarios stopping a connection: a frame always reaches it.
const STOPPING_BOUND: &str = "1";

/// How a transaction's scan is told it no longer exists: section 13.13's `detail` 1.
const NOT_HELD: u64 = 1;

/// What a scenario saw, in order, for its Then.
#[derive(Debug, Default)]
pub(crate) struct ParkedSteps {
    /// The commands answering, in the order they came.
    commands: Vec<Command>,
    /// A refusal's code and `detail`.
    refusals: Vec<(ErrorCode, u64)>,
    /// Whether anything came before the home answered.
    early: Option<Command>,
    /// Whether a connection ended with its stop.
    closed: bool,
}

/// An IRI partition `partition` of 4 owns, `at` choosing among them.
fn owned_by(partition: u32, at: u32) -> TestResult<String> {
    let map = PlacementMap::even(4, NodeId::new(1))?;
    (0..100_000)
        .map(|n| format!("http://ex.org/parked{at}-{n}"))
        .find(|iri| {
            iri_bucket(TENANT, iri)
                .is_some_and(|bucket| map.partition_of(bucket, Epoch::ZERO).get() == partition)
        })
        .context("an IRI of the partition")
}

/// The next frame of `client`, or a failure naming `what` it waited for.
async fn due(client: &mut RoleClient, what: &str) -> TestResult<ReceivedFrame> {
    let frame = compio::time::timeout(DUE, client.next_frame())
        .await
        .map_err(|_| TestError::new(format!("no frame within {DUE:?}: {what}")))?;
    Ok(frame?)
}

/// Notes the command of the next frame of `client`, which holds `stream`, and a refusal's code and `detail`.
async fn answered_on(
    client: &mut RoleClient,
    stream: u64,
    what: &str,
    seen: &mut ParkedSteps,
) -> TestResult {
    let frame = due(client, what).await?;
    let view = frame.view()?;
    assert_eq!(view.header().stream(), stream, "{what}");
    let command = view.command();
    if command == Command::Error {
        let refusal = ErrorReplyView::from_frame(&view)?;
        seen.refusals.push((refusal.code(), refusal.detail()));
    }
    seen.commands.push(command);
    Ok(())
}

/// A transaction bound to `partition`: its stream and read version.
async fn bound_to(client: &mut RoleClient, partition: u32, identity: u8) -> TestResult<(u64, u64)> {
    let bind = Bind::new(
        partition,
        BindKind::Transaction,
        TENANT,
        0,
        PRINCIPAL,
        [identity; 16],
    );
    let reply = client.request(&bind).await?;
    let read = BindView::from_frame(&reply.view()?)?.read_version();
    Ok((client.last_stream(), read))
}

/// A Write on partition 1's subject whose object partition 3 owns, sent on `stream`, and waited for until it reached the home.
async fn parked_write(client: &mut RoleClient, stream: u64, script: &HomeScript) -> TestResult {
    let (subject, object) = (owned_by(PARTITION, 1)?, owned_by(3, 1)?);
    client
        .send_on(stream, &edge_write(&subject, &object))
        .await?;
    script.asked().await
}

/// Asks the partition for its closes on a stream of its own: the stream it took.
async fn second_request(client: &mut RoleClient) -> TestResult<u64> {
    client.send_only(&Closed::request(&[PARTITION])).await?;
    Ok(client.last_stream())
}

#[when(
    "a Write on a Bind stream waits on a home's answer, and a second request arrives on its connection"
)]
fn second_beside_write(w: &mut Steps) -> TestResult {
    crate::one_process_of_two(w)?;
    let script = HomeScript::holding();
    let seen = &mut w.parked;
    run(async {
        play_home(&script).await?;
        let mut client = dial_at(FIRST_ADDRESS).await?;
        let (stream, _) = bound_to(&mut client, PARTITION, 71).await?;
        parked_write(&mut client, stream, &script).await?;
        let second = second_request(&mut client).await?;
        answered_on(
            &mut client,
            second,
            "a second request while a Write waits on its home",
            seen,
        )
        .await?;
        script.release();
        answered_on(
            &mut client,
            stream,
            "the Write's Held once its home answered",
            seen,
        )
        .await?;
        Ok(())
    })
}

#[then(
    "the second request is answered while the Write waits, and the Write is answered Held on its stream once the home answers"
)]
fn second_then_held(w: &mut Steps) -> TestResult {
    assert_eq!(w.parked.commands, [Command::Closed, Command::Held]);
    w.roles = None;
    Ok(())
}

#[when("a DirectWrite waits on a home's answer, and a second request arrives on its connection")]
fn second_beside_direct(w: &mut Steps) -> TestResult {
    crate::one_process_of_two(w)?;
    let script = HomeScript::holding();
    let seen = &mut w.parked;
    run(async {
        play_home(&script).await?;
        let mut client = dial_at(FIRST_ADDRESS).await?;
        let (subject, object) = (owned_by(PARTITION, 2)?, owned_by(3, 2)?);
        let mut terms = TermTable::new();
        let change = WriteChange::Insert {
            graph: None,
            subject: terms.iri(&subject),
            predicate: terms.iri(PREDICATE),
            object: terms.iri(&object),
        };
        let direct = DirectWrite::new(TENANT, terms, vec![change]).with_principal(PRINCIPAL);
        client.send_only(&direct).await?;
        let stream = client.last_stream();
        script.asked().await?;
        let second = second_request(&mut client).await?;
        answered_on(
            &mut client,
            second,
            "a second request while a DirectWrite waits on its home",
            seen,
        )
        .await?;
        script.release();
        answered_on(
            &mut client,
            stream,
            "the DirectWrite's Commit once its home answered",
            seen,
        )
        .await?;
        Ok(())
    })
}

#[then(
    "the second request is answered while the DirectWrite waits, and its Commit reply follows once the home answers"
)]
fn second_then_commit(w: &mut Steps) -> TestResult {
    assert_eq!(w.parked.commands, [Command::Closed, Command::Commit]);
    w.roles = None;
    Ok(())
}

#[when(
    "a Scan of a transaction waits on a home's answer, a Write follows on the transaction's stream, and a second request arrives on the connection"
)]
fn write_behind_a_read(w: &mut Steps) -> TestResult {
    crate::one_process_of_two(w)?;
    let script = HomeScript::holding();
    let seen = &mut w.parked;
    run(async {
        play_home(&script).await?;
        let mut client = dial_at(FIRST_ADDRESS).await?;
        let (stream, read) = bound_to(&mut client, PARTITION, 70).await?;
        client
            .send_only(&term_scan(70, read, &owned_by(3, 7)?))
            .await?;
        script.asked().await?;
        let write = edge_write(&owned_by(PARTITION, 7)?, &owned_by(3, 8)?);
        client.send_on(stream, &write).await?;
        let second = second_request(&mut client).await?;
        answered_on(
            &mut client,
            second,
            "a second request while a Write waits behind a read",
            seen,
        )
        .await?;
        script.release();
        // The read's Entries and the Write's Held answer in either order.
        for _ in 0..2 {
            let frame = due(&mut client, "the read's Entries, or the Write's Held").await?;
            seen.commands.push(frame.view()?.command());
        }
        Ok(())
    })
}

#[then(
    "the second request is answered while the Write waits its turn, and the Write is answered Held once the home answers"
)]
fn second_then_held_behind_a_read(w: &mut Steps) -> TestResult {
    assert_eq!(w.parked.commands.first(), Some(&Command::Closed));
    assert!(
        w.parked.commands.contains(&Command::Held),
        "{:?}",
        w.parked.commands
    );
    w.roles = None;
    Ok(())
}

/// Takes what may come within [`SILENCE`] as what `seen` notes as early.
async fn watch_silence(client: &mut RoleClient, seen: &mut ParkedSteps) -> TestResult {
    if let Ok(frame) = compio::time::timeout(SILENCE, client.next_frame()).await {
        seen.early = Some(frame?.view()?.command());
    }
    Ok(())
}

#[when("a Write waits on a home's answer, and a second Write and the Commit follow on its stream")]
fn behind_on_stream(w: &mut Steps) -> TestResult {
    crate::one_process_of_two(w)?;
    let script = HomeScript::holding();
    let seen = &mut w.parked;
    run(async {
        play_home(&script).await?;
        let mut client = dial_at(FIRST_ADDRESS).await?;
        let (stream, _) = bound_to(&mut client, PARTITION, 72).await?;
        parked_write(&mut client, stream, &script).await?;
        let (subject, object) = (owned_by(PARTITION, 3)?, owned_by(3, 3)?);
        client
            .send_on(stream, &edge_write(&subject, &object))
            .await?;
        client.send_on(stream, &Commit::new([0; 16], 0, 0)).await?;
        watch_silence(&mut client, seen).await?;
        script.release();
        for what in ["the first Held", "the second Held", "the Commit"] {
            answered_on(&mut client, stream, what, seen).await?;
        }
        let _first = id_of(&mut client, &owned_by(PARTITION, 1)?).await?;
        let _second = id_of(&mut client, &subject).await?;
        Ok(())
    })
}

#[then(
    "nothing is answered on the stream until the home answers, and then Held, Held and the Commit, in that order, and the commit holds both writes"
)]
fn held_held_commit(w: &mut Steps) -> TestResult {
    assert_eq!(w.parked.early, None);
    assert_eq!(
        w.parked.commands,
        [Command::Held, Command::Held, Command::Commit]
    );
    w.roles = None;
    Ok(())
}

#[when(
    "a Write waits on a home's answer, a second Write follows on its stream, and the home refuses the registration"
)]
fn refused_with_one_behind(w: &mut Steps) -> TestResult {
    crate::one_process_of_two(w)?;
    let refused = ErrorCode::Refused.number();
    let script = HomeScript::holding();
    script.refuse_with(refused);
    let seen = &mut w.parked;
    run(async {
        play_home(&script).await?;
        let mut client = dial_at(FIRST_ADDRESS).await?;
        let (stream, read) = bound_to(&mut client, PARTITION, 73).await?;
        parked_write(&mut client, stream, &script).await?;
        let (subject, object) = (owned_by(PARTITION, 4)?, owned_by(3, 4)?);
        client
            .send_on(stream, &edge_write(&subject, &object))
            .await?;
        // The second Write is read before the home answers: a request answered behind it shows it was taken.
        let second = second_request(&mut client).await?;
        answered_on(
            &mut client,
            second,
            "a request beside the parked write",
            seen,
        )
        .await?;
        script.release();
        answered_on(
            &mut client,
            stream,
            "the home's refusal on the transaction's stream",
            seen,
        )
        .await?;
        let scan = term_scan(73, read, &subject);
        client.send_only(&scan).await?;
        let scan_stream = client.last_stream();
        answered_on(
            &mut client,
            scan_stream,
            "a Scan of the transaction that ended",
            seen,
        )
        .await?;
        Ok(())
    })
}

#[then("the stream ends with the home's refusal alone, and the transaction holds nothing")]
fn refusal_alone(w: &mut Steps) -> TestResult {
    assert_eq!(
        w.parked.commands,
        [Command::Closed, Command::Error, Command::Error],
        "{:?}",
        w.parked.refusals
    );
    assert_eq!(
        w.parked.refusals,
        [(ErrorCode::Refused, 0), (ErrorCode::Protocol, NOT_HELD)]
    );
    w.roles = None;
    Ok(())
}

#[when("a Write waits on a home's answer, and its stream is cancelled")]
fn cancelled_while_parked(w: &mut Steps) -> TestResult {
    crate::one_process_of_two(w)?;
    let script = HomeScript::holding();
    let seen = &mut w.parked;
    run(async {
        play_home(&script).await?;
        let mut client = dial_at(FIRST_ADDRESS).await?;
        let (stream, read) = bound_to(&mut client, PARTITION, 74).await?;
        parked_write(&mut client, stream, &script).await?;
        client.send_on(stream, &Cancel).await?;
        let subject = owned_by(PARTITION, 1)?;
        client.send_only(&term_scan(74, read, &subject)).await?;
        let scan_stream = client.last_stream();
        answered_on(
            &mut client,
            scan_stream,
            "a Scan of the transaction cancelled while its Write waited",
            seen,
        )
        .await?;
        script.release();
        watch_silence(&mut client, seen).await
    })
}

#[then(
    "the partition holds no such transaction by the time a later request on the connection is answered, and no Held follows once the home answers"
)]
fn no_such_transaction(w: &mut Steps) -> TestResult {
    assert_eq!(w.parked.refusals, [(ErrorCode::Protocol, NOT_HELD)]);
    assert_eq!(w.parked.early, None, "nothing follows the Cancel");
    w.roles = None;
    Ok(())
}

#[when("a Write waits on a home's answer, and its connection closes")]
fn closed_while_parked(w: &mut Steps) -> TestResult {
    crate::one_process_of_two(w)?;
    let script = HomeScript::holding();
    let seen = &mut w.parked;
    run(async {
        play_home(&script).await?;
        let mut client = dial_at(FIRST_ADDRESS).await?;
        let (stream, read) = bound_to(&mut client, PARTITION, 75).await?;
        parked_write(&mut client, stream, &script).await?;
        drop(client);
        let subject = owned_by(PARTITION, 1)?;
        let mut compute = dial_at(FIRST_ADDRESS).await?;
        // The connection's close reaches the core in its own time; until then a Scan is refused as behind the Write.
        for _ in 0..500 {
            compute.send_only(&term_scan(75, read, &subject)).await?;
            let scan_stream = compute.last_stream();
            answered_on(
                &mut compute,
                scan_stream,
                "a Scan of the transaction whose connection closed",
                seen,
            )
            .await?;
            if seen.refusals.last() == Some(&(ErrorCode::Protocol, NOT_HELD)) {
                return Ok(());
            }
            compio::time::sleep(Duration::from_millis(2)).await;
        }
        Err(TestError::new(
            "the partition still holds a transaction whose connection closed",
        ))
    })
}

#[then("the partition no longer holds the transaction")]
fn no_longer_held(w: &mut Steps) -> TestResult {
    assert_eq!(
        w.parked.refusals.last(),
        Some(&(ErrorCode::Protocol, NOT_HELD))
    );
    w.roles = None;
    Ok(())
}

#[when(
    "a Write's frame takes its connection's parked bytes to `storage.held_bytes`, the Write waiting on a home's answer, and a second request follows on the connection"
)]
fn second_past_the_bound(w: &mut Steps) -> TestResult {
    crate::processes_holding(w, &[1], &[("storage.held_bytes", STOPPING_BOUND)])?;
    let script = HomeScript::holding();
    let seen = &mut w.parked;
    run(async {
        play_home(&script).await?;
        let mut client = dial_at(FIRST_ADDRESS).await?;
        let (stream, _) = bound_to(&mut client, PARTITION, 76).await?;
        parked_write(&mut client, stream, &script).await?;
        let second = second_request(&mut client).await?;
        watch_silence(&mut client, seen).await?;
        script.release();
        // The two streams answer in either order, whichever the connection takes first as it reads on.
        for _ in [stream, second] {
            let frame = due(&mut client, "the Write's Held, or the second answer").await?;
            seen.commands.push(frame.view()?.command());
        }
        Ok(())
    })
}

#[then(
    "the second request is unanswered until the home answers and is then answered, and nothing is refused"
)]
fn answered_past_the_bound(w: &mut Steps) -> TestResult {
    assert_eq!(w.parked.early, None);
    assert_eq!(w.parked.commands.len(), 2, "{:?}", w.parked.commands);
    assert!(w.parked.commands.contains(&Command::Held));
    assert!(w.parked.commands.contains(&Command::Closed));
    w.roles = None;
    Ok(())
}

/// A Write of `subject`'s edge to `object` on `client`'s partition, `identity` its transaction: its stream.
async fn write_on(
    client: &mut RoleClient,
    partition: u32,
    identity: u8,
    write: &Write,
) -> TestResult<u64> {
    let (stream, _) = bound_to(client, partition, identity).await?;
    client.send_on(stream, write).await?;
    Ok(stream)
}

#[when(
    "both storage processes hold `storage.held_bytes` of 1, and a Write on partition 1 names an entity partition 3 owns while a Write on partition 3 names an entity partition 1 owns"
)]
fn crossing_homes(w: &mut Steps) -> TestResult {
    crate::processes_holding(w, &[1, 3], &[("storage.held_bytes", STOPPING_BOUND)])?;
    let seen = &mut w.parked;
    run(async {
        let mut first = dial_at(FIRST_ADDRESS).await?;
        let mut second = dial_at(SECOND_ADDRESS).await?;
        let from_first = edge_write(&owned_by(1, 5)?, &owned_by(3, 6)?);
        let from_second = edge_write(&owned_by(3, 5)?, &owned_by(1, 6)?);
        let at_first = write_on(&mut first, 1, 77, &from_first).await?;
        let at_second = write_on(&mut second, 3, 78, &from_second).await?;
        answered_on(&mut first, at_first, "partition 1's Held", seen).await?;
        answered_on(&mut second, at_second, "partition 3's Held", seen).await?;
        let commit = Commit::new([0; 16], 0, 0);
        first.send_on(at_first, &commit).await?;
        second.send_on(at_second, &commit).await?;
        answered_on(&mut first, at_first, "partition 1's Commit", seen).await?;
        answered_on(&mut second, at_second, "partition 3's Commit", seen).await?;
        Ok(())
    })
}

#[then("both are answered Held and both transactions commit")]
fn both_commit(w: &mut Steps) -> TestResult {
    assert_eq!(
        w.parked.commands,
        [
            Command::Held,
            Command::Held,
            Command::Commit,
            Command::Commit
        ],
        "{:?}",
        w.parked.refusals
    );
    w.roles = None;
    Ok(())
}

#[when("a storage process stops while a Write on a Bind stream waits on a home's answer")]
fn stopped_while_parked(w: &mut Steps) -> TestResult {
    crate::one_process_of_two(w)?;
    let script = HomeScript::holding();
    let pid = w.pid;
    let util = w.roles.as_ref().context("the roles")?;
    let seen = &mut w.parked;
    run(async {
        play_home(&script).await?;
        let mut client = dial_at(FIRST_ADDRESS).await?;
        let (stream, _) = bound_to(&mut client, PARTITION, 79).await?;
        parked_write(&mut client, stream, &script).await?;
        let _stopped = util.stop_service(pid)?;
        match compio::time::timeout(DUE, client.next_frame()).await {
            Ok(Ok(frame)) => {
                let view = frame.view()?;
                assert_eq!(view.header().stream(), stream);
                let refusal = ErrorReplyView::from_frame(&view)?;
                seen.refusals.push((refusal.code(), refusal.detail()));
                Ok(())
            }
            Ok(Err(_)) => {
                seen.closed = true;
                Ok(())
            }
            Err(_) => Err(TestError::new(format!(
                "neither an answer nor a close within {DUE:?} of a stop"
            ))),
        }
    })
}

#[then("the stream is answered with code 17 or the connection closes")]
fn answered_or_closed(w: &mut Steps) -> TestResult {
    assert!(
        w.parked.closed || w.parked.refusals == [(ErrorCode::Unavailable, 0)],
        "{:?}",
        w.parked
    );
    w.roles = None;
    Ok(())
}
