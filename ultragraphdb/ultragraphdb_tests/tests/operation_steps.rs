/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! Commit-time operations a client asks for, decided by a real partition in commit order. Transactions begun before any of them
//! commits run concurrently, whichever connection carries them.

use cucumber::{then, when};
use ultragraphdb_client_compio::{Committed, Transaction};
use ultragraphdb_protocol_wire::{CommitOperation, ErrorCode, Link, Operation, TermTable};
use ultragraphdb_tool_bdd::{Context, TestError, TestResult};

use crate::Steps;
use crate::client_reads::{Pattern, TENANT, iri, read_whole, string};
use crate::endpoint_steps::declare;
use crate::request_steps::{latest_read, unique};
use crate::shells::TestClient;
use crate::transaction_steps::{in_shared, nothing};

/// An insert-if-absent of `id` holding `value`.
fn insert_if_absent(id: &str, value: &str) -> TestResult<Operation> {
    let mut terms = TermTable::new();
    let operation = CommitOperation::InsertIfAbsent {
        graph: None,
        predicate: terms.iri(id),
        value: terms.intern(string(value))?,
        properties: Vec::new(),
    };
    Ok(Operation::new(terms, operation))
}

/// An upsert-and-link of `id` holding `value`, with the property `label` for creation, linking `memory` by `link`.
fn upsert_and_link(
    (id, value): (&str, &str),
    label: Option<(&str, &str)>,
    (link, memory): (&str, &str),
) -> TestResult<Operation> {
    let mut terms = TermTable::new();
    let properties = match label {
        Some((key, text)) => vec![(terms.iri(key), terms.intern(string(text))?)],
        None => Vec::new(),
    };
    let predicate = terms.iri(id);
    let value = terms.intern(string(value))?;
    let links = vec![Link::to_node(terms.iri(link), terms.iri(memory))];
    let operation = CommitOperation::UpsertAndLink {
        graph: None,
        predicate,
        value,
        properties,
        links,
    };
    Ok(Operation::new(terms, operation))
}

/// `count` transactions begun together, each holding what `operation` makes of its number: their commits, in order.
async fn commit_together(
    client: &mut TestClient,
    count: u32,
    operation: impl Fn(u32) -> TestResult<Operation>,
) -> TestResult<Vec<Committed>> {
    let mut transactions: Vec<Transaction> = Vec::new();
    for at in 0..count {
        let own = client.begin(TENANT, false).await?;
        client.operation(&own, &operation(at)?).await?;
        transactions.push(own);
    }
    let mut commits = Vec::new();
    for own in transactions {
        commits.push(client.commit(own, None).await?);
    }
    Ok(commits)
}

/// That every commit names one node, created by the first alone, and returns it.
fn one_node(commits: &[Committed]) -> TestResult<String> {
    let first = commits.first().context("a commit")?;
    let node = first.node(0).context("the node named")?.to_string();
    for (at, committed) in commits.iter().enumerate() {
        assert_eq!(committed.node(0), Some(node.as_str()), "{committed:?}");
        let created = committed.results().first().context("a result")?.created();
        assert_eq!(created, at == 0, "{committed:?}");
    }
    Ok(node)
}

#[when(
    "eight transactions concurrently insert-if-absent a node with `epi.id` `\"x\"`, declared unique, all in the partition owning the bucket of `\"x\"`"
)]
fn eight_inserts(w: &mut Steps) -> TestResult {
    let id = w.iri("epi.id");
    let (commits, held) = in_shared(w, async |client| {
        declare(client, unique(&id)).await?;
        let commits = commit_together(client, 8, |_| insert_if_absent(&id, "x")).await?;
        let pattern = Pattern::of(None, Some(&id));
        Ok((commits, latest_read(client, &pattern).await?))
    })?;
    w.seen.facts = vec![one_node(&commits)?];
    w.seen.commits = commits;
    w.seen.answers = vec![held];
    Ok(())
}

#[then("all eight commit, all receive the same IRI, and one node holds `epi.id` `\"x\"`")]
fn one_inserted(w: &mut Steps) -> TestResult {
    assert_eq!(w.seen.commits.len(), 8, "{}", w.over());
    let held = w.seen.answers.first().context("what is held")?;
    assert_eq!(held.column(0), w.seen.facts, "{}", w.over());
    Ok(())
}

#[when(
    "eight transactions each create a memory `m1` to `m8` and concurrently upsert-and-link the entity with `epi.id` `\"x\"`, declared unique, each linking its memory to it by `HAS_ENTITY`, all in the partition owning the bucket of `\"x\"`"
)]
fn eight_upserts(w: &mut Steps) -> TestResult {
    let [id, link, m] = ["epi.id", "HAS_ENTITY", "m"].map(|name| w.iri(name));
    let (commits, held, reverse) = in_shared(w, async |client| {
        declare(client, unique(&id)).await?;
        let commits = commit_together(client, 8, |at| {
            upsert_and_link((&id, "x"), None, (&link, &format!("{m}{at}")))
        })
        .await?;
        let node = commits
            .first()
            .context("a commit")?
            .node(0)
            .context("the node")?
            .to_string();
        Ok((
            commits,
            latest_read(client, &Pattern::of(None, Some(&id))).await?,
            latest_read(
                client,
                &Pattern::of(None, Some(&link)).with_object(iri(&node)),
            )
            .await?,
        ))
    })?;
    w.seen.facts = vec![one_node(&commits)?];
    w.seen.answers = vec![held, reverse];
    Ok(())
}

#[then(
    "all eight commit and receive the same IRI, one node holds `epi.id` `\"x\"`, each memory holds `HAS_ENTITY` to that node, and a reverse read of `HAS_ENTITY` for the node returns all eight memories"
)]
fn one_upserted(w: &mut Steps) -> TestResult {
    let [held, reverse] = w.seen.answers.as_slice() else {
        return Err(TestError::new("two answers"));
    };
    assert_eq!(held.column(0), w.seen.facts, "{}", w.over());
    let mut memories = reverse.column(0);
    memories.sort();
    let m = w.iri("m");
    let mut expected: Vec<String> = (0..8).map(|at| format!("{m}{at}")).collect();
    expected.sort();
    assert_eq!(memories, expected, "{}", w.over());
    Ok(())
}

#[when(
    "two transactions concurrently upsert-and-link `epi.id` `\"y\"`, one carrying the label `\"first\"` for creation and the other `\"second\"`, both in the partition owning the bucket of `\"y\"`"
)]
fn two_creations(w: &mut Steps) -> TestResult {
    let [id, link, m, label] = ["epi.id", "link", "m", "label"].map(|name| w.iri(name));
    let (commits, labels) = in_shared(w, async |client| {
        declare(client, unique(&id)).await?;
        let commits = commit_together(client, 2, |at| {
            let text = if at == 0 { "first" } else { "second" };
            upsert_and_link(
                (&id, "y"),
                Some((&label, text)),
                (&link, &format!("{m}{at}")),
            )
        })
        .await?;
        let node = commits
            .first()
            .context("a commit")?
            .node(0)
            .context("the node")?
            .to_string();
        Ok((
            commits,
            latest_read(client, &Pattern::of(Some(&node), Some(&label))).await?,
        ))
    })?;
    w.seen.facts = vec![one_node(&commits)?];
    w.seen.answers = vec![labels];
    Ok(())
}

#[then(
    "the node holds the label of the transaction committed first and not the other, and only that transaction's reply says the node was created"
)]
fn first_label(w: &mut Steps) -> TestResult {
    let labels = w.seen.answers.first().context("the labels")?;
    assert_eq!(labels.column(0), ["first"], "{}", w.over());
    Ok(())
}

#[when(
    "a transaction in the partition owning the bucket of `\"z\"` upsert-and-links `epi.id` `\"z\"` and aborts with a conflict on another of its reads"
)]
fn upsert_aborted(w: &mut Steps) -> TestResult {
    let [id, link, m, a, p] = ["epi.id", "link", "m", "a", "p"].map(|name| w.iri(name));
    let (refused, node, linked) = in_shared(w, async |client| {
        declare(client, unique(&id)).await?;
        let own = client.begin(TENANT, false).await?;
        let _read = read_whole(client, &own, &Pattern::of(Some(&a), Some(&p))).await?;
        crate::transaction_steps::committed(client, &a, &p, &string("later")).await?;
        client
            .operation(&own, &upsert_and_link((&id, "z"), None, (&link, &m))?)
            .await?;
        let refused = client.commit(own, None).await.err().context("a conflict")?;
        Ok((
            refused,
            latest_read(client, &Pattern::of(None, Some(&id))).await?,
            latest_read(client, &Pattern::of(None, Some(&link))).await?,
        ))
    })?;
    w.seen.refusals = vec![refused];
    w.seen.answers = vec![node, linked];
    Ok(())
}

#[then("no node holds `epi.id` `\"z\"` and no link of the transaction is held")]
fn upsert_wrote_nothing(w: &mut Steps) -> TestResult {
    let refused = w.seen.refusals.first().context("the refusal")?;
    assert_eq!(
        crate::client_reads::refusal_of(refused)?.0,
        ErrorCode::Conflict,
        "{refused:?}"
    );
    for answer in &w.seen.answers {
        nothing(answer)?;
    }
    Ok(())
}

/// A counter add of `delta` on `<subject> <predicate>`.
fn counter_add(subject: &str, predicate: &str, delta: i64) -> Operation {
    let mut terms = TermTable::new();
    let operation = CommitOperation::CounterAdd {
        graph: None,
        subject: terms.iri(subject),
        predicate: terms.iri(predicate),
        delta,
    };
    Operation::new(terms, operation)
}

#[when("a hundred transactions concurrently add 1 to the counter `next-id`")]
fn hundred_adds(w: &mut Steps) -> TestResult {
    let [counters, next_id] = ["counters", "next-id"].map(|name| w.iri(name));
    let held = in_shared(w, async |client| {
        let commits =
            commit_together(client, 100, |_| Ok(counter_add(&counters, &next_id, 1))).await?;
        w_last(&commits)
    })?;
    w.seen.facts = vec![held.to_string()];
    Ok(())
}

/// The value the last of `commits` read.
fn w_last(commits: &[Committed]) -> TestResult<i64> {
    let last = commits.last().context("a commit")?;
    Ok(last.results().first().context("a result")?.value())
}

#[then("all commit and `next-id` has grown by 100")]
fn grown_by_hundred(w: &mut Steps) {
    assert_eq!(w.seen.facts, ["100"], "{}", w.over());
}
