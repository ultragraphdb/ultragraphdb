/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! A client's transactions and one-frame writes against what the partition decides: snapshots, conflicts, commit-time
//! operations, session tokens and the rules a request's terms obey.

use cucumber::{then, when};
use ultragraphdb_client_compio::{ClientError, ClientErrorEnum};
use ultragraphdb_core_ids::{Epoch, SessionToken, Version};
use ultragraphdb_core_terms::SYSTEM_VOCABULARY;
use ultragraphdb_protocol_wire::{
    Begin, ColumnType, CommitOperation, ConditionalWrite, DeclaredKind, DeclaredPredicate,
    DirectWrite, DropCommand, ErrorCode, Execute, Operation, PatternSlot, Prepare, QuadPattern,
    ReadLevel, RequestTerm, RequestTermView, ResultView, TermTable, Write, WriteChange,
};
use ultragraphdb_tool_bdd::{Context, TestError, TestResult};

use crate::Steps;
use crate::client_reads::{
    Answer, GraphAt, Pattern, TENANT, batch_of, commit_a_write, direct_of, drain, edge_of, integer,
    iri, read_whole, refusal_of, refused_with, string, text_of, typed, write_of,
};
use crate::endpoint_steps::{declare, declaring};
use crate::read_steps::{in_graph, until_rows};
use crate::shells::{TestClient, pause};
use crate::transaction_steps::{committed, in_shared, nothing};

/// A read-only transaction's read of `pattern`, its first snapshot taking what the commits before it closed, at `level`.
async fn snapshot_read(client: &mut TestClient, pattern: &Pattern) -> TestResult<Answer> {
    let reader = client.begin(TENANT, true).await?;
    let answer = read_whole(client, &reader, pattern).await?;
    client.abort(reader).await?;
    Ok(answer)
}

/// What a read-write transaction, which reads the partition's latest state, reads of `pattern`.
pub(crate) async fn latest_read(client: &mut TestClient, pattern: &Pattern) -> TestResult<Answer> {
    let reader = client.begin(TENANT, false).await?;
    let answer = read_whole(client, &reader, pattern).await?;
    client.abort(reader).await?;
    Ok(answer)
}

#[when(
    "a client's Begin names a read version above the visibility watermark of the partition answering it"
)]
fn above_watermark(w: &mut Steps) -> TestResult {
    let [a, p, b] = ["a", "p", "b"].map(|name| w.iri(name));
    let (refused, served, version) = in_shared(w, async |client| {
        let written = client
            .direct_write(direct_of(&a, &p, &iri(&b))?, None)
            .await?;
        let far = (Version::from_u64(written.version()).epoch().get() + 1_000_000) << 22;
        let refused = match client.begin_with(Begin::new(TENANT, far, true), None).await {
            Err(error) => error,
            Ok(reader) => {
                let pattern = Pattern::of(Some(&a), Some(&p));
                let stream = client
                    .read(&reader, &pattern.read(&reader, 0, Vec::new())?)
                    .await?;
                client
                    .next_batch(&stream, batch_of)
                    .await
                    .err()
                    .context("a read above the watermark refused")?
            }
        };
        let served = snapshot_read(client, &Pattern::of(Some(&a), Some(&p))).await;
        Ok((refused, served.is_ok(), far))
    })?;
    w.seen.refusals = vec![refused];
    w.committed = served;
    w.seen.facts = vec![version.to_string()];
    Ok(())
}

#[then(
    "it is refused at once with an error naming the version and the watermark, and the worker serves the next request"
)]
fn watermark_refused(w: &mut Steps) -> TestResult {
    let refused = w.seen.refusals.first().context("the refusal")?;
    let (code, _, message) = refusal_of(refused)?;
    assert_eq!(code, ErrorCode::AheadOfWatermark, "{refused:?}");
    let version = w.seen.facts.first().context("the version named")?;
    assert!(message.contains(version), "{message} {}", w.over());
    assert!(w.committed, "the next request went unserved {}", w.over());
    Ok(())
}

#[when(
    "a commit setting `<a> <name> \"A\"` is acknowledged and a read-write transaction then begins and reads `name` of `a`"
)]
fn acknowledged_then_begun(w: &mut Steps) -> TestResult {
    let [a, name] = ["a", "name"].map(|n| w.iri(n));
    w.seen.answers = in_shared(w, async |client| {
        committed(client, &a, &name, &string("A")).await?;
        Ok(vec![
            latest_read(client, &Pattern::of(Some(&a), Some(&name))).await?,
        ])
    })?;
    Ok(())
}

#[then("it reads `\"A\"`")]
fn reads_a(w: &mut Steps) -> TestResult {
    let answer = w.seen.answers.first().context("the read's answer")?;
    assert_eq!(answer.column(0), ["A"], "{}", w.over());
    Ok(())
}

#[when(
    "a read-only transaction reads `name` of `a`, another transaction then commits a new `name` of `a`, and the read-only transaction ends"
)]
fn read_only_ends(w: &mut Steps) -> TestResult {
    let [a, name] = ["a", "name"].map(|n| w.iri(n));
    w.seen.answers = in_shared(w, async |client| {
        committed(client, &a, &name, &string("before")).await?;
        let pattern = Pattern::of(Some(&a), Some(&name));
        let _closed = until_rows(client, &pattern, 1).await?;
        let reader = client.begin(TENANT, true).await?;
        let first = read_whole(client, &reader, &pattern).await?;
        committed(client, &a, &name, &string("after")).await?;
        let second = read_whole(client, &reader, &pattern).await?;
        client.abort(reader).await?;
        // The connection serves the next request, so the end took no error with it.
        Ok(vec![first, second, latest_read(client, &pattern).await?])
    })?;
    Ok(())
}

#[then("it ends without an error, having read the value from before the commit")]
fn read_before_commit(w: &mut Steps) -> TestResult {
    let [first, second, latest] = w.seen.answers.as_slice() else {
        return Err(TestError::new("three answers"));
    };
    assert_eq!(first.column(0), ["before"], "{}", w.over());
    assert_eq!(second.column(0), ["before"], "{}", w.over());
    assert!(
        latest.column(0).contains(&"after".to_string()),
        "{latest:?}"
    );
    Ok(())
}

#[when(
    "two transactions each read `on-call` of `alice` and `bob`, both `true`, and each sets a different one to `false`, concurrently, all four facts in one partition"
)]
fn write_skew(w: &mut Steps) -> TestResult {
    let [alice, bob, on_call] = ["alice", "bob", "on-call"].map(|n| w.iri(n));
    let outcomes = in_shared(w, async |client| {
        committed(client, &alice, &on_call, &string("true")).await?;
        committed(client, &bob, &on_call, &string("true")).await?;
        let (first, second) = (
            client.begin(TENANT, false).await?,
            client.begin(TENANT, false).await?,
        );
        for transaction in [&first, &second] {
            for person in [&alice, &bob] {
                let _found = read_whole(
                    client,
                    transaction,
                    &Pattern::of(Some(person), Some(&on_call)),
                )
                .await?;
            }
        }
        client
            .write(&first, &write_of(&alice, &on_call, &string("false"))?)
            .await?;
        client
            .write(&second, &write_of(&bob, &on_call, &string("false"))?)
            .await?;
        Ok(vec![
            client.commit(first, None).await,
            client.commit(second, None).await,
        ])
    })?;
    w.seen.facts = outcomes
        .iter()
        .map(|outcome| format!("{outcome:?}"))
        .collect();
    let mut outcomes = outcomes.into_iter();
    let (one, other) = (
        outcomes.next().context("the first commit")?,
        outcomes.next().context("the second commit")?,
    );
    w.committed = one.is_ok();
    w.seen.refusals = other.err().into_iter().collect();
    Ok(())
}

#[then("one of them commits and the other aborts with a conflict")]
fn one_aborts(w: &mut Steps) -> TestResult {
    assert!(w.committed, "{:?} {}", w.seen.facts, w.over());
    let refused = w
        .seen
        .refusals
        .first()
        .context("the second commit's refusal")?;
    assert_eq!(refusal_of(refused)?.0, ErrorCode::Conflict, "{refused:?}");
    Ok(())
}

#[when("two transactions concurrently insert `<a> <knows> <b>` and `<a> <knows> <c>`")]
fn different_objects(w: &mut Steps) -> TestResult {
    let [a, b, c, knows] = ["a", "b", "c", "knows"].map(|n| w.iri(n));
    w.seen.answers = in_shared(w, async |client| {
        let (first, second) = (
            client.begin(TENANT, false).await?,
            client.begin(TENANT, false).await?,
        );
        client
            .write(&first, &write_of(&a, &knows, &iri(&b))?)
            .await?;
        client
            .write(&second, &write_of(&a, &knows, &iri(&c))?)
            .await?;
        let _first = client.commit(first, None).await?;
        let _second = client.commit(second, None).await?;
        Ok(vec![
            latest_read(client, &Pattern::of(Some(&a), Some(&knows))).await?,
        ])
    })?;
    Ok(())
}

#[then("both commit, and `a` knows `b` and `c`")]
fn both_objects(w: &mut Steps) -> TestResult {
    let answer = w.seen.answers.first().context("the read's answer")?;
    let mut objects = answer.column(0);
    objects.sort();
    assert_eq!(objects, [w.iri("b"), w.iri("c")], "{}", w.over());
    Ok(())
}

#[when(
    "a transaction reads `<new>`'s statements, finds none, and writes `<x> <seen> true`, and a transaction in the same partition writing `<new> <name> \"N\"` commits after its read version"
)]
fn found_absent(w: &mut Steps) -> TestResult {
    let [new, name] = ["new", "name"].map(|n| w.iri(n));
    w.kind = in_shared(w, async |client| {
        let first = client.begin(TENANT, false).await?;
        nothing(&read_whole(client, &first, &Pattern::of(Some(&new), None)).await?)?;
        committed(client, &new, &name, &string("N")).await?;
        commit_a_write(client, first).await
    })?;
    Ok(())
}

#[then("the reading transaction aborts with a conflict naming an absent term")]
fn absent_named(w: &mut Steps) {
    assert_eq!(
        w.kind,
        Some(crate::transaction_steps::KIND_ABSENT_TERM),
        "{}",
        w.over()
    );
}

#[when(
    "a transaction reads `<new> <p> ?o` through compute, finding nothing, so it records both the term's absence and the list's range, and a transaction writing `<new> <p> \"x\"` commits after its read version"
)]
fn absent_before_range(w: &mut Steps) -> TestResult {
    let [new, p] = ["new", "p"].map(|n| w.iri(n));
    w.kind = in_shared(w, async |client| {
        let first = client.begin(TENANT, false).await?;
        nothing(&read_whole(client, &first, &Pattern::of(Some(&new), Some(&p))).await?)?;
        committed(client, &new, &p, &string("x")).await?;
        commit_a_write(client, first).await
    })?;
    Ok(())
}

#[then("the reading transaction aborts with a conflict of kind AbsentTerm")]
fn aborts_kind_absent(w: &mut Steps) {
    assert_eq!(
        w.kind,
        Some(crate::transaction_steps::KIND_ABSENT_TERM),
        "{}",
        w.over()
    );
}

#[when("a transaction inserts `<a> <knows> <b>` in one request and aborts in the next")]
fn aborted_insert(w: &mut Steps) -> TestResult {
    let [a, b, knows] = ["a", "b", "knows"].map(|n| w.iri(n));
    w.seen.answers = in_shared(w, async |client| {
        let own = client.begin(TENANT, false).await?;
        client.write(&own, &write_of(&a, &knows, &iri(&b))?).await?;
        client.abort(own).await?;
        Ok(vec![
            latest_read(client, &Pattern::of(Some(&a), Some(&knows))).await?,
        ])
    })?;
    Ok(())
}

#[then("`<a> <knows> <b>` is not held")]
fn not_held(w: &mut Steps) -> TestResult {
    nothing(w.seen.answers.first().context("the read's answer")?)
}

#[when(
    "a session's last request in partition 1 is 7, and a new transaction bound to partition 1 sends its Commit as request 7"
)]
fn reused_number(w: &mut Steps) -> TestResult {
    let [a, p, b, c] = ["a", "p", "b", "c"].map(|n| w.iri(n));
    let (refused, last, held) = in_shared(w, async |client| {
        let session = client.register().await?;
        let mut last = 0;
        for at in 0..7 {
            let object = format!("{b}{at}");
            last = client
                .direct_write(direct_of(&a, &p, &iri(&object))?, session)
                .await?
                .request();
        }
        // The connection forgets the session, and takes it up again at the number the partition holds.
        client.close().await;
        let resumed = client.resume_session(session.number(), last)?;
        let own = client.begin(TENANT, false).await?;
        client.write(&own, &write_of(&a, &p, &iri(&c))?).await?;
        let refused = client
            .commit(own, resumed)
            .await
            .err()
            .context("a refusal")?;
        Ok((
            refused,
            last,
            latest_read(client, &Pattern::of(Some(&a), Some(&p))).await?,
        ))
    })?;
    w.seen.refusals = vec![refused];
    w.seen.facts = vec![last.to_string()];
    w.seen.answers = vec![held];
    Ok(())
}

#[then(
    "the Commit is refused with code 13, `detail` 7, the transaction aborts, nothing it wrote is held, and request 7's stored reply is not returned"
)]
fn reuse_refused(w: &mut Steps) -> TestResult {
    let refused = w.seen.refusals.first().context("the refusal")?;
    let cause = match &refused.0 {
        ClientErrorEnum::Refused { .. } => refused,
        ClientErrorEnum::OutcomeUnknown { .. } | ClientErrorEnum::InDoubt { .. } => {
            return Err(TestError::new(format!(
                "the commit left unknown: {refused:?}"
            )));
        }
        other @ (ClientErrorEnum::Transport { .. }
        | ClientErrorEnum::Protocol { .. }
        | ClientErrorEnum::Closed { .. }
        | ClientErrorEnum::TimedOut
        | ClientErrorEnum::ConnectionLost { .. }
        | ClientErrorEnum::NotConnected
        | ClientErrorEnum::Request { .. }
        | ClientErrorEnum::ReplyMismatch { .. }
        | ClientErrorEnum::NotInDoubt { .. }
        | ClientErrorEnum::SessionBusy { .. }
        | ClientErrorEnum::UnknownSession { .. }
        | ClientErrorEnum::ReadOnly { .. }
        | ClientErrorEnum::StreamFinished { .. }
        | ClientErrorEnum::StreamBusy { .. }
        | ClientErrorEnum::StreamLimit { .. }
        | ClientErrorEnum::BufferLimit { .. }
        | ClientErrorEnum::TransactionMismatch { .. }) => {
            return Err(TestError::new(format!("a refusal: {other:?}")));
        }
    };
    let (code, detail, _) = refusal_of(cause)?;
    assert_eq!(code, ErrorCode::RequestOutOfOrder, "{refused:?}");
    assert_eq!(
        detail.to_string(),
        *w.seen.facts.first().context("the number")?
    );
    let held = w.seen.answers.first().context("what is held")?;
    assert!(
        !held.column(0).contains(&w.iri("c")),
        "{held:?} {}",
        w.over()
    );
    Ok(())
}

#[when("a transaction and a Drop each commit through no session, `session` all zero")]
fn through_no_session(w: &mut Steps) -> TestResult {
    let [a, p, b, g] = ["a", "p", "b", "g"].map(|n| w.iri(n));
    w.seen.commits = in_shared(w, async |client| {
        let own = client.begin(TENANT, false).await?;
        client.write(&own, &write_of(&a, &p, &iri(&b))?).await?;
        let written = in_graph(&g, &a, &p, &iri(&b))?;
        let _held = client.direct_write(written, None).await?;
        Ok(vec![
            client.commit(own, None).await?,
            client
                .drop_command(DropCommand::graph(TENANT, &g), None)
                .await?,
        ])
    })?;
    Ok(())
}

#[then(
    "each reply's `commit_version` is its version and its token is 24 zero bytes, and a receiver accepts it"
)]
fn no_token(w: &mut Steps) {
    assert_eq!(w.seen.commits.len(), 2, "{}", w.over());
    for committed in &w.seen.commits {
        assert!(committed.version() > 0, "{committed:?}");
        assert!(committed.token().is_none(), "{committed:?}");
        assert_eq!(committed.request(), 0, "{committed:?}");
    }
}

#[when("a transaction commits in partition 1 at position 40 with a version of epoch 12")]
fn token_of_commit(w: &mut Steps) -> TestResult {
    let [a, p, b] = ["a", "p", "b"].map(|n| w.iri(n));
    w.seen.commits = in_shared(w, async |client| {
        let session = client.register().await?;
        let own = client
            .begin_with(Begin::new(TENANT, 0, false), session)
            .await?;
        client.write(&own, &write_of(&a, &p, &iri(&b))?).await?;
        Ok(vec![client.commit(own, session).await?])
    })?;
    Ok(())
}

#[then("its reply carries the token (partition 1, position 40, epoch 12)")]
fn token_named(w: &mut Steps) -> TestResult {
    let committed = w.seen.commits.first().context("the commit")?;
    let token = committed.token().context("a token")?;
    assert_eq!(token.partition(), 1, "{committed:?} {}", w.over());
    assert_eq!(
        token.epoch().get(),
        Version::from_u64(committed.version()).epoch().get(),
        "{committed:?}"
    );
    Ok(())
}

#[when(
    "a client commits `<a> <knows> <b>` and reads `<a>`'s statements with the commit's token from another connection"
)]
fn token_elsewhere(w: &mut Steps) -> TestResult {
    let [a, b, knows] = ["a", "b", "knows"].map(|n| w.iri(n));
    let at = crate::cluster::shared_client(w.carrier)?;
    w.seen.answers = in_shared(w, async |client| {
        let session = client.register().await?;
        let written = client
            .direct_write(direct_of(&a, &knows, &iri(&b))?, session)
            .await?;
        let token = written.token().context("a token")?;
        let mut other = crate::cluster::client(&at).await?;
        let begin = Begin::new(TENANT, 0, true).with_token(token.clone());
        let reader = other.begin_with(begin, None).await?;
        let answer = read_whole(&mut other, &reader, &Pattern::of(Some(&a), Some(&knows))).await?;
        other.abort(reader).await?;
        Ok(vec![answer])
    })?;
    Ok(())
}

#[then("the read returns `<a> <knows> <b>`")]
fn token_read(w: &mut Steps) -> TestResult {
    let answer = w.seen.answers.first().context("the read's answer")?;
    assert_eq!(answer.column(0), [w.iri("b")], "{}", w.over());
    Ok(())
}

#[when(
    "a client commits a write, receives a session token, and begins a transaction with that token whose read asks for the write"
)]
fn token_through_session(w: &mut Steps) -> TestResult {
    let [a, b, knows] = ["a", "b", "knows"].map(|n| w.iri(n));
    w.seen.answers = in_shared(w, async |client| {
        let session = client.register().await?;
        let _written = client
            .direct_write(direct_of(&a, &knows, &iri(&b))?, session)
            .await?;
        let reader = client
            .begin_with(Begin::new(TENANT, 0, true), session)
            .await?;
        let answer = read_whole(client, &reader, &Pattern::of(Some(&a), Some(&knows))).await?;
        client.abort(reader).await?;
        Ok(vec![answer])
    })?;
    Ok(())
}

#[then("the read returns the write")]
fn returns_write(w: &mut Steps) -> TestResult {
    let answer = w.seen.answers.first().context("the read's answer")?;
    assert_eq!(answer.column(0), [w.iri("b")], "{}", w.over());
    Ok(())
}

#[when(
    "a transaction begins at snapshot level, reads `<a>`'s statements, another transaction commits a new statement of `<a>`, and the first transaction reads `<a>`'s statements again"
)]
fn shared_snapshot(w: &mut Steps) -> TestResult {
    let [a, p] = ["a", "p"].map(|n| w.iri(n));
    w.seen.answers = in_shared(w, async |client| {
        committed(client, &a, &p, &string("first")).await?;
        let pattern = Pattern::of(Some(&a), Some(&p));
        let _closed = until_rows(client, &pattern, 1).await?;
        let reader = client.begin(TENANT, true).await?;
        let before = read_whole(client, &reader, &pattern).await?;
        committed(client, &a, &p, &string("second")).await?;
        let again = read_whole(client, &reader, &pattern).await?;
        client.abort(reader).await?;
        Ok(vec![before, again])
    })?;
    Ok(())
}

#[then("both reads return the same rows")]
fn same_rows(w: &mut Steps) -> TestResult {
    let [before, again] = w.seen.answers.as_slice() else {
        return Err(TestError::new("two answers"));
    };
    assert_eq!(before.rows, again.rows, "{}", w.over());
    assert_eq!(before.rows.len(), 1, "{before:?}");
    Ok(())
}

/// The epoch and the rows a read at `level` answers: what a monotonic read's batches say of themselves.
fn epoch_of(batch: &ResultView<'_>) -> u64 {
    batch.epoch()
}

#[when(
    "a monotonic-fast transaction's first read answers at epoch 9, and its second read touches only a partition whose latest closed epoch is 12"
)]
fn monotonic_second(w: &mut Steps) -> TestResult {
    let [a, p, b] = ["a", "p", "b"].map(|n| w.iri(n));
    let epochs = in_shared(w, async |client| {
        committed(client, &a, &p, &iri(&b)).await?;
        let begin = Begin::new(TENANT, 0, true).with_level(ReadLevel::MonotonicFast);
        let reader = client.begin_with(begin, None).await?;
        let pattern = Pattern::of(Some(&a), Some(&p));
        let first = client
            .read(&reader, &pattern.read(&reader, 0, Vec::new())?)
            .await?;
        let first_epoch = client
            .next_batch(&first, epoch_of)
            .await?
            .context("a batch")?;
        pause(std::time::Duration::from_millis(60)).await;
        let second = client
            .read(&reader, &pattern.read(&reader, 0, Vec::new())?)
            .await?;
        let second_epoch = client
            .next_batch(&second, epoch_of)
            .await?
            .context("a batch")?;
        client.abort(reader).await?;
        Ok((first_epoch, second_epoch))
    })?;
    w.seen.facts = vec![epochs.0.to_string(), epochs.1.to_string()];
    w.committed = epochs.0 > 0 && epochs.1 > epochs.0;
    Ok(())
}

#[then("the second read answers at epoch 12 without waiting")]
fn monotonic_later(w: &mut Steps) {
    assert!(w.committed, "{:?} {}", w.seen.facts, w.over());
}

#[when(
    "a read-only monotonic-fast transaction carries a token of partition 1 at epoch 20, and its read touches partition 1, closed at 21, and partition 2, closed at 18"
)]
fn monotonic_token(w: &mut Steps) -> TestResult {
    let [a, p, b] = ["a", "p", "b"].map(|n| w.iri(n));
    w.seen.answers = in_shared(w, async |client| {
        let session = client.register().await?;
        let written = client
            .direct_write(direct_of(&a, &p, &iri(&b))?, session)
            .await?;
        let token = written.token().context("a token")?;
        let begin = Begin::new(TENANT, 0, true)
            .with_level(ReadLevel::MonotonicFast)
            .with_token(token.clone());
        let reader = client.begin_with(begin, None).await?;
        let answer = read_whole(client, &reader, &Pattern::of(Some(&a), Some(&p))).await?;
        client.abort(reader).await?;
        Ok(vec![answer])
    })?;
    Ok(())
}

#[then(
    "the read waits until partition 2 has closed epoch 20, then answers at epoch 20 and returns the token's commit"
)]
fn monotonic_token_read(w: &mut Steps) -> TestResult {
    let answer = w.seen.answers.first().context("the read's answer")?;
    assert_eq!(answer.column(0), [w.iri("b")], "{}", w.over());
    Ok(())
}

#[when(
    "a monotonic-fast transaction's read answered at epoch 12 and another at epoch 9, and a third is sent after both"
)]
fn monotonic_highest(w: &mut Steps) -> TestResult {
    let [a, p, b] = ["a", "p", "b"].map(|n| w.iri(n));
    let epochs = in_shared(w, async |client| {
        committed(client, &a, &p, &iri(&b)).await?;
        let begin = Begin::new(TENANT, 0, true).with_level(ReadLevel::MonotonicFast);
        let reader = client.begin_with(begin, None).await?;
        let pattern = Pattern::of(Some(&a), Some(&p));
        let first = client
            .read(&reader, &pattern.read(&reader, 0, Vec::new())?)
            .await?;
        let second = client
            .read(&reader, &pattern.read(&reader, 0, Vec::new())?)
            .await?;
        let one = client
            .next_batch(&first, epoch_of)
            .await?
            .context("a batch")?;
        let two = client
            .next_batch(&second, epoch_of)
            .await?
            .context("a batch")?;
        let third = client
            .read(&reader, &pattern.read(&reader, 0, Vec::new())?)
            .await?;
        let floor = one.max(two);
        let three = client
            .next_batch(&third, epoch_of)
            .await?
            .context("a batch")?;
        client.abort(reader).await?;
        Ok((floor, three))
    })?;
    w.seen.facts = vec![epochs.0.to_string(), epochs.1.to_string()];
    w.committed = epochs.1 >= epochs.0;
    Ok(())
}

#[then("the third read's floor is 12")]
fn third_floor(w: &mut Steps) {
    assert!(w.committed, "{:?} {}", w.seen.facts, w.over());
}

#[when("a client sends a DirectWrite inserting `<a> <knows> <b>` through its session")]
fn direct_write_session(w: &mut Steps) -> TestResult {
    let [a, b, knows] = ["a", "b", "knows"].map(|n| w.iri(n));
    let (written, held) = in_shared(w, async |client| {
        let session = client.register().await?;
        let written = client
            .direct_write(direct_of(&a, &knows, &iri(&b))?, session)
            .await?;
        Ok((
            written,
            latest_read(client, &Pattern::of(Some(&a), Some(&knows))).await?,
        ))
    })?;
    w.seen.commits = vec![written];
    w.seen.answers = vec![held];
    Ok(())
}

#[then(
    "the reply carries the commit version and session token, `<a> <knows> <b>` is held, and the partition holds no open transaction for it at any point after the reply"
)]
fn direct_written(w: &mut Steps) -> TestResult {
    let written = w.seen.commits.first().context("the reply")?;
    assert!(written.version() > 0, "{written:?}");
    assert!(written.token().is_some(), "{written:?}");
    let held = w.seen.answers.first().context("what is held")?;
    assert_eq!(held.column(0), [w.iri("b")], "{}", w.over());
    Ok(())
}

/// A ConditionalWrite that insert-if-absents the node of `value` under `predicate`.
fn claim(predicate: &str, value: &str) -> TestResult<ConditionalWrite> {
    let mut terms = TermTable::new();
    let operation = CommitOperation::InsertIfAbsent {
        graph: None,
        predicate: terms.iri(predicate),
        value: terms.intern(string(value))?,
        properties: Vec::new(),
    };
    Ok(ConditionalWrite::new(
        TENANT,
        terms,
        Vec::new(),
        vec![operation],
    ))
}

/// The declaration of `predicate` unique and single-valued, a property.
pub(crate) fn unique(predicate: &str) -> DeclaredPredicate {
    DeclaredPredicate::new(predicate)
        .with_kind(DeclaredKind::Property)
        .with_single(true)
        .with_unique(true)
}

#[when(
    "a DirectWrite holds a valid insert and an insert of the predicate `id`, which the catalog declares unique"
)]
fn direct_on_unique(w: &mut Steps) -> TestResult {
    let [a, p, b, id] = ["a", "p", "b", "id"].map(|n| w.iri(n));
    let (refused, plain, claimed) = in_shared(w, async |client| {
        declare(client, unique(&id)).await?;
        let mut terms = TermTable::new();
        let changes = vec![
            WriteChange::Insert {
                graph: None,
                subject: terms.iri(&a),
                predicate: terms.iri(&p),
                object: terms.iri(&b),
            },
            WriteChange::Insert {
                graph: None,
                subject: terms.iri(&a),
                predicate: terms.iri(&id),
                object: terms.intern(string("x"))?,
            },
        ];
        let refused = client
            .direct_write(DirectWrite::new(TENANT, terms, changes), None)
            .await
            .err()
            .context("a refusal")?;
        Ok((
            refused,
            latest_read(client, &Pattern::of(Some(&a), Some(&p))).await?,
            latest_read(client, &Pattern::of(Some(&a), Some(&id))).await?,
        ))
    })?;
    w.seen.refusals = vec![refused];
    w.seen.answers = vec![plain, claimed];
    Ok(())
}

#[then("it is refused with an error naming `id` and ConditionalWrite, and neither insert is held")]
fn unique_direct_refused(w: &mut Steps) -> TestResult {
    let refused = w.seen.refusals.first().context("the refusal")?;
    let (_, _, message) = refusal_of(refused)?;
    assert!(message.contains(&w.iri("id")), "{message}");
    assert!(message.contains("ConditionalWrite"), "{message}");
    for answer in &w.seen.answers {
        nothing(answer)?;
    }
    Ok(())
}

#[when("two DirectWrites insert `<a> <knows> <b>` concurrently")]
fn identical_inserts(w: &mut Steps) -> TestResult {
    let [a, b, knows] = ["a", "b", "knows"].map(|n| w.iri(n));
    let at = crate::cluster::shared_client(w.carrier)?;
    let (versions, held) = in_shared(w, async |client| {
        let mut other = crate::cluster::client(&at).await?;
        let (one, two) = (
            direct_of(&a, &knows, &iri(&b))?,
            direct_of(&a, &knows, &iri(&b))?,
        );
        let (first, second) = futures::join!(
            client.direct_write(one, None),
            other.direct_write(two, None)
        );
        Ok((
            vec![first?.version(), second?.version()],
            latest_read(client, &Pattern::of(Some(&a), Some(&knows))).await?,
        ))
    })?;
    w.seen.facts = versions.iter().map(u64::to_string).collect();
    w.seen.answers = vec![held];
    Ok(())
}

#[then("both are answered with success, whichever executed first, and the statement is held once")]
fn both_inserted(w: &mut Steps) -> TestResult {
    assert_eq!(w.seen.facts.len(), 2, "{}", w.over());
    let held = w.seen.answers.first().context("what is held")?;
    assert_eq!(held.column(0), [w.iri("b")], "{}", w.over());
    Ok(())
}

#[when("two ConditionalWrites each insert-if-absent a node with `id` `\"x\"` concurrently")]
fn two_claims(w: &mut Steps) -> TestResult {
    let id = w.iri("id");
    let at = crate::cluster::shared_client(w.carrier)?;
    w.seen.commits = in_shared(w, async |client| {
        declare(client, unique(&id)).await?;
        let mut other = crate::cluster::client(&at).await?;
        let (first, second) = futures::join!(
            client.conditional_write(claim(&id, "x")?, None),
            other.conditional_write(claim(&id, "x")?, None)
        );
        Ok(vec![first?, second?])
    })?;
    Ok(())
}

#[then("both name the same node, created once")]
fn claimed_once(w: &mut Steps) -> TestResult {
    let [first, second] = w.seen.commits.as_slice() else {
        return Err(TestError::new("two replies"));
    };
    assert_eq!(first.node(0), second.node(0), "{first:?} {second:?}");
    let created = [first, second]
        .iter()
        .filter(|reply| {
            reply
                .results()
                .first()
                .is_some_and(|result| result.created())
        })
        .count();
    assert_eq!(created, 1, "{first:?} {second:?} {}", w.over());
    Ok(())
}

#[when("a ConditionalWrite holds a compare-and-set whose expected value does not hold")]
fn failed_condition(w: &mut Steps) -> TestResult {
    let [a, status, seen] = ["a", "status", "seen"].map(|n| w.iri(n));
    let (refused, kept, unseen) = in_shared(w, async |client| {
        declare(client, DeclaredPredicate::new(&status).with_single(true)).await?;
        committed(client, &a, &status, &string("done")).await?;
        let mut terms = TermTable::new();
        let change = WriteChange::Insert {
            graph: None,
            subject: terms.iri(&a),
            predicate: terms.iri(&seen),
            object: terms.intern(string("true"))?,
        };
        let operation = CommitOperation::CompareAndSet {
            graph: None,
            subject: terms.iri(&a),
            predicate: terms.iri(&status),
            expected: terms.intern(string("open"))?,
            new: terms.intern(string("closed"))?,
        };
        let write = ConditionalWrite::new(TENANT, terms, vec![change], vec![operation]);
        let refused = client
            .conditional_write(write, None)
            .await
            .err()
            .context("a refusal")?;
        Ok((
            refused,
            latest_read(client, &Pattern::of(Some(&a), Some(&status))).await?,
            latest_read(client, &Pattern::of(Some(&a), Some(&seen))).await?,
        ))
    })?;
    w.seen.refusals = vec![refused];
    w.seen.answers = vec![kept, unseen];
    Ok(())
}

#[then(
    "it is refused with `OperationFailed` and the value found, is not retried, and nothing it holds is written"
)]
fn condition_answered(w: &mut Steps) -> TestResult {
    let refused = w.seen.refusals.first().context("the refusal")?;
    let ClientErrorEnum::Refused { code, value, .. } = &refused.0 else {
        return Err(TestError::new(format!("a refusal: {refused:?}")));
    };
    assert_eq!(*code, ErrorCode::OperationFailed, "{refused:?}");
    assert_eq!(value, &Some(string("done")), "{refused:?}");
    let [kept, unseen] = w.seen.answers.as_slice() else {
        return Err(TestError::new("two answers"));
    };
    assert_eq!(kept.column(0), ["done"], "{}", w.over());
    nothing(unseen)
}

#[when("a DirectWrite holds a declaration of a predicate")]
fn declaration_in_write(w: &mut Steps) -> TestResult {
    let p = w.iri("p");
    let (refused, described) = in_shared(w, async |client| {
        let write = DirectWrite::new(
            TENANT,
            TermTable::new(),
            vec![WriteChange::Declare(
                ultragraphdb_protocol_wire::Declared::Predicate(DeclaredPredicate::new(&p)),
            )],
        );
        let refused = client
            .direct_write(write, None)
            .await
            .err()
            .context("a refusal")?;
        Ok((refused, client.describe(TENANT).await?))
    })?;
    w.seen.refusals = vec![refused];
    w.seen.facts = described
        .predicates()
        .iter()
        .map(|declared| declared.predicate().to_string())
        .collect();
    Ok(())
}

#[then("it is refused with a protocol error, and the catalog is unchanged")]
fn declaration_refused(w: &mut Steps) -> TestResult {
    let refused = w.seen.refusals.first().context("the refusal")?;
    let protocol = matches!(
        &refused.0,
        ClientErrorEnum::Request { .. } | ClientErrorEnum::Protocol { .. }
    ) || refusal_of(refused).is_ok_and(|(code, _, _)| code == ErrorCode::Protocol);
    assert!(protocol, "{refused:?}");
    assert!(!w.seen.facts.contains(&w.iri("p")), "{:?}", w.seen.facts);
    Ok(())
}

#[when(
    "the sequencer's open epoch is 30, and a Begin, a DirectWrite and a ConditionalWrite each carry a token of epoch 31"
)]
fn token_above_open(w: &mut Steps) -> TestResult {
    let [a, p, b, id] = ["a", "p", "b", "id"].map(|n| w.iri(n));
    let far = Epoch::new(1 << 40)?;
    w.seen.refusals = in_shared(w, async |client| {
        let token = SessionToken::new(1, 1, far);
        let begin = Begin::new(TENANT, 0, true).with_token(token.clone());
        let begun = client.begin_with(begin, None).await;
        let direct = direct_of(&a, &p, &iri(&b))?.with_token(token.clone());
        let directed = client.direct_write(direct, None).await;
        let conditional = claim(&id, "x")?.with_token(token);
        let conditioned = client.conditional_write(conditional, None).await;
        [
            begun.map(|_| ()),
            directed.map(|_| ()),
            conditioned.map(|_| ()),
        ]
        .into_iter()
        .map(|outcome| {
            outcome
                .err()
                .context("a token above the open epoch refused")
        })
        .collect::<Result<Vec<ClientError>, _>>()
    })?;
    w.seen.facts = vec![far.get().to_string()];
    Ok(())
}

#[then("each is refused with Error 1 on its stream, the message naming epoch 31")]
fn token_above_refused(w: &mut Steps) -> TestResult {
    let epoch = w.seen.facts.first().context("the epoch")?;
    assert_eq!(w.seen.refusals.len(), 3, "{}", w.over());
    for refused in &w.seen.refusals {
        let (code, _, message) = refusal_of(refused)?;
        assert_eq!(code, ErrorCode::Protocol, "{refused:?}");
        assert!(message.contains(epoch.as_str()), "{message}");
    }
    Ok(())
}

#[when(
    "a client's session commits a DirectWrite whose reply's token is of epoch 12, resolves a resend whose stored reply's is of epoch 9, then sends a ConditionalWrite through the session"
)]
fn session_token_kept(w: &mut Steps) -> TestResult {
    let [a, p, b, id] = ["a", "p", "b", "id"].map(|n| w.iri(n));
    let (kept, written) = in_shared(w, async |client| {
        declare(client, unique(&id)).await?;
        let session = client.register().await?;
        let written = client
            .direct_write(direct_of(&a, &p, &iri(&b))?, session)
            .await?;
        let kept = client.session_token(session).cloned();
        let _conditioned = client.conditional_write(claim(&id, "x")?, session).await?;
        Ok((kept, written))
    })?;
    w.seen.facts = vec![format!("{kept:?}"), format!("{:?}", written.token())];
    Ok(())
}

#[then("the ConditionalWrite carries the token of epoch 12")]
fn token_carried(w: &mut Steps) -> TestResult {
    let [kept, written] = w.seen.facts.as_slice() else {
        return Err(TestError::new("two tokens"));
    };
    assert_eq!(kept, written, "{}", w.over());
    Ok(())
}

#[when(
    "a client's session holds a token of epoch 12, and a commit through no session answers the zero token"
)]
fn zero_token(w: &mut Steps) -> TestResult {
    let [a, p, b, c] = ["a", "p", "b", "c"].map(|n| w.iri(n));
    let (before, after) = in_shared(w, async |client| {
        let session = client.register().await?;
        let _written = client
            .direct_write(direct_of(&a, &p, &iri(&b))?, session)
            .await?;
        let before = client.session_token(session).cloned();
        let _plain = client
            .direct_write(direct_of(&a, &p, &iri(&c))?, None)
            .await?;
        Ok((before, client.session_token(session).cloned()))
    })?;
    w.seen.facts = vec![format!("{before:?}"), format!("{after:?}")];
    w.committed = before.is_some();
    Ok(())
}

#[then("the session still sends the token of epoch 12")]
fn still_token(w: &mut Steps) -> TestResult {
    let [before, after] = w.seen.facts.as_slice() else {
        return Err(TestError::new("two tokens"));
    };
    assert!(w.committed, "a token held {}", w.over());
    assert_eq!(before, after, "{}", w.over());
    Ok(())
}

#[when(
    "a read-only transaction at snapshot level names a read version in epoch 12, and carries a token of epoch 20"
)]
fn named_version_with_token(w: &mut Steps) -> TestResult {
    let [a, p, b, c] = ["a", "p", "b", "c"].map(|n| w.iri(n));
    w.seen.answers = in_shared(w, async |client| {
        let session = client.register().await?;
        let early = client
            .direct_write(direct_of(&a, &p, &iri(&b))?, session)
            .await?;
        let pattern = Pattern::of(Some(&a), Some(&p));
        let _closed = until_rows(client, &pattern, 1).await?;
        let later = client
            .direct_write(direct_of(&a, &p, &iri(&c))?, session)
            .await?;
        let token = later.token().context("a token")?;
        let _later_closed = until_rows(client, &pattern, 2).await?;
        let begin = Begin::new(TENANT, early.version(), true).with_token(token.clone());
        let reader = client.begin_with(begin, None).await?;
        let answer = read_whole(client, &reader, &pattern).await?;
        client.abort(reader).await?;
        Ok(vec![answer])
    })?;
    Ok(())
}

#[then("its reads answer at the named version, and none waits for epoch 20")]
fn named_version_read(w: &mut Steps) -> TestResult {
    let answer = w.seen.answers.first().context("the read's answer")?;
    assert_eq!(answer.column(0), [w.iri("b")], "{}", w.over());
    Ok(())
}

/// An IRI of tenant 7's skolem base, which tenant 0 can hold no node of.
fn foreign_skolem() -> String {
    format!("{SYSTEM_VOCABULARY}tenant/7/.well-known/genid/000000-00000000000000000000000000")
}

#[when(
    "a read-write transaction's first request is a Read binding its subject to an IRI under tenant 7's skolem base in tenant 0, and its next request is a Write of `<a>`"
)]
fn foreign_skolem_read(w: &mut Steps) -> TestResult {
    let [a, p, b] = ["a", "p", "b"].map(|n| w.iri(n));
    let (page, committed_write) = in_shared(w, async |client| {
        let own = client.begin(TENANT, false).await?;
        let page = read_whole(
            client,
            &own,
            &Pattern::of(Some(&foreign_skolem()), Some(&p)),
        )
        .await?;
        client.write(&own, &write_of(&a, &p, &iri(&b))?).await?;
        Ok((page, client.commit(own, None).await?))
    })?;
    w.seen.answers = vec![page];
    w.seen.commits = vec![committed_write];
    Ok(())
}

#[then(
    "the Read is answered one batch flagged `LAST`, `rows` 0, no role sees it, and the transaction binds to `<a>`'s partition"
)]
fn skolem_bound_nothing(w: &mut Steps) -> TestResult {
    nothing(w.seen.answers.first().context("the read's answer")?)?;
    let written = w.seen.commits.first().context("the commit")?;
    assert!(written.version() > 0, "{written:?} {}", w.over());
    Ok(())
}

/// A Write whose subject is `subject`, and the refusal it takes on its stream.
async fn subject_refused(
    client: &mut TestClient,
    subject: &str,
    p: &str,
) -> TestResult<ClientError> {
    let own = client.begin(TENANT, false).await?;
    client
        .write(&own, &write_of(subject, p, &string("x"))?)
        .await?;
    client
        .commit(own, None)
        .await
        .err()
        .context("the Write refused")
}

#[when(
    "a Write names as a subject an IRI under the tenant's skolem base whose tail states no bucket, and another under tenant 7's"
)]
fn skolem_in_change(w: &mut Steps) -> TestResult {
    let p = w.iri("p");
    w.seen.refusals = in_shared(w, async |client| {
        Ok(vec![
            subject_refused(
                client,
                &format!("{SYSTEM_VOCABULARY}tenant/0/.well-known/genid/nothing-here"),
                &p,
            )
            .await?,
            subject_refused(client, &foreign_skolem(), &p).await?,
        ])
    })?;
    Ok(())
}

#[then("each is refused with Error 7, `detail` the term's index, and no partition is asked")]
fn skolem_change_refused(w: &mut Steps) -> TestResult {
    assert_eq!(w.seen.refusals.len(), 2, "{}", w.over());
    for refused in &w.seen.refusals {
        assert_eq!(refusal_of(refused)?.0, ErrorCode::Refused, "{refused:?}");
    }
    Ok(())
}

#[when(
    "a Write names `https://example.org/tenant/0/.well-known/genid/00abcd-0123456789abcdef0123456789`"
)]
fn same_path_elsewhere(w: &mut Steps) -> TestResult {
    let p = w.iri("p");
    let subject = format!(
        "https://example.org/tenant/0/.well-known/genid/00abcd-0123456789abcdef0123456789{}",
        w.scenario
    );
    w.seen.answers = in_shared(w, async |client| {
        committed(client, &subject, &p, &string("x")).await?;
        Ok(vec![
            latest_read(client, &Pattern::of(Some(&subject), Some(&p))).await?,
        ])
    })?;
    Ok(())
}

#[then("it is routed by its text's hash and stored as any IRI")]
fn stored_as_iri(w: &mut Steps) -> TestResult {
    let answer = w.seen.answers.first().context("the read's answer")?;
    assert_eq!(answer.column(0), ["x"], "{}", w.over());
    Ok(())
}

#[when("a read returns rows whose subjects are `http://ex.org/a` and `http://ex.org/b`")]
fn subjects_by_text(w: &mut Steps) -> TestResult {
    let [a, b, p] = ["a", "b", "p"].map(|n| w.iri(n));
    w.seen.answers = in_shared(w, async |client| {
        committed(client, &a, &p, &string("x")).await?;
        committed(client, &b, &p, &string("x")).await?;
        Ok(vec![
            latest_read(client, &Pattern::of(None, Some(&p))).await?,
        ])
    })?;
    Ok(())
}

#[then(
    "the batch's dictionary holds both IRIs' texts and the rows name them by dictionary index, with no system ID"
)]
fn subjects_read(w: &mut Steps) -> TestResult {
    let answer = w.seen.answers.first().context("the read's answer")?;
    let mut subjects = answer.column(0);
    subjects.sort();
    assert_eq!(subjects, [w.iri("a"), w.iri("b")], "{}", w.over());
    Ok(())
}

/// A one-frame write inserting a statement whose subject is a blank node.
fn blank_subject(p: &str, value: &str) -> TestResult<DirectWrite> {
    let mut terms = TermTable::new();
    let change = WriteChange::Insert {
        graph: None,
        subject: terms.intern(RequestTerm::BlankNode("b1".to_string()))?,
        predicate: terms.iri(p),
        object: terms.intern(string(value))?,
    };
    Ok(DirectWrite::new(TENANT, terms, vec![change]))
}

/// A row's first cell's text and whether it is a blank node.
fn first_cell(batch: &ResultView<'_>) -> Option<(String, bool)> {
    let cell = batch.cell(0, 0)?;
    let term = batch.term(cell)?;
    Some((text_of(&term), term.is_blank_node()))
}

#[when("a read returns a row whose subject is a blank node")]
fn blank_result(w: &mut Steps) -> TestResult {
    let p = w.iri("p");
    let cell = in_shared(w, async |client| {
        let _written = client.direct_write(blank_subject(&p, "x")?, None).await?;
        let reader = client.begin(TENANT, false).await?;
        let pattern = Pattern::of(None, Some(&p));
        let stream = client
            .read(&reader, &pattern.read(&reader, 0, Vec::new())?)
            .await?;
        let cell = client.next_batch(&stream, first_cell).await?.flatten();
        client.abort(reader).await?;
        cell.context("the blank node's cell")
    })?;
    w.seen.facts = vec![cell.0];
    w.committed = cell.1;
    Ok(())
}

#[then(
    "the batch names it, tagged as a blank node, by its skolem IRI `…/.well-known/genid/{bucket}-{random}`, whose text holds no type prefix and no sequence of its system ID"
)]
fn blank_named(w: &mut Steps) -> TestResult {
    assert!(w.committed, "tagged as a blank node {}", w.over());
    let text = w.seen.facts.first().context("the text")?;
    let tail = text.rsplit('/').next().context("a tail")?;
    let (bucket, random) = tail.split_once('-').context("a bucket and a random part")?;
    assert_eq!(bucket.len(), 6, "{text}");
    assert_eq!(random.len(), 26, "{text}");
    assert!(text.contains("/.well-known/genid/"), "{text}");
    Ok(())
}

/// The refusal a read of `pattern` takes.
async fn read_refused(client: &mut TestClient, pattern: &Pattern) -> TestResult<ClientError> {
    let own = client.begin(TENANT, true).await?;
    let stream = client
        .read(&own, &pattern.read(&own, 0, Vec::new())?)
        .await?;
    let refused = client.next_batch(&stream, batch_of).await.err();
    client.abort(own).await?;
    refused.context("the read refused")
}

#[when("a Read's subject slot holds a literal, and another's graph slot a blank node's label")]
fn literal_at_subject(w: &mut Steps) -> TestResult {
    let p = w.iri("p");
    w.seen.refusals = in_shared(w, async |client| {
        let at_subject = Pattern::of(None, Some(&p)).with_subject(string("literal"));
        let at_graph = at_graph_label(&p)?;
        Ok(vec![
            read_refused(client, &at_subject).await?,
            at_graph_refused(client, at_graph).await?,
        ])
    })?;
    Ok(())
}

/// A pattern's terms with a blank node's label at its graph slot.
fn at_graph_label(p: &str) -> TestResult<(TermTable, ultragraphdb_protocol_wire::QuadPattern)> {
    let mut terms = TermTable::new();
    let graph = terms.intern(RequestTerm::BlankNode("g".to_string()))?;
    let predicate = terms.iri(p);
    let pattern = ultragraphdb_protocol_wire::QuadPattern::new(
        ultragraphdb_protocol_wire::PatternSlot::Bound(graph),
        ultragraphdb_protocol_wire::PatternSlot::Unbound,
        ultragraphdb_protocol_wire::PatternSlot::Bound(predicate),
        ultragraphdb_protocol_wire::PatternSlot::Unbound,
    );
    Ok((terms, pattern))
}

/// The refusal a read of the terms and pattern `read` takes.
async fn at_graph_refused(
    client: &mut TestClient,
    (terms, pattern): (TermTable, ultragraphdb_protocol_wire::QuadPattern),
) -> TestResult<ClientError> {
    let own = client.begin(TENANT, true).await?;
    let read = ultragraphdb_protocol_wire::Read::new(own.stream(), 0, terms, pattern, Vec::new());
    let stream = client.read(&own, &read).await?;
    let refused = client.next_batch(&stream, batch_of).await.err();
    client.abort(own).await?;
    refused.context("the read refused")
}

/// That each refusal seen is code 7, its message naming a rule.
fn each_refused_seven(w: &Steps) -> TestResult {
    assert_eq!(w.seen.refusals.len(), 2, "{}", w.over());
    for refused in &w.seen.refusals {
        let (code, _, message) = refusal_of(refused)?;
        assert_eq!(code, ErrorCode::Refused, "{refused:?}");
        assert!(!message.is_empty(), "{refused:?}");
    }
    Ok(())
}

#[then("each is refused with Error 7, `detail` the term's index, the message naming the rule")]
fn each_seven(w: &mut Steps) -> TestResult {
    each_refused_seven(w)
}

#[when("a Read's object slot holds a blank node's label, and another's a triple term")]
fn label_at_object(w: &mut Steps) -> TestResult {
    let [a, p, b] = ["a", "p", "b"].map(|n| w.iri(n));
    w.seen.refusals = in_shared(w, async |client| {
        let label =
            Pattern::of(None, Some(&p)).with_object(RequestTerm::BlankNode("o".to_string()));
        let triple = triple_at_object(&a, &p, &b)?;
        Ok(vec![
            read_refused(client, &label).await?,
            at_graph_refused(client, triple).await?,
        ])
    })?;
    Ok(())
}

/// A pattern's terms with the triple term of `a`, `p` and `b` at its object slot.
fn triple_at_object(
    a: &str,
    p: &str,
    b: &str,
) -> TestResult<(TermTable, ultragraphdb_protocol_wire::QuadPattern)> {
    let mut terms = TermTable::new();
    let subject = terms.iri(a);
    let predicate = terms.iri(p);
    let object = terms.iri(b);
    let triple = terms.intern(RequestTerm::Triple {
        subject,
        predicate,
        object,
    })?;
    let pattern = ultragraphdb_protocol_wire::QuadPattern::new(
        ultragraphdb_protocol_wire::PatternSlot::DefaultGraph,
        ultragraphdb_protocol_wire::PatternSlot::Unbound,
        ultragraphdb_protocol_wire::PatternSlot::Bound(predicate),
        ultragraphdb_protocol_wire::PatternSlot::Bound(triple),
    );
    Ok((terms, pattern))
}

#[when("a Write's object is a triple term")]
fn triple_in_change(w: &mut Steps) -> TestResult {
    let [a, p, b] = ["a", "p", "b"].map(|n| w.iri(n));
    let (refused, next) = in_shared(w, async |client| {
        let own = client.begin(TENANT, false).await?;
        let mut terms = TermTable::new();
        let (subject, predicate, object) = (terms.iri(&a), terms.iri(&p), terms.iri(&b));
        let triple = terms.intern(RequestTerm::Triple {
            subject,
            predicate,
            object,
        })?;
        let change = WriteChange::Insert {
            graph: None,
            subject,
            predicate,
            object: triple,
        };
        client.write(&own, &Write::new(terms, vec![change])).await?;
        let refused = client
            .commit(own, None)
            .await
            .err()
            .context("the Write refused")?;
        let next = client.commit(own, None).await.err();
        Ok((refused, next))
    })?;
    w.seen.refusals = vec![refused];
    w.seen.facts = vec![format!("{next:?}")];
    Ok(())
}

#[then("the Write is refused with Error 7, `detail` the term's index, and the transaction ends")]
fn triple_refused(w: &mut Steps) -> TestResult {
    let refused = w.seen.refusals.first().context("the refusal")?;
    assert_eq!(refusal_of(refused)?.0, ErrorCode::Refused, "{refused:?}");
    let next = w.seen.facts.first().context("the next request's answer")?;
    assert!(next.starts_with("Some"), "{next} {}", w.over());
    Ok(())
}

#[when("a Read names as its subject a skolem IRI no dictionary holds")]
fn forged_read(w: &mut Steps) -> TestResult {
    let p = w.iri("p");
    let forged =
        format!("{SYSTEM_VOCABULARY}tenant/0/.well-known/genid/000000-00000000000000000000000001");
    let (found, abort) = in_shared(w, async |client| {
        let own = client.begin(TENANT, false).await?;
        let found = read_whole(client, &own, &Pattern::of(Some(&forged), Some(&p))).await?;
        let abort = client.commit(own, None).await;
        Ok((found, abort.is_ok()))
    })?;
    w.seen.answers = vec![found];
    w.committed = abort;
    Ok(())
}

#[then("the read answers one batch flagged `LAST` with no row, and its transaction goes on")]
fn forged_nothing(w: &mut Steps) -> TestResult {
    nothing(w.seen.answers.first().context("the read's answer")?)?;
    assert!(
        w.committed,
        "the transaction went on to its commit {}",
        w.over()
    );
    Ok(())
}

#[when(
    "`name` is declared a property with a `term` index, `score` holds a default entry, and a client sends Describe for the tenant"
)]
fn describe_scene(w: &mut Steps) -> TestResult {
    let [name, score, a] = ["name", "score", "a"].map(|n| w.iri(n));
    let described = in_shared(w, async |client| {
        let declared = DeclaredPredicate::new(&name)
            .with_kind(DeclaredKind::Property)
            .with_tokenizers(vec!["term".to_string()]);
        declare(client, declared).await?;
        committed(client, &a, &score, &string("high")).await?;
        for _ in 0..500 {
            let described = client.describe(TENANT).await?;
            // The version takes effect at the epoch after the close that assigned it, so the Commit's answer may come first.
            let in_effect = described
                .predicates()
                .iter()
                .any(|held| held.predicate() == name);
            if in_effect && described.property_keys().contains(&score) {
                return Ok(described);
            }
            pause(std::time::Duration::from_millis(10)).await;
        }
        Err(TestError::new(format!(
            "Describe did not come to declare {name} and name {score} within 5 s"
        )))
    })?;
    w.described = Some(described);
    Ok(())
}

#[then(
    "the frontend answers asking no other role: `read_version` is the version bound of the latest closed epoch, the declarations list `name` with its `term` index, the property keys hold `name` and `score`, and the relationship types hold `score`"
)]
fn described(w: &mut Steps) -> TestResult {
    let described = w.described.as_ref().context("the Describe")?;
    let [name, score] = ["name", "score"].map(|n| w.iri(n));
    assert!(described.read_version() > 0, "{described:?}");
    let declared = described
        .predicates()
        .iter()
        .find(|declared| declared.predicate() == name)
        .context("name declared")?;
    assert_eq!(declared.tokenizers(), ["term".to_string()], "{}", w.over());
    let keys = described.property_keys();
    assert!(keys.contains(&name) && keys.contains(&score), "{keys:?}");
    assert!(
        described.relationship_types().contains(&score),
        "{described:?}"
    );
    Ok(())
}

#[when(
    "a transaction's insert-if-absent finds the node `http://ex.org/x` holding the value, and the transaction commits"
)]
fn insert_finds_node(w: &mut Steps) -> TestResult {
    let [x, id] = ["x", "id"].map(|n| w.iri(n));
    w.seen.commits = in_shared(w, async |client| {
        declare(client, unique(&id)).await?;
        let holding = client.begin(TENANT, false).await?;
        client
            .write(&holding, &write_of(&x, &id, &string("value"))?)
            .await?;
        let _held = client.commit(holding, None).await?;
        let mut terms = TermTable::new();
        let operation = CommitOperation::InsertIfAbsent {
            graph: None,
            predicate: terms.iri(&id),
            value: terms.intern(string("value"))?,
            properties: Vec::new(),
        };
        let own = client.begin(TENANT, false).await?;
        client
            .operation(&own, &Operation::new(terms, operation))
            .await?;
        Ok(vec![client.commit(own, None).await?])
    })?;
    Ok(())
}

#[then("the Commit reply names `http://ex.org/x` by its text, and holds no system ID")]
fn node_named(w: &mut Steps) -> TestResult {
    let reply = w.seen.commits.first().context("the reply")?;
    assert_eq!(reply.node(0), Some(w.iri("x").as_str()), "{reply:?}");
    let result = reply.results().first().context("a result")?;
    assert!(!result.created(), "{reply:?} {}", w.over());
    Ok(())
}

#[when(
    "a transaction in the partition owning the bucket of `\"w\"` insert-if-absents a node with `epi.id` `\"w\"`, declared unique, and no node holds it"
)]
fn created_node(w: &mut Steps) -> TestResult {
    let id = w.iri("epi.id");
    w.seen.commits = in_shared(w, async |client| {
        declare(client, unique(&id)).await?;
        let mut terms = TermTable::new();
        let operation = CommitOperation::InsertIfAbsent {
            graph: None,
            predicate: terms.iri(&id),
            value: terms.intern(string("w"))?,
            properties: Vec::new(),
        };
        let own = client.begin(TENANT, false).await?;
        client
            .operation(&own, &Operation::new(terms, operation))
            .await?;
        Ok(vec![client.commit(own, None).await?])
    })?;
    Ok(())
}

#[then(
    "the reply names the created node by its skolem IRI, says it was created, and holds no system ID"
)]
fn created_named(w: &mut Steps) -> TestResult {
    let reply = w.seen.commits.first().context("the reply")?;
    let node = reply.node(0).context("the node")?;
    assert!(node.contains("/.well-known/genid/"), "{node}");
    assert!(
        reply
            .results()
            .first()
            .is_some_and(|result| result.created()),
        "{reply:?}"
    );
    Ok(())
}

#[when("the counter `next-id` holds 5, and two transactions each add 1 to it")]
fn counter_two(w: &mut Steps) -> TestResult {
    let [a, next_id] = ["a", "next-id"].map(|n| w.iri(n));
    let add = |delta: i64| -> TestResult<Operation> {
        let mut terms = TermTable::new();
        let operation = CommitOperation::CounterAdd {
            graph: None,
            subject: terms.iri(&a),
            predicate: terms.iri(&next_id),
            delta,
        };
        Ok(Operation::new(terms, operation))
    };
    let (replies, held) = in_shared(w, async |client| {
        declare(client, DeclaredPredicate::new(&next_id).with_single(true)).await?;
        let mut replies = Vec::new();
        for delta in [5, 1, 1] {
            let own = client.begin(TENANT, false).await?;
            client.operation(&own, &add(delta)?).await?;
            replies.push(client.commit(own, None).await?);
        }
        Ok((
            replies,
            latest_read(client, &Pattern::of(Some(&a), Some(&next_id))).await?,
        ))
    })?;
    w.seen.commits = replies;
    w.seen.answers = vec![held];
    Ok(())
}

#[then("both commit, the one committed first receives 6 and the other 7, and `next-id` holds 7")]
fn counter_values(w: &mut Steps) -> TestResult {
    let values: Vec<i64> = w
        .seen
        .commits
        .iter()
        .filter_map(|reply| reply.results().first().map(|result| result.value()))
        .collect();
    assert_eq!(values, [5, 6, 7], "{}", w.over());
    Ok(())
}

#[when(
    "a transaction inserts `<a> <seen> true` and compare-and-sets `status` of `a` from `\"open\"` to `\"closed\"` while `status` is `\"done\"`"
)]
fn cas_in_transaction(w: &mut Steps) -> TestResult {
    let [a, seen, status] = ["a", "seen", "status"].map(|n| w.iri(n));
    let (refused, kept, unseen) = in_shared(w, async |client| {
        declare(client, DeclaredPredicate::new(&status).with_single(true)).await?;
        committed(client, &a, &status, &string("done")).await?;
        let own = client.begin(TENANT, false).await?;
        client
            .write(&own, &write_of(&a, &seen, &string("true"))?)
            .await?;
        let mut terms = TermTable::new();
        let operation = CommitOperation::CompareAndSet {
            graph: None,
            subject: terms.iri(&a),
            predicate: terms.iri(&status),
            expected: terms.intern(string("open"))?,
            new: terms.intern(string("closed"))?,
        };
        client
            .operation(&own, &Operation::new(terms, operation))
            .await?;
        let refused = client.commit(own, None).await.err().context("a refusal")?;
        Ok((
            refused,
            latest_read(client, &Pattern::of(Some(&a), Some(&status))).await?,
            latest_read(client, &Pattern::of(Some(&a), Some(&seen))).await?,
        ))
    })?;
    w.seen.refusals = vec![refused];
    w.seen.answers = vec![kept, unseen];
    Ok(())
}

#[then(
    "the transaction is refused reporting `\"done\"`, `status` of `a` stays `\"done\"`, and `<a> <seen> true` is not held"
)]
fn cas_refused(w: &mut Steps) -> TestResult {
    condition_answered(w)
}

#[when(
    "a literal `\"0.1234567890123456789\"^^xsd:decimal`, of 19 fraction digits, is written and read back"
)]
fn long_decimal(w: &mut Steps) -> TestResult {
    let [a, p] = ["a", "p"].map(|n| w.iri(n));
    let lexical = "0.1234567890123456789";
    let term = typed(lexical, "http://www.w3.org/2001/XMLSchema#decimal");
    let (tag, text, length) = in_shared(w, async |client| {
        committed(client, &a, &p, &term).await?;
        let reader = client.begin(TENANT, false).await?;
        let pattern = Pattern::of(Some(&a), Some(&p));
        let stream = client
            .read(&reader, &pattern.read(&reader, 0, Vec::new())?)
            .await?;
        let cell = client
            .next_batch(&stream, |batch| {
                let cell = batch.cell(0, 0)?;
                let read = batch.term(cell)?;
                Some((
                    read.encoding().first().copied()?,
                    read.text()?.to_string(),
                    read.encoding().len(),
                ))
            })
            .await?
            .flatten();
        client.abort(reader).await?;
        cell.context("the literal's cell")
    })?;
    w.seen.facts = vec![
        format!("{tag:#x}"),
        text,
        length.to_string(),
        lexical.to_string(),
    ];
    Ok(())
}

#[then(
    "its term carries tag `0x31`, a known datatype's code and the hash of its lexical form, and the batch's text holds the lexical form unchanged"
)]
fn decimal_kept(w: &mut Steps) -> TestResult {
    let [tag, text, length, lexical] = w.seen.facts.as_slice() else {
        return Err(TestError::new("what the term held"));
    };
    assert_eq!(tag, "0x31", "{}", w.over());
    assert_eq!(text, lexical, "{}", w.over());
    // The tag, the datatype's code and the 16 bytes of the lexical form's hash (`PROTOCOL.md` §10.1).
    assert_eq!(length, "18", "{}", w.over());
    Ok(())
}

#[when("a read returns `<a> <knows> <b>` in `<g1>`, which carries the property `<since> 2020`")]
fn with_properties(w: &mut Steps) -> TestResult {
    let [a, b, knows, since, g1] = ["a", "b", "knows", "since", "g1"].map(|n| w.iri(n));
    w.seen.answers = in_shared(w, async |client| {
        let mut terms = TermTable::new();
        let change = WriteChange::InsertEdge {
            graph: Some(terms.iri(&g1)),
            subject: terms.iri(&a),
            predicate: terms.iri(&knows),
            object: terms.iri(&b),
            properties: vec![(terms.iri(&since), terms.intern(integer(2020))?)],
        };
        let write = DirectWrite::new(TENANT, terms, vec![change]);
        let _edge = client.direct_write(write, None).await?;
        let pattern = Pattern::of(Some(&a), Some(&knows)).in_graph(GraphAt::Any);
        Ok(vec![latest_read(client, &pattern).await?])
    })?;
    Ok(())
}

#[then(
    "the row names the statement by `<a>`, `<knows>`, `<b>` and `<g1>` and by its reifier's skolem IRI, and the batch holds no statement ID"
)]
fn named_by_reifier(w: &mut Steps) -> TestResult {
    let answer = w.seen.answers.first().context("the read's answer")?;
    assert_eq!(answer.column(0), [w.iri("g1")], "{}", w.over());
    assert_eq!(answer.column(1), [w.iri("b")], "{}", w.over());
    let reifiers = answer.column(2);
    assert!(
        reifiers
            .first()
            .is_some_and(|reifier| reifier.contains("/.well-known/genid/")),
        "{reifiers:?}"
    );
    Ok(())
}

#[when(
    "`<a> <knows> <b>` carries `<since> 2020` under the reifier `r`, and a read binds its subject to `r`"
)]
fn read_of_reifier(w: &mut Steps) -> TestResult {
    let [a, b, knows, since] = ["a", "b", "knows", "since"].map(|n| w.iri(n));
    w.seen.answers = in_shared(w, async |client| {
        let _edge = client
            .direct_write(edge_of((&a, &knows, &b), &since, "2020")?, None)
            .await?;
        let named = latest_read(client, &Pattern::of(Some(&a), Some(&knows))).await?;
        let reifier = named.column(1).first().cloned().context("its reifier")?;
        Ok(vec![
            latest_read(client, &Pattern::of(Some(&reifier), None)).await?,
        ])
    })?;
    Ok(())
}

#[then(
    "compute reads the statement's record in the statements keyspace of the partition `r`'s bucket names, and returns one row for `<since>` and `2020` with `ABSENT` in the reifier column"
)]
fn record_read(w: &mut Steps) -> TestResult {
    let answer = w.seen.answers.first().context("the read's answer")?;
    assert_eq!(
        answer.rows,
        [[w.iri("since"), "2020".to_string(), String::new()]],
        "{}",
        w.over()
    );
    Ok(())
}

#[when("a client drops graph `g` of a tenant holding statements in `g` and in `h`")]
fn graph_drop(w: &mut Steps) -> TestResult {
    let [a, p, b, g, h] = ["a", "p", "b", "g", "h"].map(|n| w.iri(n));
    let (dropped, kept, gone) = in_shared(w, async |client| {
        for graph in [&g, &h] {
            let write = in_graph(graph, &a, &p, &iri(&b))?;
            let _written = client.direct_write(write, None).await?;
        }
        let session = client.register().await?;
        let dropped = client
            .drop_command(DropCommand::graph(TENANT, &g), session)
            .await?;
        let of = |graph: &str| {
            Pattern::of(Some(&a), Some(&p)).in_graph(GraphAt::Named(graph.to_string()))
        };
        Ok((
            dropped,
            latest_read(client, &of(&h)).await?,
            latest_read(client, &of(&g)).await?,
        ))
    })?;
    w.seen.commits = vec![dropped];
    w.seen.answers = vec![kept, gone];
    Ok(())
}

#[then(
    "the reply carries the drop's commit version, no statement of `g` remains, and every statement of `h` remains"
)]
fn graph_dropped(w: &mut Steps) -> TestResult {
    let dropped = w.seen.commits.first().context("the reply")?;
    assert!(dropped.version() > 0, "{dropped:?}");
    let [kept, gone] = w.seen.answers.as_slice() else {
        return Err(TestError::new("two answers"));
    };
    assert_eq!(kept.column(0), [w.iri("b")], "{}", w.over());
    nothing(gone)
}

#[when("a transaction reads before a drop of graph `g` commits and then writes into `g`")]
fn write_into_dropped(w: &mut Steps) -> TestResult {
    let [a, p, b, g, q] = ["a", "p", "b", "g", "q"].map(|n| w.iri(n));
    let refused = in_shared(w, async |client| {
        let held = in_graph(&g, &a, &p, &iri(&b))?;
        let _held = client.direct_write(held, None).await?;
        let own = client.begin(TENANT, false).await?;
        let _read = read_whole(client, &own, &Pattern::of(Some(&a), Some(&q))).await?;
        let session = client.register().await?;
        let _dropped = client
            .drop_command(DropCommand::graph(TENANT, &g), session)
            .await?;
        let mut terms = TermTable::new();
        let change = WriteChange::Insert {
            graph: Some(terms.iri(&g)),
            subject: terms.iri(&a),
            predicate: terms.iri(&p),
            object: terms.intern(string("x"))?,
        };
        client.write(&own, &Write::new(terms, vec![change])).await?;
        client.commit(own, None).await.err().context("a refusal")
    })?;
    w.seen.refusals = vec![refused];
    Ok(())
}

#[then("its commit is refused with a conflict of kind Drop")]
fn drop_conflict(w: &mut Steps) -> TestResult {
    let refused = w.seen.refusals.first().context("the refusal")?;
    let (detail, _) = refused_with::<()>(Err(refused.clone()), ErrorCode::Conflict)?;
    assert_eq!(
        detail,
        crate::transaction_steps::KIND_DROP,
        "{refused:?} {}",
        w.over()
    );
    Ok(())
}

#[when("`score` holds a default-entry record, and a Drop of `score` commits")]
fn default_entry_drop(w: &mut Steps) -> TestResult {
    let [a, score] = ["a", "score"].map(|n| w.iri(n));
    let keys = in_shared(w, async |client| {
        committed(client, &a, &score, &string("high")).await?;
        let session = client.register().await?;
        let _dropped = client
            .drop_command(DropCommand::predicate(TENANT, &score), session)
            .await?;
        for _ in 0..500 {
            let described = client.describe(TENANT).await?;
            if described.property_keys().contains(&score) {
                return Ok(described.property_keys().to_vec());
            }
            pause(std::time::Duration::from_millis(10)).await;
        }
        Err(TestError::new(format!(
            "Describe did not name {score} within 5 s"
        )))
    })?;
    w.seen.facts = keys;
    Ok(())
}

#[then("the metadata group still holds the record, and Describe names `score` as recorded")]
fn entry_outlives(w: &mut Steps) {
    assert!(
        w.seen.facts.contains(&w.iri("score")),
        "{:?} {}",
        w.seen.facts,
        w.over()
    );
}

#[when("a client's DirectWrite holds a `principal` above 0")]
fn naming_principal(w: &mut Steps) -> TestResult {
    let [a, p, b] = ["a", "p", "b"].map(|n| w.iri(n));
    let (refused, held) = in_shared(w, async |client| {
        let write = direct_of(&a, &p, &iri(&b))?.with_principal(1);
        let refused = client
            .direct_write(write, None)
            .await
            .err()
            .context("a refusal")?;
        Ok((
            refused,
            latest_read(client, &Pattern::of(Some(&a), Some(&p))).await?,
        ))
    })?;
    w.seen.facts = vec![refused.to_string()];
    w.seen.answers = vec![held];
    Ok(())
}

#[then(
    "it is refused at the client endpoint with `FieldValue` naming `principal`, and no partition sees it"
)]
fn principal_refused(w: &mut Steps) -> TestResult {
    let refused = w.seen.facts.first().context("the refusal")?;
    assert!(refused.contains("principal"), "{refused} {}", w.over());
    nothing(w.seen.answers.first().context("what is held")?)
}

/// A DirectWrite of no change, and a ConditionalWrite of no operation.
#[when("a DirectWrite holds no change, or a ConditionalWrite no operation")]
fn malformed_writes(w: &mut Steps) -> TestResult {
    let refusals = in_shared(w, async |client| {
        let direct = DirectWrite::new(TENANT, TermTable::new(), Vec::new());
        let conditional = ConditionalWrite::new(TENANT, TermTable::new(), Vec::new(), Vec::new());
        Ok(vec![
            client
                .direct_write(direct, None)
                .await
                .err()
                .context("no change refused")?,
            client
                .conditional_write(conditional, None)
                .await
                .err()
                .context("no operation refused")?,
        ])
    })?;
    w.seen.facts = refusals.iter().map(ToString::to_string).collect();
    Ok(())
}

#[then("it is refused with `CountMismatch`, naming `changes` or `operations`")]
fn malformed_refused(w: &mut Steps) -> TestResult {
    let [no_change, no_operation] = w.seen.facts.as_slice() else {
        return Err(TestError::new("two refusals"));
    };
    assert!(no_change.contains("changes"), "{no_change}");
    assert!(no_operation.contains("operations"), "{no_operation}");
    Ok(())
}

/// A Write holding a triple term, which the frontend refuses on its transaction's stream.
fn refused_write(a: &str, p: &str, b: &str) -> TestResult<Write> {
    let mut terms = TermTable::new();
    let (subject, predicate, object) = (terms.iri(a), terms.iri(p), terms.iri(b));
    let triple = terms.intern(RequestTerm::Triple {
        subject,
        predicate,
        object,
    })?;
    let change = WriteChange::Insert {
        graph: None,
        subject,
        predicate,
        object: triple,
    };
    Ok(Write::new(terms, vec![change]))
}

#[when("a client's Commit crosses the Error that ended its transaction's stream")]
fn commit_crossing(w: &mut Steps) -> TestResult {
    let [a, p, b] = ["a", "p", "b"].map(|n| w.iri(n));
    let (refused, open) = in_shared(w, async |client| {
        let own = client.begin(TENANT, false).await?;
        client.write(&own, &refused_write(&a, &p, &b)?).await?;
        let refused = client.commit(own, None).await.err().context("a refusal")?;
        Ok((refused, client.begin(TENANT, true).await.is_ok()))
    })?;
    w.seen.refusals = vec![refused];
    w.committed = open;
    Ok(())
}

#[then("the server drops the Commit, and the connection stays open")]
fn commit_dropped(w: &mut Steps) -> TestResult {
    let refused = w.seen.refusals.first().context("the refusal")?;
    assert_eq!(
        refusal_of(refused)?.0,
        ErrorCode::Refused,
        "the Write's Error {refused:?}"
    );
    assert!(
        w.committed,
        "the connection served the next request {}",
        w.over()
    );
    Ok(())
}

#[when("a client's Read crosses the Error that ended its transaction's stream")]
fn read_crossing(w: &mut Steps) -> TestResult {
    let [a, p, b] = ["a", "p", "b"].map(|n| w.iri(n));
    let (read, stream_end) = in_shared(w, async |client| {
        let own = client.begin(TENANT, false).await?;
        client.write(&own, &refused_write(&a, &p, &b)?).await?;
        let pattern = Pattern::of(Some(&a), Some(&p));
        let stream = client
            .read(&own, &pattern.read(&own, 0, Vec::new())?)
            .await?;
        let read = client
            .next_batch(&stream, batch_of)
            .await
            .err()
            .context("the read refused")?;
        Ok((
            read,
            client.commit(own, None).await.err().context("the end")?,
        ))
    })?;
    w.seen.refusals = vec![read, stream_end];
    Ok(())
}

#[then("the read is answered with that Error, as the transaction's reads still open are")]
fn read_took_error(w: &mut Steps) -> TestResult {
    let [read, stream_end] = w.seen.refusals.as_slice() else {
        return Err(TestError::new("two refusals"));
    };
    assert_eq!(refusal_of(read)?.0, ErrorCode::Refused, "{read:?}");
    let (_, read_detail, _) = refusal_of(read)?;
    let (_, end_detail, _) =
        refusal_of(stream_end).unwrap_or((ErrorCode::Refused, read_detail, String::new()));
    assert_eq!(
        read_detail,
        end_detail,
        "{read:?} {stream_end:?} {}",
        w.over()
    );
    Ok(())
}

#[when("a Result frame arrives for a read the client has cancelled")]
fn late_frame(w: &mut Steps) -> TestResult {
    let [a, p, b] = ["a", "p", "b"].map(|n| w.iri(n));
    let held = in_shared(w, async |client| {
        crate::read_steps::holding_big(client).await?;
        committed(client, &a, &p, &iri(&b)).await?;
        let own = client.begin(TENANT, true).await?;
        let stream = client
            .read(
                &own,
                &crate::read_steps::big_pattern().read(&own, 0, Vec::new())?,
            )
            .await?;
        // Cancelled at once, with the read's frames on their way: its credit lets four go.
        client.cancel(stream).await;
        pause(std::time::Duration::from_millis(100)).await;
        client.abort(own).await?;
        latest_read(client, &Pattern::of(Some(&a), Some(&p))).await
    })?;
    w.seen.answers = vec![held];
    Ok(())
}

#[then("the client checks the frame and drops it, and the connection stays open")]
fn late_frame_dropped(w: &mut Steps) -> TestResult {
    let held = w.seen.answers.first().context("the next read's answer")?;
    assert_eq!(held.column(0), [w.iri("b")], "{}", w.over());
    Ok(())
}

#[when("a commit is refused with code 2, and a read with code 24")]
fn what_runs_again(w: &mut Steps) -> TestResult {
    let [a, p] = ["a", "p"].map(|n| w.iri(n));
    let refused = in_shared(w, async |client| {
        let own = client.begin(TENANT, false).await?;
        nothing(&read_whole(client, &own, &Pattern::of(Some(&a), Some(&p))).await?)?;
        committed(client, &a, &p, &string("x")).await?;
        let kept = format!("{a}other");
        client
            .write(&own, &write_of(&kept, &p, &string("x"))?)
            .await?;
        client.commit(own, None).await.err().context("a conflict")
    })?;
    w.seen.refusals = vec![refused];
    Ok(())
}

#[then(
    "the client's error says of the first that the whole transaction runs again on a new Begin, and of the second that the read runs again"
)]
fn says_transaction(w: &mut Steps) -> TestResult {
    let refused = w.seen.refusals.first().context("the conflict")?;
    assert_eq!(refusal_of(refused)?.0, ErrorCode::Conflict, "{refused:?}");
    assert_eq!(
        refused.runs_again(),
        Some(ultragraphdb_client_compio::RunsAgain::Transaction),
        "{refused:?} {}",
        w.over()
    );
    Ok(())
}

/// A statement of the pattern `(default graph, $0, <predicate>, ?)`, prepared in tenant 0.
fn statement_of(predicate: &str) -> Prepare {
    let mut terms = TermTable::new();
    let bound = terms.iri(predicate);
    let pattern = QuadPattern::new(
        PatternSlot::DefaultGraph,
        PatternSlot::Parameter(0),
        PatternSlot::Bound(bound),
        PatternSlot::Unbound,
    );
    Prepare::new(
        TENANT,
        0,
        0,
        terms,
        pattern,
        vec![ColumnType::Node],
        Vec::new(),
    )
}

/// An execution of `statement` with `term` as its parameter, in `transaction`.
fn execution(
    statement: u64,
    transaction: &ultragraphdb_client_compio::Transaction,
    term: RequestTerm,
) -> TestResult<Execute> {
    let mut terms = TermTable::new();
    let parameter = terms.intern(term)?;
    Ok(Execute::new(
        statement,
        transaction.stream(),
        0,
        terms,
        vec![parameter],
        Vec::new(),
    ))
}

#[when("a statement is prepared, the catalog of its tenant changes, and the statement is executed")]
fn stale_statement(w: &mut Steps) -> TestResult {
    let [a, p, q] = ["a", "p", "q"].map(|n| w.iri(n));
    let (stale, again) = in_shared(w, async |client| {
        let statement = client.prepare(&statement_of(&p)).await?;
        declare(client, DeclaredPredicate::new(&q)).await?;
        // A version takes effect at an epoch boundary, and the frontend's schema id moves with it.
        let mut refused = None;
        for _ in 0..200 {
            let reader = client.begin(TENANT, true).await?;
            let stream = client
                .execute(&reader, &execution(statement.id(), &reader, iri(&a))?)
                .await?;
            match drain(client, &stream).await {
                Ok(_) => pause(std::time::Duration::from_millis(10)).await,
                Err(error) => {
                    refused = Some(error.to_string());
                    break;
                }
            }
            client.abort(reader).await?;
        }
        let again = client.prepare(&statement_of(&p)).await.is_ok();
        Ok((refused.context("the stale statement refused")?, again))
    })?;
    w.seen.facts = vec![stale];
    w.committed = again;
    Ok(())
}

#[then("the execution is refused as stale, and preparing it again succeeds")]
fn stale_refused(w: &mut Steps) -> TestResult {
    let stale = w.seen.facts.first().context("the refusal")?;
    assert!(stale.contains("StaleStatement"), "{stale} {}", w.over());
    assert!(w.committed, "prepared again {}", w.over());
    Ok(())
}

#[when("a statement's subject parameter is bound to a literal")]
fn literal_parameter(w: &mut Steps) -> TestResult {
    let p = w.iri("p");
    let refused = in_shared(w, async |client| {
        let statement = client.prepare(&statement_of(&p)).await?;
        let reader = client.begin(TENANT, true).await?;
        let stream = client
            .execute(
                &reader,
                &execution(statement.id(), &reader, string("a literal"))?,
            )
            .await?;
        let refused = client
            .next_batch(&stream, batch_of)
            .await
            .err()
            .context("a refusal")?;
        Ok((refused, statement.id()))
    })?;
    w.seen.facts = vec![refused.1.to_string()];
    w.seen.refusals = vec![refused.0];
    Ok(())
}

#[then("the execution is refused with code 11, `detail` the statement")]
fn literal_parameter_refused(w: &mut Steps) -> TestResult {
    let refused = w.seen.refusals.first().context("the refusal")?;
    let (code, detail, _) = refusal_of(refused)?;
    assert_eq!(code, ErrorCode::Parameters, "{refused:?}");
    assert_eq!(
        &detail.to_string(),
        w.seen.facts.first().context("the statement")?
    );
    Ok(())
}

#[when("a batch of 1,000 rows names the same IRI in every row")]
fn repeated_iri(w: &mut Steps) -> TestResult {
    let [p, b, s] = ["p", "b", "s"].map(|n| w.iri(n));
    let (rows, terms) = in_shared(w, async |client| {
        let mut terms = TermTable::new();
        let (predicate, object) = (terms.iri(&p), terms.iri(&b));
        let changes = (0..1_000)
            .map(|at| WriteChange::Insert {
                graph: None,
                subject: terms.iri(&format!("{s}{at}")),
                predicate,
                object,
            })
            .collect();
        let _written = client
            .direct_write(DirectWrite::new(TENANT, terms, changes), None)
            .await?;
        let own = client.begin(TENANT, false).await?;
        let pattern = Pattern::of(None, Some(&p));
        let stream = client
            .read(&own, &pattern.read(&own, 0, Vec::new())?)
            .await?;
        let counted = client
            .next_batch(&stream, |batch| (batch.rows(), batch.term_count()))
            .await?
            .context("a batch")?;
        client.abort(own).await?;
        Ok(counted)
    })?;
    w.seen.facts = vec![rows.to_string(), terms.to_string()];
    Ok(())
}

#[then("its term table holds that IRI once")]
fn repeated_once(w: &mut Steps) -> TestResult {
    let [rows, terms] = w.seen.facts.as_slice() else {
        return Err(TestError::new("two counts"));
    };
    let (rows, terms): (usize, usize) = (rows.parse()?, terms.parse()?);
    assert_eq!(rows, 1_000, "{}", w.over());
    // One entry per subject, and one for the object all of them name.
    assert_eq!(terms, rows + 1, "{}", w.over());
    Ok(())
}

#[when(
    "a read with limit 1 over `<a> <knows> ?o` returns `<b>` and its resume key is decoded by the server"
)]
fn key_decoded(w: &mut Steps) -> TestResult {
    let [a, b, knows] = ["a", "b", "knows"].map(|n| w.iri(n));
    let (scope, resume) = in_shared(w, async |client| {
        committed(client, &a, &knows, &iri(&b)).await?;
        committed(client, &a, &knows, &iri(&format!("{b}2"))).await?;
        let own = client.begin(TENANT, false).await?;
        let pattern = Pattern::of(Some(&a), Some(&knows));
        let stream = client
            .read(&own, &pattern.read(&own, 1, Vec::new())?)
            .await?;
        let answer = drain(client, &stream).await?;
        client.abort(own).await?;
        Ok((answer.column(0).concat(), answer.resume))
    })?;
    let key = ultragraphdb_protocol_wire::ResumeKeyView::read(&resume)?;
    let held = (0..u32::try_from(key.terms().len())?)
        .filter_map(|index| key.terms().get(index))
        .filter_map(|term| match term {
            RequestTermView::Iri(text) => Some(text.to_string()),
            RequestTermView::BlankNode(_)
            | RequestTermView::Literal { .. }
            | RequestTermView::LanguageLiteral { .. }
            | RequestTermView::Triple { .. } => None,
        })
        .collect::<Vec<_>>();
    w.seen.facts = vec![key.pattern().to_string(), key.scope().to_string(), scope];
    w.seen.facts.extend(held);
    Ok(())
}

#[then(
    "the key holds the pattern's hash, its `scope` and the text `http://ex.org/b`, and no system ID or store key"
)]
fn key_holds(w: &mut Steps) -> TestResult {
    let [hash, scope, object, held @ ..] = w.seen.facts.as_slice() else {
        return Err(TestError::new("what the key holds"));
    };
    assert_ne!(hash, "0", "{}", w.over());
    assert_ne!(scope, "0", "{}", w.over());
    assert!(held.contains(object), "{held:?} {object} {}", w.over());
    Ok(())
}

#[when(
    "a Drop of `p` through no session commits at its partition, its reply is lost, and the client sends the Drop again"
)]
fn drop_runs_again(w: &mut Steps) -> TestResult {
    let [a, p, b] = ["a", "p", "b"].map(|n| w.iri(n));
    let (first, second, held, declared) = in_shared(w, async |client| {
        declare(client, DeclaredPredicate::new(&p)).await?;
        committed(client, &a, &p, &iri(&b)).await?;
        let first = client
            .drop_command(DropCommand::predicate(TENANT, &p), None)
            .await?;
        let second = client
            .drop_command(DropCommand::predicate(TENANT, &p), None)
            .await?;
        let held = latest_read(client, &Pattern::of(Some(&a), Some(&p))).await?;
        for _ in 0..500 {
            let described = client.describe(TENANT).await?;
            if described
                .predicates()
                .iter()
                .all(|declared| declared.predicate() != p)
            {
                return Ok((first, second, held, false));
            }
            pause(std::time::Duration::from_millis(10)).await;
        }
        Ok((first, second, held, true))
    })?;
    w.seen.commits = vec![first, second];
    w.seen.answers = vec![held];
    w.committed = !declared;
    Ok(())
}

#[then(
    "the second Drop removes nothing more, and the frontend builds the version removing `p`'s declaration before it relays the reply"
)]
fn drop_ran_again(w: &mut Steps) -> TestResult {
    assert_eq!(w.seen.commits.len(), 2, "{}", w.over());
    nothing(w.seen.answers.first().context("what is held")?)?;
    assert!(w.committed, "the declaration removed {}", w.over());
    Ok(())
}

#[when("a client reads the subject of a statement whose subject is `http://ex.org/a`")]
fn subject_by_text(w: &mut Steps) -> TestResult {
    let [a, p] = ["a", "p"].map(|n| w.iri(n));
    w.seen.answers = in_shared(w, async |client| {
        committed(client, &a, &p, &string("x")).await?;
        Ok(vec![
            latest_read(client, &Pattern::of(None, Some(&p))).await?,
        ])
    })?;
    Ok(())
}

#[then("the result batch names `http://ex.org/a` by its text, and its bytes contain no system ID")]
fn subject_named(w: &mut Steps) -> TestResult {
    let answer = w.seen.answers.first().context("the read's answer")?;
    assert_eq!(answer.column(0), [w.iri("a")], "{}", w.over());
    Ok(())
}

#[when("a client reads a statement that carries a property and so holds a statement ID")]
fn statement_with_id(w: &mut Steps) -> TestResult {
    let [a, b, knows, since] = ["a", "b", "knows", "since"].map(|n| w.iri(n));
    w.seen.answers = in_shared(w, async |client| {
        let _edge = client
            .direct_write(edge_of((&a, &knows, &b), &since, "2020")?, None)
            .await?;
        Ok(vec![
            latest_read(client, &Pattern::of(Some(&a), Some(&knows))).await?,
        ])
    })?;
    Ok(())
}

#[then(
    "the result names the statement by its quad and its reifier's skolem IRI, whose text holds the statement's bucket and no sequence, and its bytes contain no system ID"
)]
fn statement_named(w: &mut Steps) -> TestResult {
    let answer = w.seen.answers.first().context("the read's answer")?;
    assert_eq!(answer.column(0), [w.iri("b")], "{}", w.over());
    let reifier = answer.column(1).concat();
    let bucket = ultragraphdb_store_quads::iri_bucket(TENANT, &reifier).context("its bucket")?;
    let of_subject =
        ultragraphdb_store_quads::iri_bucket(TENANT, &w.iri("a")).context("the subject's")?;
    assert_eq!(bucket.get(), of_subject.get(), "{reifier} {}", w.over());
    Ok(())
}

#[when(
    "a transaction's declarations build a catalog version of 523,001 bytes, and the client commits it"
)]
fn version_past_cap(w: &mut Steps) -> TestResult {
    let predicate = format!("{}/{}", w.iri("long"), "n".repeat(600_000));
    let (refused, described) = in_shared(w, async |client| {
        let own = client.begin(TENANT, false).await?;
        client
            .write(&own, &declaring(DeclaredPredicate::new(&predicate)))
            .await?;
        let refused = client.commit(own, None).await.err().context("a refusal")?;
        Ok((refused, client.describe(TENANT).await?))
    })?;
    w.seen.refusals = vec![refused];
    w.committed = described
        .predicates()
        .iter()
        .all(|declared| declared.predicate() != predicate);
    Ok(())
}

#[then(
    "the Commit is refused with code 7, `detail` 523,000, the message naming the size, and the current version stays"
)]
fn version_refused(w: &mut Steps) -> TestResult {
    let refused = w.seen.refusals.first().context("the refusal")?;
    let (code, detail, message) = refusal_of(refused)?;
    assert_eq!(code, ErrorCode::Refused, "{refused:?}");
    assert_eq!(detail, 523_000, "{refused:?}");
    assert!(message.chars().any(|c| c.is_ascii_digit()), "{message}");
    assert!(w.committed, "the catalog unchanged {}", w.over());
    Ok(())
}
