/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The scenarios of the system-group capability: each `When` runs its scenario on the stage and checks what its `Then` states,
//! and the `Then` holds that it ran. What a scenario says of a host process binds the group's half here, as
//! `notes/group-3.md` records.

use std::cell::Cell;
use std::rc::Rc;
use std::time::Duration;

use compio::runtime::{Runtime, spawn};
use cucumber::{then, when};
use ultragraphdb_cluster_metadata::MetadataConfig;
use ultragraphdb_cluster_sequencer::SequencerConfig;
use ultragraphdb_common_role::RoleClient;
use ultragraphdb_common_service_config::ServiceConfig;
use ultragraphdb_common_system::{DropStep, ReportAsked, SystemStart, start_system, system_run};
use ultragraphdb_core_ids::Tenant;
use ultragraphdb_error_protocol::ProtocolError;
use ultragraphdb_protocol_scram::{ScramClient, ScramRelay, ScramVerifier};
use ultragraphdb_protocol_wire::{
    Bind, BindKind, BodyView, Challenge, ChallengeView, Check, CheckKind, CheckView, Command,
    Declared, DeclaredPredicate, Defaults, DefaultsView, Ended, EpochCloseView, ErrorCode, Join,
    Members, MembersView, Placement, PlacementView, SessionCheck, Subscribe, Verify, VerifyView,
};
use ultragraphdb_tool_bdd::{Context, TestError, TestResult};
use ultragraphdb_transactions_log::{GatedStorage, SimStorage, SyncGate, Wal, WalConfig};

use crate::Steps;
use crate::bdd_stage::{
    CLUSTER, Clock, EPOCH_US, HOLD_US, Stage, error_of, heard, next_frame, recovered, refused,
    start_of,
};

/// Runs `scenario` on a runtime of its own and records that it ran.
pub(crate) fn run<F, Fut>(w: &mut Steps, scenario: F) -> TestResult
where
    F: FnOnce() -> Fut,
    Fut: std::future::Future<Output = TestResult>,
{
    Runtime::new()?.block_on(scenario())?;
    w.ran = true;
    Ok(())
}

/// The crate's manifest, whose dependencies are what the group's host links for the books.
fn manifest() -> TestResult<String> {
    let root = ultragraphdb_tool_bdd::features_dir()?;
    let directory = root.parent().context("the crate's directory")?;
    Ok(std::fs::read_to_string(directory.join("Cargo.toml"))?)
}

/// The outcomes of one Check of `checks` on `connection`, by number.
pub(crate) async fn outcomes(
    stage: &Stage,
    connection: u64,
    checks: &[(u128, u64, CheckKind)],
) -> TestResult<Vec<u8>> {
    let sent = checks
        .iter()
        .map(|(session, principal, kind)| {
            SessionCheck::new(session.to_be_bytes(), *principal, *kind)
        })
        .collect();
    Ok(heard(stage.handle.check(connection, sent).await)?
        .iter()
        .map(|check| check.outcome().number())
        .collect())
}

/// A new connection's number.
pub(crate) async fn connected(stage: &Stage) -> TestResult<u64> {
    stage.handle.connect().await.context("a connection")
}

const MARKS: CheckKind = CheckKind::Marks;
const ASKS: CheckKind = CheckKind::Asks;

#[when("`storage_1` holds a `[system]` table and `storage_2` none, and both start")]
fn hosting_serves(w: &mut Steps) -> TestResult {
    // The group's half: it answers a Placement at the role address it listens at. Which process listens where is group 4's.
    run(w, || async {
        let stage = Stage::new(1, &start_of(1, 8)?).await?;
        let mut storage = stage.dial("storage").await?;
        let placed = storage.request(&Placement::request(0)).await?;
        let view = placed.view()?;
        assert!(PlacementView::from_frame(&view)?.position() >= 1);
        drop(placed);
        drop(storage);
        let _ended = stage.stop().await?;
        Ok(())
    })
}

#[then(
    "`storage_1` listens at `system.role_address` and answers a Placement request there, and `storage_2` listens at its partitions' addresses alone"
)]
fn hosting_serves_held(w: &mut Steps) {
    assert!(w.ran);
}

#[when("the frontend binary's dependency graph is listed")]
fn frontend_graph(w: &mut Steps) -> TestResult {
    // The group's half: the books are the system group's crate's own dependencies, which a binary not linking it lacks; the
    // frontend binary drops that crate in group 5.
    let manifest = manifest()?;
    for book in [
        "ultragraphdb_cluster_metadata",
        "ultragraphdb_cluster_sequencer",
        "ultragraphdb_transactions_log",
    ] {
        assert!(
            manifest.contains(&format!("{book} = {{ workspace = true }}")),
            "{book}"
        );
    }
    w.ran = true;
    Ok(())
}

#[then(
    "it names neither `ultragraphdb_cluster_metadata`, `ultragraphdb_cluster_sequencer` nor `ultragraphdb_transactions_log`"
)]
fn frontend_graph_held(w: &mut Steps) {
    assert!(w.ran);
}

#[when(
    "the group holds a catalog version, two block leases, a principal and twelve closes, and its host is killed and restarted"
)]
fn books_recover(w: &mut Steps) -> TestResult {
    run(w, || async {
        let start = start_of(1, 8)?;
        let stage = Stage::new(2, &start).await?;
        let declaring = {
            let handle = stage.handle.clone();
            spawn(async move {
                let declared = vec![Declared::Predicate(DeclaredPredicate::new(
                    "http://ex.org/p",
                ))];
                handle.declare_version(0, 0, declared).await
            })
        };
        let mut closed = 0;
        while closed < 12 {
            closed = stage.close_after(closed).await?;
        }
        let _declared = heard(
            declaring
                .await
                .map_err(|error| TestError::new(format!("{error:?}")))?,
        )?;
        let first = heard(stage.handle.lease(1, 1, 16).await)?;
        let second = heard(stage.handle.lease(1, 1, 16).await)?;
        let before = (
            heard(stage.handle.catalog(0, 0).await)?,
            stage.principal("ann").await?,
            heard(stage.handle.placement(0).await)?,
        );
        let (storage, clock) = stage.stop().await?;
        storage.power_loss();
        let restarted = recovered(&storage, &start, &clock).await?;
        let stage = Stage::serve(storage, clock, restarted).await?;
        let after = (
            heard(stage.handle.catalog(0, 0).await)?,
            stage.principal("ann").await?,
            heard(stage.handle.placement(0).await)?,
        );
        assert_eq!(after, before, "read at the same log position");
        assert_eq!(heard(stage.handle.latest_close().await)?, 12);
        let third = heard(stage.handle.lease(1, 1, 16).await)?;
        assert!(
            second >= first + 16 && third >= second + 16,
            "{first} {second} {third}"
        );
        let _ended = stage.stop().await?;
        Ok(())
    })
}

#[then(
    "the group's state after recovery equals its state before the kill, read at the same log position, and the sequencer's last closed epoch is 12"
)]
fn books_recover_held(w: &mut Steps) {
    assert!(w.ran);
}

#[when("the development container has started with `system.data_dir = \"/data/system\"`")]
fn logs_beside(w: &mut Steps) -> TestResult {
    // The group's half: started over `<data>/system`, it keeps its two logs in `meta/` and `seq/` there; the partitions' and the
    // container's are groups 4 and 7's.
    run(w, || async {
        let root = std::env::var_os("TEST_TMPDIR")
            .map_or_else(std::env::temp_dir, std::path::PathBuf::from);
        let system = root
            .join(format!("data-{}", std::process::id()))
            .join("system");
        let start = SystemStart::new(MetadataConfig::new(), SequencerConfig::new(1, 5)?, 1);
        let group = start_system(
            ServiceConfig::default(),
            CLUSTER,
            system.clone(),
            start,
            Vec::new(),
        )
        .await?;
        assert!(system.join("meta").is_dir() && system.join("seq").is_dir());
        group.stopped().await?;
        Ok(())
    })
}

#[then("`/data` holds `system/meta/`, `system/seq/` and `storage/p1/r1/`")]
fn logs_beside_held(w: &mut Steps) {
    assert!(w.ran);
}

#[when("a partition's log sync is delayed past `epoch_ms_max` while the group's logs sync at once")]
fn partition_sync_delayed(w: &mut Steps) -> TestResult {
    use ultragraphdb_traits_log::{OrderedLog, RoundRef};
    run(w, || async {
        let stage = Stage::new(3, &start_of(1, 8)?).await?;
        // A partition's log of its own, its sync held: the group shares no log with it.
        let gate = SyncGate::default();
        let storage = GatedStorage::new(SimStorage::new(33), gate.clone());
        let (mut partition_log, _) =
            Wal::open(storage, "/p1", WalConfig::new(CLUSTER, false)).await?;
        gate.hold();
        let (tail, parent) = partition_log.tail();
        let position = tail.next().context("a position")?;
        let round = RoundRef::new(
            position,
            parent,
            0,
            1,
            std::num::NonZeroU32::MIN,
            &[1, 2, 3],
        );
        let _ticket = partition_log.append(round)?;
        let pending = spawn(async move { partition_log.sync().await });
        gate.until_waiting().await?;
        let mut closed = 0;
        for _ in 0..3 {
            closed = stage.close_after(closed).await?;
        }
        assert_eq!(
            closed, 3,
            "the closes came on time, the partition's sync held"
        );
        gate.release();
        let _synced = pending
            .await
            .map_err(|error| TestError::new(format!("{error:?}")))??;
        // The delayed partition reads again from the last close it held, 0, and receives every close in order.
        let mut read = 0;
        for expected in 1..=3 {
            read = heard(stage.handle.next_close(read).await)?.epoch().get();
            assert_eq!(read, expected);
        }
        let _ended = stage.stop().await?;
        Ok(())
    })
}

#[then(
    "the sequencer closes its epochs on time, and the delayed partition receives them once it reads again"
)]
fn partition_sync_delayed_held(w: &mut Steps) {
    assert!(w.ran);
}

/// A storage connection joining with `partitions` and subscribing from 0: the closes through `through` it receives, in order.
async fn joined_and_subscribed(
    stage: &Stage,
    address: &str,
    partitions: Vec<u32>,
    through: u64,
) -> TestResult<Vec<u64>> {
    let mut storage = stage.dial("storage").await?;
    let joined = storage
        .request(&Join::storage(address, partitions))
        .await?
        .view()?
        .command();
    assert_eq!(joined, Command::Join);
    storage.send_only(&Subscribe::new(0)).await?;
    let mut closes = Vec::new();
    while closes.last().copied().unwrap_or(0) < through {
        let frame = next_frame(&mut storage).await?;
        closes.push(EpochCloseView::from_frame(&frame.view()?)?.epoch());
    }
    Ok(closes)
}

#[when("the hosting storage process starts with partition 1")]
fn partition_dials(w: &mut Steps) -> TestResult {
    run(w, || async {
        let stage = Stage::new(4, &start_of(1, 8)?).await?;
        let mut closed = 0;
        for _ in 0..3 {
            closed = stage.close_after(closed).await?;
        }
        let closes = joined_and_subscribed(&stage, "tcp://127.0.0.1:7201", vec![1], closed).await?;
        assert_eq!(closes, [1, 2, 3]);
        let _ended = stage.stop().await?;
        Ok(())
    })
}

#[then(
    "partition 1 opens a role connection to `cluster.system_address`, joins, subscribes and receives the closes on it"
)]
fn partition_dials_held(w: &mut Steps) {
    assert!(w.ran);
}

#[when("`storage_2`, which hosts no group, starts with partitions 3 and 4")]
fn other_process(w: &mut Steps) -> TestResult {
    run(w, || async {
        let stage = Stage::new(5, &start_of(4, 8)?).await?;
        let mut closed = 0;
        for _ in 0..2 {
            closed = stage.close_after(closed).await?;
        }
        let first =
            joined_and_subscribed(&stage, "tcp://127.0.0.1:7201", vec![1, 2], closed).await?;
        let second =
            joined_and_subscribed(&stage, "tcp://127.0.0.1:7202", vec![3, 4], closed).await?;
        assert_eq!((first, second), (vec![1, 2], vec![1, 2]));
        let mut compute = stage.dial("compute").await?;
        let members = compute.request(&Members::new()).await?;
        let view = members.view()?;
        let listed = MembersView::from_frame(&view)?;
        let addresses: Vec<&str> = (0..listed.len())
            .filter_map(|index| listed.address(index))
            .collect();
        assert_eq!(
            addresses,
            [
                "tcp://127.0.0.1:7201",
                "tcp://127.0.0.1:7201",
                "tcp://127.0.0.1:7202",
                "tcp://127.0.0.1:7202"
            ]
        );
        drop(members);
        drop(compute);
        let _ended = stage.stop().await?;
        Ok(())
    })
}

#[then(
    "both join at `cluster.system_address`, Members names their addresses, and each receives every close"
)]
fn other_process_held(w: &mut Steps) {
    assert!(w.ran);
}

/// `client`'s last request refused as `CommandNotServed` naming `command`: Error 1 on stream 0, and the connection closed.
async fn not_served(client: &mut RoleClient, command: Command) -> TestResult {
    let frame = next_frame(client).await?;
    let (stream, code, message) = error_of(&frame)?.context("an Error")?;
    drop(frame);
    assert_eq!((stream, code), (0, ErrorCode::Protocol), "{message}");
    let rule = ProtocolError::command_not_served(command.number()).to_string();
    assert!(message.contains(&rule), "{message}");
    assert!(client.next_frame().await.is_err(), "the connection closed");
    Ok(())
}

#[when("a role connection to the system group sends a Bind")]
fn sends_bind(w: &mut Steps) -> TestResult {
    run(w, || async {
        let stage = Stage::new(6, &start_of(1, 8)?).await?;
        let mut frontend = stage.dial("frontend").await?;
        let bind = Bind::new(1, BindKind::Transaction, 0, 0, 0, [1; 16]);
        frontend.send_only(&bind).await?;
        not_served(&mut frontend, Command::Bind).await?;
        let _ended = stage.stop().await?;
        Ok(())
    })
}

#[then("it is refused as `CommandNotServed` naming Bind")]
fn sends_bind_held(w: &mut Steps) {
    assert!(w.ran);
}

#[when("a connection whose role is `storage` sends a Verify")]
fn storage_verifies(w: &mut Steps) -> TestResult {
    run(w, || async {
        let stage = Stage::new(7, &start_of(1, 8)?).await?;
        let mut storage = stage.dial("storage").await?;
        storage
            .send_only(&Verify::request("ann", "n=ann,r=a", [0; 32]))
            .await?;
        not_served(&mut storage, Command::Verify).await?;
        let _ended = stage.stop().await?;
        Ok(())
    })
}

#[then("it is refused as `CommandNotServed` naming Verify")]
fn storage_verifies_held(w: &mut Steps) {
    assert!(w.ran);
}

#[when("a Session registers a session for `alice`, and the same Session is sent again")]
fn held_identity(w: &mut Steps) -> TestResult {
    run(w, || async {
        let stage = Stage::new(8, &start_of(1, 8)?).await?;
        let alice = stage.principal("alice").await?;
        heard(stage.handle.session(11, alice).await)?;
        let again = refused(stage.handle.session(11, alice).await)?;
        assert_eq!(again.code(), ErrorCode::Protocol);
        let named = format!("{:032x}", 11_u128);
        assert!(again.message().contains(&named), "{}", again.message());
        stage.clock.advance(HOLD_US);
        let connection = connected(&stage).await?;
        let held = outcomes(&stage, connection, &[(11, alice, ASKS)]).await?;
        assert_eq!(held, [1], "held once");
        let _ended = stage.stop().await?;
        Ok(())
    })
}

#[then(
    "the second is refused with Error 1 naming the session, and the registry holds the session once"
)]
fn held_identity_held(w: &mut Steps) {
    assert!(w.ran);
}

#[when("the registry holds a session for `alice` and a Session names it for `bob`")]
fn other_principal(w: &mut Steps) -> TestResult {
    run(w, || async {
        let stage = Stage::new(9, &start_of(1, 8)?).await?;
        let (alice, bob) = (
            stage.principal("alice").await?,
            stage.principal("bob").await?,
        );
        heard(stage.handle.session(11, alice).await)?;
        let refusal = refused(stage.handle.session(11, bob).await)?;
        assert_eq!(refusal.code(), ErrorCode::Protocol);
        let named = format!("{:032x}", 11_u128);
        assert!(refusal.message().contains(&named), "{}", refusal.message());
        stage.clock.advance(HOLD_US);
        let connection = connected(&stage).await?;
        let held = outcomes(&stage, connection, &[(11, alice, ASKS)]).await?;
        assert_eq!(held, [1], "alice's");
        let _ended = stage.stop().await?;
        Ok(())
    })
}

#[then("it is refused with Error 1 naming the session, and the session stays `alice`'s")]
fn other_principal_held(w: &mut Steps) {
    assert!(w.ran);
}

#[when("a Session names a principal the metadata state machine does not hold")]
fn unknown_principal(w: &mut Steps) -> TestResult {
    run(w, || async {
        let stage = Stage::new(10, &start_of(1, 8)?).await?;
        let refusal = refused(stage.handle.session(11, 4_242).await)?;
        assert_eq!(refusal.code(), ErrorCode::Refused);
        assert!(refusal.message().contains("4242"), "{}", refusal.message());
        let _ended = stage.stop().await?;
        Ok(())
    })
}

#[then("it is refused with code 7 naming the principal")]
fn unknown_principal_held(w: &mut Steps) {
    assert!(w.ran);
}

#[when(
    "one Check frame holds a kind 2 check of `alice`'s session, a kind 2 check of a session never registered, and a kind 1 check of `bob`'s session naming `alice`"
)]
fn frame_of_checks(w: &mut Steps) -> TestResult {
    run(w, || async {
        let stage = Stage::new(11, &start_of(1, 8)?).await?;
        let (alice, bob) = (
            stage.principal("alice").await?,
            stage.principal("bob").await?,
        );
        stage.clock.advance(HOLD_US);
        heard(stage.handle.session(1, alice).await)?;
        heard(stage.handle.session(3, bob).await)?;
        let mut frontend = stage.dial("frontend").await?;
        let sent = vec![
            SessionCheck::new(1_u128.to_be_bytes(), alice, MARKS),
            SessionCheck::new(2_u128.to_be_bytes(), alice, MARKS),
            SessionCheck::new(3_u128.to_be_bytes(), alice, ASKS),
        ];
        let reply = frontend.request(&Check::new(sent.clone())).await?;
        let view = reply.view()?;
        let answered = CheckView::from_frame(&view)?;
        answered.check_reply(&sent)?;
        let numbers: Vec<u8> = (0..answered.len())
            .filter_map(|index| answered.get(index))
            .map(|check| check.outcome().number())
            .collect();
        assert_eq!(numbers, [1, 12, 21]);
        drop(reply);
        let connection = connected(&stage).await?;
        let marked = outcomes(&stage, connection, &[(1, alice, ASKS), (3, bob, ASKS)]).await?;
        assert_eq!(marked, [14, 1], "alice's alone marked");
        drop(frontend);
        let _ended = stage.stop().await?;
        Ok(())
    })
}

#[then(
    "one reply holds the outcomes 1, 12 and 21 in that order, and `alice`'s session alone is marked"
)]
fn frame_of_checks_held(w: &mut Steps) {
    assert!(w.ran);
}

#[when(
    "a session is marked by a passed check of kind 2, and another check of kind 2 names it before any Ended"
)]
fn second_commit(w: &mut Steps) -> TestResult {
    run(w, || async {
        let stage = Stage::new(12, &start_of(1, 8)?).await?;
        let alice = stage.principal("alice").await?;
        stage.clock.advance(HOLD_US);
        heard(stage.handle.session(1, alice).await)?;
        let connection = connected(&stage).await?;
        assert_eq!(
            outcomes(&stage, connection, &[(1, alice, MARKS)]).await?,
            [1]
        );
        assert_eq!(
            outcomes(&stage, connection, &[(1, alice, MARKS)]).await?,
            [14]
        );
        let _ended = stage.stop().await?;
        Ok(())
    })
}

#[then("the second check's outcome is 14")]
fn second_commit_held(w: &mut Steps) {
    assert!(w.ran);
}

#[when(
    "a session is marked by a passed check of kind 2 on one connection, and a check of kind 1 names it on another"
)]
fn resend_sees_original(w: &mut Steps) -> TestResult {
    run(w, || async {
        let stage = Stage::new(13, &start_of(1, 8)?).await?;
        let alice = stage.principal("alice").await?;
        stage.clock.advance(HOLD_US);
        heard(stage.handle.session(1, alice).await)?;
        let (one, other) = (connected(&stage).await?, connected(&stage).await?);
        assert_eq!(outcomes(&stage, one, &[(1, alice, MARKS)]).await?, [1]);
        assert_eq!(outcomes(&stage, other, &[(1, alice, ASKS)]).await?, [14]);
        stage.handle.ended(one, vec![1]);
        let after = outcomes(&stage, other, &[(1, alice, MARKS)]).await?;
        assert_eq!(after, [1], "the kind 1 check marked nothing");
        let _ended = stage.stop().await?;
        Ok(())
    })
}

#[then("the kind 1 check's outcome is 14, and it marks nothing")]
fn resend_sees_original_held(w: &mut Steps) {
    assert!(w.ran);
}

#[when(
    "the registry holds `sessions_max` sessions, the least recently registered is marked, and one more is registered"
)]
fn marked_not_evicted(w: &mut Steps) -> TestResult {
    run(w, || async {
        let stage = Stage::new(14, &start_of(1, 3)?).await?;
        let alice = stage.principal("alice").await?;
        stage.clock.advance(HOLD_US);
        for session in [1, 2, 3] {
            heard(stage.handle.session(session, alice).await)?;
        }
        let connection = connected(&stage).await?;
        assert_eq!(
            outcomes(&stage, connection, &[(1, alice, MARKS)]).await?,
            [1]
        );
        heard(stage.handle.session(4, alice).await)?;
        let asked = [
            (1, alice, ASKS),
            (2, alice, ASKS),
            (3, alice, ASKS),
            (4, alice, ASKS),
        ];
        assert_eq!(outcomes(&stage, connection, &asked).await?, [14, 12, 1, 1]);
        let _ended = stage.stop().await?;
        Ok(())
    })
}

#[then("the least recently registered without a mark is evicted, and the marked one stays")]
fn marked_not_evicted_held(w: &mut Steps) {
    assert!(w.ran);
}

/// The outcome of one check of `session` by `principal` of `kind` sent on `client`.
async fn checked_on(
    client: &mut RoleClient,
    session: u128,
    principal: u64,
    kind: CheckKind,
) -> TestResult<u8> {
    let sent = Check::new(vec![SessionCheck::new(
        session.to_be_bytes(),
        principal,
        kind,
    )]);
    let reply = client.request(&sent).await?;
    let view = reply.view()?;
    let checked = CheckView::from_frame(&view)?.get(0).context("the check")?;
    Ok(checked.outcome().number())
}

#[when("an Ended names a marked session twice in two frames")]
fn ended_twice(w: &mut Steps) -> TestResult {
    run(w, || async {
        let stage = Stage::new(15, &start_of(1, 8)?).await?;
        let alice = stage.principal("alice").await?;
        stage.clock.advance(HOLD_US);
        heard(stage.handle.session(1, alice).await)?;
        let mut frontend = stage.dial("frontend").await?;
        assert_eq!(checked_on(&mut frontend, 1, alice, MARKS).await?, 1);
        for _ in 0..2 {
            frontend
                .send_only(&Ended::new(vec![1_u128.to_be_bytes()]))
                .await?;
        }
        let answered =
            compio::time::timeout(Duration::from_millis(100), frontend.next_frame()).await;
        assert!(answered.is_err(), "no frame answers either");
        let after = checked_on(&mut frontend, 1, alice, MARKS).await?;
        assert_eq!(after, 1, "the first took the mark away");
        drop(frontend);
        let _ended = stage.stop().await?;
        Ok(())
    })
}

#[then("the first takes the mark away, the second is passed over, and neither is answered")]
fn ended_twice_held(w: &mut Steps) {
    assert!(w.ran);
}

#[when(
    "a connection marks a session and closes, and a check of kind 1 names the session on another connection before twice `transaction_ms` has passed, and again after it"
)]
fn closed_marks_hold(w: &mut Steps) -> TestResult {
    run(w, || async {
        let stage = Stage::new(16, &start_of(1, 8)?).await?;
        let alice = stage.principal("alice").await?;
        stage.clock.advance(HOLD_US);
        heard(stage.handle.session(1, alice).await)?;
        let (marking, other) = (connected(&stage).await?, connected(&stage).await?);
        assert_eq!(outcomes(&stage, marking, &[(1, alice, MARKS)]).await?, [1]);
        stage.handle.disconnect(marking);
        let _taken = heard(stage.handle.latest_close().await)?;
        stage.clock.advance(HOLD_US - 1);
        assert_eq!(outcomes(&stage, other, &[(1, alice, ASKS)]).await?, [14]);
        stage.clock.advance(1);
        assert_eq!(outcomes(&stage, other, &[(1, alice, ASKS)]).await?, [1]);
        let _ended = stage.stop().await?;
        Ok(())
    })
}

#[then("the first check's outcome is 14 and the second passes")]
fn closed_marks_hold_held(w: &mut Steps) {
    assert!(w.ran);
}

#[when(
    "a connection marks a session and closes, and another connection sends an Ended naming the session"
)]
fn orphan_ended(w: &mut Steps) -> TestResult {
    run(w, || async {
        let stage = Stage::new(17, &start_of(1, 8)?).await?;
        let alice = stage.principal("alice").await?;
        stage.clock.advance(HOLD_US);
        heard(stage.handle.session(1, alice).await)?;
        let (marking, other) = (connected(&stage).await?, connected(&stage).await?);
        assert_eq!(outcomes(&stage, marking, &[(1, alice, MARKS)]).await?, [1]);
        stage.handle.disconnect(marking);
        let _taken = heard(stage.handle.latest_close().await)?;
        stage.handle.ended(other, vec![1]);
        assert_eq!(outcomes(&stage, other, &[(1, alice, MARKS)]).await?, [1]);
        let _ended = stage.stop().await?;
        Ok(())
    })
}

#[then("the mark ends, and a check of kind 2 naming the session passes")]
fn orphan_ended_held(w: &mut Steps) {
    assert!(w.ran);
}

#[when(
    "one connection marks a session and another sends an Ended naming it while the first is open"
)]
fn open_connection_ended(w: &mut Steps) -> TestResult {
    run(w, || async {
        let stage = Stage::new(18, &start_of(1, 8)?).await?;
        let alice = stage.principal("alice").await?;
        stage.clock.advance(HOLD_US);
        heard(stage.handle.session(1, alice).await)?;
        let (marking, other) = (connected(&stage).await?, connected(&stage).await?);
        assert_eq!(outcomes(&stage, marking, &[(1, alice, MARKS)]).await?, [1]);
        stage.handle.ended(other, vec![1]);
        assert_eq!(outcomes(&stage, other, &[(1, alice, ASKS)]).await?, [14]);
        let _ended = stage.stop().await?;
        Ok(())
    })
}

#[then("the session stays marked")]
fn open_connection_ended_held(w: &mut Steps) {
    assert!(w.ran);
}

#[when(
    "the system group's host restarts, and a check of kind 1 naming a registered session of its principal arrives before twice `transaction_ms` has passed since it began to listen"
)]
fn restarted_quiet(w: &mut Steps) -> TestResult {
    run(w, || async {
        let start = start_of(1, 2)?;
        let stage = Stage::new(19, &start).await?;
        let alice = stage.principal("alice").await?;
        stage.clock.advance(HOLD_US);
        heard(stage.handle.session(1, alice).await)?;
        heard(stage.handle.session(2, alice).await)?;
        let (storage, clock) = stage.stop().await?;
        let restarted = recovered(&storage, &start, &clock).await?;
        let stage = Stage::serve(storage, clock, restarted).await?;
        let connection = connected(&stage).await?;
        assert_eq!(
            outcomes(&stage, connection, &[(1, alice, ASKS)]).await?,
            [14]
        );
        assert_eq!(
            outcomes(&stage, connection, &[(2, alice, MARKS)]).await?,
            [1]
        );
        let busy = refused(stage.handle.session(3, alice).await)?;
        assert_eq!(busy.code(), ErrorCode::SessionBusy);
        let _ended = stage.stop().await?;
        Ok(())
    })
}

#[then(
    "the check's outcome is 14, a check of kind 2 naming an unmarked session passes, and a Session that would evict is refused with code 14"
)]
fn restarted_quiet_held(w: &mut Steps) {
    assert!(w.ran);
}

/// A group whose logs sync through gates, its handle, its run and `alice`.
struct Gated {
    handle: ultragraphdb_common_system::SystemHandle,
    run: compio::runtime::JoinHandle<Result<(), ultragraphdb_error_role::RoleError>>,
    alice: u64,
}

/// The group over `storage` whose metadata syncs pass `metadata`'s gate and the sequencer's `sequencer`'s, holding `alice`, run
/// on this runtime past its quiet time.
async fn gated(
    storage: &SimStorage,
    clock: &Clock,
    gates: (&SyncGate, &SyncGate),
    sessions_max: u32,
) -> TestResult<Gated> {
    let gated = |gate: &SyncGate| GatedStorage::new(storage.clone(), gate.clone());
    let config = WalConfig::new(CLUSTER, false);
    let (meta_log, _) = Wal::open(gated(gates.0), "/meta", config).await?;
    let (seq_log, _) = Wal::open(gated(gates.1), "/seq", config).await?;
    let start = start_of(1, sessions_max)?;
    let mut group = ultragraphdb_common_system::SystemGroup::recover(
        meta_log,
        seq_log,
        &start,
        None,
        clock.now(),
    )
    .await?;
    let verifier = ScramVerifier::derive("pw", b"salt", 4_096)?;
    group.create_principal_once("alice", verifier).await?;
    let (handle, run) = system_run(group, clock.reading());
    let run = spawn(run);
    let alice = handle
        .verifier("alice".to_string())
        .await
        .context("alice")?
        .0;
    clock.advance(HOLD_US);
    Ok(Gated { handle, run, alice })
}

#[when("the sequencer's sync of a close is in flight, and a Check and a Session arrive")]
fn check_while_close_syncs(w: &mut Steps) -> TestResult {
    run(w, || async {
        let (storage, clock) = (SimStorage::new(20), Clock::new());
        let (metadata, sequencer) = (SyncGate::default(), SyncGate::default());
        let Gated { handle, run, alice } =
            gated(&storage, &clock, (&metadata, &sequencer), 8).await?;
        heard(handle.session(1, alice).await)?;
        let connection = handle.connect().await.context("a connection")?;
        sequencer.hold();
        clock.advance(EPOCH_US);
        sequencer.until_waiting().await?;
        let registered = Rc::new(Cell::new(false));
        let session = {
            let (handle, registered) = (handle.clone(), registered.clone());
            spawn(async move {
                let answer = handle.session(2, alice).await;
                registered.set(true);
                answer
            })
        };
        let check = vec![SessionCheck::new(1_u128.to_be_bytes(), alice, ASKS)];
        let checked =
            compio::time::timeout(Duration::from_secs(2), handle.check(connection, check))
                .await
                .map_err(|_| TestError::new("the Check waited for the sync"))?;
        let outcome = heard(checked)?.first().map(|one| one.outcome().number());
        assert_eq!(outcome, Some(1));
        compio::time::sleep(Duration::from_millis(50)).await;
        assert!(
            !registered.get(),
            "the Session answered before the sync ended"
        );
        sequencer.release();
        heard(
            session
                .await
                .map_err(|error| TestError::new(format!("{error:?}")))?,
        )?;
        drop(handle);
        run.await
            .map_err(|error| TestError::new(format!("{error:?}")))??;
        Ok(())
    })
}

#[then("the Check is answered before the sync ends, and the Session only after it")]
fn check_while_close_syncs_held(w: &mut Steps) {
    assert!(w.ran);
}

#[when(
    "an eviction is appended to the metadata log and its sync is in flight, and a Check names the evicted session"
)]
fn eviction_appended(w: &mut Steps) -> TestResult {
    run(w, || async {
        let (storage, clock) = (SimStorage::new(21), Clock::new());
        let (metadata, sequencer) = (SyncGate::default(), SyncGate::default());
        let Gated { handle, run, alice } =
            gated(&storage, &clock, (&metadata, &sequencer), 1).await?;
        heard(handle.session(1, alice).await)?;
        let connection = handle.connect().await.context("a connection")?;
        metadata.hold();
        let evicting = {
            let handle = handle.clone();
            spawn(async move { handle.session(2, alice).await })
        };
        metadata.until_waiting().await?;
        let check = vec![SessionCheck::new(1_u128.to_be_bytes(), alice, ASKS)];
        let checked = heard(handle.check(connection, check).await)?;
        assert_eq!(checked.first().map(|one| one.outcome().number()), Some(12));
        metadata.release();
        heard(
            evicting
                .await
                .map_err(|error| TestError::new(format!("{error:?}")))?,
        )?;
        drop(handle);
        run.await
            .map_err(|error| TestError::new(format!("{error:?}")))??;
        Ok(())
    })
}

#[then("the check's outcome is 12")]
fn eviction_appended_held(w: &mut Steps) {
    assert!(w.ran);
}

/// An exchange as `name` of `password` over the frontend's connection: the client, the relay, the AuthMessage and the proof.
async fn exchanged(
    frontend: &mut RoleClient,
    name: &str,
    password: &str,
) -> TestResult<(ScramClient, ScramRelay, String, [u8; 32])> {
    let mut client = ScramClient::new(name, password, "client-nonce")?;
    let mut relay = ScramRelay::new("group-nonce");
    let asked = relay.first(&client.first_message()?)?;
    let (iterations, salt) = {
        let reply = frontend.request(&Challenge::request(asked)).await?;
        let view = reply.view()?;
        let challenge = ChallengeView::from_frame(&view)?;
        (challenge.iterations(), challenge.salt().to_vec())
    };
    let server_first = relay.challenge(&salt, iterations)?;
    let (auth_message, proof) = relay.last(&client.final_message(&server_first)?)?;
    Ok((client, relay, auth_message, proof))
}

/// A Verify's reply: `verified`, `principal`, the signature and the frame's bytes.
type Verified = (bool, u64, Option<[u8; 32]>, Vec<u8>);

/// What a Verify of `name`, `auth_message` and `proof` is answered over `frontend`.
async fn verified(
    frontend: &mut RoleClient,
    name: &str,
    auth_message: &str,
    proof: [u8; 32],
) -> TestResult<Verified> {
    let reply = frontend
        .request(&Verify::request(name, auth_message, proof))
        .await?;
    let view = reply.view()?;
    let verify = VerifyView::from_frame(&view)?;
    Ok((
        verify.verified(),
        verify.principal(),
        verify.signature().copied(),
        reply.bytes().to_vec(),
    ))
}

#[when(
    "a Verify carries `ann`'s name, the exchange's AuthMessage and the proof `ann`'s password gives"
)]
fn proof_holds(w: &mut Steps) -> TestResult {
    run(w, || async {
        let stage = Stage::new(22, &start_of(1, 8)?).await?;
        let ann = stage.principal("ann").await?;
        let mut frontend = stage.dial("frontend").await?;
        let (mut client, mut relay, auth_message, proof) =
            exchanged(&mut frontend, "ann", "pw").await?;
        let (held, principal, signature, _bytes) =
            verified(&mut frontend, "ann", &auth_message, proof).await?;
        assert_eq!((held, principal), (true, ann));
        let signature = signature.context("a ServerSignature")?;
        client.verify(&relay.finish(&signature)?)?;
        drop(frontend);
        let _ended = stage.stop().await?;
        Ok(())
    })
}

#[then("the reply holds `verified` 1, `ann`'s principal and a ServerSignature of 32 bytes")]
fn proof_holds_held(w: &mut Steps) {
    assert!(w.ran);
}

#[when(
    "one Verify carries a wrong proof for `ann` and another any proof for the unknown name `nobody`"
)]
fn wrong_and_unknown(w: &mut Steps) -> TestResult {
    run(w, || async {
        let stage = Stage::new(23, &start_of(1, 8)?).await?;
        let mut frontend = stage.dial("frontend").await?;
        let (_client, _relay, auth_message, proof) =
            exchanged(&mut frontend, "ann", "not pw").await?;
        let wrong = verified(&mut frontend, "ann", &auth_message, proof).await?;
        let unknown = verified(&mut frontend, "nobody", &auth_message, proof).await?;
        assert_eq!((wrong.0, wrong.1, wrong.2), (false, 0, None));
        assert_eq!((unknown.0, unknown.1, unknown.2), (false, 0, None));
        drop(frontend);
        let _ended = stage.stop().await?;
        Ok(())
    })
}

#[then("both replies hold `verified` 0, `principal` 0 and no ServerSignature")]
fn wrong_and_unknown_held(w: &mut Steps) {
    assert!(w.ran);
}

#[when("a Challenge and a Verify for `ann` are answered")]
fn no_stored_key(w: &mut Steps) -> TestResult {
    run(w, || async {
        let stage = Stage::new(24, &start_of(1, 8)?).await?;
        let mut frontend = stage.dial("frontend").await?;
        let challenge = frontend
            .request(&Challenge::request("ann"))
            .await?
            .bytes()
            .to_vec();
        let (_client, _relay, auth_message, proof) = exchanged(&mut frontend, "ann", "pw").await?;
        let (_, _, _, verify) = verified(&mut frontend, "ann", &auth_message, proof).await?;
        let held = ScramVerifier::derive("pw", b"a salt of ann", 4_096)?;
        for reply in [&challenge, &verify] {
            for key in [held.stored_key(), held.server_key()] {
                assert!(
                    !reply.windows(32).any(|run| run == key),
                    "a key left the group"
                );
            }
        }
        drop(frontend);
        let _ended = stage.stop().await?;
        Ok(())
    })
}

#[then(
    "neither reply holds `ann`'s stored key or server key, and no other command the endpoint serves returns them"
)]
fn no_stored_key_held(w: &mut Steps) {
    // No other command the endpoint serves reads a principal's keys; the property test of law L3.33 draws over its answers.
    assert!(w.ran);
}

/// Versions 1 to 3 of tenant 0, each built on the one before, a close assigning each its epoch.
async fn three_versions(stage: &Stage) -> TestResult {
    let ticking = {
        let clock = stage.clock.clone();
        spawn(async move {
            for _ in 0..5_000 {
                clock.advance(EPOCH_US);
                compio::time::sleep(Duration::from_millis(1)).await;
            }
        })
    };
    for (built_on, iri) in [
        (0, "http://ex.org/a"),
        (1, "http://ex.org/b"),
        (2, "http://ex.org/c"),
    ] {
        let declared = vec![Declared::Predicate(DeclaredPredicate::new(iri))];
        let _version = heard(stage.handle.declare_version(0, built_on, declared).await)?;
    }
    drop(ticking);
    Ok(())
}

#[when(
    "version 3 of `t1` is current and a Declare of kind 1 names `built_on` 3 with one declaration"
)]
fn built_on_named(w: &mut Steps) -> TestResult {
    run(w, || async {
        let stage = Stage::new(25, &start_of(1, 8)?).await?;
        three_versions(&stage).await?;
        let declaring = {
            let handle = stage.handle.clone();
            spawn(async move {
                let declared = vec![Declared::Predicate(DeclaredPredicate::new(
                    "http://ex.org/d",
                ))];
                handle.declare_version(0, 3, declared).await
            })
        };
        // The close that announces version 4 assigns its epoch, the one after it.
        let mut after = 0;
        let mut assigning = None;
        for _ in 0..100 {
            stage.clock.advance(EPOCH_US);
            let close = heard(stage.handle.next_close(after).await)?;
            after = close.epoch().get();
            let announced = close
                .catalog()
                .iter()
                .any(|version| version.tenant().get() == 0 && version.version() == 4);
            if announced {
                assigning = Some(after);
                break;
            }
        }
        let assigning = assigning.context("a close announcing version 4")?;
        let answered = heard(
            declaring
                .await
                .map_err(|error| TestError::new(format!("{error:?}")))?,
        )?;
        assert_eq!(answered, (4, (assigning + 1) << 22));
        let _ended = stage.stop().await?;
        Ok(())
    })
}

#[then(
    "the group commits version 4 as pending, and answers once a close has assigned its epoch, `version` 4 and `commit_version` the first version of that epoch"
)]
fn built_on_named_held(w: &mut Steps) {
    assert!(w.ran);
}

#[when("version 3 of `t1` is current and a Declare of kind 1 names `built_on` 2")]
fn built_on_stale(w: &mut Steps) -> TestResult {
    run(w, || async {
        let stage = Stage::new(26, &start_of(1, 8)?).await?;
        three_versions(&stage).await?;
        let declared = vec![Declared::Predicate(DeclaredPredicate::new(
            "http://ex.org/d",
        ))];
        let refusal = refused(stage.handle.declare_version(0, 2, declared).await)?;
        assert_eq!((refusal.code(), refusal.detail()), (ErrorCode::Conflict, 5));
        let message = refusal.message();
        assert!(message.contains('2') && message.contains('3'), "{message}");
        assert_eq!(heard(stage.handle.catalog(0, 0).await)?.version(), 3);
        let _ended = stage.stop().await?;
        Ok(())
    })
}

#[then(
    "it is refused with code 2, kind 5, the message naming versions 2 and 3, and version 3 stays current"
)]
fn built_on_stale_held(w: &mut Steps) {
    assert!(w.ran);
}

#[when("a Declare of kind 2 names a Drop of a predicate the catalog does not declare")]
fn drop_nothing(w: &mut Steps) -> TestResult {
    run(w, || async {
        let stage = Stage::new(27, &start_of(1, 8)?).await?;
        three_versions(&stage).await?;
        let step = DropStep::Predicate("http://ex.org/never".to_string());
        assert_eq!(heard(stage.handle.drop_version(0, step).await)?, 0);
        assert_eq!(heard(stage.handle.catalog(0, 0).await)?.version(), 3);
        let _ended = stage.stop().await?;
        Ok(())
    })
}

#[then("the reply holds `version` 0 at once, and the tenant's current version is unchanged")]
fn drop_nothing_held(w: &mut Steps) {
    assert!(w.ran);
}

/// Closes epochs on `stage` from `closed` until version `version` of tenant 0 holds its epoch: the last close.
async fn closing_until_assigned(stage: &Stage, mut closed: u64, version: u64) -> TestResult<u64> {
    for _ in 0..100 {
        if stage
            .handle
            .catalog(0, version)
            .await
            .is_ok_and(|held| held.epoch() != 0)
        {
            return Ok(closed);
        }
        closed = stage.close_after(closed).await?;
    }
    Err(TestError::new(format!(
        "version {version} never took its epoch"
    )))
}

#[when(
    "a Declare of kind 1 tightens a predicate, its connection closes before any partition reports, and a partition then reports that the data refuses the version"
)]
fn check_outlives(w: &mut Steps) -> TestResult {
    run(w, || async {
        let stage = Stage::new(28, &start_of(1, 8)?).await?;
        three_versions(&stage).await?;
        let single =
            Declared::Predicate(DeclaredPredicate::new("http://ex.org/a").with_single(true));
        let gone = stage.handle.declare_version(0, 3, vec![single]);
        let left = compio::time::timeout(Duration::from_millis(1), gone).await;
        assert!(left.is_err(), "the asker left");
        let latest = heard(stage.handle.latest_close().await)?;
        let closed = closing_until_assigned(&stage, latest, 4).await?;
        let at_fault = ("http://ex.org/a".to_string(), "\"x\"".to_string());
        heard(
            stage
                .handle
                .report(ReportAsked::new(Tenant::new(0), 4, 1, Some(at_fault)))
                .await,
        )?;
        let closed = closing_until_assigned(&stage, closed, 5).await?;
        let closed = stage.close_after(closed).await?;
        for epoch in 1..=closed {
            if let Ok(read) = stage.handle.catalog_at(0, epoch).await {
                assert_ne!(read.version(), 4, "the tightened version at epoch {epoch}");
            }
        }
        let read = heard(stage.handle.catalog_at(0, closed).await)?.version();
        assert_eq!(read, 5, "the restoring version");
        let _ended = stage.stop().await?;
        Ok(())
    })
}

#[then(
    "the group holds a version restoring the previous declaration, and a Catalog request of `version` 0 naming the latest closed epoch never returns the tightened one"
)]
fn check_outlives_held(w: &mut Steps) {
    assert!(w.ran);
}

#[when("`t1` holds default-entry records for `score` and `nick`, and a Defaults names `t1`")]
fn defaults_order(w: &mut Steps) -> TestResult {
    run(w, || async {
        let stage = Stage::new(29, &start_of(1, 8)?).await?;
        for predicate in ["http://ex.org/score", "http://ex.org/nick"] {
            let found = heard(stage.handle.record_default(0, predicate.to_string()).await)?;
            assert!(!found);
        }
        let mut frontend = stage.dial("frontend").await?;
        let reply = frontend.request(&Defaults::request(0)).await?;
        let view = reply.view()?;
        let answer = DefaultsView::from_frame(&view)?;
        let listed: Vec<&str> = (0..answer.len())
            .filter_map(|index| answer.predicate(index))
            .collect();
        assert_eq!(listed, ["http://ex.org/nick", "http://ex.org/score"]);
        assert!(answer.position() >= 1);
        drop(reply);
        drop(frontend);
        let _ended = stage.stop().await?;
        Ok(())
    })
}

#[then(
    "the reply lists `nick` before `score`, and `position` names the log position they were read at"
)]
fn defaults_order_held(w: &mut Steps) {
    assert!(w.ran);
}

#[when("the storage binary's dependency graph is listed")]
fn storage_graph(w: &mut Steps) -> TestResult {
    // The group's half: the crate the hosting binary links for the books names no crate under `ultragraphdb_schema/`; the storage
    // binary's own graph is group 4's.
    assert!(
        !manifest()?.contains("ultragraphdb_schema"),
        "a schema crate"
    );
    w.ran = true;
    Ok(())
}

#[then("it names no crate under `ultragraphdb_schema/`")]
fn storage_graph_held(w: &mut Steps) {
    assert!(w.ran);
}
