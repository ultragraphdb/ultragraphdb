/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The commands the frontend alone sends the system group, over a role connection: Challenge, Verify, Session, Check, Ended,
//! Declare and Defaults (`PROTOCOL.md` §13.8, §13.9, §13.23 to §13.25, §13.27, §13.28).

use compio::runtime::Runtime;
use ultragraphdb_cluster_metadata::MetadataConfig;
use ultragraphdb_common_role::RoleClient;
use ultragraphdb_protocol_scram::{ScramClient, ScramRelay, ScramVerifier};
use ultragraphdb_protocol_wire::{
    BodyBuilder, BodyView, Challenge, ChallengeView, Check, CheckKind, CheckOutcome, CheckView,
    Command, Credit, Declare, DeclareView, Declared, DeclaredPredicate, Defaults, DefaultsFrames,
    DropKind, Ended, ErrorCode, FLAG_LAST, MAX_FRAME_FLOOR_U32, Session, SessionCheck, Verify,
    VerifyView,
};
use ultragraphdb_tool_bdd::{Context, TestError, TestResult};
use ultragraphdb_transactions_log::SimStorage;

use super::{Served, group_with, next_frame, refusal_code, serving, watched};
use crate::types::system_group::{closed_after, heard, principal, starting};

/// A group of one partition, `transaction_ms` 50, holding `alice`, `bob` and `ann` (`ann`'s password `pw`), served at a role
/// address.
async fn served(name: &str, seed: u64) -> TestResult<Served> {
    serving(name, &SimStorage::new(seed), |storage, clock| async move {
        let start = starting(MetadataConfig::new())?.with_transaction_ms(50);
        let mut owner = group_with(storage, clock, start).await?;
        let verifier = ScramVerifier::derive("pw", b"a salt of ann", 4_096)?;
        owner.create_principal_once("ann", verifier).await?;
        Ok(owner)
    })
    .await
}

/// How many millisecond polls a wait on the group takes before it fails.
const POLLS_MAX: u32 = 5_000;

/// Tenant 0's current version once it is `version`, read each millisecond: what a connection sends reaches the group in its
/// own time, and the clock does not move meanwhile.
async fn version_current(group: &Served, version: u64) -> TestResult {
    for _ in 0..POLLS_MAX {
        if group
            .handle
            .catalog(0, 0)
            .await
            .is_ok_and(|held| held.version() == version)
        {
            return Ok(());
        }
        compio::time::sleep(std::time::Duration::from_millis(1)).await;
    }
    Err(TestError::new(format!(
        "version {version} was never current"
    )))
}

/// The outcomes of a Check of `checks` sent on `client`, its reply checked against the request as its requester checks it.
pub(crate) async fn checked(
    client: &mut RoleClient,
    checks: &[(u128, u64, CheckKind)],
) -> TestResult<Vec<u8>> {
    let sent: Vec<SessionCheck> = checks
        .iter()
        .map(|(session, principal, kind)| {
            SessionCheck::new(session.to_be_bytes(), *principal, *kind)
        })
        .collect();
    let reply = client.request(&Check::new(sent.clone())).await?;
    let view = reply.view()?;
    let checked = CheckView::from_frame(&view)?;
    checked.check_reply(&sent)?;
    Ok((0..checked.len())
        .filter_map(|index| checked.get(index))
        .map(|check| check.outcome().number())
        .collect())
}

/// L3.32, L3.33, and the scenarios "A proof that holds is answered with its signature", "An unknown name and a wrong proof answer
/// alike" and "No frame carries a stored key", over the frontend's connection: a Challenge answers `ann`'s salt and count, a
/// Verify of her proof `verified` 1, her principal and a signature of 32 bytes the client accepts; a wrong proof and an unknown
/// name answer `verified` 0, `principal` 0 and no signature; no reply holds her stored key or server key.
#[test]
fn test_a_clients_proof_is_checked_over_the_frontends_connection() -> TestResult {
    Runtime::new()?.block_on(async {
        let group = served("proof", 601).await?;
        let ann = principal(&group.handle, "ann").await?;
        let mut frontend = group.dial("frontend").await?;
        let mut client = ScramClient::new("ann", "pw", "client-nonce")?;
        let mut relay = ScramRelay::new("group-nonce");
        let name = relay.first(&client.first_message()?)?;
        let mut replies = Vec::new();
        let (iterations, salt) = {
            let reply = frontend.request(&Challenge::request(name.clone())).await?;
            replies.push(reply.bytes().to_vec());
            let view = reply.view()?;
            let challenge = ChallengeView::from_frame(&view)?;
            (challenge.iterations(), challenge.salt().to_vec())
        };
        assert_eq!(
            (iterations, salt.as_slice()),
            (4_096, b"a salt of ann".as_slice())
        );
        let server_first = relay.challenge(&salt, iterations)?;
        let (auth_message, proof) = relay.last(&client.final_message(&server_first)?)?;
        let verified = {
            let reply = frontend
                .request(&Verify::request(name.clone(), auth_message.clone(), proof))
                .await?;
            replies.push(reply.bytes().to_vec());
            let view = reply.view()?;
            let verify = VerifyView::from_frame(&view)?;
            (
                verify.verified(),
                verify.principal(),
                verify.signature().copied(),
            )
        };
        assert_eq!((verified.0, verified.1), (true, ann));
        let signature = verified.2.context("a signature")?;
        client.verify(&relay.finish(&signature)?)?;
        let mut wrong = proof;
        if let Some(byte) = wrong.last_mut() {
            *byte ^= 0x80;
        }
        for (asked, sent_proof) in [(name.as_str(), wrong), ("nobody", proof)] {
            let reply = frontend
                .request(&Verify::request(asked, auth_message.clone(), sent_proof))
                .await?;
            replies.push(reply.bytes().to_vec());
            let view = reply.view()?;
            let verify = VerifyView::from_frame(&view)?;
            assert_eq!(
                (verify.verified(), verify.principal(), verify.signature()),
                (false, 0, None),
                "{asked}"
            );
            let request = Verify::request(asked, auth_message.clone(), sent_proof);
            assert_eq!(
                view.header().command_header(),
                &request.command_header(),
                "the request's fields"
            );
            assert_eq!(
                view.body(),
                request.body().as_slice(),
                "the request's sections"
            );
        }
        let held = ScramVerifier::derive("pw", b"a salt of ann", 4_096)?;
        for reply in &replies {
            for key in [held.stored_key(), held.server_key()] {
                assert!(
                    !reply.windows(32).any(|run| run == key),
                    "a key left the group"
                );
            }
        }
        drop(frontend);
        group.stop().await?;
        Ok(())
    })
}

/// C3.1, C3.10, and the scenario "A held identity is refused whoever holds it", over the frontend's connection: a Session is
/// answered with its own bytes once durable, and sent again refused with Error 1 naming the session.
#[test]
fn test_a_session_is_answered_with_its_bytes() -> TestResult {
    Runtime::new()?.block_on(async {
        let group = served("session", 602).await?;
        let alice = principal(&group.handle, "alice").await?;
        let mut frontend = group.dial("frontend").await?;
        let session = [0x5a; 16];
        let sent = Session::new(session, alice);
        let reply = frontend.request(&sent).await?;
        let view = reply.view()?;
        assert_eq!(view.command(), Command::Session);
        assert_eq!(view.header().command_header(), &sent.command_header());
        assert!(view.header().flags() & FLAG_LAST != 0);
        drop(reply);
        let refused = frontend.request(&sent).await.err().context("a refusal")?;
        assert_eq!(
            refusal_code(&refused),
            Some(ErrorCode::Protocol),
            "{refused}"
        );
        assert!(
            refused
                .to_string()
                .contains(&format!("{:032x}", u128::from_be_bytes(session))),
            "{refused}"
        );
        drop(frontend);
        group.stop().await?;
        Ok(())
    })
}

/// C3.23 to C3.25, and the scenario "A frame of checks is answered whole and in order", over the frontend's connection: one reply
/// holds the outcomes 1, 12 and 21 in order with every other field as sent; a Check whose check holds an outcome is refused with
/// `FieldValue` naming `check` and its index, marking nothing.
#[test]
fn test_a_check_is_answered_whole_over_the_frontends_connection() -> TestResult {
    Runtime::new()?.block_on(async {
        let group = served("check", 603).await?;
        let (alice, bob) = (
            principal(&group.handle, "alice").await?,
            principal(&group.handle, "bob").await?,
        );
        group.clock.advance(100_000);
        heard(group.handle.session(1, alice).await)?;
        heard(group.handle.session(3, bob).await)?;
        let mut frontend = group.dial("frontend").await?;
        let frame = [
            (1, alice, CheckKind::Marks),
            (2, alice, CheckKind::Marks),
            (3, alice, CheckKind::Asks),
        ];
        assert_eq!(checked(&mut frontend, &frame).await?, [1, 12, 21]);
        let answered = SessionCheck::new(3u128.to_be_bytes(), bob, CheckKind::Marks)
            .with_outcome(CheckOutcome::Passed);
        let refused = frontend
            .request(&Check::new(vec![answered]))
            .await
            .err()
            .context("a refusal")?;
        assert_eq!(
            refusal_code(&refused),
            Some(ErrorCode::Protocol),
            "{refused}"
        );
        let rule = ultragraphdb_error_protocol::ProtocolError::field_value(
            Command::Check.number(),
            "check",
            0,
        );
        assert!(refused.to_string().contains(&rule.to_string()), "{refused}");
        assert_eq!(
            checked(&mut frontend, &[(3, bob, CheckKind::Marks)]).await?,
            [1],
            "the refused check marked nothing"
        );
        drop(frontend);
        group.stop().await?;
        Ok(())
    })
}

/// An Ended body the session reads in place, its sessions in the order given, which no builder of the wire writes out of order.
struct UnorderedEnded(Vec<u8>);

impl BodyBuilder for UnorderedEnded {
    const COMMAND: Command = Command::Ended;

    fn command_header(&self) -> [u8; 128] {
        [0; 128]
    }

    fn body(&self) -> Vec<u8> {
        self.0.clone()
    }
}

/// C3.26, C3.29, C3.32, and the scenario "An Ended sent again changes nothing", over the frontend's connection: an Ended takes
/// away its own connection's mark and is answered by no frame, sent again it is passed over, and one its body's rules refuse is
/// dropped unanswered, ending no mark.
#[test]
fn test_an_ended_is_answered_by_nothing() -> TestResult {
    Runtime::new()?.block_on(async {
        let group = served("ended", 604).await?;
        let alice = principal(&group.handle, "alice").await?;
        group.clock.advance(100_000);
        heard(group.handle.session(1, alice).await)?;
        heard(group.handle.session(2, alice).await)?;
        let mut frontend = group.dial("frontend").await?;
        assert_eq!(
            checked(
                &mut frontend,
                &[(1, alice, CheckKind::Marks), (2, alice, CheckKind::Marks)]
            )
            .await?,
            [1, 1]
        );
        // Sessions 2 then 1: the body of an Ended of 1 and 2 with its two records swapped.
        let mut body = Ended::new(vec![1u128.to_be_bytes(), 2u128.to_be_bytes()]).body();
        let at = body.len() - 32;
        let (first, second) = (body[at..at + 16].to_vec(), body[at + 16..].to_vec());
        body[at..at + 16].copy_from_slice(&second);
        body[at + 16..].copy_from_slice(&first);
        frontend.send_only(&UnorderedEnded(body)).await?;
        assert_eq!(watched(&mut frontend).await?, None, "dropped unanswered");
        assert_eq!(
            checked(
                &mut frontend,
                &[(1, alice, CheckKind::Asks), (2, alice, CheckKind::Asks)]
            )
            .await?,
            [14, 14]
        );
        frontend
            .send_only(&Ended::new(vec![1u128.to_be_bytes()]))
            .await?;
        assert_eq!(watched(&mut frontend).await?, None, "no frame answers it");
        frontend
            .send_only(&Ended::new(vec![1u128.to_be_bytes()]))
            .await?;
        assert_eq!(watched(&mut frontend).await?, None, "nor the same again");
        assert_eq!(
            checked(
                &mut frontend,
                &[(1, alice, CheckKind::Asks), (2, alice, CheckKind::Asks)]
            )
            .await?,
            [1, 14]
        );
        drop(frontend);
        group.stop().await?;
        Ok(())
    })
}

/// The scenarios "A declaring transaction's version is built on the version it names" and "A Drop that changes nothing builds no
/// version", over the frontend's connection: a Declare of `kind` 1 is answered once a close assigns its version's epoch with
/// every field as sent and `version` and `commit_version` filled; one of `kind` 2 for a Drop of a predicate never declared is
/// answered at once with the bytes of its request.
#[test]
fn test_a_declare_is_answered_on_its_stream() -> TestResult {
    Runtime::new()?.block_on(async {
        let group = served("declare", 605).await?;
        let alice = principal(&group.handle, "alice").await?;
        let mut frontend = group.dial("frontend").await?;
        let declared = vec![Declared::Predicate(DeclaredPredicate::new(
            "http://ex.org/p",
        ))];
        let sent = Declare::declarations(0, 0, alice, declared);
        let (reply, closed) = futures::join!(frontend.request(&sent), async {
            // The Declare reaches the group over the connection: once its version is pending, one close assigns its epoch.
            let before = heard(group.handle.latest_close().await)?;
            version_current(&group, 1).await?;
            closed_after(&group.handle, &group.clock, before).await
        });
        let _closed = closed?;
        let reply = reply?;
        let view = reply.view()?;
        let declare = DeclareView::from_frame(&view)?;
        assert_eq!(
            (
                declare.version(),
                declare.commit_version() & ((1 << 22) - 1)
            ),
            (1, 0)
        );
        assert!(declare.commit_version() > 0);
        let expected = sent.clone().with_version(1, declare.commit_version());
        assert_eq!(
            view.header().command_header(),
            &expected.command_header(),
            "every field as sent"
        );
        assert_eq!(
            view.body(),
            expected.body().as_slice(),
            "the declarations as sent"
        );
        drop(reply);
        let never = Declare::drop(
            0,
            alice,
            DropKind::Predicate,
            Some("http://ex.org/never".to_string()),
        );
        let reply = frontend.request(&never).await?;
        let view = reply.view()?;
        assert_eq!(
            view.header().command_header(),
            &never.command_header(),
            "the bytes of its request"
        );
        assert_eq!(view.body(), never.body().as_slice());
        drop(reply);
        drop(frontend);
        group.stop().await?;
        Ok(())
    })
}

/// C3.70, L3.30: a Defaults over the frontend's connection at the floor of `max_frame` answers every recorded predicate in the
/// order of their texts' bytes, across as many frames as they take, the last alone flagged `LAST`; a tenant holding none answers
/// one frame flagged `LAST`.
#[test]
fn test_defaults_continue_across_frames() -> TestResult {
    Runtime::new()?.block_on(async {
        let group = served("defaults", 606).await?;
        let mut predicates: Vec<String> = (0..150)
            .map(|index| format!("http://ex.org/{index:03}/{}", "d".repeat(3_900)))
            .collect();
        for predicate in predicates.iter().rev() {
            assert!(!heard(
                group.handle.record_default(0, predicate.clone()).await
            )?);
        }
        predicates.sort();
        let mut frontend = group.dial_taking("frontend", MAX_FRAME_FLOOR_U32).await?;
        let (read, frames) = defaults_read(&mut frontend, 0).await?;
        assert!(frames >= 2, "{frames} frames");
        assert_eq!(read.predicates(), predicates.as_slice());
        assert!(read.position().context("a position")? >= 1);
        drop(frontend);
        group.stop().await?;
        Ok(())
    })
}

/// A Defaults answer read across its frames on `client`, granting credit as each arrives: the answer and the frames it took.
async fn defaults_read(
    client: &mut RoleClient,
    tenant: u64,
) -> TestResult<(DefaultsFrames, usize)> {
    client.send_only(&Defaults::request(tenant)).await?;
    let stream = client.last_stream();
    let mut frames = DefaultsFrames::new();
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
