/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! A read-write transaction's reads through the real frontend, compute and storage: its own writes, and what its commit
//! validates. Every scenario names IRIs of its own in the cluster its run shares.

use compio::runtime::Runtime;
use cucumber::{then, when};
use ultragraphdb_client_compio::Transaction;
use ultragraphdb_protocol_wire::RequestTerm;
use ultragraphdb_tool_bdd::{Context, TestError, TestResult};

use crate::Steps;
use crate::client_reads::{
    Pattern, TENANT, commit_a_write, direct_of, edge_of, iri, property_of, read, read_whole,
    string, write_of,
};
use crate::cluster::{TestClient, client, shared_client};

/// Conflict kind Range (`PROTOCOL.md` §9.4).
const KIND_RANGE: u64 = 3;
/// Conflict kind AbsentTerm.
const KIND_ABSENT_TERM: u64 = 4;

/// Runs `scene` with a client of the cluster `w`'s run shares.
fn in_shared<T>(
    w: &Steps,
    scene: impl AsyncFnOnce(&mut TestClient) -> TestResult<T>,
) -> TestResult<T> {
    let address = shared_client(w.carrier)?;
    Runtime::new()?.block_on(async {
        let mut client = client(&address).await?;
        scene(&mut client).await
    })
}

/// Commits `subject` `predicate` `object` in one frame, as another transaction does.
async fn committed(
    client: &mut TestClient,
    subject: &str,
    predicate: &str,
    object: &RequestTerm,
) -> TestResult {
    let _committed = client
        .direct_write(direct_of(subject, predicate, object)?, None)
        .await?;
    Ok(())
}

/// A read finding nothing, or the failure naming what it found.
fn nothing(found: &crate::client_reads::Answer) -> TestResult {
    if found.rows.is_empty() {
        Ok(())
    } else {
        Err(TestError::new(format!("T1 found {found:?}")))
    }
}

#[when(
    "a transaction writes `<a> <knows> <b>`, the write is answered, and the same transaction reads `<a> <knows> ?o` before committing"
)]
fn own_write(w: &mut Steps) -> TestResult {
    let [a, b, knows] = ["a", "b", "knows"].map(|name| w.iri(name));
    w.answers = in_shared(w, async |client| {
        let pattern = Pattern::of(Some(&a), Some(&knows));
        let own = client.begin(TENANT, false).await?;
        client.write(&own, &write_of(&a, &knows, &iri(&b))?).await?;
        let seen = read_whole(client, &own, &pattern).await?;
        let other = client.begin(TENANT, false).await?;
        let not_seen = read_whole(client, &other, &pattern).await?;
        client.abort(other).await?;
        client.abort(own).await?;
        Ok(vec![seen, not_seen])
    })?;
    Ok(())
}

#[then("the read returns `<b>`, and a read by any other transaction does not")]
fn sees_own(w: &mut Steps) -> TestResult {
    let [seen, not_seen] = w.answers.as_slice() else {
        return Err(TestError::new(format!("two answers: {:?}", w.answers)));
    };
    assert_eq!(seen.column(0), vec![w.iri("b")], "{}", w.over());
    assert!(not_seen.rows.is_empty(), "{not_seen:?} {}", w.over());
    Ok(())
}

#[when(
    "transaction T1 reads `<nobody> <p> ?o` through compute, finding nothing, transaction T2 commits `<nobody> <p> \"x\"`, and T1 commits a write"
)]
fn term_nobody_holds(w: &mut Steps) -> TestResult {
    let [nobody, p] = ["nobody", "p"].map(|name| w.iri(name));
    w.kind = in_shared(w, async |client| {
        let first = client.begin(TENANT, false).await?;
        nothing(&read_whole(client, &first, &Pattern::of(Some(&nobody), Some(&p))).await?)?;
        committed(client, &nobody, &p, &string("x")).await?;
        commit_a_write(client, first).await
    })?;
    Ok(())
}

#[then("T1 aborts with a conflict of kind AbsentTerm")]
fn aborts_absent(w: &mut Steps) {
    assert_eq!(w.kind, Some(KIND_ABSENT_TERM), "{}", w.over());
}

#[when(
    "transaction T1 reads `<a> <knows> ?o` through compute, finding nothing, transaction T2 commits `<a> <knows> <b>`, and T1 commits a write"
)]
fn range_insert(w: &mut Steps) -> TestResult {
    let [a, b, c, knows, name] = ["a", "b", "c", "knows", "name"].map(|n| w.iri(n));
    w.kind = in_shared(w, async |client| {
        // Both terms exist, so T1's read records a range rather than an absent term.
        committed(client, &a, &name, &string("y")).await?;
        committed(client, &c, &knows, &iri(&b)).await?;
        let first = client.begin(TENANT, false).await?;
        nothing(&read_whole(client, &first, &Pattern::of(Some(&a), Some(&knows))).await?)?;
        committed(client, &a, &knows, &iri(&b)).await?;
        commit_a_write(client, first).await
    })?;
    Ok(())
}

#[then("T1 aborts with a conflict of kind Range")]
fn aborts_range(w: &mut Steps) {
    assert_eq!(w.kind, Some(KIND_RANGE), "{}", w.over());
}

#[when(
    "transaction T1 reads `?s <knows> <b>` through compute, transaction T2 commits `<c> <knows> <b>` in the same partition, and T1 commits a write"
)]
fn reverse_range(w: &mut Steps) -> TestResult {
    let [a, b, c, knows] = ["a", "b", "c", "knows"].map(|n| w.iri(n));
    w.kind = in_shared(w, async |client| {
        committed(client, &a, &knows, &iri(&b)).await?;
        let first = client.begin(TENANT, false).await?;
        let pattern = Pattern::of(None, Some(&knows)).with_object(iri(&b));
        let found = read_whole(client, &first, &pattern).await?;
        if found.column(0) != [a.clone()] {
            return Err(TestError::new(format!("T1 found {found:?}")));
        }
        committed(client, &c, &knows, &iri(&b)).await?;
        commit_a_write(client, first).await
    })?;
    Ok(())
}

/// Commits `<a> <name> "x"` and `<z> <name> "x"`, then reads `?s <name> ?o` with limit 1 in a new transaction T1: T1,
/// the subject its read did not reach, and the page's resume key.
async fn first_of_two(
    client: &mut TestClient,
    subjects: [&str; 2],
    name: &str,
) -> TestResult<(Transaction, String, Vec<u8>)> {
    for subject in subjects {
        committed(client, subject, name, &string("x")).await?;
    }
    let first = client.begin(TENANT, false).await?;
    let page = read(
        client,
        &first,
        &Pattern::of(None, Some(name)),
        (1, Vec::new()),
    )
    .await?;
    let [row] = page.rows.as_slice() else {
        return Err(TestError::new(format!("one row: {page:?}")));
    };
    if page.resume.is_empty() {
        return Err(TestError::new(
            "a page stopped at its limit holds a resume key",
        ));
    }
    let returned = row.first().context("the row's subject")?;
    let unreached = subjects
        .into_iter()
        .find(|subject| subject != returned)
        .context("the subject the read did not reach")?;
    Ok((first, unreached.to_string(), page.resume))
}

#[when(
    "T1 reads with limit 1 a range holding `<a>` and `<z>`, T2 commits an insert past `<a>` that T1's read did not reach, and T1 commits without resuming"
)]
fn stopped_at_limit(w: &mut Steps) -> TestResult {
    let [a, z, name] = ["a", "z", "name"].map(|n| w.iri(n));
    w.kind = in_shared(w, async |client| {
        let (first, unreached, _resume) = first_of_two(client, [&a, &z], &name).await?;
        committed(client, &unreached, &name, &string("past")).await?;
        commit_a_write(client, first).await
    })?;
    w.committed = w.kind.is_none();
    Ok(())
}

#[then("T1 commits")]
fn t1_commits(w: &mut Steps) {
    assert!(w.committed, "conflict kind {:?} {}", w.kind, w.over());
}

#[when(
    "T1 reads with limit 1, resumes to the range's end, T2 commits an insert into the resumed part, and T1 commits a write"
)]
fn resumed_records(w: &mut Steps) -> TestResult {
    let [a, z, name] = ["a", "z", "name"].map(|n| w.iri(n));
    w.kind = in_shared(w, async |client| {
        let (first, unreached, resume) = first_of_two(client, [&a, &z], &name).await?;
        let rest = read(client, &first, &Pattern::of(None, Some(&name)), (0, resume)).await?;
        if rest.column(0) != [unreached.clone()] {
            return Err(TestError::new(format!("the rest: {rest:?}")));
        }
        committed(client, &unreached, &name, &string("past")).await?;
        commit_a_write(client, first).await
    })?;
    Ok(())
}

#[when(
    "T1 reads the properties of the reifier `r` through compute, T2 sets a property through `r` and commits, and T1 commits a write"
)]
fn record_conflict(w: &mut Steps) -> TestResult {
    let [a, b, knows, since] = ["a", "b", "knows", "since"].map(|n| w.iri(n));
    w.kind = in_shared(w, async |client| {
        let _edge = client
            .direct_write(edge_of((&a, &knows, &b), &since, "2019")?, None)
            .await?;
        // The reifier, named by a transaction of its own, so T1 reads the statement's record alone.
        let naming = client.begin(TENANT, false).await?;
        let named = read_whole(client, &naming, &Pattern::of(Some(&a), Some(&knows))).await?;
        client.abort(naming).await?;
        let reifier = named.column(1).first().cloned().context("its reifier")?;
        let first = client.begin(TENANT, false).await?;
        let properties = read_whole(client, &first, &Pattern::of(Some(&reifier), None)).await?;
        if properties.rows.len() != 1 {
            return Err(TestError::new(format!("one property: {properties:?}")));
        }
        let _set = client
            .direct_write(property_of(&reifier, &since, "2020")?, None)
            .await?;
        commit_a_write(client, first).await
    })?;
    Ok(())
}
