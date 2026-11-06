/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! A client against a tenant spread over partitions: what is routed where, what crosses partitions and is refused, what is read
//! across them. The cluster's three partitions are real and hold every subject the placement gives them.

use cucumber::{then, when};
use ultragraphdb_client_compio::{ClientError, Committed};
use ultragraphdb_core_ids::Version;
use ultragraphdb_core_terms::{FLOAT32_VECTOR, SYSTEM_VOCABULARY, format_vector};
use ultragraphdb_protocol_wire::{
    Begin, CommitOperation, ConditionalWrite, DirectWrite, DropCommand, ErrorCode, Link, Operation,
    ReadLevel, RequestTerm, TermTable, Write, WriteChange,
};
use ultragraphdb_store_quads::iri_bucket;
use ultragraphdb_tool_bdd::{Context, TestError, TestResult};

use crate::Steps;
use crate::client_reads::{
    Answer, Pattern, TENANT, batch_of, direct_of, edge_of, iri, property_of, read_whole,
    refusal_of, string, typed, write_of,
};
use crate::cluster::{iri_of_spread, partition_of, spread_client, spread_ranges};
use crate::endpoint_steps::declare;
use crate::read_steps::until_rows;
use crate::request_steps::{latest_read, unique};
use crate::shells::{TestClient, with_client};
use crate::transaction_steps::{committed, nothing};

/// Runs `scene` with a client of the shared cluster of several partitions.
fn in_spread<T>(
    w: &Steps,
    scene: impl AsyncFnOnce(&mut TestClient) -> TestResult<T>,
) -> TestResult<T> {
    with_client(&spread_client(w.carrier)?, scene)
}

impl Steps {
    /// An IRI of this scenario alone, named `name`, in `partition`'s buckets.
    fn iri_in(&self, partition: u32, name: &str) -> TestResult<String> {
        iri_of_spread(&format!("{}/", self.iri(name)), partition)
    }
}

/// The two partitions a code 23 or 25's `detail` names: bits 0 to 31, and bits 32 to 63.
fn partitions_of(detail: u64) -> (u64, u64) {
    (detail & u64::from(u32::MAX), detail >> 32)
}

/// That the refusal is of `code` and names the partitions `one` and `other` in either order.
fn names_partitions(refused: &ClientError, code: ErrorCode, one: u64, other: u64) -> TestResult {
    let (found, detail, _) = refusal_of(refused)?;
    assert_eq!(found, code, "{refused:?}");
    let (first, second) = partitions_of(detail);
    assert!(
        (first, second) == (one, other) || (first, second) == (other, one),
        "{refused:?}: partitions {first} and {second}"
    );
    Ok(())
}

/// A Write of `<subject> <p> <b>` for each of `subjects`.
fn write_of_subjects(subjects: [&str; 2], p: &str, b: &str) -> Write {
    let mut terms = TermTable::new();
    let changes = subjects
        .into_iter()
        .map(|subject| WriteChange::Insert {
            graph: None,
            subject: terms.iri(subject),
            predicate: terms.iri(p),
            object: terms.iri(b),
        })
        .collect();
    Write::new(terms, changes)
}

#[when(
    "a transaction's first Write names subjects the frontend's map places in partitions 1 and 2"
)]
fn first_spreading(w: &mut Steps) -> TestResult {
    let (one, two, p, b) = (
        w.iri_in(1, "one")?,
        w.iri_in(2, "two")?,
        w.iri("p"),
        w.iri("b"),
    );
    let (refused, next, held) = in_spread(w, async |client| {
        let own = client.begin(TENANT, false).await?;
        client
            .write(&own, &write_of_subjects([&one, &two], &p, &b))
            .await?;
        let refused = client.commit(own, None).await.err().context("a refusal")?;
        let next = client.commit(own, None).await.err();
        let held = [
            latest_read(client, &Pattern::of(Some(&one), Some(&p))).await?,
            latest_read(client, &Pattern::of(Some(&two), Some(&p))).await?,
        ];
        Ok((refused, next, held))
    })?;
    w.seen.refusals = vec![refused];
    w.seen.facts = vec![format!("{next:?}")];
    w.seen.answers = held.to_vec();
    Ok(())
}

#[then(
    "it is refused with code 23 on the transaction's stream, `detail` partitions 1 and 2, no partition receives a Bind, and the transaction ends"
)]
fn first_spreading_refused(w: &mut Steps) -> TestResult {
    let refused = w.seen.refusals.first().context("the refusal")?;
    names_partitions(refused, ErrorCode::CrossPartition, 1, 2)?;
    let next = w.seen.facts.first().context("the next request's answer")?;
    assert!(next.starts_with("Some"), "the transaction ended: {next}");
    for held in &w.seen.answers {
        nothing(held)?;
    }
    Ok(())
}

#[when("a DirectWrite writes subjects of partitions 1 and 2")]
fn direct_spreading(w: &mut Steps) -> TestResult {
    let (one, two, p, b) = (
        w.iri_in(1, "one")?,
        w.iri_in(2, "two")?,
        w.iri("p"),
        w.iri("b"),
    );
    let (refused, held) = in_spread(w, async |client| {
        let write = write_of_subjects([&one, &two], &p, &b);
        let direct = DirectWrite::new(TENANT, write.terms().clone(), write.changes().to_vec());
        let refused = client
            .direct_write(direct, None)
            .await
            .err()
            .context("a refusal")?;
        let held = [
            latest_read(client, &Pattern::of(Some(&one), Some(&p))).await?,
            latest_read(client, &Pattern::of(Some(&two), Some(&p))).await?,
        ];
        Ok((refused, held))
    })?;
    w.seen.refusals = vec![refused];
    w.seen.answers = held.to_vec();
    Ok(())
}

#[then("the frontend refuses it with code 23 before any relay, and neither is held")]
fn direct_spreading_refused(w: &mut Steps) -> TestResult {
    let refused = w.seen.refusals.first().context("the refusal")?;
    names_partitions(refused, ErrorCode::CrossPartition, 1, 2)?;
    for held in &w.seen.answers {
        nothing(held)?;
    }
    Ok(())
}

/// A transaction bound to partition 1 by a write of `<one> <p> "x"`, which then writes `<two> <p> "x"`: the refusal it takes,
/// what is held of `<one>` and `<two>` after it, and whether the refusal says to run it again.
async fn crossing(
    client: &mut TestClient,
    (one, two, p): (&str, &str, &str),
) -> TestResult<(ClientError, Vec<Answer>)> {
    let own = client.begin(TENANT, false).await?;
    client.write(&own, &write_of(one, p, &string("x"))?).await?;
    client.write(&own, &write_of(two, p, &string("x"))?).await?;
    let refused = client.commit(own, None).await.err().context("a refusal")?;
    let held = vec![
        latest_read(client, &Pattern::of(Some(one), Some(p))).await?,
        latest_read(client, &Pattern::of(Some(two), Some(p))).await?,
    ];
    Ok((refused, held))
}

/// That a transaction bound to partition 1 aborted naming partitions 1 and 2, not retryable, holding nothing.
fn crossing_refused(w: &Steps) -> TestResult {
    let refused = w.seen.refusals.first().context("the refusal")?;
    names_partitions(refused, ErrorCode::CrossPartition, 1, 2)?;
    assert!(!refused.retryable(), "{refused:?} {}", w.over());
    for held in &w.seen.answers {
        nothing(held)?;
    }
    Ok(())
}

#[when(
    "a transaction bound to partition 1 writes a statement whose subject's bucket partition 2 owns"
)]
fn reaching_second(w: &mut Steps) -> TestResult {
    let (one, two, p) = (w.iri_in(1, "one")?, w.iri_in(2, "two")?, w.iri("p"));
    let (refused, held) = in_spread(w, async |client| crossing(client, (&one, &two, &p)).await)?;
    w.seen.refusals = vec![refused];
    w.seen.answers = held;
    Ok(())
}

#[then(
    "it aborts with a cross-partition error naming partitions 1 and 2, not marked retryable, and nothing it wrote is held"
)]
fn second_refused(w: &mut Steps) -> TestResult {
    crossing_refused(w)
}

#[when(
    "a transaction writes a statement whose subject partition 1 owns and then writes one whose subject partition 2 owns"
)]
fn reaching_second_again(w: &mut Steps) -> TestResult {
    reaching_second(w)
}

#[then("the transaction aborts with `CrossPartition` naming partitions 1 and 2, and holds nothing")]
fn second_holds_nothing(w: &mut Steps) -> TestResult {
    crossing_refused(w)
}

#[when(
    "a transaction bound to partition 1 sets a property of a statement named by its reifier, the statement's subject's bucket owned by partition 2"
)]
fn reifier_crossing(w: &mut Steps) -> TestResult {
    let (one, two, b, knows, since, p) = (
        w.iri_in(1, "one")?,
        w.iri_in(2, "two")?,
        w.iri("b"),
        w.iri("knows"),
        w.iri("since"),
        w.iri("p"),
    );
    let (refused, kept) = in_spread(w, async |client| {
        let _edge = client
            .direct_write(edge_of((&two, &knows, &b), &since, "2020")?, None)
            .await?;
        let named = latest_read(client, &Pattern::of(Some(&two), Some(&knows))).await?;
        let reifier = named.column(1).first().cloned().context("its reifier")?;
        let own = client.begin(TENANT, false).await?;
        client
            .write(&own, &write_of(&one, &p, &string("x"))?)
            .await?;
        let set = property_of(&reifier, &since, "2021")?;
        let write = Write::new(set.terms().clone(), set.changes().to_vec());
        client.write(&own, &write).await?;
        let refused = client.commit(own, None).await.err().context("a refusal")?;
        let kept = latest_read(client, &Pattern::of(Some(&one), Some(&p))).await?;
        Ok((refused, kept))
    })?;
    w.seen.refusals = vec![refused];
    w.seen.answers = vec![kept];
    Ok(())
}

#[when(
    "`alice` is homed in partition 1 and `bob` in partition 2, both `on-call` `true`, and a read-write transaction reads `on-call` of `alice` and then of `bob`"
)]
fn skew_across(w: &mut Steps) -> TestResult {
    let (alice, bob, on_call) = (w.iri_in(1, "alice")?, w.iri_in(2, "bob")?, w.iri("on-call"));
    let refused = in_spread(w, async |client| {
        committed(client, &alice, &on_call, &string("true")).await?;
        committed(client, &bob, &on_call, &string("true")).await?;
        let own = client.begin(TENANT, false).await?;
        let _first = read_whole(client, &own, &Pattern::of(Some(&alice), Some(&on_call))).await?;
        let second = Pattern::of(Some(&bob), Some(&on_call));
        let stream = client
            .read(&own, &second.read(&own, 0, Vec::new())?)
            .await?;
        client
            .next_batch(&stream, batch_of)
            .await
            .err()
            .context("the second read refused")
    })?;
    w.seen.refusals = vec![refused];
    Ok(())
}

#[then(
    "the second read aborts the transaction with `CrossPartition` naming partitions 1 and 2, and it writes nothing"
)]
fn skew_refused(w: &mut Steps) -> TestResult {
    let refused = w.seen.refusals.first().context("the refusal")?;
    names_partitions(refused, ErrorCode::CrossPartition, 1, 2)
}

#[when(
    "a tenant's run spans partitions 1 and 2, `<b>` is homed in partition 2, and a read-write transaction bound to partition 1 reads the subjects of `knows` pointing at `<b>`"
)]
fn reverse_over_spread(w: &mut Steps) -> TestResult {
    let (one, b, knows, p) = (
        w.iri_in(1, "one")?,
        w.iri_in(2, "b")?,
        w.iri("knows"),
        w.iri("p"),
    );
    let refused = in_spread(w, async |client| {
        committed(client, &one, &knows, &iri(&b)).await?;
        let own = client.begin(TENANT, false).await?;
        client
            .write(&own, &write_of(&one, &p, &string("x"))?)
            .await?;
        let pattern = Pattern::of(None, Some(&knows)).with_object(iri(&b));
        let stream = client
            .read(&own, &pattern.read(&own, 0, Vec::new())?)
            .await?;
        client
            .next_batch(&stream, batch_of)
            .await
            .err()
            .context("the reverse read refused")
    })?;
    w.seen.refusals = vec![refused];
    Ok(())
}

#[then("the transaction aborts with `CrossPartition` naming partitions 1 and 2")]
fn reverse_refused(w: &mut Steps) -> TestResult {
    let refused = w.seen.refusals.first().context("the refusal")?;
    let (code, detail, _) = refusal_of(refused)?;
    assert_eq!(code, ErrorCode::CrossPartition, "{refused:?}");
    assert_eq!(partitions_of(detail).0, 1, "{refused:?} {}", w.over());
    Ok(())
}

#[when(
    "a transaction's first request is a read, compute answers code 23, and the refreshed map leaves the read's subject where it was"
)]
fn first_read_unmoved(w: &mut Steps) -> TestResult {
    let (one, two, p) = (w.iri_in(1, "one")?, w.iri_in(2, "two")?, w.iri("p"));
    let (refused, next) = in_spread(w, async |client| {
        committed(client, &one, &p, &string("x")).await?;
        committed(client, &two, &p, &string("x")).await?;
        let own = client.begin(TENANT, false).await?;
        let pattern = Pattern::of(None, Some(&p));
        let stream = client
            .read(&own, &pattern.read(&own, 0, Vec::new())?)
            .await?;
        let refused = client
            .next_batch(&stream, batch_of)
            .await
            .err()
            .context("the first read refused")?;
        Ok((refused, client.commit(own, None).await.err()))
    })?;
    w.seen.refusals = vec![refused];
    w.seen.refusals.extend(next);
    Ok(())
}

#[then(
    "the client's read is refused with code 23, and its next request on the transaction's stream too"
)]
fn first_read_refused(w: &mut Steps) -> TestResult {
    let [read, next] = w.seen.refusals.as_slice() else {
        return Err(TestError::new(format!(
            "two refusals: {:?}",
            w.seen.refusals
        )));
    };
    assert_eq!(refusal_of(read)?.0, ErrorCode::CrossPartition, "{read:?}");
    assert_eq!(refusal_of(next)?.0, ErrorCode::CrossPartition, "{next:?}");
    Ok(())
}

#[when("a tenant's buckets span partitions 1 and 2, and a client drops the tenant's data")]
fn drop_over_several(w: &mut Steps) -> TestResult {
    let (one, p) = (w.iri_in(1, "one")?, w.iri("p"));
    let (refused, held) = in_spread(w, async |client| {
        committed(client, &one, &p, &string("x")).await?;
        let session = client.register().await?;
        let refused = client
            .drop_command(DropCommand::tenant_data(TENANT), session)
            .await
            .err()
            .context("a refusal")?;
        Ok((
            refused,
            latest_read(client, &Pattern::of(Some(&one), Some(&p))).await?,
        ))
    })?;
    w.seen.refusals = vec![refused];
    w.seen.answers = vec![held];
    Ok(())
}

#[then(
    "the Drop is refused with `CrossPartition` naming partitions 1 and 2, and nothing is dropped"
)]
fn drop_refused(w: &mut Steps) -> TestResult {
    let refused = w.seen.refusals.first().context("the refusal")?;
    let (code, detail, _) = refusal_of(refused)?;
    assert_eq!(code, ErrorCode::CrossPartition, "{refused:?}");
    assert_eq!(partitions_of(detail), (1, 2), "{refused:?}");
    let held = w.seen.answers.first().context("what is held")?;
    assert_eq!(held.column(0), ["x"], "{}", w.over());
    Ok(())
}

/// A Write deleting the pattern `<subject> <predicate>`.
fn delete_pattern(subject: &str, predicate: &str) -> Write {
    let mut terms = TermTable::new();
    let change = WriteChange::DeletePattern {
        graph: None,
        subject: terms.iri(subject),
        predicate: terms.iri(predicate),
    };
    Write::new(terms, vec![change])
}

#[when(
    "a transaction bound to partition 1 deletes the pattern `<b>`, `knows`, and `<b>` is homed in partition 2"
)]
fn delete_crossing(w: &mut Steps) -> TestResult {
    let (one, b, c, knows, p) = (
        w.iri_in(1, "one")?,
        w.iri_in(2, "b")?,
        w.iri("c"),
        w.iri("knows"),
        w.iri("p"),
    );
    let (refused, held) = in_spread(w, async |client| {
        committed(client, &b, &knows, &iri(&c)).await?;
        let own = client.begin(TENANT, false).await?;
        client
            .write(&own, &write_of(&one, &p, &string("x"))?)
            .await?;
        client.write(&own, &delete_pattern(&b, &knows)).await?;
        let refused = client.commit(own, None).await.err().context("a refusal")?;
        Ok((
            refused,
            latest_read(client, &Pattern::of(Some(&b), Some(&knows))).await?,
        ))
    })?;
    w.seen.refusals = vec![refused];
    w.seen.answers = vec![held];
    Ok(())
}

#[then(
    "the transaction aborts with `CrossPartition` naming partitions 1 and 2, and every `knows` of `<b>` is still held"
)]
fn delete_refused(w: &mut Steps) -> TestResult {
    let refused = w.seen.refusals.first().context("the refusal")?;
    names_partitions(refused, ErrorCode::CrossPartition, 1, 2)?;
    let held = w.seen.answers.first().context("what is held")?;
    assert_eq!(held.column(0), [w.iri("c")], "{}", w.over());
    Ok(())
}

#[when(
    "`<a>`, homed in partition 1, holds `<a> <knows> <b>`, and a transaction in partition 2, `<b>`'s home, deletes the pattern `<b>`, `name`"
)]
fn delete_at_home(w: &mut Steps) -> TestResult {
    let (a, b, knows, name) = (
        w.iri_in(1, "a")?,
        w.iri_in(2, "b")?,
        w.iri("knows"),
        w.iri("name"),
    );
    let (named, edge) = in_spread(w, async |client| {
        committed(client, &a, &knows, &iri(&b)).await?;
        committed(client, &b, &name, &string("B")).await?;
        let own = client.begin(TENANT, false).await?;
        client.write(&own, &delete_pattern(&b, &name)).await?;
        let _deleted = client.commit(own, None).await?;
        Ok((
            latest_read(client, &Pattern::of(Some(&b), Some(&name))).await?,
            latest_read(client, &Pattern::of(Some(&a), Some(&knows))).await?,
        ))
    })?;
    w.seen.answers = vec![named, edge];
    w.seen.facts = vec![b];
    Ok(())
}

#[then(
    "it commits in partition 2 alone, `<b>` holds no `name`, and `<a> <knows> <b>` is still held"
)]
fn delete_kept_edge(w: &mut Steps) -> TestResult {
    let [named, edge] = w.seen.answers.as_slice() else {
        return Err(TestError::new("two answers"));
    };
    nothing(named)?;
    assert_eq!(edge.column(0), w.seen.facts, "{}", w.over());
    Ok(())
}

#[when("a transaction bound to partition 1 writes `<a> <knows> <b>`, `<b>` homed in partition 2")]
fn reference_elsewhere(w: &mut Steps) -> TestResult {
    let (a, b, knows) = (w.iri_in(1, "a")?, w.iri_in(2, "b")?, w.iri("knows"));
    let (written, held) = in_spread(w, async |client| {
        let session = client.register().await?;
        let own = client.begin(TENANT, false).await?;
        client.write(&own, &write_of(&a, &knows, &iri(&b))?).await?;
        let written = client.commit(own, session).await?;
        Ok((
            written,
            latest_read(client, &Pattern::of(Some(&a), Some(&knows))).await?,
        ))
    })?;
    w.seen.facts = vec![b];
    w.seen.commits = vec![written];
    w.seen.answers = vec![held];
    Ok(())
}

#[then(
    "it registers the reference at partition 2, commits in partition 1 alone, and does not abort"
)]
fn reference_stays(w: &mut Steps) -> TestResult {
    let written = w.seen.commits.first().context("the commit")?;
    let token = written.token().context("a token")?;
    assert_eq!(token.partition(), 1, "{written:?}");
    let held = w.seen.answers.first().context("what is held")?;
    assert_eq!(held.column(0), w.seen.facts, "{}", w.over());
    Ok(())
}

#[when("a read binds its subject to `http://ex.org/a` over two partitions")]
fn bound_subject(w: &mut Steps) -> TestResult {
    let (a, c, p) = (w.iri_in(2, "a")?, w.iri_in(1, "c")?, w.iri("p"));
    w.seen.answers = in_spread(w, async |client| {
        committed(client, &a, &p, &string("A")).await?;
        committed(client, &c, &p, &string("C")).await?;
        Ok(vec![
            until_rows(client, &Pattern::of(Some(&a), Some(&p)), 1).await?,
        ])
    })?;
    Ok(())
}

#[then("compute asks only the partition of `http://ex.org/a`'s bucket, and returns its rows")]
fn bound_subject_read(w: &mut Steps) -> TestResult {
    let answer = w.seen.answers.first().context("the answer")?;
    assert_eq!(answer.column(0), ["A"], "{}", w.over());
    Ok(())
}

#[when(
    "`<a>` in partition 1 and `<c>` in partition 2 each hold `name` `\"Ann\"`, and a read binds its object to `\"Ann\"` and its predicate to `name`"
)]
fn literal_object(w: &mut Steps) -> TestResult {
    let (a, c, name) = (w.iri_in(1, "a")?, w.iri_in(2, "c")?, w.iri("name"));
    let answer = in_spread(w, async |client| {
        committed(client, &a, &name, &string("Ann")).await?;
        committed(client, &c, &name, &string("Ann")).await?;
        let pattern = Pattern::of(None, Some(&name)).with_object(string("Ann"));
        until_rows(client, &pattern, 2).await
    })?;
    w.seen.answers = vec![answer];
    w.seen.facts = vec![a, c];
    Ok(())
}

/// That the one answer's first column holds what the facts hold, in any order.
fn holds_the_facts(w: &Steps) -> TestResult {
    let answer = w.seen.answers.first().context("the answer")?;
    let mut found = answer.column(0);
    found.sort();
    let mut expected = w.seen.facts.clone();
    expected.sort();
    assert_eq!(found, expected, "{}", w.over());
    Ok(())
}

#[then(
    "compute asks every partition of the tenant's run, reads no referrer set, and returns `<a>` and `<c>` merged in key order"
)]
fn literal_object_read(w: &mut Steps) -> TestResult {
    holds_the_facts(w)
}

#[when(
    "a read leaves its subject and object unbound over two partitions that each hold matching rows"
)]
fn unbound_subject(w: &mut Steps) -> TestResult {
    let (a, c, p) = (w.iri_in(1, "a")?, w.iri_in(2, "c")?, w.iri("p"));
    let answer = in_spread(w, async |client| {
        committed(client, &a, &p, &string("x")).await?;
        committed(client, &c, &p, &string("x")).await?;
        until_rows(client, &Pattern::of(None, Some(&p)), 2).await
    })?;
    w.seen.answers = vec![answer];
    w.seen.facts = vec![a, c];
    Ok(())
}

#[then("compute returns the rows of both partitions merged in key order")]
fn unbound_subject_read(w: &mut Steps) -> TestResult {
    holds_the_facts(w)
}

/// The term of a `float32vector` literal.
fn vector(elements: &[f32]) -> RequestTerm {
    typed(&format_vector(elements.iter().copied()), FLOAT32_VECTOR)
}

#[when(
    "`<a>` in partition 1 and `<c>` in partition 2 each hold `embedding` `v`, `<d>` holds another vector, and a read binds its object to `v` and its predicate to `embedding`"
)]
fn vector_object(w: &mut Steps) -> TestResult {
    let (a, c, d, embedding) = (
        w.iri_in(1, "a")?,
        w.iri_in(2, "c")?,
        w.iri_in(3, "d")?,
        w.iri("embedding"),
    );
    let (v, other) = (vector(&[0.25, 0.5]), vector(&[0.75, 1.0]));
    let answer = in_spread(w, async |client| {
        committed(client, &a, &embedding, &v).await?;
        committed(client, &c, &embedding, &v).await?;
        committed(client, &d, &embedding, &other).await?;
        let pattern = Pattern::of(None, Some(&embedding)).with_object(v.clone());
        until_rows(client, &pattern, 2).await
    })?;
    w.seen.answers = vec![answer];
    w.seen.facts = vec![a, c];
    Ok(())
}

#[then(
    "compute asks every partition of the tenant's run for the forward lists of `embedding`, and returns `<a>` and `<c>` alone"
)]
fn vector_read(w: &mut Steps) -> TestResult {
    holds_the_facts(w)
}

#[when(
    "`<a>` in partition 1 and `<c>` in partition 3 each hold `knows` to `<b>`, homed in partition 2, and a read binds its object to `<b>` and its predicate to `knows` at a closed epoch"
)]
fn gathers_referrers(w: &mut Steps) -> TestResult {
    let (a, c, b, knows) = (
        w.iri_in(1, "a")?,
        w.iri_in(3, "c")?,
        w.iri_in(2, "b")?,
        w.iri("knows"),
    );
    let answer = in_spread(w, async |client| {
        committed(client, &a, &knows, &iri(&b)).await?;
        committed(client, &c, &knows, &iri(&b)).await?;
        let pattern = Pattern::of(None, Some(&knows)).with_object(iri(&b));
        until_rows(client, &pattern, 2).await
    })?;
    w.seen.answers = vec![answer];
    w.seen.facts = vec![a, c];
    Ok(())
}

#[then("compute reads `R(b)` at partition 2, asks partitions 1 and 3, and returns `<a>` and `<c>`")]
fn referrers_read(w: &mut Steps) -> TestResult {
    holds_the_facts(w)
}

/// A read-only Begin at level latest.
fn at_latest() -> Begin {
    Begin::new(TENANT, 0, true).with_level(ReadLevel::Latest)
}

#[when("a transaction at latest-state level reads with its subject unbound over two partitions")]
fn latest_across(w: &mut Steps) -> TestResult {
    let (a, c, p) = (w.iri_in(1, "a")?, w.iri_in(2, "c")?, w.iri("p"));
    let refused = in_spread(w, async |client| {
        committed(client, &a, &p, &string("x")).await?;
        committed(client, &c, &p, &string("x")).await?;
        let own = client.begin_with(at_latest(), None).await?;
        let pattern = Pattern::of(None, Some(&p));
        let stream = client
            .read(&own, &pattern.read(&own, 0, Vec::new())?)
            .await?;
        client
            .next_batch(&stream, batch_of)
            .await
            .err()
            .context("the read refused")
    })?;
    w.seen.refusals = vec![refused];
    Ok(())
}

#[then("the read is refused with an error naming the level and the two partitions")]
fn latest_refused(w: &mut Steps) -> TestResult {
    let refused = w.seen.refusals.first().context("the refusal")?;
    let (code, _, message) = refusal_of(refused)?;
    assert_eq!(code, ErrorCode::LevelRefused, "{refused:?}");
    assert!(message.contains("latest"), "{message} {}", w.over());
    Ok(())
}

#[when(
    "a transaction at latest level sends two reads beside each other, the first binding it to partition 1"
)]
fn latest_two_reads(w: &mut Steps) -> TestResult {
    let (a, c, p) = (w.iri_in(1, "a")?, w.iri_in(2, "c")?, w.iri("p"));
    let (first, second) = in_spread(w, async |client| {
        committed(client, &a, &p, &string("A")).await?;
        committed(client, &c, &p, &string("C")).await?;
        let own = client.begin_with(at_latest(), None).await?;
        let one = Pattern::of(Some(&a), Some(&p));
        let two = Pattern::of(Some(&c), Some(&p));
        let first = client.read(&own, &one.read(&own, 0, Vec::new())?).await?;
        let second = client.read(&own, &two.read(&own, 0, Vec::new())?).await?;
        let found = crate::client_reads::drain(client, &first).await?;
        let refused = client
            .next_batch(&second, batch_of)
            .await
            .err()
            .context("the second read refused")?;
        Ok((found, refused))
    })?;
    w.seen.answers = vec![first];
    w.seen.refusals = vec![second];
    Ok(())
}

#[then(
    "the frontend sends the second only after the first's first Result names partition 1, with `partition` 1, and compute refuses it with code 25 when it reaches another partition"
)]
fn latest_second_refused(w: &mut Steps) -> TestResult {
    let first = w.seen.answers.first().context("the first read's answer")?;
    assert_eq!(first.column(0), ["A"], "{}", w.over());
    let refused = w.seen.refusals.first().context("the refusal")?;
    assert_eq!(
        refusal_of(refused)?.0,
        ErrorCode::LevelRefused,
        "{refused:?}"
    );
    Ok(())
}

#[when("a transaction at latest level sends two reads, and the first is refused before any Result")]
fn latest_first_refused(w: &mut Steps) -> TestResult {
    let (a, p) = (w.iri_in(2, "a")?, w.iri("p"));
    let (first, second) = in_spread(w, async |client| {
        committed(client, &a, &p, &string("A")).await?;
        let own = client.begin_with(at_latest(), None).await?;
        let bad = Pattern::of(None, Some(&p)).with_subject(string("a literal"));
        let good = Pattern::of(Some(&a), Some(&p));
        let first = client.read(&own, &bad.read(&own, 0, Vec::new())?).await?;
        let second = client.read(&own, &good.read(&own, 0, Vec::new())?).await?;
        let refused = client
            .next_batch(&first, batch_of)
            .await
            .err()
            .context("the first read refused")?;
        let found = crate::client_reads::drain(client, &second).await?;
        Ok((refused, found))
    })?;
    w.seen.refusals = vec![first];
    w.seen.answers = vec![second];
    Ok(())
}

#[then("the second goes out as the transaction's first read and binds it")]
fn latest_second_binds(w: &mut Steps) -> TestResult {
    let refused = w.seen.refusals.first().context("the refusal")?;
    assert_eq!(refusal_of(refused)?.0, ErrorCode::Refused, "{refused:?}");
    let found = w.seen.answers.first().context("the second read's answer")?;
    assert_eq!(found.column(0), ["A"], "{}", w.over());
    Ok(())
}

/// A `T` whose first write names a new blank node.
fn blank_write(p: &str, value: &str) -> TestResult<Write> {
    let mut terms = TermTable::new();
    let change = WriteChange::Insert {
        graph: None,
        subject: terms.intern(RequestTerm::BlankNode("b1".to_string()))?,
        predicate: terms.iri(p),
        object: terms.intern(string(value))?,
    };
    Ok(Write::new(terms, vec![change]))
}

#[when(
    "three transactions whose first writes name only new blank nodes arrive over three partitions"
)]
fn blanks_in_turn(w: &mut Steps) -> TestResult {
    let p = w.iri("p");
    let tokens = in_spread(w, async |client| {
        let session = client.register().await?;
        let mut partitions = Vec::new();
        for value in ["one", "two", "three"] {
            let own = client.begin(TENANT, false).await?;
            client.write(&own, &blank_write(&p, value)?).await?;
            let written = client.commit(own, session).await?;
            let token = written.token().context("a token")?;
            partitions.push(token.partition());
        }
        Ok(partitions)
    })?;
    w.seen.facts = tokens.iter().map(u32::to_string).collect();
    Ok(())
}

#[then(
    "the frontend binds them to partitions 1, 2 and 3 in turn, and each blank node's bucket is one its partition owns"
)]
fn blanks_bound(w: &mut Steps) -> TestResult {
    let mut partitions = w.seen.facts.clone();
    partitions.sort();
    partitions.dedup();
    assert_eq!(
        partitions,
        ["1", "2", "3"],
        "{:?} {}",
        w.seen.facts,
        w.over()
    );
    Ok(())
}

/// The partition a skolem IRI's bucket places it in.
fn partition_of_iri(text: &str) -> TestResult<u32> {
    let bucket = iri_bucket(TENANT, text).context("the IRI's bucket")?;
    partition_of(spread_ranges()?, bucket.get()).context("its partition")
}

#[when(
    "a ConditionalWrite's changes name only new blank nodes as subjects, its operation is an insert-if-absent of `epi.id` `\"x\"`, and partition 2 owns the bucket of `\"x\"`"
)]
fn claim_route(w: &mut Steps) -> TestResult {
    let (id, label) = (w.iri("epi.id"), w.iri("label"));
    let partitions = in_spread(w, async |client| {
        declare(client, unique(&id)).await?;
        let mut partitions = Vec::new();
        for value in 0..12 {
            let mut terms = TermTable::new();
            let change = WriteChange::Insert {
                graph: None,
                subject: terms.intern(RequestTerm::BlankNode("b1".to_string()))?,
                predicate: terms.iri(&label),
                object: terms.intern(string("x"))?,
            };
            let operation = CommitOperation::InsertIfAbsent {
                graph: None,
                predicate: terms.iri(&id),
                value: terms.intern(string(&format!("x{value}")))?,
                properties: Vec::new(),
            };
            let write = ConditionalWrite::new(TENANT, terms, vec![change], vec![operation]);
            let session = client.register().await?;
            let reply = client.conditional_write(write, session).await?;
            let node = reply.node(0).context("the node named")?;
            let token = reply.token().context("a token")?;
            partitions.push((partition_of_iri(node)?, token.partition()));
        }
        Ok(partitions)
    })?;
    w.seen.facts = partitions.iter().map(|at| format!("{at:?}")).collect();
    let mut places: Vec<u32> = partitions.iter().map(|at| at.0).collect();
    places.sort_unstable();
    places.dedup();
    w.committed = partitions.iter().all(|(node, token)| node == token) && places.len() > 1;
    Ok(())
}

#[then("the frontend relays it to partition 2, and its reply names the node")]
fn claim_routed(w: &mut Steps) {
    assert!(
        w.committed,
        "each claim where its node lies: {:?} {}",
        w.seen.facts,
        w.over()
    );
}

/// Tries claims of `id` in a transaction bound to partition 1, until one's value another partition owns: the refusal.
async fn claim_elsewhere(
    client: &mut TestClient,
    (one, p, id): (&str, &str, &str),
) -> TestResult<(ClientError, String)> {
    for value in 0..64 {
        let text = format!("x{value}");
        let own = client.begin(TENANT, false).await?;
        client.write(&own, &write_of(one, p, &string("x"))?).await?;
        let mut terms = TermTable::new();
        let predicate = terms.iri(id);
        let value_term = terms.intern(string(&text))?;
        let link_predicate = terms.iri(p);
        let other = terms.iri(one);
        let operation = CommitOperation::UpsertAndLink {
            graph: None,
            predicate,
            value: value_term,
            properties: Vec::new(),
            links: vec![Link::from_node(link_predicate, other)],
        };
        client
            .operation(&own, &Operation::new(terms, operation))
            .await?;
        if let Err(refused) = client.commit(own, None).await {
            return Ok((refused, text));
        }
    }
    Err(TestError::new("no claim of 64 another partition owned"))
}

#[when(
    "`epi.id` is declared unique, partition 2 owns the bucket of `\"x\"`, and a transaction bound to partition 1 upsert-and-links the entity with `epi.id` `\"x\"`"
)]
fn claim_crossing(w: &mut Steps) -> TestResult {
    let (one, p, id) = (w.iri_in(1, "one")?, w.iri("p"), w.iri("epi.id"));
    let (refused, value) = in_spread(w, async |client| {
        declare(client, unique(&id)).await?;
        claim_elsewhere(client, (&one, &p, &id)).await
    })?;
    w.seen.refusals = vec![refused];
    w.seen.facts = vec![value];
    Ok(())
}

#[then(
    "the transaction aborts with a cross-partition error naming partitions 1 and 2, not marked retryable, and nothing it wrote is held"
)]
fn claim_refused(w: &mut Steps) -> TestResult {
    let refused = w.seen.refusals.first().context("the refusal")?;
    let (code, detail, _) = refusal_of(refused)?;
    assert_eq!(code, ErrorCode::CrossPartition, "{refused:?} {}", w.over());
    assert_eq!(partitions_of(detail).0, 1, "{refused:?}");
    assert!(!refused.retryable(), "{refused:?}");
    Ok(())
}

/// A skolem IRI of tenant 0 no dictionary holds, whose bucket `partition` owns.
fn forged_in(partition: u32) -> TestResult<String> {
    let ranges = spread_ranges()?;
    let (start, _, _) = ranges
        .iter()
        .find(|(_, _, holder)| *holder == partition)
        .context("a range of the partition")?;
    Ok(format!(
        "{SYSTEM_VOCABULARY}tenant/0/.well-known/genid/{start:06x}-00000000000000000000000abc"
    ))
}

#[when(
    "a transaction in partition 1 writes an edge to a skolem IRI whose bucket partition 2 owns, and partition 2's dictionary does not hold it"
)]
fn forged_edge(w: &mut Steps) -> TestResult {
    let (one, knows, forged) = (w.iri_in(1, "one")?, w.iri("knows"), forged_in(2)?);
    let (refused, next) = in_spread(w, async |client| {
        let own = client.begin(TENANT, false).await?;
        client
            .write(&own, &write_of(&one, &knows, &iri(&forged))?)
            .await?;
        let refused = client.commit(own, None).await.err().context("a refusal")?;
        Ok((refused, client.commit(own, None).await.err()))
    })?;
    w.seen.refusals = vec![refused];
    w.seen.facts = vec![format!("{next:?}")];
    Ok(())
}

#[then(
    "the registration's outcome is 7, and the Write is refused with code 7, ending the transaction"
)]
fn forged_refused(w: &mut Steps) -> TestResult {
    let refused = w.seen.refusals.first().context("the refusal")?;
    assert_eq!(refusal_of(refused)?.0, ErrorCode::Refused, "{refused:?}");
    let next = w.seen.facts.first().context("the next request")?;
    assert!(next.starts_with("Some"), "the transaction ended: {next}");
    Ok(())
}

#[when(
    "a transaction in `<new>`'s home reads `<new>`'s statements, finds none, and writes `<x> <seen> true`, and a registration from another partition creates `<new>`'s dictionary entry there after the read version, with no statement of `<new>` committed in the home"
)]
fn entry_conflicts_nothing(w: &mut Steps) -> TestResult {
    let (new, other, knows, x, seen) = (
        w.iri_in(2, "new")?,
        w.iri_in(1, "other")?,
        w.iri("knows"),
        w.iri_in(2, "x")?,
        w.iri("seen"),
    );
    w.kind = in_spread(w, async |client| {
        let first = client.begin(TENANT, false).await?;
        nothing(&read_whole(client, &first, &Pattern::of(Some(&new), None)).await?)?;
        committed(client, &other, &knows, &iri(&new)).await?;
        client
            .write(&first, &write_of(&x, &seen, &string("true"))?)
            .await?;
        match client.commit(first, None).await {
            Ok(_) => Ok(None),
            Err(refused) => Ok(Some(refusal_of(&refused)?.1)),
        }
    })?;
    Ok(())
}

#[then("the reading transaction commits")]
fn reader_commits(w: &mut Steps) {
    assert_eq!(w.kind, None, "{}", w.over());
}

#[when(
    "two transactions in `<a>`'s partition concurrently insert `<a> <knows> <b>`, which is not held, `<b>` new and homed in another partition"
)]
fn same_new_statement(w: &mut Steps) -> TestResult {
    let (a, b, knows) = (w.iri_in(1, "a")?, w.iri_in(2, "b")?, w.iri("knows"));
    let (kinds, held) = in_spread(w, async |client| {
        let (first, second) = (
            client.begin(TENANT, false).await?,
            client.begin(TENANT, false).await?,
        );
        let unread = w_unread(&a);
        for transaction in [&first, &second] {
            let _found = read_whole(client, transaction, &unread).await?;
            client
                .write(transaction, &write_of(&a, &knows, &iri(&b))?)
                .await?;
        }
        let one = client.commit(first, None).await;
        let two = client.commit(second, None).await;
        let retried = client.begin(TENANT, false).await?;
        let held = read_whole(client, &retried, &Pattern::of(Some(&a), Some(&knows))).await?;
        client.abort(retried).await?;
        Ok((vec![one, two], held))
    })?;
    w.seen.facts = kinds.iter().map(|outcome| format!("{outcome:?}")).collect();
    let mut outcomes = kinds.into_iter();
    w.committed = outcomes.next().is_some_and(|one| one.is_ok());
    w.seen.refusals = outcomes.next().and_then(Result::err).into_iter().collect();
    w.seen.answers = vec![held];
    w.seen.facts.push(b);
    Ok(())
}

/// What each transaction reads first, so both hold a version before either commits.
fn w_unread(a: &str) -> Pattern {
    Pattern::of(Some(a), Some("http://ex.org/unread"))
}

#[then(
    "both registrations of `<b>` yield one ID and neither aborts; one transaction commits and the other aborts with a conflict, and its retry finds the statement held"
)]
fn same_statement_conflict(w: &mut Steps) -> TestResult {
    assert!(w.committed, "{:?} {}", w.seen.facts, w.over());
    let refused = w
        .seen
        .refusals
        .first()
        .context("the second commit's refusal")?;
    assert_eq!(refusal_of(refused)?.0, ErrorCode::Conflict, "{refused:?}");
    let held = w.seen.answers.first().context("what is held")?;
    let b = w.seen.facts.last().context("<b>")?;
    assert_eq!(&held.column(0), std::slice::from_ref(b), "{}", w.over());
    Ok(())
}

#[when(
    "a session commits in partition 1 at epoch 20, and its next transaction, carrying that token, commits in partition 2 while partition 2's latest delivered close is 17"
)]
fn causally_later(w: &mut Steps) -> TestResult {
    let (one, two, p) = (w.iri_in(1, "one")?, w.iri_in(2, "two")?, w.iri("p"));
    w.seen.commits = in_spread(w, async |client| {
        let session = client.register().await?;
        let first = client
            .direct_write(direct_of(&one, &p, &string("x"))?, session)
            .await?;
        let own = client
            .begin_with(Begin::new(TENANT, 0, false), session)
            .await?;
        client
            .write(&own, &write_of(&two, &p, &string("x"))?)
            .await?;
        let second = client.commit(own, session).await?;
        Ok(vec![first, second])
    })?;
    Ok(())
}

/// The epoch of a commit's version.
fn epoch_of(committed: &Committed) -> u64 {
    Version::from_u64(committed.version()).epoch().get()
}

#[then("partition 2 commits it only after close 19 is delivered, at epoch 20 or later")]
fn not_earlier(w: &mut Steps) -> TestResult {
    let [first, second] = w.seen.commits.as_slice() else {
        return Err(TestError::new("two commits"));
    };
    assert!(
        epoch_of(second) >= epoch_of(first),
        "{first:?} {second:?} {}",
        w.over()
    );
    assert_eq!(second.token().context("a token")?.partition(), 2);
    Ok(())
}
