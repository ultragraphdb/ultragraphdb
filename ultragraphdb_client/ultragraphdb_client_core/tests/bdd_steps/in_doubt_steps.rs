/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The steps of "A commit in flight is in doubt once sent" and "Exactly-once commits through sessions".

use cucumber::{then, when};

use ultragraphdb_client_core::{ClientError, ClientErrorEnum, Reply};
use ultragraphdb_protocol_wire::{
    BodyView, Command, DirectWrite, ErrorCode, FrameView, HeaderView, MAX_FRAME_SIZE, TermTable,
    WriteChange,
};
use ultragraphdb_tool_bdd::{Context as _, TestResult};

use crate::utils::harness::{CLUSTER, Harness, committed, server};
use crate::{Steps, begin, commits, session};

/// The refusal an error names, its code and detail, or the one that left it in doubt.
fn refused(error: &ClientError) -> Option<(ErrorCode, u64)> {
    if let ClientErrorEnum::Refused { code, detail, .. } = &error.0 {
        return Some((*code, *detail));
    }
    if let ClientErrorEnum::InDoubt {
        cause: Some(cause), ..
    } = &error.0
    {
        return refused(cause);
    }
    None
}

/// A DirectWrite of one insert.
pub(crate) fn insert() -> DirectWrite {
    let mut terms = TermTable::new();
    let a = terms.iri("http://ex.org/a");
    let knows = terms.iri("http://ex.org/knows");
    let b = terms.iri("http://ex.org/b");
    DirectWrite::new(
        0,
        terms,
        vec![WriteChange::Insert {
            graph: None,
            subject: a,
            predicate: knows,
            object: b,
        }],
    )
}

/// The commands of the frames the server received, each with its stream.
pub(crate) fn frames(h: &Harness) -> Vec<(u16, u64)> {
    h.server
        .received()
        .iter()
        .filter_map(|f| HeaderView::new(f).ok())
        .map(|header| (header.command(), header.stream()))
        .collect()
}

/// How many frames of `command` the server received.
pub(crate) fn count(h: &Harness, command: Command) -> usize {
    frames(h)
        .iter()
        .filter(|(number, _)| *number == command.number())
        .count()
}

#[when(
    "the connection closes once with a commit queued and unsent, and once with a commit sent and unanswered"
)]
fn closes_twice(w: &mut Steps) -> TestResult {
    let mut h = Harness::ready(server())?;
    let session = session(&mut h)?;
    let tx = begin(&mut h, false)?;
    let unsent = h.core.commit(tx, session, h.now)?;
    h.core.close();
    w.errors.push(
        h.core
            .take_reply(unsent)
            .context("an answer")?
            .err()
            .context("an error")?,
    );
    assert_eq!(h.core.next_request(session), Some(1), "the number stays");
    h.connect()?;
    h.until(|h| h.core.is_ready())?;
    let tx2 = begin(&mut h, false)?;
    let sent = h.core.commit(tx2, session, h.now)?;
    h.blackhole = true;
    let _moved: bool = h.step()?;
    h.core.close();
    w.errors.push(
        h.core
            .take_reply(sent)
            .context("an answer")?
            .err()
            .context("an error")?,
    );
    h.blackhole = false;
    h.connect()?;
    h.until(|h| h.core.is_ready())?;
    let resolving = h.core.resolve(session, h.now)?;
    w.answers.push(committed(h.settle(resolving)?)?);
    w.harness = Some(h);
    w.session = Some(session);
    Ok(())
}

#[then(
    "the first answers `ConnectionLost`, the second `InDoubt`, and resolving the second reports whether it committed"
)]
fn lost_then_in_doubt(w: &mut Steps) -> TestResult {
    let session = w.session.context("a session")?;
    let first = w.errors.first().context("the first answer")?;
    assert!(
        matches!(first.0, ClientErrorEnum::ConnectionLost { .. }),
        "{first:?}"
    );
    let second = w.errors.get(1).context("the second answer")?;
    assert!(
        matches!(
            second.0,
            ClientErrorEnum::InDoubt { session: s, request: 1, .. } if s == session.number()
        ),
        "{second:?}"
    );
    let h = w.harness.as_ref().context("a harness")?;
    let answer = w.answers.first().context("the resolved answer")?;
    assert_eq!(answer.version(), h.server.version());
    assert_eq!(h.server.applied(), 1, "it committed once");
    assert_eq!(h.core.next_request(session), Some(2));
    Ok(())
}

#[when("a client's Commit through a session is answered code 17")]
fn commit_answered_17(w: &mut Steps) -> TestResult {
    let mut h = Harness::ready(server())?;
    let session = session(&mut h)?;
    h.server.refuse_next_commit(ErrorCode::Unavailable, 0);
    let tx = begin(&mut h, false)?;
    let p = h.core.commit(tx, session, h.now)?;
    w.errors.push(h.settle(p)?.err().context("an error")?);
    let next = begin(&mut h, false)?;
    w.errors.push(
        h.core
            .commit(next, session, h.now)
            .err()
            .context("refused")?,
    );
    w.harness = Some(h);
    w.session = Some(session);
    Ok(())
}

#[then(
    "the client answers `InDoubt`, keeps the request number, and sends no other commit through the session until a resend resolves it"
)]
fn in_doubt_keeps_the_number(w: &mut Steps) -> TestResult {
    let session = w.session.context("a session")?;
    let h = w.harness.as_ref().context("a harness")?;
    let answer = w.errors.first().context("the answer")?;
    assert!(
        matches!(answer.0, ClientErrorEnum::InDoubt { request: 1, .. }),
        "{answer:?}"
    );
    assert_eq!(
        refused(answer),
        Some((ErrorCode::Unavailable, 0)),
        "its cause"
    );
    let next = w.errors.get(1).context("the next commit")?;
    assert!(
        matches!(next.0, ClientErrorEnum::InDoubt { request: 1, .. }),
        "{next:?}"
    );
    assert_eq!(h.core.next_request(session), Some(1));
    assert_eq!(commits(h).len(), 1, "no other commit went out");
    Ok(())
}

#[when("a DirectWrite in doubt is resent and refused with code 7")]
fn resent_refused(w: &mut Steps) -> TestResult {
    let mut h = Harness::ready(server())?;
    let session = session(&mut h)?;
    h.server.refuse_next_commit(ErrorCode::Unavailable, 0);
    h.server.refuse_next_commit(ErrorCode::Refused, 0);
    let p = h.core.direct_write(insert(), session, h.now)?;
    w.errors.push(h.settle(p)?.err().context("in doubt")?);
    let resend = h.core.resolve(session, h.now)?;
    w.errors.push(h.settle(resend)?.err().context("refused")?);
    w.harness = Some(h);
    w.session = Some(session);
    Ok(())
}

#[then("the client reports that it did not commit")]
fn did_not_commit(w: &mut Steps) -> TestResult {
    let session = w.session.context("a session")?;
    let answer = w.errors.get(1).context("the resend's answer")?;
    assert!(
        matches!(
            answer.0,
            ClientErrorEnum::Refused {
                code: ErrorCode::Refused,
                ..
            }
        ),
        "{answer:?}"
    );
    let h = w.harness.as_mut().context("a harness")?;
    assert_eq!(h.server.applied(), 0);
    assert_eq!(
        h.core.next_request(session),
        Some(1),
        "the number is reused"
    );
    let again = h.core.direct_write(insert(), session, h.now)?;
    assert!(
        matches!(h.settle(again)?, Ok(Reply::Committed(_))),
        "no doubt remains"
    );
    Ok(())
}

#[when("a transaction committing through no session loses its reply")]
fn sessionless_lost(w: &mut Steps) -> TestResult {
    let mut h = Harness::ready(server())?;
    let tx = begin(&mut h, false)?;
    h.server.drop_after_next_commit();
    let p = h.core.commit(tx, None, h.now)?;
    w.errors.push(h.settle(p)?.err().context("an error")?);
    w.harness = Some(h);
    Ok(())
}

#[then("the client reports the outcome as unknown, and resends nothing")]
fn unknown_nothing_resent(w: &mut Steps) -> TestResult {
    let answer = w.errors.first().context("the answer")?;
    assert!(
        matches!(answer.0, ClientErrorEnum::OutcomeUnknown { .. }),
        "{answer:?}"
    );
    let h = w.harness.as_ref().context("a harness")?;
    assert_eq!(commits(h).len(), 1, "one Commit, never resent");
    assert!(!h.core.needs_connection());
    Ok(())
}

#[when("a client commits request 7 of its session, loses the reply, and sends request 7 again")]
fn request_seven(w: &mut Steps) -> TestResult {
    let mut h = Harness::ready(server())?;
    let session = session(&mut h)?;
    let session = h.core.resume_session(session.number(), 7)?;
    let tx = begin(&mut h, false)?;
    h.server.drop_after_next_commit();
    let p = h.core.commit(tx, session, h.now)?;
    w.answers.push(committed(h.settle(p)?)?);
    w.harness = Some(h);
    w.session = Some(session);
    Ok(())
}

#[then("it receives the same reply and commit version, and the transaction's writes are held once")]
fn same_reply(w: &mut Steps) -> TestResult {
    let session = w.session.context("a session")?;
    let h = w.harness.as_ref().context("a harness")?;
    let answer = w.answers.first().context("an answer")?;
    assert_eq!(answer.request(), 7);
    assert_eq!(answer.version(), h.server.version(), "the version applied");
    assert_eq!(h.server.applied(), 1);
    assert_eq!(
        commits(h),
        vec![(session.number(), 7), (session.number(), 7)]
    );
    Ok(())
}

#[when("a Commit through a session is answered code 17, and the partition had committed it")]
fn committed_answered_17(w: &mut Steps) -> TestResult {
    let mut h = Harness::ready(server())?;
    let session = session(&mut h)?;
    let tx = begin(&mut h, false)?;
    h.server
        .apply_and_refuse_next_commit(ErrorCode::Unavailable, 0);
    let p = h.core.commit(tx, session, h.now)?;
    w.errors.push(h.settle(p)?.err().context("in doubt")?);
    w.numbers
        .push(h.core.next_request(session).context("a counter")?);
    let resend = h.core.resolve(session, h.now)?;
    w.answers.push(committed(h.settle(resend)?)?);
    w.numbers
        .push(h.core.next_request(session).context("a counter")?);
    w.numbers.push(tx.stream());
    w.harness = Some(h);
    Ok(())
}

#[then(
    "the client keeps the request number, sends the Commit again on a new stream, receives the stored reply and only then moves its counter"
)]
fn resolved_by_resend(w: &mut Steps) -> TestResult {
    let h = w.harness.as_ref().context("a harness")?;
    assert_eq!(w.numbers.first(), Some(&1), "kept while in doubt");
    assert_eq!(w.numbers.get(1), Some(&2), "moved by the stored reply");
    let streams: Vec<u64> = frames(h)
        .iter()
        .filter(|(command, _)| *command == Command::Commit.number())
        .map(|(_, stream)| *stream)
        .collect();
    assert_eq!(streams.len(), 2);
    assert_eq!(
        streams.first(),
        w.numbers.get(2),
        "first on its transaction's stream"
    );
    assert!(streams.get(1) != streams.first(), "again on a new stream");
    let answer = w.answers.first().context("the stored reply")?;
    assert_eq!(answer.version(), h.server.version());
    assert_eq!(h.server.applied(), 1, "applied once");
    Ok(())
}

#[when(
    "a retried Commit is recalled over two partitions, partition 1 answers 18 and the role connection to partition 2 closes under the Recall"
)]
fn recall_reaches_no_answer(w: &mut Steps) -> TestResult {
    let mut h = Harness::ready(server())?;
    let session = session(&mut h)?;
    let tx = begin(&mut h, false)?;
    h.server.drop_after_next_commit();
    let p = h.core.commit(tx, session, h.now)?;
    h.until(|h| h.connections == 2)?;
    // The frontend's answer to the retry, which the test server plays: code 17, then the stored reply.
    h.server.refuse_next_commit(ErrorCode::Unavailable, 0);
    h.until(|h| commits(h).len() == 2)?;
    let at = h.now;
    w.answers.push(committed(h.settle(p)?)?);
    w.numbers.push(u64::try_from((h.now - at).as_nanos())?);
    w.harness = Some(h);
    Ok(())
}

#[then("the frontend answers code 17, and the client resends after a delay")]
fn resent_after_a_delay(w: &mut Steps) -> TestResult {
    let h = w.harness.as_ref().context("a harness")?;
    let delay = u64::try_from(h.core.config().retry_delay().as_nanos())?;
    assert!(
        w.numbers.first().is_some_and(|waited| *waited >= delay),
        "{:?}",
        w.numbers
    );
    assert_eq!(
        commits(h).len(),
        3,
        "the Commit, the retry, the retry after the delay"
    );
    assert_eq!(h.server.applied(), 1);
    Ok(())
}

#[when(
    "a retried Commit is recalled, every partition answers 18, and the registry has evicted the session meanwhile"
)]
fn evicted_meanwhile(w: &mut Steps) -> TestResult {
    let mut h = Harness::ready(server())?;
    let session = session(&mut h)?;
    let tx = begin(&mut h, false)?;
    h.server.drop_after_next_commit();
    let p = h.core.commit(tx, session, h.now)?;
    h.until(|h| h.connections == 2)?;
    h.server.refuse_next_commit(ErrorCode::SessionEvicted, 0);
    w.errors.push(h.settle(p)?.err().context("an error")?);
    w.harness = Some(h);
    w.session = Some(session);
    Ok(())
}

#[then("the frontend answers code 12, and the client reports the outcome as unknown")]
fn evicted_unknown(w: &mut Steps) -> TestResult {
    let session = w.session.context("a session")?;
    let answer = w.errors.first().context("the answer")?;
    assert!(
        matches!(answer.0, ClientErrorEnum::OutcomeUnknown { .. }),
        "{answer:?}"
    );
    let h = w.harness.as_ref().context("a harness")?;
    assert_eq!(h.core.next_request(session), None, "the session is gone");
    Ok(())
}

#[when(
    "a DirectWrite through a session is answered code 17, the partition had not committed it, and the client sends it again"
)]
fn direct_write_resent(w: &mut Steps) -> TestResult {
    let mut h = Harness::ready(server())?;
    let session = session(&mut h)?;
    h.server.refuse_next_commit(ErrorCode::Unavailable, 0);
    let p = h.core.direct_write(insert(), session, h.now)?;
    w.errors.push(h.settle(p)?.err().context("in doubt")?);
    let resend = h.core.resolve(session, h.now)?;
    w.answers.push(committed(h.settle(resend)?)?);
    w.harness = Some(h);
    Ok(())
}

#[then(
    "the frontend relays it, not recalls it, the partition commits it, and the reply carries its commit version"
)]
fn relayed_and_committed(w: &mut Steps) -> TestResult {
    let h = w.harness.as_ref().context("a harness")?;
    assert_eq!(count(h, Command::DirectWrite), 2, "the frame sent again");
    assert_eq!(count(h, Command::Commit), 0, "nothing recalled");
    let last = h.server.received().last().context("a frame")?;
    let view = FrameView::validate(last, CLUSTER, MAX_FRAME_SIZE)?;
    let first = h
        .server
        .received()
        .iter()
        .find(|f| HeaderView::new(f).is_ok_and(|v| v.command() == Command::DirectWrite.number()))
        .context("the first send")?;
    let first = FrameView::validate(first, CLUSTER, MAX_FRAME_SIZE)?;
    assert_eq!(view.body(), first.body(), "the same changes");
    assert_eq!(
        ultragraphdb_protocol_wire::DirectWriteView::from_frame(&view)?.request(),
        1,
        "the same number"
    );
    let answer = w.answers.first().context("the reply")?;
    assert_eq!(answer.version(), h.server.version());
    assert_eq!(h.server.applied(), 1);
    Ok(())
}
