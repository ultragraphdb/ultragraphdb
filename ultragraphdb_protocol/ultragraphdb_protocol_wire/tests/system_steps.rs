/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The wire-protocol scenarios of stage 5.8, `features/wire-protocol/system_group.feature`: each frame laid out by
//! hand in `utils/system_frames.rs` or read from the golden frames, and refused or read by its view.

use cucumber::{then, when};
use ultragraphdb_error_protocol::ProtocolErrorEnum;
use ultragraphdb_protocol_wire::{
    BodyBuilder, BodyView, Challenge, ChallengeView, Check, CheckKind, CheckOutcome, CheckView,
    Command, Declare, DeclareView, Defaults, DefaultsFrames, DefaultsView, DropKind, Ended,
    EndedView, FLAG_LAST, FrameView, Header, MAX_FRAME_SIZE, RoleKind, Session, SessionCheck,
    SessionView, SubscribeView, Verify, VerifyView,
};
use ultragraphdb_tool_bdd::{Context, TestResult};

use crate::Steps;
use crate::utils::golden::{CREDIT, put, reseal};
use crate::utils::golden_bodies::{CLUSTER, frame_of, view_any, with_command_header};
use crate::utils::system_frames::{
    DeclareFields, SESSION_A, SESSION_B, SESSION_C, check_frame, check_record, count,
    declare_frame, drop_sections, ended_frame, entity_type_sections, field, role, session_frame,
};
use crate::utils::term_records::refusal;

/// What the stage 5.8 scenarios carry from one step to the next.
#[derive(Debug, Default)]
pub struct SystemSteps {
    refusals: Vec<ProtocolErrorEnum>,
    frames: Vec<Vec<u8>>,
    predicates: Vec<String>,
    rebuilt: Vec<(String, bool)>,
}

impl SystemSteps {
    /// The refusals the scenario's `When` recorded.
    pub fn refusals(&self) -> &[ProtocolErrorEnum] {
        &self.refusals
    }
}

/// Two checks of `first` and `second`, each for principal 9, of kind 1, with no outcome.
fn two_checks(first: [u8; 16], second: [u8; 16]) -> Vec<u8> {
    let mut section = check_record(first, 9, 1, 0);
    section.extend(check_record(second, 9, 1, 0));
    section
}

#[when("a Check holds two checks whose sessions are equal, or the second below the first")]
fn check_sessions_do_not_ascend(w: &mut Steps) -> TestResult {
    let equal = check_frame(two_checks(SESSION_A, SESSION_A))?;
    let below = check_frame(two_checks(SESSION_B, SESSION_A))?;
    w.system.refusals = vec![refusal::<CheckView>(&equal)?, refusal::<CheckView>(&below)?];
    Ok(())
}

#[then("it is refused with `FieldValue` naming `check` and the second check's index")]
fn refused_naming_the_second_check(w: &mut Steps) -> TestResult {
    assert_eq!(
        w.system.refusals,
        [field(535, "check", 1), field(535, "check", 1)]
    );
    Ok(())
}

#[when("a Check request holds an outcome other than 0, or a reply an outcome of 0 or 7")]
fn check_outcomes_out_of_place(w: &mut Steps) -> TestResult {
    let sent = [
        SessionCheck::new(SESSION_A, 9, CheckKind::Marks),
        SessionCheck::new(SESSION_B, 9, CheckKind::Asks),
    ];
    // A request whose second check holds outcome 1, as the system group, which receives it, takes it.
    let mut answered_request = check_record(SESSION_A, 9, 2, 0);
    answered_request.extend(check_record(SESSION_B, 9, 1, 1));
    let request = check_frame(answered_request)?;
    let as_request = CheckView::from_frame(&frame_of(&request)?)?
        .check_request()
        .err()
        .context("the request is refused")?;
    // A reply whose second check holds outcome 0, as the requester takes it.
    let mut unanswered_reply = check_record(SESSION_A, 9, 2, 1);
    unanswered_reply.extend(check_record(SESSION_B, 9, 1, 0));
    let reply = check_frame(unanswered_reply)?;
    let as_reply = CheckView::from_frame(&frame_of(&reply)?)?
        .check_reply(&sent)
        .err()
        .context("the reply is refused")?;
    // A reply whose second check holds outcome 7, which no Check holds.
    let mut outside = check_record(SESSION_A, 9, 2, 1);
    outside.extend(check_record(SESSION_B, 9, 1, 7));
    w.system.refusals = vec![
        as_request.0,
        as_reply.0,
        refusal::<CheckView>(&check_frame(outside)?)?,
    ];
    Ok(())
}

#[then("each is refused with `FieldValue` naming `check` and the check's index")]
fn each_refused_naming_its_check(w: &mut Steps) -> TestResult {
    assert_eq!(w.system.refusals, vec![field(535, "check", 1); 3]);
    Ok(())
}

#[when(
    "a Subscribe holds `latest` 1 with `from` 5, or `latest` 1 on a connection whose role is `storage`"
)]
fn subscribe_latest_out_of_place(w: &mut Steps) -> TestResult {
    let golden = role("subscribe")?;
    let with_from = with_command_header(&golden, |h| {
        put(h, 0, 5, 8);
        put(h, 8, 1, 1);
    });
    let from_none = with_command_header(&golden, |h| {
        put(h, 0, 0, 8);
        put(h, 8, 1, 1);
    });
    let on_storage = SubscribeView::from_frame(&frame_of(&from_none)?)?
        .check_sender(RoleKind::Storage)
        .err()
        .context("a storage connection's Subscribe is refused")?;
    w.system.refusals = vec![refusal::<SubscribeView>(&with_from)?, on_storage.0];
    Ok(())
}

#[then("each is refused with `FieldValue` naming `latest`")]
fn each_refused_naming_latest(w: &mut Steps) -> TestResult {
    assert_eq!(w.system.refusals, vec![field(514, "latest", 1); 2]);
    Ok(())
}

#[when("a Catalog request names `version` 3 and `epoch` 9")]
fn catalog_request_names_both(w: &mut Steps) -> TestResult {
    let bytes = with_command_header(&role("catalog")?, |h| {
        put(h, 8, 3, 8);
        put(h, 16, 9, 8);
    });
    w.system.refusals = vec![refusal::<ultragraphdb_protocol_wire::CatalogView>(&bytes)?];
    Ok(())
}

// The refusal is asserted by `ugs_steps::catalog_refused`: stage 6.6's scenario of a Drop's `catalog` ends in the same
// words, and one step answers both.

#[when("an Ended holds an empty section 0, or a section of 24 bytes")]
fn ended_sections_broken(w: &mut Steps) -> TestResult {
    let mut one_and_a_half = SESSION_A.to_vec();
    one_and_a_half.extend_from_slice(&SESSION_C[..8]);
    w.system.refusals = vec![
        refusal::<EndedView>(&ended_frame(Vec::new())?)?,
        refusal::<EndedView>(&ended_frame(one_and_a_half)?)?,
    ];
    Ok(())
}

#[then(
    "the first is refused with `CountMismatch` naming `sessions`, the second with `SectionLength`"
)]
fn ended_refusals(w: &mut Steps) -> TestResult {
    assert_eq!(
        w.system.refusals,
        [
            count(536, "sessions", 0, 1),
            ProtocolErrorEnum::SectionLength {
                command: 536,
                section: 0,
                len: 24,
                unit: 16
            }
        ]
    );
    Ok(())
}

#[when("a Session's `session` is all zero")]
fn session_of_all_zero(w: &mut Steps) -> TestResult {
    w.system.refusals = vec![refusal::<SessionView>(&session_frame([0; 16], 9)?)?];
    Ok(())
}

#[then("it is refused with `FieldValue` naming `session`")]
fn refused_naming_session(w: &mut Steps) -> TestResult {
    assert_eq!(w.system.refusals, [field(534, "session", 0)]);
    Ok(())
}

/// The fields of a Declare of `kind` 2 for a Drop of kind `drop`, in tenant 3, for principal 9.
fn dropping(drop: u8) -> DeclareFields {
    DeclareFields {
        kind: 2,
        drop,
        tenant: 3,
        principal: 9,
        ..DeclareFields::default()
    }
}

#[when("a Declare of kind 2 holds a record in section 0, or names `drop` 1 with no term")]
fn declare_drop_broken(w: &mut Steps) -> TestResult {
    let with_record = declare_frame(dropping(3), &entity_type_sections(&["urn:a"]))?;
    let with_no_term = declare_frame(dropping(1), &drop_sections(&[]))?;
    w.system.refusals = vec![
        refusal::<DeclareView>(&with_record)?,
        refusal::<DeclareView>(&with_no_term)?,
    ];
    Ok(())
}

#[then(
    "the first is refused with `CountMismatch` naming `declarations`, the second with `CountMismatch` naming `drop terms`"
)]
fn declare_drop_refusals(w: &mut Steps) -> TestResult {
    assert_eq!(
        w.system.refusals,
        [
            count(538, "declarations", 1, 0),
            count(538, "drop terms", 0, 1)
        ]
    );
    Ok(())
}

#[when("a Declare of kind 1 holds no record in section 0")]
fn declare_of_no_record(w: &mut Steps) -> TestResult {
    let fields = DeclareFields {
        kind: 1,
        tenant: 3,
        built_on: 11,
        principal: 9,
        ..DeclareFields::default()
    };
    let bytes = declare_frame(fields, &drop_sections(&[]))?;
    w.system.refusals = vec![refusal::<DeclareView>(&bytes)?];
    Ok(())
}

#[then("it is refused with `CountMismatch` naming `declarations`")]
fn refused_naming_declarations(w: &mut Steps) -> TestResult {
    assert_eq!(w.system.refusals, [count(538, "declarations", 0, 1)]);
    Ok(())
}

#[when("a tenant's default-entry predicates fill two Defaults frames")]
fn defaults_fill_two_frames(w: &mut Steps) -> TestResult {
    // Three predicates of 5 bytes, 29 bytes of term table each: a frame of 256 + 16 + 2 × 29 bytes holds two.
    w.system.predicates = ["urn:B", "urn:a", "urn:ab"]
        .iter()
        .map(|text| (*text).to_string())
        .collect();
    let reply = Defaults::reply(3, 7, w.system.predicates.clone());
    w.system.frames = reply.frames(
        Header::new(CLUSTER, Command::Defaults).with_stream(3),
        256 + 16 + 2 * 29,
    )?;
    Ok(())
}

#[then(
    "the first carries no `LAST` and the second does, both hold one command header, and the predicates ascend by their texts' bytes across both"
)]
fn defaults_continue(w: &mut Steps) -> TestResult {
    let [first, second] = w.system.frames.as_slice() else {
        return Err(ultragraphdb_tool_bdd::TestError::new(format!(
            "{} frames",
            w.system.frames.len()
        )));
    };
    let (first_view, second_view) = (frame_of(first)?, frame_of(second)?);
    assert_eq!(first_view.header().flags() & FLAG_LAST, 0);
    assert_eq!(second_view.header().flags() & FLAG_LAST, FLAG_LAST);
    assert_eq!(
        first_view.header().command_header(),
        second_view.header().command_header()
    );
    let mut read = Vec::new();
    for view in [&first_view, &second_view] {
        let part = DefaultsView::from_frame(view)?;
        for index in 0..part.len() {
            read.push(part.predicate(index).context("a predicate")?.to_string());
        }
    }
    assert_eq!(read, w.system.predicates);
    assert!(
        read.windows(2)
            .all(|pair| pair[0].as_bytes() < pair[1].as_bytes())
    );
    let mut taken = DefaultsFrames::new();
    taken.push(&first_view)?;
    assert!(!taken.is_last());
    taken.push(&second_view)?;
    assert!(taken.is_last());
    assert_eq!(taken.predicates(), w.system.predicates.as_slice());
    Ok(())
}

#[when(
    "the golden frames of Session, Check, Ended, Declare, Defaults, Challenge and Verify are decoded and built again"
)]
fn goldens_decoded_and_built(w: &mut Steps) -> TestResult {
    let names = [
        "session",
        "check",
        "check_reply",
        "ended",
        "declare",
        "declare_reply",
        "declare_drop",
        "defaults",
        "defaults_reply",
        "challenge",
        "challenge_reply",
        "verify",
        "verify_reply",
    ];
    w.system.frames = names
        .iter()
        .map(|name| role(name))
        .collect::<TestResult<_>>()?;
    w.system.rebuilt = names
        .iter()
        .zip(&w.system.frames)
        .map(|(name, bytes)| -> TestResult<(String, bool)> {
            Ok(((*name).to_string(), rebuilt(bytes)? == *bytes))
        })
        .collect::<TestResult<_>>()?;
    Ok(())
}

/// The frame `bytes` read through its command's view and built again from the owned body under its own header.
fn rebuilt(bytes: &[u8]) -> TestResult<Vec<u8>> {
    let frame = frame_of(bytes)?;
    let h = frame.header();
    let header = Header::new(h.cluster(), frame.command())
        .with_sequence(h.sequence())
        .with_release(h.release())
        .with_stream(h.stream())
        .with_flags(h.flags());
    let command = frame.command();
    let built = if command == Command::Session {
        SessionView::from_frame(&frame)?.to_body()?.frame(header)
    } else if command == Command::Check {
        CheckView::from_frame(&frame)?.to_body()?.frame(header)
    } else if command == Command::Ended {
        EndedView::from_frame(&frame)?.to_body()?.frame(header)
    } else if command == Command::Declare {
        DeclareView::from_frame(&frame)?.to_body()?.frame(header)
    } else if command == Command::Defaults {
        DefaultsView::from_frame(&frame)?.to_body()?.frame(header)
    } else if command == Command::Challenge {
        ChallengeView::from_frame(&frame)?.to_body()?.frame(header)
    } else if command == Command::Verify {
        VerifyView::from_frame(&frame)?.to_body()?.frame(header)
    } else {
        return Err(ultragraphdb_tool_bdd::TestError::new(format!(
            "a golden frame of {command:?}"
        )));
    };
    Ok(built?)
}

#[then("each decodes to the fields its generator names and builds to the same bytes")]
fn goldens_hold_their_fields(w: &mut Steps) -> TestResult {
    assert_eq!(w.system.rebuilt.len(), 13);
    for (name, same) in &w.system.rebuilt {
        assert!(same, "{name} builds to other bytes");
    }
    for bytes in &w.system.frames {
        assert_eq!(view_any(bytes)?, Ok(()));
    }
    // The fields `generate_roles.py` names for each frame, built here from their values.
    session_goldens_hold()?;
    declare_goldens_hold()?;
    login_goldens_hold()
}

/// The golden Session, Check, check reply and Ended frames hold the fields `generate_roles.py` names.
fn session_goldens_hold() -> TestResult {
    let checks = vec![
        SessionCheck::new(SESSION_A, 9, CheckKind::Marks),
        SessionCheck::new(SESSION_B, 9, CheckKind::Asks),
        SessionCheck::new(SESSION_C, 11, CheckKind::Marks),
    ];
    let outcomes = [
        CheckOutcome::Passed,
        CheckOutcome::NotHeld,
        CheckOutcome::OtherPrincipal,
    ];
    let answered: Vec<SessionCheck> = checks
        .iter()
        .zip(outcomes)
        .map(|(check, outcome)| check.with_outcome(outcome))
        .collect();
    assert_eq!(
        body_of::<SessionView>(&role("session")?)?,
        Session::new(SESSION_A, 9)
    );
    assert_eq!(body_of::<CheckView>(&role("check")?)?, Check::new(checks));
    assert_eq!(
        body_of::<CheckView>(&role("check_reply")?)?,
        Check::new(answered)
    );
    assert_eq!(
        body_of::<EndedView>(&role("ended")?)?,
        Ended::new(vec![SESSION_A, SESSION_C])
    );
    Ok(())
}

/// The golden Declare and Defaults frames hold the fields `generate_roles.py` names.
fn declare_goldens_hold() -> TestResult {
    let declare = body_of::<DeclareView>(&role("declare_reply")?)?;
    assert_eq!(
        (declare.tenant(), declare.built_on(), declare.principal()),
        (3, 11, 9)
    );
    assert_eq!(
        (
            declare.version(),
            declare.commit_version(),
            declare.declared().len()
        ),
        (12, 45 << 22, 3)
    );
    assert_eq!(
        body_of::<DeclareView>(&role("declare_drop")?)?,
        Declare::drop(
            3,
            9,
            DropKind::Predicate,
            Some("http://ex.org/age".to_string())
        )
    );
    assert_eq!(
        body_of::<DefaultsView>(&role("defaults")?)?,
        Defaults::request(3)
    );
    let predicates = ["Zeta", "age", "name"]
        .iter()
        .map(|name| format!("http://ex.org/{name}"))
        .collect();
    assert_eq!(
        body_of::<DefaultsView>(&role("defaults_reply")?)?,
        Defaults::reply(3, 1_250, predicates)
    );
    Ok(())
}

/// The golden Challenge and Verify frames hold the fields `generate_roles.py` names.
fn login_goldens_hold() -> TestResult {
    assert_eq!(
        body_of::<ChallengeView>(&role("challenge")?)?,
        Challenge::request("user")
    );
    let verify: Verify = body_of::<VerifyView>(&role("verify_reply")?)?;
    assert_eq!(
        (verify.name(), verify.verified(), verify.principal()),
        ("user", true, 7)
    );
    assert!(!body_of::<VerifyView>(&role("verify")?)?.verified());
    Ok(())
}

/// The owned body the view `V` reads `bytes` back to.
fn body_of<'a, V: BodyView<'a>>(bytes: &'a [u8]) -> TestResult<V::Body> {
    Ok(V::from_frame(&frame_of(bytes)?)?.to_body()?)
}

#[when("a role connection sends command 540")]
fn command_540(w: &mut Steps) -> TestResult {
    let number: u16 = 540;
    let mut bytes = CREDIT.bytes()?;
    put(&mut bytes, 54, number.into(), 2);
    reseal(&mut bytes);
    w.refusals = vec![
        FrameView::validate(&bytes, CLUSTER, MAX_FRAME_SIZE)
            .err()
            .context("command 540 is refused")?,
    ];
    w.checksums = vec![(number.into(), 0)];
    Ok(())
}
