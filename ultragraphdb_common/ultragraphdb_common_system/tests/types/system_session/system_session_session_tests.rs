/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The endpoint table of `PROTOCOL.md` §11.12 by the connection's role, a Subscribe of `latest`, a CloseRequest's `partition`
//! by role, and a Catalog answered across frames: what a [`ultragraphdb_common_system::SystemSession`] decides of each
//! connection.

use compio::runtime::Runtime;
use ultragraphdb_cluster_metadata::MetadataConfig;
use ultragraphdb_cluster_sequencer::SequencerConfig;
use ultragraphdb_common_role::RoleClient;
use ultragraphdb_common_system::SystemStart;
use ultragraphdb_core_catalog::CatalogContents;
use ultragraphdb_core_mapping::{Declaration, PredicateDeclaration};
use ultragraphdb_error_protocol::ProtocolError;
use ultragraphdb_protocol_wire::{
    Bind, BindKind, BodyView, Catalog, CatalogFrames, CheckKind, CloseRequest, Command, Credit,
    Declared, DeclaredPredicate, Ended, EpochCloseView, ErrorCode, FLAG_LAST, Join, Lease,
    MAX_FRAME_FLOOR, MAX_FRAME_FLOOR_U32, Placement, Session, SessionCheck, Subscribe, Verify,
};
use ultragraphdb_tool_bdd::{Context, TestError, TestResult};
use ultragraphdb_transactions_log::SimStorage;

use super::{Served, error_in, group_with, next_frame, refusal_code, serving, watched};
use crate::types::system_group::{EPOCH_US, heard, principal, starting};

/// A group of one partition, epochs of 1 to 5 ms, `transaction_ms` 50, served at a role address.
async fn served(name: &str, seed: u64) -> TestResult<Served> {
    serving(name, &SimStorage::new(seed), |storage, clock| async move {
        let start = starting(MetadataConfig::new())?.with_transaction_ms(50);
        group_with(storage, clock, start).await
    })
    .await
}

/// The group's clock moved past `epoch_ms_min`, 1 ms, once what the connections sent has reached it, and the group asked, so it
/// takes up a close it was asked for.
async fn epoch_min_passed(group: &Served) -> TestResult {
    compio::time::sleep(std::time::Duration::from_millis(50)).await;
    group.clock.advance(1_000);
    let _latest = heard(group.handle.latest_close().await)?;
    Ok(())
}

/// A group whose epochs last up to 10 s, so only a request closes one early.
async fn served_slowly(name: &str, seed: u64) -> TestResult<Served> {
    serving(name, &SimStorage::new(seed), |storage, clock| async move {
        let start = SystemStart::new(MetadataConfig::new(), SequencerConfig::new(1, 10_000)?, 1);
        group_with(storage, clock, start).await
    })
    .await
}

/// `client`'s request refused as `CommandNotServed` naming `command`, as a connection error: Error 1 on stream 0, and the
/// connection closed.
async fn refused_not_served(client: &mut RoleClient, command: Command) -> TestResult {
    let frame = next_frame(client).await?;
    let (stream, code, message) = error_in(&frame)?;
    drop(frame);
    assert_eq!((stream, code), (0, ErrorCode::Protocol), "{message}");
    let rule = ProtocolError::command_not_served(command.number()).to_string();
    assert!(message.contains(&rule), "{message}");
    assert!(client.next_frame().await.is_err(), "the connection closed");
    Ok(())
}

/// One frame of `command` a connection of `role` sends, each body as its builder makes it.
async fn sent(client: &mut RoleClient, command: Command) -> TestResult {
    let session = 1u128.to_be_bytes();
    if command == Command::Verify {
        client
            .send_only(&Verify::request("ann", "n=ann,r=x", [0; 32]))
            .await?;
    } else if command == Command::Session {
        client.send_only(&Session::new(session, 1)).await?;
    } else if command == Command::Check {
        client
            .send_only(&ultragraphdb_protocol_wire::Check::new(vec![
                SessionCheck::new(session, 1, CheckKind::Asks),
            ]))
            .await?;
    } else if command == Command::Ended {
        client.send_only(&Ended::new(vec![session])).await?;
    } else if command == Command::Subscribe {
        client.send_only(&Subscribe::new(0)).await?;
    } else if command == Command::CloseRequest {
        client.send_only(&CloseRequest::new(1, 1)).await?;
    } else if command == Command::Lease {
        client.send_only(&Lease::request(1, 1, 8)).await?;
    } else if command == Command::Join {
        client
            .send_only(&Join::compute("tcp://127.0.0.1:9"))
            .await?;
    } else {
        return Err(TestError::new(format!("no body for {command:?}")));
    }
    Ok(())
}

/// L3.35, and the scenario "A command another role requests is refused": a command the endpoint table does not have the
/// connection's role send, the role being its SCRAM username, is refused as `CommandNotServed` naming it, closing the
/// connection.
#[test]
fn test_a_command_another_role_requests_is_refused() -> TestResult {
    Runtime::new()?.block_on(async {
        let group = served("roles", 501).await?;
        let refusals = [
            ("storage", Command::Verify),
            ("storage", Command::Session),
            ("storage", Command::Check),
            ("storage", Command::Ended),
            ("compute", Command::Subscribe),
            ("compute", Command::CloseRequest),
            ("compute", Command::Lease),
            ("frontend", Command::Join),
            ("frontend", Command::Lease),
        ];
        for (role, command) in refusals {
            let mut client = group.dial(role).await?;
            sent(&mut client, command).await?;
            refused_not_served(&mut client, command).await?;
        }
        group.stop().await?;
        Ok(())
    })
}

/// The scenario "A command the endpoint does not serve is refused": a Bind at the system group's role address is refused as
/// `CommandNotServed` naming Bind.
#[test]
fn test_a_command_the_endpoint_does_not_serve_is_refused() -> TestResult {
    Runtime::new()?.block_on(async {
        let group = served("bind", 502).await?;
        let mut frontend = group.dial("frontend").await?;
        frontend
            .send_only(&Bind::new(1, BindKind::Transaction, 0, 0, 0, [1; 16]))
            .await?;
        refused_not_served(&mut frontend, Command::Bind).await?;
        group.stop().await?;
        Ok(())
    })
}

/// L3.35, and the scenario "A storage process may not ask for the latest": a Subscribe of `latest` 1 on a storage connection is
/// refused with `FieldValue` naming `latest`, on its stream, and the connection stays open.
#[test]
fn test_a_storage_process_may_not_ask_for_the_latest() -> TestResult {
    Runtime::new()?.block_on(async {
        let group = served("latest-storage", 503).await?;
        let mut storage = group.dial("storage").await?;
        let refused = storage
            .request(&Subscribe::at_latest())
            .await
            .err()
            .context("a refusal")?;
        assert_eq!(
            refusal_code(&refused),
            Some(ErrorCode::Protocol),
            "{refused}"
        );
        let rule = ProtocolError::field_value(Command::Subscribe.number(), "latest", 1).to_string();
        assert!(refused.to_string().contains(&rule), "{refused}");
        let placed = storage
            .request(&Placement::request(0))
            .await?
            .view()?
            .command();
        assert_eq!(placed, Command::Placement, "the connection stays open");
        drop(storage);
        group.stop().await?;
        Ok(())
    })
}

/// L3.34, and the scenario "A late subscriber is not refused for trimmed closes": a frontend's Subscribe of `latest` 1 begins with
/// the latest durable close, whatever its epoch, and the close one above it follows.
#[test]
fn test_a_subscriber_holding_no_close_begins_at_the_latest() -> TestResult {
    Runtime::new()?.block_on(async {
        let group = served("latest", 504).await?;
        for _ in 0..3 {
            let after = heard(group.handle.latest_close().await)?;
            let _closed =
                crate::types::system_group::closed_after(&group.handle, &group.clock, after)
                    .await?;
        }
        let latest = heard(group.handle.latest_close().await)?;
        assert!(latest >= 3, "{latest}");
        let mut frontend = group.dial("frontend").await?;
        frontend.send_only(&Subscribe::at_latest()).await?;
        let first = {
            let frame = next_frame(&mut frontend).await?;
            EpochCloseView::from_frame(&frame.view()?)?.epoch()
        };
        assert_eq!(first, latest);
        group.clock.advance(EPOCH_US);
        let second = {
            let frame = next_frame(&mut frontend).await?;
            EpochCloseView::from_frame(&frame.view()?)?.epoch()
        };
        assert_eq!(second, latest + 1);
        drop(frontend);
        group.stop().await?;
        Ok(())
    })
}

/// L3.34: before any close, a Subscribe of `latest` 1 begins with the close of epoch 1.
#[test]
fn test_a_subscriber_holding_no_close_before_any_begins_at_epoch_one() -> TestResult {
    Runtime::new()?.block_on(async {
        let group = served_slowly("latest-first", 505).await?;
        assert_eq!(heard(group.handle.latest_close().await)?, 0, "no close yet");
        let mut frontend = group.dial("frontend").await?;
        frontend.send_only(&Subscribe::at_latest()).await?;
        frontend.send_only(&CloseRequest::frontend(1)).await?;
        epoch_min_passed(&group).await?;
        let first = {
            let frame = next_frame(&mut frontend).await?;
            EpochCloseView::from_frame(&frame.view()?)?.epoch()
        };
        assert_eq!(first, 1);
        drop(frontend);
        group.stop().await?;
        Ok(())
    })
}

/// C3.73, C3.74, L3.35, and the scenarios "The frontend's request closes the open epoch" and "A request for a closed epoch closes
/// nothing": a frontend's CloseRequest of `partition` 0 naming the open epoch closes it early; one naming a closed epoch, one
/// above the open one, one of `partition` 0 from storage and one of `partition` 3 from the frontend are dropped, and no frame
/// answers any of them.
#[test]
fn test_a_close_request_is_taken_by_its_senders_role() -> TestResult {
    Runtime::new()?.block_on(async {
        let group = served_slowly("close-request", 506).await?;
        let mut frontend = group.dial("frontend").await?;
        let mut storage = group.dial("storage").await?;
        frontend.send_only(&Subscribe::new(0)).await?;
        let subscribed = frontend.last_stream();
        storage.send_only(&CloseRequest::frontend(1)).await?;
        frontend.send_only(&CloseRequest::new(3, 1)).await?;
        frontend.send_only(&CloseRequest::frontend(2)).await?;
        assert_eq!(
            watched(&mut frontend).await?,
            None,
            "nothing closed, nothing answered"
        );
        assert_eq!(
            watched(&mut storage).await?,
            None,
            "the storage connection's request dropped unanswered"
        );
        frontend.send_only(&CloseRequest::frontend(1)).await?;
        epoch_min_passed(&group).await?;
        let closed = next_frame(&mut frontend).await?;
        let view = closed.view()?;
        assert_eq!(
            (
                view.header().stream(),
                EpochCloseView::from_frame(&view)?.epoch()
            ),
            (subscribed, 1)
        );
        drop(closed);
        frontend.send_only(&CloseRequest::frontend(1)).await?;
        assert_eq!(
            watched(&mut frontend).await?,
            None,
            "a closed epoch closes nothing"
        );
        drop((frontend, storage));
        group.stop().await?;
        Ok(())
    })
}

/// The length of the encoding of a catalog version of `tenant` numbered `version` holding one predicate of an IRI of `length`
/// bytes, by `ultragraphdb_core_catalog`'s encoder, which the session carries as opaque bytes.
fn version_length(tenant: u64, version: u64, length: usize) -> TestResult<usize> {
    let iri = oxrdf::NamedNode::new(iri_of(length))?;
    let contents = CatalogContents::new(
        tenant,
        version,
        vec![Declaration::Predicate(PredicateDeclaration::new(iri))],
    );
    Ok(contents.encode()?.len())
}

/// An IRI of `length` bytes.
fn iri_of(length: usize) -> String {
    let stem = "http://ex.org/";
    format!("{stem}{}", "a".repeat(length.saturating_sub(stem.len())))
}

/// The IRI length whose version holds exactly `bytes`, the encoding growing by one byte a byte of IRI past the stem.
fn iri_length_for(bytes: usize) -> TestResult<usize> {
    let base = version_length(0, 1, 100)?;
    let grown = version_length(0, 1, 101)?;
    assert_eq!(
        grown,
        base + 1,
        "the encoding grows by one byte a byte of IRI here"
    );
    Ok(bytes - base + 100)
}

/// A Catalog answer read across its frames on `client`, granting credit as each arrives: the assembled answer and the frames it
/// took.
pub(crate) async fn catalog_read(
    client: &mut RoleClient,
    request: &Catalog,
) -> TestResult<(CatalogFrames, usize)> {
    client.send_only(request).await?;
    let stream = client.last_stream();
    let mut frames = CatalogFrames::new();
    for taken in 1..=64 {
        let last = {
            let frame = next_frame(client).await?;
            let view = frame.view()?;
            assert_eq!(view.header().stream(), stream);
            frames.push(&view)?;
            view.header().flags() & FLAG_LAST != 0
        };
        if last {
            return Ok((frames, taken));
        }
        client.send_on(stream, &Credit::new(1)).await?;
    }
    Err(TestError::new("the answer took more than 64 frames"))
}

/// C3.64, and the scenario "A catalog version past one frame travels in several": a version of 523,001 bytes read at the floor of
/// `max_frame` comes in two Catalog frames, the second alone flagged `LAST`, its bytes those committed.
#[test]
fn test_a_catalog_version_past_one_frame_travels_in_several() -> TestResult {
    Runtime::new()?.block_on(async {
        let group = served("catalog-frames", 507).await?;
        let length = iri_length_for(523_001)?;
        let ticking = crate::types::system_group::ticking(&group.clock);
        let declared = Declared::Predicate(DeclaredPredicate::new(iri_of(length)));
        let (version, _) = heard(group.handle.declare_version(0, 0, vec![declared]).await)?;
        drop(ticking);
        let committed = heard(group.handle.catalog(0, version).await)?;
        assert_eq!(committed.bytes().len(), 523_001);
        let mut storage = group.dial_taking("storage", MAX_FRAME_FLOOR_U32).await?;
        let (read, frames) = catalog_read(&mut storage, &Catalog::request(0, version)).await?;
        assert_eq!(frames, 2);
        let view = read.view().context("the answer")?;
        assert_eq!(view.bytes(), committed.bytes());
        assert_eq!((view.tenant(), view.version()), (0, version));
        assert_eq!(
            MAX_FRAME_FLOOR, 524_288,
            "the floor of `max_frame` (`PROTOCOL.md` §11.9)"
        );
        drop(storage);
        group.stop().await?;
        Ok(())
    })
}

/// A Catalog request of `version` 0 naming an epoch on a role connection reads the version a client may read there.
#[test]
fn test_a_catalog_request_naming_an_epoch_is_answered_at_it() -> TestResult {
    Runtime::new()?.block_on(async {
        let group = served("catalog-at", 508).await?;
        let ticking = crate::types::system_group::ticking(&group.clock);
        let (version, commit_version) = heard(
            group
                .handle
                .declare_version(
                    0,
                    0,
                    vec![Declared::Predicate(DeclaredPredicate::new(
                        "http://ex.org/p",
                    ))],
                )
                .await,
        )?;
        let _closed = heard(group.handle.next_close(commit_version >> 22).await)?;
        drop(ticking);
        let latest = heard(group.handle.latest_close().await)?;
        let mut frontend = group.dial("frontend").await?;
        let (read, _) = catalog_read(&mut frontend, &Catalog::request_at(0, latest)).await?;
        assert_eq!(read.view().context("the answer")?.version(), version);
        let refused = frontend
            .request(&Catalog::request_at(0, latest + 1_000))
            .await
            .err()
            .context("a refusal")?;
        assert_eq!(
            refusal_code(&refused),
            Some(ErrorCode::Protocol),
            "{refused}"
        );
        drop(frontend);
        group.stop().await?;
        Ok(())
    })
}

/// The scenario "A closed connection's marks hold for the limit": a frontend connection marks a session and closes; a check of
/// kind 1 on another connection answers 14 before twice `transaction_ms` and passes after it.
#[test]
fn test_a_closed_connections_marks_hold_for_twice_the_limit() -> TestResult {
    use super::system_session_frontend_tests::checked;
    Runtime::new()?.block_on(async {
        let group = served("closed-marks", 509).await?;
        let alice = principal(&group.handle, "alice").await?;
        group.clock.advance(100_000);
        heard(group.handle.session(1, alice).await)?;
        let mut first = group.dial("frontend").await?;
        assert_eq!(
            checked(&mut first, &[(1, alice, CheckKind::Marks)]).await?,
            [1]
        );
        let mut second = group.dial("frontend").await?;
        drop(first);
        // The session of the connection closed is dropped once its server reads the close, and the group takes the close at
        // the clock's reading then, which this test does not move meanwhile.
        compio::time::sleep(std::time::Duration::from_millis(100)).await;
        group.clock.advance(100_000 - 1);
        assert_eq!(
            checked(&mut second, &[(1, alice, CheckKind::Asks)]).await?,
            [14]
        );
        group.clock.advance(1);
        assert_eq!(
            checked(&mut second, &[(1, alice, CheckKind::Asks)]).await?,
            [1]
        );
        drop(second);
        group.stop().await?;
        Ok(())
    })
}
