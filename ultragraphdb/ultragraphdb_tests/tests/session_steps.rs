/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! Exactly-once through sessions, with a real network's losses: a [`LossyLink`] between a real client and a real frontend takes
//! a request or a reply, and the client resends what it holds in doubt. The reply a link lost is kept, so a resend's answer is
//! compared with the bytes the frontend first sent.

use cucumber::{then, when};
use ultragraphdb_client_compio::{ClientError, ClientErrorEnum, Committed};
use ultragraphdb_protocol_wire::{
    BodyView, Command, CommitView, DropCommand, DropCommandView, ErrorCode, FrameView,
    MAX_FRAME_SIZE, RequestTerm, TermTable, WriteChange,
};
use ultragraphdb_tests::CLUSTER;
use ultragraphdb_tool_bdd::{Context, TestError, TestResult};

use crate::Steps;
use crate::client_reads::{
    GraphAt, Pattern, TENANT, batch_of, direct_of, iri, refusal_of, string, write_of,
};
use crate::cluster::{iri_of_spread, shared_client, spread_client};
use crate::lossy_link::LossyLink;
use crate::read_steps::in_graph;
use crate::request_steps::latest_read;
use crate::shells::{Knobs, TestClient, borrowed, keep, run};

/// A client of the link, one attempt at each commit, so a lost reply is the caller's to resolve.
async fn linked(link: &LossyLink) -> TestResult<TestClient> {
    let mut client = TestClient::new(&link.client_at(), &Knobs::one_attempt())?;
    client.connect().await?;
    Ok(client)
}

/// The commit version and request of the Commit reply `frame`, as the frontend sent it.
fn sent_commit(frame: &[u8]) -> TestResult<(u64, u64)> {
    let view = FrameView::validate(frame, CLUSTER, MAX_FRAME_SIZE)?;
    let reply = CommitView::from_frame(&view)?.to_body()?;
    Ok((reply.commit_version(), reply.request()))
}

/// That `answer` left request `request` of `session` in doubt.
fn in_doubt(answer: Result<Committed, ClientError>) -> TestResult<u64> {
    match answer {
        Err(ClientError(ClientErrorEnum::InDoubt { request, .. })) => Ok(request),
        other => Err(TestError::new(format!("in doubt expected: {other:?}"))),
    }
}

#[when("a client commits request 7 of its session, loses the reply, and sends request 7 again")]
fn retried_commit(w: &mut Steps) -> TestResult {
    let [a, p, b] = ["a", "p", "b"].map(|n| w.iri(n));
    let at = shared_client(w.carrier)?;
    let link = LossyLink::to(&at)?;
    let (resolved, held) = run(async {
        let mut client = linked(&link).await?;
        let session = client.register().await?;
        let own = client.begin(TENANT, false).await?;
        client.write(&own, &write_of(&a, &p, &iri(&b))?).await?;
        link.lose_reply(Command::Commit);
        let _doubt = in_doubt(client.commit(own, session).await)?;
        let resolved = client.resolve(session).await?;
        let mut other = borrowed(&at).await?;
        let held = latest_read(&mut other, &Pattern::of(Some(&a), Some(&p))).await?;
        keep(&at, other);
        Ok((resolved, held))
    })?;
    w.seen.commits = vec![resolved];
    w.seen.answers = vec![held];
    w.link = Some(link);
    Ok(())
}

/// That the resend answered what the lost reply said: its commit version and request.
fn same_as_lost(w: &Steps) -> TestResult {
    let link = w.link.as_ref().context("the link")?;
    let lost = link.lost();
    let frame = lost.first().context("the reply the link lost")?;
    let answered = w.seen.commits.first().context("the resend's answer")?;
    assert_eq!(
        sent_commit(frame)?,
        (answered.version(), answered.request()),
        "{}",
        w.over()
    );
    Ok(())
}

#[then("it receives the same reply and commit version, and the transaction's writes are held once")]
fn retried_commit_once(w: &mut Steps) -> TestResult {
    same_as_lost(w)?;
    let held = w.seen.answers.first().context("what is held")?;
    assert_eq!(held.column(0), [w.iri("b")], "{}", w.over());
    Ok(())
}

#[when("a client loses the reply to request 7 of its session, a DirectWrite, and sends it again")]
fn retried_write(w: &mut Steps) -> TestResult {
    let [a, p, b] = ["a", "p", "b"].map(|n| w.iri(n));
    let at = shared_client(w.carrier)?;
    let link = LossyLink::to(&at)?;
    let (resolved, held) = run(async {
        let mut client = linked(&link).await?;
        let session = client.register().await?;
        link.lose_reply(Command::Commit);
        let _doubt = in_doubt(
            client
                .direct_write(direct_of(&a, &p, &iri(&b))?, session)
                .await,
        )?;
        let resolved = client.resolve(session).await?;
        let mut other = borrowed(&at).await?;
        let held = latest_read(&mut other, &Pattern::of(Some(&a), Some(&p))).await?;
        keep(&at, other);
        Ok((resolved, held))
    })?;
    w.seen.commits = vec![resolved];
    w.seen.answers = vec![held];
    w.link = Some(link);
    Ok(())
}

#[then("it receives the same reply and commit version, and the writes are held once")]
fn retried_write_once(w: &mut Steps) -> TestResult {
    retried_commit_once(w)
}

#[when(
    "a client sends request 8 of its session again, with no transaction open for it, and the session's last committed request is 7"
)]
fn never_committed(w: &mut Steps) -> TestResult {
    let [a, p, b] = ["a", "p", "b"].map(|n| w.iri(n));
    let at = shared_client(w.carrier)?;
    let link = LossyLink::to(&at)?;
    let (request, resolved, held) = run(async {
        let mut client = linked(&link).await?;
        let session = client.register().await?;
        let first = client
            .direct_write(direct_of(&a, &p, &iri(&format!("{b}0")))?, session)
            .await?;
        let own = client.begin(TENANT, false).await?;
        client.write(&own, &write_of(&a, &p, &iri(&b))?).await?;
        link.lose_request(Command::Commit);
        let request = in_doubt(client.commit(own, session).await)?;
        assert_eq!(request, first.request() + 1, "the session's next number");
        let resolved = client.resolve(session).await;
        let mut other = borrowed(&at).await?;
        let held = latest_read(&mut other, &Pattern::of(Some(&a), Some(&p))).await?;
        keep(&at, other);
        Ok((request, resolved, held))
    })?;
    w.seen.refusals = resolved.err().into_iter().collect();
    w.seen.facts = vec![request.to_string()];
    w.seen.answers = vec![held];
    Ok(())
}

#[then("the request is refused as not committed, `detail` 8, and nothing is committed")]
fn not_committed(w: &mut Steps) -> TestResult {
    let refused = w.seen.refusals.first().context("the resend's refusal")?;
    let (code, detail, _) = refusal_of(refused)?;
    assert_eq!(code, ErrorCode::NotCommitted, "{refused:?}");
    assert_eq!(
        &detail.to_string(),
        w.seen.facts.first().context("the request")?
    );
    let held = w.seen.answers.first().context("what is held")?;
    assert!(
        !held.column(0).contains(&w.iri("b")),
        "{held:?} {}",
        w.over()
    );
    Ok(())
}

#[when("that Commit's reply is lost, and the client sends it again")]
fn empty_resent(w: &mut Steps) -> TestResult {
    let at = shared_client(w.carrier)?;
    let link = LossyLink::to(&at)?;
    let resolved = run(async {
        let mut client = linked(&link).await?;
        let session = client.register().await?;
        let own = client.begin(TENANT, false).await?;
        link.lose_reply(Command::Commit);
        let _doubt = in_doubt(client.commit(own, session).await)?;
        Ok(client.resolve(session).await)
    })?;
    w.seen.refusals = resolved.err().into_iter().collect();
    Ok(())
}

#[then("every partition answers 18, and the client receives code 18")]
fn empty_resend_refused(w: &mut Steps) -> TestResult {
    let refused = w.seen.refusals.first().context("the resend's refusal")?;
    assert_eq!(
        refusal_of(refused)?.0,
        ErrorCode::NotCommitted,
        "{refused:?}"
    );
    Ok(())
}

#[when(
    "a Drop's reply is lost, and the client sends it again with the same session and request number"
)]
fn drop_resent(w: &mut Steps) -> TestResult {
    let [a, p, b, g] = ["a", "p", "b", "g"].map(|n| w.iri(n));
    let at = shared_client(w.carrier)?;
    let link = LossyLink::to(&at)?;
    let (lost, resolved, gone) = run(async {
        let mut client = linked(&link).await?;
        let _held = client
            .direct_write(in_graph(&g, &a, &p, &iri(&b))?, None)
            .await?;
        let session = client.register().await?;
        link.lose_reply(Command::Drop);
        let _doubt = in_doubt(
            client
                .drop_command(DropCommand::graph(TENANT, &g), session)
                .await,
        )?;
        let resolved = client.resolve(session).await?;
        let frame = link.lost();
        let frame = frame.first().context("the reply the link lost")?;
        let view = FrameView::validate(frame, CLUSTER, MAX_FRAME_SIZE)?;
        let reply = DropCommandView::from_frame(&view)?.to_body()?;
        let mut other = borrowed(&at).await?;
        let pattern = Pattern::of(Some(&a), Some(&p)).in_graph(GraphAt::Named(g.clone()));
        let gone = latest_read(&mut other, &pattern).await?;
        keep(&at, other);
        Ok(((reply.commit_version(), reply.request()), resolved, gone))
    })?;
    w.seen.facts = vec![
        format!("{lost:?}"),
        format!("{:?}", (resolved.version(), resolved.request())),
    ];
    w.seen.answers = vec![gone];
    Ok(())
}

#[then("the second answer is the stored reply, and the drop is applied once")]
fn drop_once(w: &mut Steps) -> TestResult {
    let [lost, answered] = w.seen.facts.as_slice() else {
        return Err(TestError::new("the two replies"));
    };
    assert_eq!(lost, answered, "{}", w.over());
    crate::transaction_steps::nothing(w.seen.answers.first().context("what is held")?)
}

#[when(
    "the connection closes once with a commit queued and unsent, and once with a commit sent and unanswered"
)]
fn sent_and_unsent(w: &mut Steps) -> TestResult {
    let [a, p, b] = ["a", "p", "b"].map(|n| w.iri(n));
    let at = shared_client(w.carrier)?;
    let link = LossyLink::to(&at)?;
    let (unsent, sent, resolved) = run(async {
        let mut client = linked(&link).await?;
        let session = client.register().await?;
        let own = client.begin(TENANT, false).await?;
        client.write(&own, &write_of(&a, &p, &iri(&b))?).await?;
        link.lose_reply(Command::Result);
        let pattern = Pattern::of(Some(&a), Some(&p));
        let stream = client
            .read(&own, &pattern.read(&own, 0, Vec::new())?)
            .await?;
        let lost = client.next_batch(&stream, batch_of).await;
        if lost.is_ok() {
            return Err(TestError::new("a read over a closed link answered"));
        }
        let unsent = client.commit(own, session).await;
        let second = client.begin(TENANT, false).await?;
        client.write(&second, &write_of(&a, &p, &iri(&b))?).await?;
        link.lose_reply(Command::Commit);
        let sent = client.commit(second, session).await;
        Ok((unsent, sent, client.resolve(session).await))
    })?;
    w.seen.facts = vec![format!("{unsent:?}"), format!("{sent:?}")];
    // A real client writes a commit as it is asked, so the unsent one is asked after its connection went: its stream went with
    // it, and nothing is sent. A commit queued behind a frame still being written is the core's own suite's to bind.
    w.committed = matches!(
        (unsent, sent),
        (
            Err(ClientError(
                ClientErrorEnum::ConnectionLost { .. } | ClientErrorEnum::StreamFinished { .. }
            )),
            Err(ClientError(ClientErrorEnum::InDoubt { .. }))
        )
    );
    w.seen.commits = vec![resolved?];
    Ok(())
}

#[then(
    "the first answers `ConnectionLost`, the second `InDoubt`, and resolving the second reports whether it committed"
)]
fn lost_then_doubt(w: &mut Steps) -> TestResult {
    assert!(w.committed, "{:?} {}", w.seen.facts, w.over());
    let resolved = w.seen.commits.first().context("the resolve's answer")?;
    assert!(resolved.version() > 0, "{resolved:?}");
    Ok(())
}

#[when("a transaction committing through no session loses its reply")]
fn no_session_lost(w: &mut Steps) -> TestResult {
    let [a, p, b] = ["a", "p", "b"].map(|n| w.iri(n));
    let at = shared_client(w.carrier)?;
    let link = LossyLink::to(&at)?;
    let answered = run(async {
        let mut client = TestClient::new(&link.client_at(), &Knobs::default())?;
        client.connect().await?;
        let own = client.begin(TENANT, false).await?;
        client.write(&own, &write_of(&a, &p, &iri(&b))?).await?;
        link.lose_reply(Command::Commit);
        Ok(client.commit(own, None).await)
    })?;
    w.seen.facts = vec![format!("{answered:?}")];
    w.committed = matches!(
        answered,
        Err(ClientError(ClientErrorEnum::OutcomeUnknown {
            session: 0,
            request: 0
        }))
    );
    w.link = Some(link);
    Ok(())
}

#[then("the client reports the outcome as unknown, and resends nothing")]
fn unknown_outcome(w: &mut Steps) -> TestResult {
    assert!(w.committed, "{:?} {}", w.seen.facts, w.over());
    let link = w.link.as_ref().context("the link")?;
    assert_eq!(
        link.connections(),
        1,
        "no connection after the loss {}",
        w.over()
    );
    Ok(())
}

/// A one-frame write of a statement of a new blank node.
fn blank_write(p: &str) -> TestResult<ultragraphdb_protocol_wire::DirectWrite> {
    let mut terms = TermTable::new();
    let change = WriteChange::Insert {
        graph: None,
        subject: terms.intern(RequestTerm::BlankNode("b1".to_string()))?,
        predicate: terms.iri(p),
        object: terms.intern(string("x"))?,
    };
    Ok(ultragraphdb_protocol_wire::DirectWrite::new(
        TENANT,
        terms,
        vec![change],
    ))
}

#[when(
    "a DirectWrite through a session names only new blank nodes as subjects, its reply is lost, and the client sends it again"
)]
fn blank_resent(w: &mut Steps) -> TestResult {
    let p = w.iri("p");
    let at = spread_client(w.carrier)?;
    let link = LossyLink::to(&at)?;
    let (resolved, held) = run(async {
        let mut client = linked(&link).await?;
        let session = client.register().await?;
        link.lose_reply(Command::Commit);
        let _doubt = in_doubt(client.direct_write(blank_write(&p)?, session).await)?;
        let resolved = client.resolve(session).await?;
        let mut other = borrowed(&at).await?;
        let held =
            crate::read_steps::until_rows(&mut other, &Pattern::of(None, Some(&p)), 1).await?;
        keep(&at, other);
        Ok((resolved, held))
    })?;
    w.seen.commits = vec![resolved];
    w.seen.answers = vec![held];
    w.link = Some(link);
    Ok(())
}

#[then(
    "both go to the partition at the position XXH3-64 of its `session` and `request` bytes gives among the partitions holding the tenant's run, and the second is answered the stored reply"
)]
fn blank_once(w: &mut Steps) -> TestResult {
    same_as_lost(w)?;
    let held = w.seen.answers.first().context("what is held")?;
    assert_eq!(
        held.rows.len(),
        1,
        "the write held once: {held:?} {}",
        w.over()
    );
    Ok(())
}

#[when(
    "a session commits request 1 in partition 1, request 2 in partition 2 and request 3 in partition 1, then loses the reply to request 3 and sends it again on a new connection"
)]
fn two_partitions(w: &mut Steps) -> TestResult {
    let [p, b] = ["p", "b"].map(|n| w.iri(n));
    let stem = w.iri("s");
    let (first, second) = (iri_of_spread(&stem, 1)?, iri_of_spread(&stem, 2)?);
    let third = format!("{first}third");
    let third =
        if crate::cluster::partition_of(crate::cluster::spread_ranges()?, bucket_of(&third)?)
            == Some(1)
        {
            third
        } else {
            iri_of_spread(&format!("{stem}/third/"), 1)?
        };
    let at = spread_client(w.carrier)?;
    let link = LossyLink::to(&at)?;
    let (replies, held) = run(async {
        let mut client = linked(&link).await?;
        let session = client.register().await?;
        let mut replies = Vec::new();
        for subject in [&first, &second] {
            let own = client.begin(TENANT, false).await?;
            client
                .write(&own, &write_of(subject, &p, &iri(&b))?)
                .await?;
            replies.push(client.commit(own, session).await?);
        }
        let own = client.begin(TENANT, false).await?;
        client.write(&own, &write_of(&third, &p, &iri(&b))?).await?;
        link.lose_reply(Command::Commit);
        let _doubt = in_doubt(client.commit(own, session).await)?;
        replies.push(client.resolve(session).await?);
        let mut other = borrowed(&at).await?;
        let mut held = Vec::new();
        for subject in [&first, &second, &third] {
            held.push(latest_read(&mut other, &Pattern::of(Some(subject), Some(&p))).await?);
        }
        keep(&at, other);
        Ok((replies, held))
    })?;
    w.seen.commits = replies;
    w.seen.answers = held;
    w.link = Some(link);
    Ok(())
}

/// The bucket of `text` in the tenant the scenarios write.
fn bucket_of(text: &str) -> TestResult<u32> {
    Ok(ultragraphdb_store_quads::iri_bucket(TENANT, text)
        .context("the IRI's bucket")?
        .get())
}

#[then(
    "all three commit once, partition 1 accepts request 3 after its last, 1, and the retry receives request 3's stored reply from partition 1"
)]
fn two_partitions_once(w: &mut Steps) -> TestResult {
    let requests: Vec<u64> = w.seen.commits.iter().map(Committed::request).collect();
    assert_eq!(requests, [1, 2, 3], "{}", w.over());
    let link = w.link.as_ref().context("the link")?;
    let lost = link.lost();
    let frame = lost.first().context("the reply the link lost")?;
    let answered = w.seen.commits.last().context("the resend's answer")?;
    assert_eq!(
        sent_commit(frame)?,
        (answered.version(), answered.request())
    );
    let token = answered.token().context("its token")?;
    assert_eq!(token.partition(), 1, "{answered:?} {}", w.over());
    for held in &w.seen.answers {
        assert_eq!(held.column(0), [w.iri("b")], "{held:?} {}", w.over());
    }
    Ok(())
}
