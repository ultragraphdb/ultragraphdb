/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! What a client reads through the real compute and storage: its own writes in order, reads held open by their credit, graphs,
//! and how a statement is named. Every scenario names IRIs of its own in the cluster its run shares.

use std::sync::atomic::{AtomicBool, Ordering};

use cucumber::{then, when};
use ultragraphdb_protocol_wire::{DirectWrite, RequestTerm, TermTable, WriteChange};
use ultragraphdb_tool_bdd::{Context, TestError, TestResult};

use crate::Steps;
use crate::client_reads::{
    Answer, GraphAt, Pattern, TENANT, batch_of, commit_a_write, drain, edge_of, iri, read,
    read_whole, string, write_of,
};
use crate::shells::TestClient;
use crate::transaction_steps::{committed, in_shared, nothing};

/// Rows of the fixture of long literals, far more than one read's credit of frames.
const BIG_ROWS: u32 = 14;

/// The bytes of a literal of that fixture: two fill a frame of the client's `max_frame`.
const BIG_BYTES: usize = 400_000;

/// The predicate of that fixture.
pub(crate) const BIG: &str = "http://ex.org/fixture/blob";

/// Whether this run's shared cluster holds the fixture.
static BIG_WRITTEN: AtomicBool = AtomicBool::new(false);

/// The next run's cluster holds no fixture yet.
pub(crate) fn forget_big() {
    BIG_WRITTEN.store(false, Ordering::Relaxed);
}

/// Writes the fixture a read takes more frames than its credit to answer, once per shared cluster.
pub(crate) async fn holding_big(client: &mut TestClient) -> TestResult {
    if BIG_WRITTEN.load(Ordering::Relaxed) {
        return Ok(());
    }
    write_big(client).await?;
    BIG_WRITTEN.store(true, Ordering::Relaxed);
    Ok(())
}

/// Writes the fixture in `client`'s cluster.
pub(crate) async fn write_big(client: &mut TestClient) -> TestResult {
    let fill = "x".repeat(BIG_BYTES);
    for row in 0..BIG_ROWS {
        let subject = format!("http://ex.org/fixture/s{row}");
        committed(client, &subject, BIG, &string(&format!("{row}{fill}"))).await?;
    }
    Ok(())
}

/// The pattern of the fixture's every row.
pub(crate) fn big_pattern() -> Pattern {
    Pattern::of(None, Some(BIG))
}

/// A one-frame write of `subject` `predicate` `object` in `graph`.
pub(crate) fn in_graph(
    graph: &str,
    subject: &str,
    predicate: &str,
    object: &RequestTerm,
) -> TestResult<DirectWrite> {
    let mut terms = TermTable::new();
    let change = WriteChange::Insert {
        graph: Some(terms.iri(graph)),
        subject: terms.iri(subject),
        predicate: terms.iri(predicate),
        object: terms.intern(object.clone())?,
    };
    Ok(DirectWrite::new(TENANT, terms, vec![change]))
}

/// A read-write transaction with the fixture's read open and its first batch taken, and the write `<subject> <p> "x"` sent on
/// its stream: the transaction, the read.
async fn write_behind_a_read(
    client: &mut TestClient,
    subject: &str,
    predicate: &str,
) -> TestResult<(
    ultragraphdb_client_compio::Transaction,
    ultragraphdb_client_compio::ResultStream,
)> {
    holding_big(client).await?;
    let transaction = client.begin(TENANT, false).await?;
    let stream = client
        .read(
            &transaction,
            &big_pattern().read(&transaction, 0, Vec::new())?,
        )
        .await?;
    let _first = client
        .next_batch(&stream, batch_of)
        .await?
        .context("the read's first batch")?;
    client
        .write(&transaction, &write_of(subject, predicate, &string("x"))?)
        .await?;
    Ok((transaction, stream))
}

/// What a new transaction reads of `subject` `predicate`.
async fn seen_after(client: &mut TestClient, subject: &str, predicate: &str) -> TestResult<Answer> {
    let reader = client.begin(TENANT, false).await?;
    let answer = read_whole(
        client,
        &reader,
        &Pattern::of(Some(subject), Some(predicate)),
    )
    .await?;
    client.abort(reader).await?;
    Ok(answer)
}

#[when(
    "a transaction writes `<new> <p> \"x\"`, creating `<new>`, removes every object of `<a> <p> ?o`, and reads both"
)]
fn new_term_and_emptied_list(w: &mut Steps) -> TestResult {
    let [new, a, p, b] = ["new", "a", "p", "b"].map(|name| w.iri(name));
    w.seen.answers = in_shared(w, async |client| {
        committed(client, &a, &p, &iri(&b)).await?;
        let own = client.begin(TENANT, false).await?;
        let mut terms = TermTable::new();
        let changes = vec![
            WriteChange::Insert {
                graph: None,
                subject: terms.iri(&new),
                predicate: terms.iri(&p),
                object: terms.intern(string("x"))?,
            },
            WriteChange::DeletePattern {
                graph: None,
                subject: terms.iri(&a),
                predicate: terms.iri(&p),
            },
        ];
        client
            .write(
                &own,
                &ultragraphdb_protocol_wire::Write::new(terms, changes),
            )
            .await?;
        let answers = vec![
            read_whole(client, &own, &Pattern::of(Some(&new), Some(&p))).await?,
            read_whole(client, &own, &Pattern::of(Some(&a), Some(&p))).await?,
        ];
        client.abort(own).await?;
        Ok(answers)
    })?;
    Ok(())
}

#[then(
    "the read returns `\"x\"` for `<new>` with `<new>` named by its text, and nothing for `<a>`"
)]
fn new_term_read(w: &mut Steps) -> TestResult {
    let [new, emptied] = w.seen.answers.as_slice() else {
        return Err(TestError::new("two answers"));
    };
    assert_eq!(new.column(0), ["x"], "{}", w.over());
    nothing(emptied)
}

#[when(
    "a client sends a read of its transaction, and a write on another stream before the read is answered"
)]
fn write_after_read(w: &mut Steps) -> TestResult {
    let [a, b, c, knows] = ["a", "b", "c", "knows"].map(|name| w.iri(name));
    w.seen.answers = in_shared(w, async |client| {
        committed(client, &a, &knows, &iri(&b)).await?;
        let pattern = Pattern::of(Some(&a), Some(&knows));
        let own = client.begin(TENANT, false).await?;
        let stream = client
            .read(&own, &pattern.read(&own, 0, Vec::new())?)
            .await?;
        client.write(&own, &write_of(&a, &knows, &iri(&c))?).await?;
        let first = drain(client, &stream).await?;
        let second = read_whole(client, &own, &pattern).await?;
        client.abort(own).await?;
        Ok(vec![first, second])
    })?;
    Ok(())
}

#[then(
    "the frontend sends the write only after the read is answered, and the read returns none of it"
)]
fn read_without_later_write(w: &mut Steps) -> TestResult {
    let [before, after] = w.seen.answers.as_slice() else {
        return Err(TestError::new("two answers"));
    };
    assert_eq!(before.column(0), [w.iri("b")], "{}", w.over());
    assert!(
        after.column(0).contains(&w.iri("c")),
        "{after:?} {}",
        w.over()
    );
    Ok(())
}

#[when(
    "a client sends a write and then a read of its transaction on another stream, without waiting for the write's reply"
)]
fn read_after_write(w: &mut Steps) -> TestResult {
    let [a, b, knows] = ["a", "b", "knows"].map(|name| w.iri(name));
    w.seen.answers = in_shared(w, async |client| {
        let own = client.begin(TENANT, false).await?;
        client.write(&own, &write_of(&a, &knows, &iri(&b))?).await?;
        let answer = read_whole(client, &own, &Pattern::of(Some(&a), Some(&knows))).await?;
        client.abort(own).await?;
        Ok(vec![answer])
    })?;
    Ok(())
}

#[then(
    "the frontend sends the read to compute only after the write is answered, and the read returns the write"
)]
fn read_of_earlier_write(w: &mut Steps) -> TestResult {
    let answer = w.seen.answers.first().context("the read's answer")?;
    assert_eq!(answer.column(0), [w.iri("b")], "{}", w.over());
    Ok(())
}

#[when(
    "T1 reads `<a> <name> ?o` with limit 1 and its graph unbound, `<a>` holding it in two graphs, T2 commits an insert into the graph T1's read did not reach, and T1 commits a write"
)]
fn limit_before_graph(w: &mut Steps) -> TestResult {
    let [a, name, g1, g2] = ["a", "name", "g1", "g2"].map(|n| w.iri(n));
    w.kind = in_shared(w, async |client| {
        for graph in [&g1, &g2] {
            let write = in_graph(graph, &a, &name, &string("x"))?;
            let _written = client.direct_write(write, None).await?;
        }
        let first = client.begin(TENANT, false).await?;
        let pattern = Pattern::of(Some(&a), Some(&name)).in_graph(GraphAt::Any);
        let page = read(client, &first, &pattern, (1, Vec::new())).await?;
        let reached = page.column(0);
        let unreached = [&g1, &g2]
            .into_iter()
            .find(|graph| !reached.contains(graph))
            .context("the graph the read did not reach")?;
        let insert = in_graph(unreached, &a, &name, &string("later"))?;
        let _inserted = client.direct_write(insert, None).await?;
        commit_a_write(client, first).await
    })?;
    w.committed = w.kind.is_none();
    Ok(())
}

#[when(
    "a read of a transaction is open, a Write the client sent after it is held behind it, and the client sends Commit"
)]
fn commit_takes_held(w: &mut Steps) -> TestResult {
    let [a, p] = ["a", "p"].map(|name| w.iri(name));
    let (late, seen) = in_shared(w, async |client| {
        let (own, stream) = write_behind_a_read(client, &a, &p).await?;
        let _committed = client.commit(own, None).await?;
        let late = client.next_batch(&stream, batch_of).await;
        Ok((
            late.is_ok_and(|batch| batch.is_some()),
            seen_after(client, &a, &p).await?,
        ))
    })?;
    w.committed = !late;
    w.seen.answers = vec![seen];
    Ok(())
}

#[then(
    "the frontend cancels the read, sends the Write on the Bind stream, then the Commit; the Write is in what commits, and no further frame of the read reaches the client"
)]
fn commit_took_held(w: &mut Steps) -> TestResult {
    assert!(
        w.committed,
        "a frame of the read after its Commit {}",
        w.over()
    );
    let answer = w.seen.answers.first().context("what the commit left")?;
    assert_eq!(answer.column(0), ["x"], "{}", w.over());
    Ok(())
}

#[when("a read of a transaction is open, a Write is held behind it, and the client sends Abort")]
fn abort_drops_held(w: &mut Steps) -> TestResult {
    let [a, p] = ["a", "p"].map(|name| w.iri(name));
    w.seen.answers = in_shared(w, async |client| {
        let (own, _stream) = write_behind_a_read(client, &a, &p).await?;
        client.abort(own).await?;
        Ok(vec![seen_after(client, &a, &p).await?])
    })?;
    Ok(())
}

#[then(
    "the frontend cancels the read, the partition never receives the Write, and nothing of the transaction is held"
)]
fn abort_dropped_held(w: &mut Steps) -> TestResult {
    nothing(w.seen.answers.first().context("what the abort left")?)
}

#[when(
    "a read of a transaction is open, a Write is held behind it, and the client cancels the read"
)]
fn cancel_releases_held(w: &mut Steps) -> TestResult {
    let [a, p] = ["a", "p"].map(|name| w.iri(name));
    w.seen.answers = in_shared(w, async |client| {
        let (own, stream) = write_behind_a_read(client, &a, &p).await?;
        client.cancel(stream).await;
        let answer = read_whole(client, &own, &Pattern::of(Some(&a), Some(&p))).await?;
        client.abort(own).await?;
        Ok(vec![answer])
    })?;
    Ok(())
}

#[then(
    "the frontend sends the read's Cancel and then the Write, without waiting for the read's `LAST`"
)]
fn cancel_released_held(w: &mut Steps) -> TestResult {
    let answer = w
        .seen
        .answers
        .first()
        .context("what the read after it found")?;
    assert_eq!(answer.column(0), ["x"], "{}", w.over());
    Ok(())
}

#[when(
    "a Write of a transaction is unanswered, a read behind it is held, and the client cancels the read"
)]
fn cancel_held_read(w: &mut Steps) -> TestResult {
    let [a, b, knows] = ["a", "b", "knows"].map(|name| w.iri(name));
    w.seen.answers = in_shared(w, async |client| {
        let own = client.begin(TENANT, false).await?;
        client.write(&own, &write_of(&a, &knows, &iri(&b))?).await?;
        let pattern = Pattern::of(Some(&a), Some(&knows));
        let stream = client
            .read(&own, &pattern.read(&own, 0, Vec::new())?)
            .await?;
        client.cancel(stream).await;
        let _committed = client.commit(own, None).await?;
        Ok(vec![seen_after(client, &a, &knows).await?])
    })?;
    Ok(())
}

#[then("no ComputeRead is sent for it, and the transaction goes on")]
fn held_read_dropped(w: &mut Steps) -> TestResult {
    let answer = w.seen.answers.first().context("what the commit left")?;
    assert_eq!(answer.column(0), [w.iri("b")], "{}", w.over());
    Ok(())
}

#[when(
    "a read of a transaction has reached its `LAST`, and the transaction then ends with an Error"
)]
fn answered_then_ended(w: &mut Steps) -> TestResult {
    let [a, b, p, q] = ["a", "b", "p", "q"].map(|name| w.iri(name));
    let (last, late) = in_shared(w, async |client| {
        committed(client, &a, &p, &iri(&b)).await?;
        let first = client.begin(TENANT, false).await?;
        let pattern = Pattern::of(Some(&a), Some(&p));
        let stream = client
            .read(&first, &pattern.read(&first, 0, Vec::new())?)
            .await?;
        let answer = drain(client, &stream).await?;
        committed(client, &a, &q, &iri(&b)).await?;
        committed(client, &a, &p, &string("later")).await?;
        let kind = commit_a_write(client, first).await?;
        let _kind = kind.context("a commit refused by the later write")?;
        Ok((answer, client.next_batch(&stream, batch_of).await?))
    })?;
    w.seen.answers = vec![last];
    w.seen.facts = vec![format!("{late:?}")];
    Ok(())
}

#[then("the frontend sends nothing further on that read's stream")]
fn nothing_further(w: &mut Steps) -> TestResult {
    let answer = w.seen.answers.first().context("the read's answer")?;
    assert_eq!(answer.column(0), [w.iri("b")], "{}", w.over());
    assert_eq!(w.seen.facts, ["None"], "{}", w.over());
    Ok(())
}

#[when("a read binds its object to `http://ex.org/b` and its predicate to `http://ex.org/knows`")]
fn bound_object(w: &mut Steps) -> TestResult {
    let [a, c, b, knows] = ["a", "c", "b", "knows"].map(|name| w.iri(name));
    w.seen.answers = in_shared(w, async |client| {
        committed(client, &a, &knows, &iri(&b)).await?;
        committed(client, &c, &knows, &iri(&b)).await?;
        let reader = client.begin(TENANT, true).await?;
        let pattern = Pattern::of(None, Some(&knows)).with_object(iri(&b));
        let answer = until_rows(client, &pattern, 2).await?;
        client.abort(reader).await?;
        Ok(vec![answer])
    })?;
    Ok(())
}

/// What a read-only transaction reads of `pattern`, in new transactions at newer snapshots until `rows` rows have closed.
pub(crate) async fn until_rows(
    client: &mut TestClient,
    pattern: &Pattern,
    rows: usize,
) -> TestResult<Answer> {
    let mut last = Answer::default();
    for _ in 0..500 {
        let reader = client.begin(TENANT, true).await?;
        last = read_whole(client, &reader, pattern).await?;
        client.abort(reader).await?;
        if last.rows.len() >= rows {
            return Ok(last);
        }
        crate::shells::pause(std::time::Duration::from_millis(10)).await;
    }
    Err(TestError::new(format!(
        "no snapshot of 500 read {rows} rows: the last read {last:?}"
    )))
}

#[then(
    "it reads the reverse lists of `http://ex.org/b` under `http://ex.org/knows` in the partitions `R(b)` names, and returns each subject"
)]
fn reverse_read(w: &mut Steps) -> TestResult {
    let answer = w.seen.answers.first().context("the read's answer")?;
    let mut subjects = answer.column(0);
    subjects.sort();
    assert_eq!(subjects, [w.iri("a"), w.iri("c")], "{}", w.over());
    Ok(())
}

#[when(
    "`<a> <p> <b>` is held in the default graph and in `<g1>`, and a read leaves the graph unbound"
)]
fn every_graph(w: &mut Steps) -> TestResult {
    let [a, p, b, g1] = ["a", "p", "b", "g1"].map(|name| w.iri(name));
    w.seen.answers = in_shared(w, async |client| {
        committed(client, &a, &p, &iri(&b)).await?;
        let _named = client
            .direct_write(in_graph(&g1, &a, &p, &iri(&b))?, None)
            .await?;
        let pattern = Pattern::of(Some(&a), Some(&p)).in_graph(GraphAt::Any);
        Ok(vec![until_rows(client, &pattern, 2).await?])
    })?;
    Ok(())
}

#[then("it returns both statements, each with its graph")]
fn every_graph_read(w: &mut Steps) -> TestResult {
    let answer = w.seen.answers.first().context("the read's answer")?;
    assert_eq!(answer.column(1), [w.iri("b"), w.iri("b")], "{}", w.over());
    let mut graphs = answer.column(0);
    graphs.sort();
    assert_eq!(graphs, [String::new(), w.iri("g1")], "{}", w.over());
    Ok(())
}

#[when("`<a> <knows> <b>` is held without a statement ID and a read returns it")]
fn without_statement_id(w: &mut Steps) -> TestResult {
    let [a, b, knows] = ["a", "b", "knows"].map(|name| w.iri(name));
    w.seen.answers = in_shared(w, async |client| {
        committed(client, &a, &knows, &iri(&b)).await?;
        Ok(vec![
            until_rows(client, &Pattern::of(Some(&a), Some(&knows)), 1).await?,
        ])
    })?;
    Ok(())
}

#[then("the row names the statement by `<a>`, `<knows>`, `<b>` and its graph alone")]
fn named_by_quad(w: &mut Steps) -> TestResult {
    let answer = w.seen.answers.first().context("the read's answer")?;
    assert_eq!(answer.rows, [[w.iri("b"), String::new()]], "{}", w.over());
    Ok(())
}

#[when(
    "`<a> <knows> <b>` is held without a statement ID, a parallel edge of it is inserted, and a read returns both"
)]
fn parallel_edges(w: &mut Steps) -> TestResult {
    let [a, b, knows, since] = ["a", "b", "knows", "since"].map(|name| w.iri(name));
    w.seen.answers = in_shared(w, async |client| {
        committed(client, &a, &knows, &iri(&b)).await?;
        let _edge = client
            .direct_write(edge_of((&a, &knows, &b), &since, "2020")?, None)
            .await?;
        Ok(vec![
            until_rows(client, &Pattern::of(Some(&a), Some(&knows)), 2).await?,
        ])
    })?;
    Ok(())
}

/// Two rows of one object, each with a reifier of its own.
fn two_reifiers(w: &Steps, of: &str) -> TestResult {
    let answer = w.seen.answers.first().context("the read's answer")?;
    assert_eq!(answer.column(0), [of, of], "{}", w.over());
    let reifiers = answer.column(1);
    assert!(
        reifiers
            .iter()
            .all(|reifier| reifier.contains("/.well-known/genid/")),
        "{reifiers:?}"
    );
    assert_ne!(reifiers.first(), reifiers.get(1), "{}", w.over());
    Ok(())
}

#[then(
    "two rows return, each carrying its own statement's reifier IRI, and neither carries a statement ID"
)]
fn parallel_read(w: &mut Steps) -> TestResult {
    two_reifiers(w, &w.iri("b"))
}

#[when(
    "`<a> <knows> <b>` is held as two parallel edges, and a read binds its object to `<b>` and its predicate to `knows`"
)]
fn reverse_parallel(w: &mut Steps) -> TestResult {
    let [a, b, knows, since] = ["a", "b", "knows", "since"].map(|name| w.iri(name));
    w.seen.answers = in_shared(w, async |client| {
        for year in ["2019", "2020"] {
            let _edge = client
                .direct_write(edge_of((&a, &knows, &b), &since, year)?, None)
                .await?;
        }
        let pattern = Pattern::of(None, Some(&knows)).with_object(iri(&b));
        Ok(vec![until_rows(client, &pattern, 2).await?])
    })?;
    Ok(())
}

#[then(
    "two rows return for `<a>`, each with its statement's reifier from the reverse list's object record, and compute sends no Scan of a forward list"
)]
fn reverse_parallel_read(w: &mut Steps) -> TestResult {
    two_reifiers(w, &w.iri("a"))
}

#[when(
    "`<a> <p> <b>` is held in `<g1>` and `<g2>`, and a read of `<a> <p> ?o` leaves the graph unbound"
)]
fn walked_graphs(w: &mut Steps) -> TestResult {
    let [a, p, b, g1, g2] = ["a", "p", "b", "g1", "g2"].map(|name| w.iri(name));
    w.seen.answers = in_shared(w, async |client| {
        for graph in [&g1, &g2] {
            let write = in_graph(graph, &a, &p, &iri(&b))?;
            let _written = client.direct_write(write, None).await?;
        }
        let pattern = Pattern::of(Some(&a), Some(&p)).in_graph(GraphAt::Any);
        Ok(vec![until_rows(client, &pattern, 2).await?])
    })?;
    Ok(())
}

#[then(
    "compute finds each graph by a Scan of limit 1 from the keyspace's tenant and then from just past the graph found, cancels each after its first frame, and returns both statements"
)]
fn walked_read(w: &mut Steps) -> TestResult {
    let answer = w.seen.answers.first().context("the read's answer")?;
    let mut graphs = answer.column(0);
    graphs.sort();
    let mut expected = [w.iri("g1"), w.iri("g2")];
    expected.sort();
    assert_eq!(graphs, expected, "{}", w.over());
    Ok(())
}

#[when(
    "T1 reads with its graph unbound through compute, finding `<g1>` alone, T2 commits a statement in a graph the walk passed over, and T1 commits a write"
)]
fn graph_in_gap(w: &mut Steps) -> TestResult {
    let [a, p, b, g1, g0] = ["a", "p", "b", "g1", "g0"].map(|name| w.iri(name));
    w.kind = in_shared(w, async |client| {
        let held = in_graph(&g1, &a, &p, &iri(&b))?;
        let _held = client.direct_write(held, None).await?;
        let first = client.begin(TENANT, false).await?;
        let pattern = Pattern::of(Some(&a), Some(&p)).in_graph(GraphAt::Any);
        let found = read_whole(client, &first, &pattern).await?;
        if found.column(0) != [g1.clone()] {
            return Err(TestError::new(format!("T1 found {found:?}")));
        }
        let gap = in_graph(&g0, &a, &p, &iri(&b))?;
        let _gap = client.direct_write(gap, None).await?;
        commit_a_write(client, first).await
    })?;
    Ok(())
}

#[when(
    "a transaction's first read takes six frames at `initial_credit` 4, and after its first batch the client sends a second read and waits on it"
)]
fn second_read_after_first_frame(w: &mut Steps) -> TestResult {
    let [a, p, b] = ["a", "p", "b"].map(|name| w.iri(name));
    let (second, first_rest) = in_shared(w, async |client| {
        holding_big(client).await?;
        committed(client, &a, &p, &iri(&b)).await?;
        let own = client.begin(TENANT, false).await?;
        let first = client
            .read(&own, &big_pattern().read(&own, 0, Vec::new())?)
            .await?;
        let _batch = client
            .next_batch(&first, batch_of)
            .await?
            .context("the first read's first batch")?;
        let pattern = Pattern::of(Some(&a), Some(&p));
        let second = client
            .read(&own, &pattern.read(&own, 0, Vec::new())?)
            .await?;
        let answer = drain(client, &second).await?;
        let rest = drain(client, &first).await?;
        client.abort(own).await?;
        Ok((answer, rest))
    })?;
    w.seen.answers = vec![second, first_rest];
    Ok(())
}

#[then(
    "the frontend sends the second read to compute on the first read's first frame, and the client receives its rows with the first read still open"
)]
fn second_read_answered(w: &mut Steps) -> TestResult {
    let [second, rest] = w.seen.answers.as_slice() else {
        return Err(TestError::new("two answers"));
    };
    assert_eq!(second.column(0), [w.iri("b")], "{}", w.over());
    assert!(!rest.rows.is_empty(), "the first read went on {}", w.over());
    Ok(())
}

#[when("a read-write transaction creates `<new>` and the connection carrying it closes after 1 s")]
fn closed_connection(w: &mut Steps) -> TestResult {
    let [new, p] = ["new", "p"].map(|name| w.iri(name));
    let at = crate::cluster::shared_client(w.carrier)?;
    let (held, later) = in_shared(w, async |client| {
        let mut gone = crate::cluster::client(&at).await?;
        let own = gone.begin(TENANT, false).await?;
        gone.write(&own, &write_of(&new, &p, &string("x"))?).await?;
        let _bound = read_whole(&mut gone, &own, &Pattern::of(Some(&new), Some(&p))).await?;
        gone.close().await;
        drop(gone);
        let held = seen_after(client, &new, &p).await?;
        committed(client, &new, &p, &string("later")).await?;
        Ok((held, seen_after(client, &new, &p).await?))
    })?;
    w.seen.answers = vec![held, later];
    Ok(())
}

#[then(
    "the host aborts the transaction then, not at its limit, and its entry for `<new>` is released"
)]
fn closed_aborted(w: &mut Steps) -> TestResult {
    let [held, later] = w.seen.answers.as_slice() else {
        return Err(TestError::new("two answers"));
    };
    nothing(held)?;
    assert_eq!(later.column(0), ["later"], "{}", w.over());
    Ok(())
}

#[when("a transaction's write set grows past the Welcome's `max_write_set` and it commits")]
fn past_write_set(w: &mut Steps) -> TestResult {
    let [a, p] = ["a", "p"].map(|name| w.iri(name));
    let (refused, bound, held) = in_shared(w, async |client| {
        let bound = u64::from(client.welcome().context("the Welcome")?.max_write_set());
        let own = client.begin(TENANT, false).await?;
        let fill = "z".repeat(BIG_BYTES);
        for at in 0..(bound as usize / BIG_BYTES + 2) {
            // Each literal of its own: one text repeated is one dictionary entry.
            let long = format!("{at}{fill}");
            client
                .write(&own, &write_of(&format!("{a}{at}"), &p, &string(&long))?)
                .await?;
        }
        let refused = client.commit(own, None).await.err().context("a refusal")?;
        Ok((
            refused,
            bound,
            seen_after(client, &format!("{a}0"), &p).await?,
        ))
    })?;
    w.seen.refusals = vec![refused];
    w.seen.facts = vec![bound.to_string()];
    w.seen.answers = vec![held];
    Ok(())
}

#[then(
    "the commit fails with a too-large error that is not marked retryable, its `detail` the bound, and nothing it wrote is held"
)]
fn write_set_refused(w: &mut Steps) -> TestResult {
    let refused = w.seen.refusals.first().context("the refusal")?;
    let (code, detail, _) = crate::client_reads::refusal_of(refused)?;
    assert_eq!(
        code,
        ultragraphdb_protocol_wire::ErrorCode::TooLarge,
        "{refused:?}"
    );
    assert_eq!(
        &detail.to_string(),
        w.seen.facts.first().context("the bound")?
    );
    assert!(!refused.retryable(), "{refused:?} {}", w.over());
    nothing(w.seen.answers.first().context("what is held")?)
}

/// How many rows of the long read of `A long read flows without a manual grant`, each row's object long enough that they take
/// more frames than the Welcome's credit.
const LONG_READ_ROWS: u32 = 10_000;

#[when("a client reads 10,000 rows through a shell with an initial credit of 4 frames")]
fn long_read(w: &mut Steps) -> TestResult {
    let [s, p] = ["s", "p"].map(|name| w.iri(name));
    let (rows, batches, credit) = in_shared(w, async |client| {
        let credit = client.welcome().context("the Welcome")?.initial_credit();
        let fill = "x".repeat(400);
        for page in 0..LONG_READ_ROWS / 1_000 {
            let mut terms = TermTable::new();
            let predicate = terms.iri(&p);
            let mut changes = Vec::new();
            for at in 0..1_000 {
                changes.push(WriteChange::Insert {
                    graph: None,
                    subject: terms.iri(&format!("{s}/{page}/{at}")),
                    predicate,
                    object: terms.intern(string(&format!("{page}.{at}{fill}")))?,
                });
            }
            let _page = client
                .direct_write(DirectWrite::new(TENANT, terms, changes), None)
                .await?;
        }
        let own = client.begin(TENANT, false).await?;
        let pattern = Pattern::of(None, Some(&p));
        let stream = client
            .read(&own, &pattern.read(&own, 0, Vec::new())?)
            .await?;
        let (mut rows, mut batches) = (0_u32, 0_u32);
        while let Some(found) = client.next_batch(&stream, |batch| batch.rows()).await? {
            rows += found;
            batches += 1;
        }
        client.abort(own).await?;
        Ok((rows, batches, credit))
    })?;
    w.seen.facts = vec![rows.to_string(), batches.to_string(), credit.to_string()];
    Ok(())
}

#[then(
    "every row arrives, credit being granted as batches are consumed, and no grant of zero frames is sent"
)]
fn long_read_arrived(w: &mut Steps) -> TestResult {
    let [rows, batches, credit] = w.seen.facts.as_slice() else {
        return Err(TestError::new("the counts"));
    };
    assert_eq!(rows, &LONG_READ_ROWS.to_string(), "{}", w.over());
    let (batches, credit): (u32, u32) = (batches.parse()?, credit.parse()?);
    assert!(
        batches > credit,
        "{batches} frames past credit {credit} {}",
        w.over()
    );
    Ok(())
}
