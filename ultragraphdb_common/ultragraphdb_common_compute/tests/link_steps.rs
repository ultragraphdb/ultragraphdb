/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The scenarios of `features/client-endpoint/system_link.feature`: endpoint cores asking a system group the test plays.

use std::time::Duration;

use cucumber::{then, when};
use ultragraphdb_common_compute::{
    AuthThrottle, DoubleFrame, LoginLimits, SystemPlay, partition_answer,
};
use ultragraphdb_common_role::Answer;
use ultragraphdb_core_catalog::CatalogContents;
use ultragraphdb_core_ids::{Epoch, SessionToken, Version};
use ultragraphdb_core_mapping::{Declaration, PredicateDeclaration};
use ultragraphdb_protocol_wire::{
    Begin, BeginView, BodyView, CATALOG_BYTES_MAX, CheckKind, CheckOutcome, CheckView, Command,
    Commit, ConflictKind, Describe, DescribeFrames, DropCommand, EndedView, ErrorCode, ErrorReply,
    FLAG_LAST, FrameView, HeaderView, MAX_FRAME_SIZE, ReadLevel,
};
use ultragraphdb_tool_bdd::{Context, TestError, TestResult};

use crate::Steps;
use crate::endpoint_stage::{BIND_VERSION, CLUSTER, until};
use crate::played_stage::{Played, now, played_run};

/// A refusal as a test's error.
fn replied<T>(result: Result<T, ErrorReply>) -> TestResult<T> {
    result.map_err(|refusal| TestError::new(format!("{refusal:?}")))
}

fn view_of(bytes: &[u8]) -> TestResult<FrameView<'_>> {
    Ok(FrameView::validate(bytes, CLUSTER, MAX_FRAME_SIZE)?)
}

/// The sessions of each Check frame the group took, in order.
fn check_frames(played: &Played) -> TestResult<Vec<Vec<[u8; 16]>>> {
    played
        .group
        .frames(Command::Check)
        .iter()
        .map(|frame| {
            let checks = CheckView::from_frame(&view_of(frame.bytes())?)?;
            Ok((0..checks.len())
                .filter_map(|at| checks.get(at))
                .map(|check| check.session())
                .collect())
        })
        .collect()
}

/// A partition answering as nothing goes wrong.
fn ordinary(frame: &DoubleFrame) -> Answer {
    partition_answer(frame, CLUSTER, BIND_VERSION)
}

#[when(
    "eight connections of one endpoint core each queue the check of a Commit through a session of their own before the core's sending task runs next"
)]
fn eight_checks_in_one_turn(w: &mut Steps) -> TestResult {
    played_run(w, || async {
        let played = Played::start(SystemPlay::new(CLUSTER, 1), 1).await?;
        // Sessions 8 to 1, queued in that order before the sending task runs: session 3's outcome is 12.
        played
            .play
            .outcome(3u128.to_be_bytes(), CheckOutcome::NotHeld);
        let system = played.system(0)?.clone();
        let asked: Vec<_> = (1..=8u128)
            .rev()
            .map(|n| {
                let link = system.clone();
                compio::runtime::spawn(async move {
                    (n, link.check(n.to_be_bytes(), 41, CheckKind::Marks).await)
                })
            })
            .collect();
        for each in asked {
            let (n, outcome) = each.await.map_err(|_| TestError::new("a check's task"))?;
            let code = outcome.err().map(|refusal| refusal.code());
            assert_eq!(
                code,
                (n == 3).then_some(ErrorCode::SessionEvicted),
                "session {n}"
            );
        }
        assert_eq!(
            check_frames(&played)?,
            vec![(1..=8u128).map(u128::to_be_bytes).collect::<Vec<_>>()]
        );
        Ok(())
    })
}

#[then(
    "the core sends one Check frame holding eight checks, in ascending order of their sessions, and routes each Commit by its own outcome"
)]
fn eight_checks_in_one_turn_held(w: &mut Steps) {
    assert!(w.ran, "the scenario ran");
}

#[when("a Commit and a resend through one session arrive at one core within one turn")]
fn two_checks_of_one_session(w: &mut Steps) -> TestResult {
    played_run(w, || async {
        let played = Played::start(SystemPlay::new(CLUSTER, 1), 1).await?;
        let session = 5u128.to_be_bytes();
        // The group decides both: the first passes and marks the session, the second finds it in flight.
        played.play.outcome(session, CheckOutcome::Passed);
        played.play.outcome(session, CheckOutcome::InFlight);
        let system = played.system(0)?.clone();
        let (first, second) = (system.clone(), system.clone());
        let commit =
            compio::runtime::spawn(async move { first.check(session, 41, CheckKind::Marks).await });
        let resend =
            compio::runtime::spawn(async move { second.check(session, 41, CheckKind::Asks).await });
        let commit = commit
            .await
            .map_err(|_| TestError::new("the Commit's check"))?;
        let resend = resend
            .await
            .map_err(|_| TestError::new("the resend's check"))?;
        assert!(commit.is_ok());
        assert_eq!(
            resend.err().map(|refusal| refusal.code()),
            Some(ErrorCode::SessionBusy)
        );
        assert_eq!(check_frames(&played)?, vec![vec![session], vec![session]]);
        Ok(())
    })
}

#[then(
    "the turn's Check frame holds one check of that session, the second request's check goes in the next frame, and the core answers neither by itself"
)]
fn two_checks_of_one_session_held(w: &mut Steps) {
    assert!(w.ran, "the scenario ran");
}

#[when(
    "a core's connection to the system group closes while a Commit of a session is at its partition, the partition answers the Commit, and the core dials again"
)]
fn an_ended_after_an_outage(w: &mut Steps) -> TestResult {
    played_run(w, || async {
        let played = Played::start(SystemPlay::new(CLUSTER, 1), 1).await?;
        let holding = std::rc::Rc::new(std::cell::RefCell::new(None::<DoubleFrame>));
        let held = std::rc::Rc::clone(&holding);
        let first = std::rc::Rc::new(std::cell::Cell::new(true));
        let _partition = played
            .partition(1, move |frame| {
                // The first Commit is held, the next answered at once.
                if frame.command() == Command::Commit && first.replace(false) {
                    *held.borrow_mut() = Some(frame.clone());
                    return Answer::Open;
                }
                ordinary(frame)
            })
            .await?;
        let mut client = played.ann(0).await?;
        let session = client.register().await?;
        let transaction = client.begin(0).await?;
        client.write(transaction, "http://ex.org/s").await?;
        client.on(transaction, &Commit::new(session, 1, 0)).await?;
        until("the Commit at its partition", || holding.borrow().is_some()).await?;
        played.group.drop_connections();
        let system = played.system(0)?;
        until("the link down", || !system.is_up()).await?;
        let frame = holding.borrow_mut().take().context("the held Commit")?;
        if let Answer::Reply(reply) = ordinary(&frame) {
            frame.answer_with(reply);
        }
        let reply = client
            .got(transaction)
            .await
            .context("the Commit's reply")?;
        assert_eq!(reply.command, Command::Commit, "the reply");
        until("the link up again", || system.is_up()).await?;
        until("the Ended", || played.group.count(Command::Ended) >= 1).await?;
        let endeds = played.group.frames(Command::Ended);
        let ended = endeds.last().context("an Ended")?;
        assert_eq!(
            EndedView::from_frame(&view_of(ended.bytes())?)?.sessions(),
            &[session]
        );
        replied(system.answering(now().await + Duration::from_secs(5)).await)?;
        let next = client.begin(0).await.context("the next Begin")?;
        client.write(next, "http://ex.org/t").await?;
        client.on(next, &Commit::new(session, 2, 0)).await?;
        let passed = client.got(next).await.context("the next Commit's reply")?;
        assert_eq!(passed.command, Command::Commit, "the next passes");
        Ok(())
    })
}

#[then(
    "the client receives the Commit's reply, the core sends an Ended naming the session on the new connection, and the session's next Commit passes its check"
)]
fn an_ended_after_an_outage_held(w: &mut Steps) {
    assert!(w.ran, "the scenario ran");
}

#[when(
    "a Commit of a session is at its partition, its reply not yet sent, and the client resends it on a connection to another endpoint core"
)]
fn a_resend_on_another_core(w: &mut Steps) -> TestResult {
    played_run(w, || async {
        let played = Played::start(SystemPlay::new(CLUSTER, 1), 2).await?;
        let partition = played
            .partition(1, |frame| {
                if frame.command() == Command::Commit {
                    return Answer::Open;
                }
                ordinary(frame)
            })
            .await?;
        let mut first = played.ann(0).await?;
        let session = first.register().await?;
        let transaction = first.begin(0).await?;
        first.write(transaction, "http://ex.org/s").await?;
        // The group marks the session at the Commit's check, and answers the resend's that it is in flight.
        played.play.outcome(session, CheckOutcome::Passed);
        played.play.outcome(session, CheckOutcome::InFlight);
        first.on(transaction, &Commit::new(session, 1, 0)).await?;
        until("the Commit at its partition", || {
            partition.count(Command::Commit) == 1
        })
        .await?;
        let mut second = played.ann(1).await?;
        let resend = second.send(&Commit::new(session, 1, 0)).await?;
        assert_eq!(second.refused(resend).await?.0, ErrorCode::SessionBusy);
        assert_eq!(partition.count(Command::Recall), 0, "no Recall");
        Ok(())
    })
}

#[then("the resend is answered code 14, and no partition is asked a Recall")]
fn a_resend_on_another_core_held(w: &mut Steps) {
    assert!(w.ran, "the scenario ran");
}

/// The read version a Begin's reply on `stream` names.
async fn read_version(client: &mut crate::endpoint_stage::Client, stream: u64) -> TestResult<u64> {
    let got = client.got(stream).await?;
    assert_eq!(
        got.command,
        Command::Begin,
        "{:?} {}",
        got.code,
        got.message
    );
    Ok(BeginView::from_frame(&view_of(&got.bytes)?)?.read_version())
}

/// The version bound of `epoch`, from the core's own ids.
fn bound(epoch: u64) -> TestResult<u64> {
    Ok(Version::bound_of(Epoch::new(epoch)?))
}

#[when("an authenticated client sends a Begin naming tenant 0")]
fn a_begin_asks_no_other_process(w: &mut Steps) -> TestResult {
    played_run(w, || async {
        let played = Played::start(SystemPlay::new(CLUSTER, 1), 1).await?;
        played.play.close(6, 0, &[]);
        let system = played.system(0)?;
        until("the close of 6", || system.view().latest_closed() == 6).await?;
        until("tenant 0 read with its placement", || {
            !system.view().unread(0)
        })
        .await?;
        let mut client = played.ann(0).await?;
        let before = played.group.commands(&[]).len();
        let begin = client
            .send(&Begin::new(0, 0, true).with_level(ReadLevel::Snapshot))
            .await?;
        assert_eq!(read_version(&mut client, begin).await?, bound(6)?);
        assert_eq!(
            played.group.commands(&[]).len(),
            before,
            "no frame to the group"
        );
        Ok(())
    })
}

#[then(
    "compute answers with the read version of the latest close its view holds, and sends no frame to the system group for it"
)]
fn a_begin_asks_no_other_process_held(w: &mut Steps) {
    assert!(w.ran, "the scenario ran");
}

#[when(
    "an endpoint core subscribes with `latest` after the sequencer has closed 40 epochs and trimmed the first 30"
)]
fn a_late_core(w: &mut Steps) -> TestResult {
    played_run(w, || async {
        let play = SystemPlay::new(CLUSTER, 1);
        // The played group holds the latest close alone, as one that trimmed the first 30 holds no earlier one to send.
        play.close(40, 0, &[]);
        let played = Played::start(play, 1).await?;
        assert_eq!(played.system(0)?.view().latest_closed(), 40);
        let mut client = played.ann(0).await?;
        let begin = client
            .send(&Begin::new(0, 0, true).with_level(ReadLevel::Snapshot))
            .await?;
        assert_eq!(read_version(&mut client, begin).await?, bound(40)?);
        Ok(())
    })
}

#[then(
    "the first close on its stream is that of epoch 40, and its view holds epoch 40 before it answers its first Begin"
)]
fn a_late_core_held(w: &mut Steps) {
    assert!(w.ran, "the scenario ran");
}

/// A token of `epoch`, as a commit of partition 1 returns it.
fn token(epoch: u64) -> TestResult<SessionToken> {
    Ok(SessionToken::new(1, 1, Epoch::new(epoch)?))
}

#[when(
    "a client's commit returned a token of epoch 9 while the core's stream has delivered the close of epoch 7, and the client sends a Begin naming the token on that core"
)]
fn a_token_ahead(w: &mut Steps) -> TestResult {
    played_run(w, || async {
        let played = Played::start(SystemPlay::new(CLUSTER, 1), 1).await?;
        played.play.close(7, 0, &[]);
        let system = played.system(0)?;
        until("the close of 7", || system.view().latest_closed() == 7).await?;
        let mut client = played.ann(0).await?;
        let begin = client
            .send(
                &Begin::new(0, 0, true)
                    .with_level(ReadLevel::Snapshot)
                    .with_token(token(9)?),
            )
            .await?;
        client.silent(begin, 20).await?;
        played.play.close(8, 0, &[]);
        assert_eq!(read_version(&mut client, begin).await?, bound(9)?);
        // A token two closes ahead with a short deadline and no close: refused with code 24, `detail` 0.
        let late = client
            .send(
                &Begin::new(0, 0, true)
                    .with_level(ReadLevel::Snapshot)
                    .with_token(token(11)?)
                    .with_deadline_ms(30),
            )
            .await?;
        assert_eq!(client.refused(late).await?, (ErrorCode::TimedOut, 0));
        Ok(())
    })
}

#[then(
    "the Begin is answered once the close of epoch 8 arrives, and is refused with code 24 only if its deadline is reached with that close not arrived"
)]
fn a_token_ahead_held(w: &mut Steps) {
    assert!(w.ran, "the scenario ran");
}

#[when(
    "a client reads at epoch 9 on one connection, commits there, and begins a transaction naming the commit's token on a connection to a core whose view holds epoch 8"
)]
fn a_token_carries_the_floor(w: &mut Steps) -> TestResult {
    played_run(w, || async {
        let played = Played::start(SystemPlay::new(CLUSTER, 1), 1).await?;
        played.play.close(8, 0, &[]);
        let system = played.system(0)?;
        until("the close of 8", || system.view().latest_closed() == 8).await?;
        let mut client = played.ann(0).await?;
        let begin = client
            .send(
                &Begin::new(0, 0, true)
                    .with_level(ReadLevel::Snapshot)
                    .with_token(token(9)?),
            )
            .await?;
        let version = read_version(&mut client, begin).await?;
        assert!(version >= bound(9)?, "{version}");
        until("the early close asked", || {
            played.group.count(Command::CloseRequest) == 1
        })
        .await?;
        Ok(())
    })
}

#[then("the second transaction's snapshot is at or above the token's epoch")]
fn a_token_carries_the_floor_held(w: &mut Steps) {
    assert!(w.ran, "the scenario ran");
}

/// A catalog version of `tenant` declaring `predicates`, as the metadata group stores it.
fn version_of(tenant: u64, version: u64, predicates: &[&str]) -> TestResult<Vec<u8>> {
    let declarations = predicates
        .iter()
        .map(|iri| {
            Ok(Declaration::Predicate(PredicateDeclaration::new(
                oxrdf::NamedNode::new(*iri)?,
            )))
        })
        .collect::<TestResult<Vec<_>>>()?;
    Ok(CatalogContents::new(tenant, version, declarations).encode()?)
}

#[when(
    "one core commits a declaring transaction that makes version 4 of `t1`, and before the next close another core commits a declaring transaction built on version 3"
)]
fn a_stale_view(w: &mut Steps) -> TestResult {
    played_run(w, || async {
        let play = SystemPlay::new(CLUSTER, 1).with_tenants(&[1]);
        for version in 1..=3 {
            play.version(1, version, version, version_of(1, version, &[])?);
        }
        let played = Played::start(play, 2).await?;
        let mut first = played.ann(0).await?;
        let mut second = played.ann(1).await?;
        let one = first.begin(1).await?;
        let two = second.begin(1).await?;
        for (client, transaction, predicate) in [
            (&mut first, one, "http://ex.org/a"),
            (&mut second, two, "http://ex.org/b"),
        ] {
            let changes = vec![ultragraphdb_protocol_wire::WriteChange::Declare(
                ultragraphdb_protocol_wire::Declared::Predicate(
                    ultragraphdb_protocol_wire::DeclaredPredicate::new(predicate),
                ),
            )];
            client
                .on(
                    transaction,
                    &ultragraphdb_protocol_wire::Write::new(
                        ultragraphdb_protocol_wire::TermTable::new(),
                        changes,
                    ),
                )
                .await?;
        }
        first.commit(one).await?;
        assert_eq!(first.got(one).await?.command, Command::Commit, "version 4");
        second.commit(two).await?;
        let refused = second.got(two).await?;
        assert_eq!(refused.code, Some(ErrorCode::Conflict));
        assert!(
            refused.message.contains('3') && refused.message.contains('4'),
            "{}",
            refused.message
        );
        played.play.close(4, 0, &[(1, 4)]);
        let system = played.system(1)?;
        until("version 4 announced", || {
            system.view().current_version(1) == 4
        })
        .await?;
        let retry = second.begin(1).await?;
        let changes = vec![ultragraphdb_protocol_wire::WriteChange::Declare(
            ultragraphdb_protocol_wire::Declared::Predicate(
                ultragraphdb_protocol_wire::DeclaredPredicate::new("http://ex.org/b"),
            ),
        )];
        second
            .on(
                retry,
                &ultragraphdb_protocol_wire::Write::new(
                    ultragraphdb_protocol_wire::TermTable::new(),
                    changes,
                ),
            )
            .await?;
        second.commit(retry).await?;
        assert_eq!(
            second.got(retry).await?.command,
            Command::Commit,
            "built on 4"
        );
        Ok(())
    })
}

#[then(
    "the second Commit is refused with code 2, kind 5, the message naming versions 3 and 4, and the client's retry after the close builds on version 4"
)]
fn a_stale_view_held(w: &mut Steps) {
    assert!(w.ran, "the scenario ran");
}

/// The predicates a Describe's frames on `stream` name, read as one body.
async fn described(
    client: &mut crate::endpoint_stage::Client,
    stream: u64,
) -> TestResult<(usize, Vec<String>, u64)> {
    let mut frames = DescribeFrames::new();
    let mut count = 0;
    loop {
        let got = client.got(stream).await?;
        assert_eq!(
            got.command,
            Command::Describe,
            "{:?} {}",
            got.code,
            got.message
        );
        count += 1;
        frames.push(&view_of(&got.bytes)?)?;
        if HeaderView::new(&got.bytes)?.flags() & FLAG_LAST != 0 {
            break;
        }
    }
    let view = frames.view()?;
    let body = view.to_body()?;
    Ok((
        count,
        body.predicates()
            .iter()
            .map(|p| p.predicate().to_string())
            .collect(),
        body.catalog(),
    ))
}

#[when(
    "a tightening of `nick` is being checked, and a client describes the tenant on a core that did not send the Declare"
)]
fn describe_during_a_check(w: &mut Steps) -> TestResult {
    played_run(w, || async {
        let play = SystemPlay::new(CLUSTER, 1);
        play.version(0, 1, 1, version_of(0, 1, &["http://ex.org/name"])?);
        // Version 2 tightens `nick` and is being checked: pending, no epoch of its own yet.
        play.version(
            0,
            2,
            0,
            version_of(0, 2, &["http://ex.org/name", "http://ex.org/nick"])?,
        );
        let played = Played::start(play, 1).await?;
        let mut client = played.ann(0).await?;
        let stream = client.send(&Describe::request(0)).await?;
        let (_, predicates, catalog) = described(&mut client, stream).await?;
        assert!(
            predicates.contains(&"http://ex.org/name".to_string()),
            "{predicates:?}"
        );
        assert_eq!(catalog, 1, "the version before the one being checked");
        assert!(
            !predicates.contains(&"http://ex.org/nick".to_string()),
            "{predicates:?}"
        );
        Ok(())
    })
}

#[then("the Describe reports the version before it")]
fn describe_during_a_check_held(w: &mut Steps) {
    assert!(w.ran, "the scenario ran");
}

#[when(
    "a core's view holds the close of epoch 7 and version 3 of `t1`, its subscription is opened again with `latest`, and the first close on the new stream is that of epoch 12, version 4 of `t1` and a placement version having been announced by the closes of epochs 8 to 11"
)]
fn a_subscription_opened_again(w: &mut Steps) -> TestResult {
    played_run(w, || async {
        let play = SystemPlay::new(CLUSTER, 1).with_tenants(&[1]);
        play.version(1, 3, 3, version_of(1, 3, &[])?);
        let played = Played::start(play, 1).await?;
        played.play.close(7, 0, &[(1, 3)]);
        let system = played.system(0)?;
        until("the close of 7", || system.view().latest_closed() == 7).await?;
        played.play.version(1, 4, 9, version_of(1, 4, &[])?);
        let (placements, catalogs) = (
            played.group.count(Command::Placement),
            played.group.count(Command::Catalog),
        );
        played.play.end_subscriptions();
        played.play.close(12, 0, &[]);
        until("a second Subscribe", || {
            played.group.count(Command::Subscribe) == 2
        })
        .await?;
        replied(system.answering(now().await + Duration::from_secs(5)).await)?;
        assert!(
            played.group.count(Command::Placement) > placements,
            "Placement read again"
        );
        assert!(
            played.group.count(Command::Catalog) > catalogs,
            "Catalog read again"
        );
        assert_eq!(system.view().latest_closed(), 12);
        assert_eq!(system.view().current_version(1), 4);
        assert!(system.view().map().is_some(), "the placement in effect");
        let mut client = played.ann(0).await?;
        let begin = client
            .send(&Begin::new(1, 0, true).with_level(ReadLevel::Snapshot))
            .await?;
        assert_eq!(read_version(&mut client, begin).await?, bound(12)?);
        Ok(())
    })
}

#[then(
    "the core reads Placement and, with Catalog, the current version of `t1` before it answers its next Begin, and its view then holds epoch 12, version 4 of `t1` and the placement in effect"
)]
fn a_subscription_opened_again_held(w: &mut Steps) {
    assert!(w.ran, "the scenario ran");
}

#[when("one address fails to authenticate past the throttle's threshold, and then past the ban's")]
fn a_throttled_source(w: &mut Steps) -> TestResult {
    played_run(w, || async {
        let start = now().await;
        let ban = Duration::from_secs(60);
        let mut throttle = AuthThrottle::new(LoginLimits::new(3, 2, ban));
        let (source, other) = (
            std::net::IpAddr::from([10, 0, 0, 1]),
            std::net::IpAddr::from([10, 0, 0, 2]),
        );
        let mut delays = Vec::new();
        for _ in 0..5 {
            delays.push(throttle.delay(source));
            let _banned = throttle.failed(source, start);
        }
        // No delay through 3 failures, the threshold; past it, 100 ms after the 4th (`PROTOCOL.md` §11.9; C5.36).
        assert_eq!(
            delays,
            [0, 0, 0, 0, 100].map(Duration::from_millis).to_vec()
        );
        assert!(
            !throttle.admits(source, start),
            "banned past 3 + 2 failures"
        );
        assert!(throttle.admits(other, start), "another address");
        assert!(
            throttle.admits(source, start + ban),
            "the ban's time passed"
        );
        Ok(())
    })
}

#[then(
    "its attempts past the first threshold wait a growing delay, its connections past the second are closed as accepted, another address still authenticates, and once the ban's time has passed the address authenticates again"
)]
fn a_throttled_source_held(w: &mut Steps) {
    assert!(w.ran, "the scenario ran");
}

#[when(
    "a retried Commit is recalled, every partition answers 18, and the second Check answers 14, the system group having begun to listen less than twice `transaction_ms` before"
)]
fn a_quiet_time_retry(w: &mut Steps) -> TestResult {
    played_run(w, || async {
        let played = Played::start(SystemPlay::new(CLUSTER, 1), 1).await?;
        let _partition = played
            .partition(1, |frame| {
                if frame.command() == Command::Recall {
                    return Answer::Refuse(
                        ErrorReply::new(ErrorCode::NotCommitted, "not held").with_detail(1),
                    );
                }
                ordinary(frame)
            })
            .await?;
        let mut client = played.ann(0).await?;
        let session = client.register().await?;
        // The first check of kind 1 passes; the second, in the group's quiet time, answers 14.
        played.play.outcome(session, CheckOutcome::Passed);
        played.play.outcome(session, CheckOutcome::InFlight);
        let retry = client.send(&Commit::new(session, 1, 0)).await?;
        assert_eq!(client.refused(retry).await?.0, ErrorCode::SessionBusy);
        assert_eq!(check_frames(&played)?.len(), 2, "two checks of kind 1");
        Ok(())
    })
}

#[then("compute answers code 14, and the client resends after a delay")]
fn a_quiet_time_retry_held(w: &mut Steps) {
    assert!(w.ran, "the scenario ran");
}

/// `count` predicates of `length` bytes each.
fn long_predicates(count: usize, length: usize) -> Vec<String> {
    (0..count)
        .map(|at| format!("http://ex.org/{at:05}/{}", "p".repeat(length)))
        .collect()
}

#[when("a tenant's Describe answer takes two frames within the client's `max_frame`")]
fn a_describe_across_frames(w: &mut Steps) -> TestResult {
    played_run(w, || async {
        let play = SystemPlay::new(CLUSTER, 1);
        // 80 predicates of 14,000 bytes: 1,120,000 bytes of text, past one frame of the client's 1,048,576 and within two.
        let predicates = long_predicates(80, 14_000);
        let named: Vec<&str> = predicates.iter().map(String::as_str).collect();
        play.version(0, 1, 1, version_of(0, 1, &named)?);
        let played = Played::start(play, 1).await?;
        let mut client = played.ann(0).await?;
        let stream = client.send(&Describe::request(0)).await?;
        let (frames, read, catalog) = described(&mut client, stream).await?;
        assert_eq!(frames, 2);
        assert_eq!(read, predicates);
        // Each frame names the first's command header, or the client refuses it: both name the version played, 1.
        assert_eq!(catalog, 1, "the version the answer is drawn from");
        Ok(())
    })
}

#[then(
    "compute sends two Describe frames with one command header under the client's credit, the first without `LAST` and the second with it, and the client reads the declarations one frame would hold"
)]
fn a_describe_across_frames_held(w: &mut Steps) {
    assert!(w.ran, "the scenario ran");
}

#[when("a tenant's Describe answer's sections hold more than `catalog_bytes_max` bytes together")]
fn a_describe_past_its_bound(w: &mut Steps) -> TestResult {
    played_run(w, || async {
        let play = SystemPlay::new(CLUSTER, 1);
        // A version of 1,100 predicates of 14,000 bytes, 15,400,000 bytes of text within `catalog_bytes_max`, 16,777,216, which
        // a Catalog carries, and 150 default-entry records of as many: the answer's sections hold 17,500,000 bytes and more.
        let predicates = long_predicates(1_100, 14_000);
        let named: Vec<&str> = predicates.iter().map(String::as_str).collect();
        play.version(0, 1, 1, version_of(0, 1, &named)?);
        let recorded: Vec<String> = (0..150)
            .map(|at| format!("http://ex.org/recorded/{at:05}/{}", "r".repeat(14_000)))
            .collect();
        play.defaults(0, recorded);
        let played = Played::start(play, 1).await?;
        let mut client = played.ann(0).await?;
        let stream = client.send(&Describe::request(0)).await?;
        let expected = u64::try_from(CATALOG_BYTES_MAX)?;
        assert_eq!(
            client.refused(stream).await?,
            (ErrorCode::TooLarge, expected)
        );
        Ok(())
    })
}

#[then("it is refused with code 6, `detail` `catalog_bytes_max`")]
fn a_describe_past_its_bound_held(w: &mut Steps) {
    assert!(w.ran, "the scenario ran");
}

/// A Drop answered on `client`: the frame it got.
async fn dropped(
    client: &mut crate::endpoint_stage::Client,
    drop_of: &DropCommand,
) -> TestResult<crate::endpoint_stage::Got> {
    let stream = client.send(drop_of).await?;
    client.got(stream).await
}

/// Whether `got` is the early refusal of a Drop naming `named` where `in_effect` is in effect: code 2, kind 5, naming both
/// (`PROTOCOL.md` §6.5).
fn refused_early(got: &crate::endpoint_stage::Got, named: u64, in_effect: u64) -> bool {
    got.code == Some(ErrorCode::Conflict)
        && got.detail == ConflictKind::Catalog.number()
        && got.message.contains(&format!("catalog version {named}"))
        && got.message.contains(&format!("{in_effect} is in effect"))
}

#[when(
    "version 1 of a tenant is in effect at the endpoint core's open epoch, and a client sends Drops of kind 3 naming `catalog` 0, 2 and 1, then a Drop of kind 2 and one of kind 1, naming none"
)]
fn drops_against_the_version_in_effect(w: &mut Steps) -> TestResult {
    played_run(w, || async {
        let play = SystemPlay::new(CLUSTER, 1);
        let name = "http://ex.org/name";
        // Version 1 takes effect at epoch 1, the group's latest close: the core's open epoch is 2.
        let in_effect = 1;
        play.version(0, in_effect, 1, version_of(0, in_effect, &[name])?);
        let played = Played::start(play, 1).await?;
        let partition = played.partition(1, ordinary).await?;
        let mut client = played.ann(0).await?;
        for named in [in_effect.saturating_sub(1), in_effect.saturating_add(1)] {
            let got = dropped(
                &mut client,
                &DropCommand::tenant_data(0).with_catalog(named),
            )
            .await?;
            assert!(
                refused_early(&got, named, in_effect),
                "{:?} {}",
                got.code,
                got.message
            );
        }
        assert_eq!(
            (
                partition.count(Command::Bind),
                partition.count(Command::Drop)
            ),
            (0, 0),
            "no partition saw them"
        );
        let passed = [
            DropCommand::tenant_data(0).with_catalog(in_effect),
            DropCommand::graph(0, "http://ex.org/g"),
            DropCommand::predicate(0, name),
        ];
        for drop_of in &passed {
            let got = dropped(&mut client, drop_of).await?;
            assert_eq!(got.command, Command::Drop, "{}", got.message);
        }
        assert_eq!(partition.count(Command::Drop), passed.len());
        Ok(())
    })
}

#[then(
    "the first two are refused with code 2, kind 5, each message naming its version and version 1, before any partition sees them, and the other three reach the partition"
)]
fn drops_against_the_version_in_effect_held(w: &mut Steps) {
    assert!(w.ran, "the scenario ran");
}

#[when(
    "version 1 of a tenant is in effect, a Drop of kind 1 makes version 2 pending, and a client sends a Drop of kind 3 naming `catalog` 1"
)]
fn a_drop_beside_a_pending_version(w: &mut Steps) -> TestResult {
    played_run(w, || async {
        let play = SystemPlay::new(CLUSTER, 1);
        let name = "http://ex.org/name";
        let in_effect = 1;
        play.version(0, in_effect, 1, version_of(0, in_effect, &[name])?);
        let played = Played::start(play, 1).await?;
        let partition = played.partition(1, ordinary).await?;
        let mut client = played.ann(0).await?;
        let got = dropped(&mut client, &DropCommand::predicate(0, name)).await?;
        assert_eq!(got.command, Command::Drop, "{}", got.message);
        // The Drop's Declare of kind 2 committed the next version as pending, with no epoch (`PROTOCOL.md` §13.27).
        assert_eq!(played.group.count(Command::Declare), 1);
        let pending = played.system(0)?.view().current_version(0);
        assert_eq!(pending, in_effect.saturating_add(1), "the core learned it");
        let got = dropped(
            &mut client,
            &DropCommand::tenant_data(0).with_catalog(in_effect),
        )
        .await?;
        assert_eq!(got.command, Command::Drop, "{:?} {}", got.code, got.message);
        assert_eq!(
            partition.count(Command::Drop),
            2,
            "both reached the partition"
        );
        Ok(())
    })
}

#[then("compute passes it to the partition")]
fn a_drop_beside_a_pending_version_held(w: &mut Steps) {
    assert!(w.ran, "the scenario ran");
}

#[when(
    "version 2 of a tenant is being checked, its effective epoch at or below the endpoint core's open epoch, Describe names version 1, and a client sends a Drop of kind 3 naming `catalog` 2 and one naming `catalog` 1"
)]
fn a_drop_during_a_check(w: &mut Steps) -> TestResult {
    played_run(w, || async {
        let play = SystemPlay::new(CLUSTER, 1);
        let (name, nick) = ("http://ex.org/name", "http://ex.org/nick");
        let (before, checked) = (1, 2);
        play.version(0, before, 1, version_of(0, before, &[name])?);
        // Version 2 is being checked: the group names it to no Describe, so the play holds it with no epoch of its own,
        // while the close of epoch 2 announces it, taking effect at 3, the core's open epoch.
        play.version(0, checked, 0, version_of(0, checked, &[name, nick])?);
        let played = Played::start(play, 1).await?;
        let partition = played.partition(1, ordinary).await?;
        played.play.close(2, 0, &[(0, checked)]);
        let system = played.system(0)?;
        until("the close of 2", || system.view().latest_closed() == 2).await?;
        let mut client = played.ann(0).await?;
        let stream = client.send(&Describe::request(0)).await?;
        let (_, _, described) = described(&mut client, stream).await?;
        assert_eq!(described, before, "Describe leaves the checked version out");
        let got = dropped(
            &mut client,
            &DropCommand::tenant_data(0).with_catalog(checked),
        )
        .await?;
        assert_eq!(got.command, Command::Drop, "{:?} {}", got.code, got.message);
        let got = dropped(
            &mut client,
            &DropCommand::tenant_data(0).with_catalog(before),
        )
        .await?;
        assert!(
            refused_early(&got, before, checked),
            "{:?} {}",
            got.code,
            got.message
        );
        assert_eq!(partition.count(Command::Drop), 1, "the first alone");
        Ok(())
    })
}

#[then(
    "the first reaches the partition, and the second is refused with code 2, kind 5, the message naming versions 1 and 2"
)]
fn a_drop_during_a_check_held(w: &mut Steps) {
    assert!(w.ran, "the scenario ran");
}
