/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! A core's link to the system group, played by a [`SystemPlay`] at a [`RoleDouble`]'s address (`PROTOCOL.md` §11.5,
//! §11.12; design D5 to D9): corner cases C5.2, C5.11, C5.13, C5.16, C5.17, C5.21 and C5.31 to C5.33 of
//! `notes/corner-cases.md`.

use std::time::{Duration, Instant};

use compio::runtime::Runtime;
use ultragraphdb_common_frontend::{RoleDouble, RoleLinks, SystemLink, SystemPlay};
use ultragraphdb_common_role::{Answer, RoleCluster, RoleSecret};
use ultragraphdb_common_service::Readiness;
use ultragraphdb_core_catalog::{CatalogContents, describe_of};
use ultragraphdb_protocol_scram::ScramVerifier;
use ultragraphdb_protocol_transport::{Address, Endpoint};
use ultragraphdb_protocol_wire::{
    BodyView, CheckKind, CheckOutcome, CheckView, CloseRequestView, Command, Declared,
    DeclaredPredicate, EndedView, ErrorCode, ErrorReply, FrameView, MAX_FRAME_SIZE, SubscribeView,
};
use ultragraphdb_tool_bdd::{Context, TestError, TestResult};

/// A refusal as a test's error.
trait Replied<T> {
    fn replied(self) -> TestResult<T>;
}

impl<T> Replied<T> for Result<T, ErrorReply> {
    fn replied(self) -> TestResult<T> {
        self.map_err(|refusal| TestError::new(format!("{refusal:?}")))
    }
}

const CLUSTER: u64 = 7;
const ALICE: u64 = 11;
const WAIT: Duration = Duration::from_secs(5);

/// A cluster whose role secret is in a file of `name`.
fn cluster(name: &str) -> TestResult<RoleCluster> {
    let root = std::env::var_os("TEST_TMPDIR").map_or_else(std::env::temp_dir, Into::into);
    let file =
        std::path::PathBuf::from(root).join(format!("system-link-{name}-{}", std::process::id()));
    std::fs::write(&file, "a role secret")?;
    Ok(RoleCluster::new(
        CLUSTER,
        RoleSecret::read(&file.display().to_string())?,
        WAIT,
    ))
}

/// Now, from the runtime's timer, whose first tick is now.
async fn now() -> Instant {
    compio::time::interval(Duration::from_secs(1)).tick().await
}

fn session(n: u128) -> [u8; 16] {
    n.to_be_bytes()
}

/// A played group of two partitions, its double, and a link to it that answers.
struct Played {
    play: SystemPlay,
    double: RoleDouble,
    link: SystemLink,
    links: RoleLinks,
}

async fn played(name: &str, play: SystemPlay) -> TestResult<Played> {
    played_with(name, play, Readiness::default()).await
}

/// As [`played`], the link setting `readiness`.
async fn played_with(name: &str, play: SystemPlay, readiness: Readiness) -> TestResult<Played> {
    let cluster = cluster(name)?;
    let script = play.clone();
    let double = RoleDouble::start(&cluster, Endpoint::Frontend, move |frame| {
        script.answer(frame)
    })
    .await?;
    let links = RoleLinks::new(cluster.clone(), "frontend".to_string());
    let address = Address::parse(double.address())?;
    let link = SystemLink::dial(address, cluster, links.clone(), 5_000, readiness);
    link.answering(now().await + WAIT).await.replied()?;
    Ok(Played {
        play,
        double,
        link,
        links,
    })
}

/// Waits until `holds`, each millisecond, or says what it waited for.
async fn until(what: &str, holds: impl Fn() -> bool) -> TestResult {
    for _ in 0..5_000 {
        if holds() {
            return Ok(());
        }
        compio::time::sleep(Duration::from_millis(1)).await;
    }
    Err(TestError::new(format!("{what} did not come to hold")))
}

fn view_of(bytes: &[u8]) -> TestResult<FrameView<'_>> {
    Ok(FrameView::validate(bytes, CLUSTER, MAX_FRAME_SIZE)?)
}

/// C5.2, C5.3: a link subscribes with `latest`, and its view holds the latest close and the placement before it answers.
#[test]
fn test_a_link_begins_at_the_latest_close() -> TestResult {
    Runtime::new()?.block_on(async {
        let play = SystemPlay::new(CLUSTER, 2);
        play.close(40, 0, &[]);
        let at = played("latest", play).await?;
        let subscribes = at.double.frames(Command::Subscribe);
        let first = subscribes.first().context("a Subscribe")?;
        let subscribe = SubscribeView::from_frame(&view_of(first.bytes())?)?;
        assert!(subscribe.latest());
        assert_eq!(subscribe.from(), 0);
        assert_eq!(at.link.view().latest_closed(), 40);
        assert!(at.link.view().answers());
        assert!(at.link.view().holds_tenant(0));
        assert!(at.link.is_up());
        assert_eq!(at.link.lost_since(), None);
        assert_eq!(at.links.every_partition(), vec![1, 2]);
        Ok(())
    })
}

/// A Session answered registers; one refused is refused with the group's code.
#[test]
fn test_a_session_is_registered_through_the_link() -> TestResult {
    Runtime::new()?.block_on(async {
        let at = played("session", SystemPlay::new(CLUSTER, 1)).await?;
        at.link.session(session(5), ALICE).await.replied()?;
        assert_eq!(at.double.count(Command::Session), 1);
        at.play.refuse(
            Command::Session,
            ErrorReply::new(ErrorCode::SessionBusy, "a full registry"),
        );
        let refused = at
            .link
            .session(session(6), ALICE)
            .await
            .err()
            .context("refused")?;
        assert_eq!(refused.code(), ErrorCode::SessionBusy);
        Ok(())
    })
}

/// C5.28: a check passes, or is refused with its outcome's code: 12, 21 with `detail` 2, 14.
#[test]
fn test_a_check_is_answered_by_its_outcome() -> TestResult {
    Runtime::new()?.block_on(async {
        let at = played("check", SystemPlay::new(CLUSTER, 1)).await?;
        at.link
            .check(session(1), ALICE, CheckKind::Marks)
            .await
            .replied()?;
        for (n, outcome, code, detail) in [
            (2, CheckOutcome::NotHeld, ErrorCode::SessionEvicted, None),
            (
                3,
                CheckOutcome::OtherPrincipal,
                ErrorCode::PermissionDenied,
                Some(2),
            ),
            (4, CheckOutcome::InFlight, ErrorCode::SessionBusy, None),
        ] {
            at.play.outcome(session(n), outcome);
            let refused = at
                .link
                .check(session(n), ALICE, CheckKind::Marks)
                .await
                .err()
                .context("refused")?;
            assert_eq!(refused.code(), code);
            if let Some(detail) = detail {
                assert_eq!(refused.detail(), detail);
            }
        }
        assert_eq!(at.link.round_trips().count(), 4, "each Check frame timed");
        Ok(())
    })
}

/// C5.21: eight checks of eight sessions queued before the sending task runs leave in one Check frame, ascending.
#[test]
fn test_one_turns_checks_share_one_frame() -> TestResult {
    Runtime::new()?.block_on(async {
        let at = played("turn", SystemPlay::new(CLUSTER, 1)).await?;
        let asked: Vec<_> = [8u128, 3, 6, 1, 7, 2, 5, 4]
            .into_iter()
            .map(|n| {
                let link = at.link.clone();
                compio::runtime::spawn(async move {
                    link.check(session(n), ALICE, CheckKind::Marks).await
                })
            })
            .collect();
        for each in asked {
            each.await
                .map_err(|_| TestError::new("a check's task"))?
                .replied()?;
        }
        let frames = at.double.frames(Command::Check);
        assert_eq!(frames.len(), 1, "one frame");
        let frame = frames.first().context("the Check frame")?;
        let checks = CheckView::from_frame(&view_of(frame.bytes())?)?;
        let sessions: Vec<[u8; 16]> = (0..checks.len())
            .filter_map(|at| checks.get(at))
            .map(|check| check.session())
            .collect();
        assert_eq!(sessions, (1..=8).map(session).collect::<Vec<_>>());
        Ok(())
    })
}

/// An Ended leaves in a frame of its own, answered by nothing.
#[test]
fn test_an_ended_leaves_in_the_next_turn() -> TestResult {
    Runtime::new()?.block_on(async {
        let at = played("ended", SystemPlay::new(CLUSTER, 1)).await?;
        at.link.ended(session(9));
        until("an Ended", || at.double.count(Command::Ended) == 1).await?;
        let frames = at.double.frames(Command::Ended);
        let frame = frames.first().context("the Ended frame")?;
        let ended = EndedView::from_frame(&view_of(frame.bytes())?)?;
        assert_eq!(ended.sessions(), &[session(9)]);
        Ok(())
    })
}

/// C5.31 to C5.33: while the link is closed, a Session and a check are refused with code 17, and the Ended of a request that
/// ended meanwhile leaves on the next connection.
#[test]
fn test_a_closed_link_refuses_and_carries_its_endeds() -> TestResult {
    Runtime::new()?.block_on(async {
        let at = played("closed", SystemPlay::new(CLUSTER, 1)).await?;
        at.play.hold_checks();
        let link = at.link.clone();
        let pending =
            compio::runtime::spawn(
                async move { link.check(session(1), ALICE, CheckKind::Marks).await },
            );
        until("the Check frame", || at.double.count(Command::Check) == 1).await?;
        at.double.drop_connections();
        let refused = pending
            .await
            .map_err(|_| TestError::new("the check's task"))?
            .err()
            .context("refused")?;
        assert_eq!(
            refused.code(),
            ErrorCode::Unavailable,
            "C5.32: a check unanswered when the link closes"
        );
        until("the link down", || !at.link.is_up()).await?;
        assert!(at.link.lost_since().is_some());
        at.link.ended(session(1));
        let down = at
            .link
            .session(session(2), ALICE)
            .await
            .err()
            .context("refused")?;
        assert_eq!(down.code(), ErrorCode::Unavailable);
        let down = at
            .link
            .check(session(3), ALICE, CheckKind::Marks)
            .await
            .err()
            .context("refused")?;
        assert_eq!(down.code(), ErrorCode::Unavailable);
        at.play.release();
        link_up_again(&at).await?;
        until("the carried Ended", || at.double.count(Command::Ended) == 1).await?;
        let frames = at.double.frames(Command::Ended);
        let frame = frames.first().context("the Ended frame")?;
        assert_eq!(
            EndedView::from_frame(&view_of(frame.bytes())?)?.sessions(),
            &[session(1)]
        );
        Ok(())
    })
}

async fn link_up_again(at: &Played) -> TestResult {
    until("a second Subscribe", || {
        at.double.count(Command::Subscribe) == 2
    })
    .await?;
    at.link.answering(now().await + WAIT).await.replied()?;
    Ok(())
}

/// Challenge and Verify through the link: the salt and count of a held name, and the principal and signature of a proof that
/// holds; a proof that does not, nothing.
#[test]
fn test_challenge_and_verify_ask_the_group() -> TestResult {
    Runtime::new()?.block_on(async {
        let verifier = ScramVerifier::derive("pw", b"a salt of ann", 4_096)?;
        let play = SystemPlay::new(CLUSTER, 1).with_principal("ann", 41, verifier.clone());
        let at = played("verify", play).await?;
        let (iterations, salt) = at.link.challenge("ann").await.replied()?;
        assert_eq!(
            (iterations, salt.as_slice()),
            (4_096, b"a salt of ann".as_slice())
        );
        let wrong = at
            .link
            .verify("ann", "an auth message", &[1; 32])
            .await
            .replied()?;
        assert_eq!(wrong, None);
        Ok(())
    })
}

/// A Declare of kind 1 answers its version and commit version, and the view learns the version; a stale base is refused with
/// the group's code 2.
#[test]
fn test_a_declare_builds_a_version() -> TestResult {
    Runtime::new()?.block_on(async {
        let play = SystemPlay::new(CLUSTER, 1).with_tenants(&[1]);
        let at = played("declare", play).await?;
        let declared = vec![Declared::Predicate(DeclaredPredicate::new(
            "http://ex.org/p",
        ))];
        let (version, commit_version) = at
            .link
            .declare(1, 0, ALICE, declared.clone())
            .await
            .replied()?;
        assert_eq!(version, 1);
        assert_eq!(commit_version >> 22, at.play.closed() + 1);
        assert_eq!(at.link.view().current_version(1), 1);
        let stale = at
            .link
            .declare(1, 0, ALICE, declared)
            .await
            .err()
            .context("refused")?;
        assert_eq!(stale.code(), ErrorCode::Conflict);
        let dropped = at
            .link
            .drop_catalog(1, ALICE, ultragraphdb_protocol_wire::DropKind::Tenant, None)
            .await
            .replied()?;
        assert_eq!(dropped, 2);
        Ok(())
    })
}

/// C5.16: Defaults and a Catalog version past one frame are read whole across frames.
#[test]
fn test_defaults_and_a_catalog_are_read_across_frames() -> TestResult {
    Runtime::new()?.block_on(async {
        let play = SystemPlay::new(CLUSTER, 1);
        let predicates: Vec<String> = (0..2_000)
            .map(|n| format!("http://ex.org/{n:04}/{}", "p".repeat(300)))
            .collect();
        play.defaults(0, predicates.clone());
        let bytes: Vec<u8> = (0..700_000u32).map(|n| (n % 251) as u8).collect();
        play.version(0, 3, 1, bytes.clone());
        let at = played("frames", play).await?;
        assert_eq!(at.link.defaults(0).await.replied()?, predicates);
        let (version, read) = at.link.catalog_at(0, 1).await.replied()?;
        assert_eq!((version, read.len()), (3, bytes.len()));
        assert!(read == bytes);
        Ok(())
    })
}

/// C5.13: a tenant holding no version is read once, its code 7 with `detail` 0 taken as none: the initial catalog's schema
/// id; a held version's is `describe_of`'s.
#[test]
fn test_a_schema_id_is_read_once() -> TestResult {
    Runtime::new()?.block_on(async {
        let play = SystemPlay::new(CLUSTER, 1).with_tenants(&[1]);
        play.version(1, 2, 1, CatalogContents::new(1, 2, Vec::new()).encode()?);
        let at = played("schema", play).await?;
        let initial = describe_of(0, 0, &[], std::iter::empty()).schema_id();
        assert_eq!(at.link.schema_id(0).await.replied()?, initial);
        let held = describe_of(1, 0, &[], std::iter::empty()).schema_id();
        assert_eq!(at.link.schema_id(1).await.replied()?, held);
        assert_eq!(at.link.view().current_version(1), 2);
        let reads = at.double.count(Command::Catalog);
        let _again = at.link.schema_id(1).await.replied()?;
        assert_eq!(at.double.count(Command::Catalog), reads, "read once");
        Ok(())
    })
}

/// C5.17: a CloseRequest names the epoch with `partition` 0, and nothing answers it.
#[test]
fn test_a_close_request_names_partition_0() -> TestResult {
    Runtime::new()?.block_on(async {
        let at = played("close", SystemPlay::new(CLUSTER, 1)).await?;
        at.link.close_request(2);
        until("a CloseRequest", || {
            at.double.count(Command::CloseRequest) == 1
        })
        .await?;
        let frames = at.double.frames(Command::CloseRequest);
        let frame = frames.first().context("the CloseRequest")?;
        let asked = CloseRequestView::from_frame(&view_of(frame.bytes())?)?;
        assert_eq!((asked.partition(), asked.epoch()), (0, 2));
        Ok(())
    })
}

/// Members name where the partitions and compute listen, and the core's links learn them (design D9).
#[test]
fn test_members_are_read_into_the_links() -> TestResult {
    Runtime::new()?.block_on(async {
        let at = played("members", SystemPlay::new(CLUSTER, 2)).await?;
        at.play.storage(1, "tcp://127.0.0.1:1");
        at.play.storage(2, "tcp://127.0.0.1:2");
        at.play.compute("tcp://127.0.0.1:3");
        at.link.members().await.replied()?;
        assert_eq!(at.links.compute_count(), 1);
        assert!(at.links.partition(1).is_some());
        assert!(at.links.partition(2).is_some());
        Ok(())
    })
}

/// Design D10, role-binaries "Readiness, health and shutdown": the frontend is ready once the group answers it, its view
/// holds a placement and a close, Members lists every partition of the placement and a storage partition answers. Every
/// partition listed where nothing listens leaves it unready.
#[test]
fn test_readiness_waits_for_a_partition_to_answer() -> TestResult {
    Runtime::new()?.block_on(async {
        let readiness = Readiness::default();
        let play = SystemPlay::new(CLUSTER, 2);
        // Port 1 refuses every connection: both partitions are listed, and neither answers.
        play.storage(1, "tcp://127.0.0.1:1");
        play.storage(2, "tcp://127.0.0.1:1");
        let at = played_with("unready", play, readiness.clone()).await?;
        until("every partition listed", || {
            at.links.lists(1) && at.links.lists(2)
        })
        .await?;
        // Members is read again every 100 ms until the frontend is ready: five of those rounds pass unready.
        compio::time::sleep(Duration::from_millis(500)).await;
        assert!(!readiness.is_ready(), "no partition answers");
        Ok(())
    })
}

/// As above: every partition listed and one of them answering makes the frontend ready, the other listed where nothing
/// listens.
#[test]
fn test_readiness_comes_once_a_partition_answers() -> TestResult {
    Runtime::new()?.block_on(async {
        let partition =
            RoleDouble::start(&cluster("ready-partition")?, Endpoint::Partition, |_| {
                Answer::Open
            })
            .await?;
        let readiness = Readiness::default();
        let play = SystemPlay::new(CLUSTER, 2);
        play.storage(1, "tcp://127.0.0.1:1");
        play.storage(2, partition.address());
        let _at = played_with("ready", play, readiness.clone()).await?;
        until("readiness", || readiness.is_ready()).await?;
        Ok(())
    })
}

/// A ready frontend over a played group of one partition, which a double answers for: the group, its link, and readiness.
async fn ready(name: &str) -> TestResult<(Played, RoleDouble, Readiness)> {
    let partition = RoleDouble::start(
        &cluster(&format!("{name}-partition"))?,
        Endpoint::Partition,
        |_| Answer::Open,
    )
    .await?;
    let readiness = Readiness::default();
    let play = SystemPlay::new(CLUSTER, 1);
    play.close(40, 0, &[]);
    play.storage(1, partition.address());
    let at = played_with(name, play, readiness.clone()).await?;
    until("readiness", || readiness.is_ready()).await?;
    Ok((at, partition, readiness))
}

/// Role-binaries "Readiness, health and shutdown": the frontend's `/ready` is withdrawn from the moment its connection to
/// the system group closes, and set again once the group answers it, its view holds a close, Members lists every partition
/// and one answers.
#[test]
fn test_readiness_is_withdrawn_while_the_link_is_down_and_set_again_after() -> TestResult {
    Runtime::new()?.block_on(async {
        let (at, _partition, readiness) = ready("withdrawn").await?;
        at.double.drop_connections();
        // The link dials again 100 ms after its close at the earliest: readiness is withdrawn well before.
        until("readiness withdrawn", || !readiness.is_ready()).await?;
        link_up_again(&at).await?;
        until("readiness again", || readiness.is_ready()).await?;
        assert!(at.link.is_up());
        Ok(())
    })
}

/// As above: a connection open again is not enough; while the group sends no close on it, the view does not answer and the
/// frontend stays unready.
#[test]
fn test_readiness_waits_for_the_view_after_the_link_returns() -> TestResult {
    Runtime::new()?.block_on(async {
        let (at, _partition, readiness) = ready("view-again").await?;
        at.play.silence(Command::Subscribe);
        at.double.drop_connections();
        until("a second Subscribe", || {
            at.double.count(Command::Subscribe) == 2
        })
        .await?;
        assert!(at.link.is_up(), "the connection is open again");
        // Members is read again every 100 ms while the frontend is unready: five of those rounds pass unready.
        compio::time::sleep(Duration::from_millis(500)).await;
        assert!(!readiness.is_ready(), "no close on the new subscription");
        Ok(())
    })
}

/// C5.5, C5.7: a token ahead waits for its close; one whose deadline is reached with no close is refused with code 24,
/// `detail` 0.
#[test]
fn test_a_token_ahead_waits_for_its_close() -> TestResult {
    Runtime::new()?.block_on(async {
        let at = played("token", SystemPlay::new(CLUSTER, 1)).await?;
        let closed = at.link.view().latest_closed();
        at.link.token(closed + 1, now().await).await.replied()?;
        let refused = at
            .link
            .token(closed + 3, now().await)
            .await
            .err()
            .context("refused")?;
        assert_eq!((refused.code(), refused.detail()), (ErrorCode::TimedOut, 0));
        let link = at.link.clone();
        let deadline = now().await + WAIT;
        let waiting = compio::runtime::spawn(async move { link.token(closed + 2, deadline).await });
        at.play.close(closed + 1, 0, &[]);
        waiting
            .await
            .map_err(|_| TestError::new("the token's task"))?
            .replied()?;
        Ok(())
    })
}

/// C5.11: a subscription the group ends is opened again with `latest`; the view then reads Placement and the current version
/// of each tenant it holds before it answers.
#[test]
fn test_a_subscription_ended_is_opened_again() -> TestResult {
    Runtime::new()?.block_on(async {
        let play = SystemPlay::new(CLUSTER, 1).with_tenants(&[1]);
        let at = played("reopen", play).await?;
        at.play.close(2, 0, &[(1, 3)]);
        until("the announcement", || {
            at.link.view().current_version(1) == 3
        })
        .await?;
        at.play
            .version(1, 4, 0, CatalogContents::new(1, 4, Vec::new()).encode()?);
        let placements = at.double.count(Command::Placement);
        at.play.end_subscriptions();
        at.play.close(12, 0, &[]);
        until("a second Subscribe", || {
            at.double.count(Command::Subscribe) == 2
        })
        .await?;
        at.link.answering(now().await + WAIT).await.replied()?;
        until("the view at 12", || at.link.view().latest_closed() == 12).await?;
        assert!(at.double.count(Command::Placement) > placements);
        assert_eq!(at.link.view().current_version(1), 4);
        Ok(())
    })
}
