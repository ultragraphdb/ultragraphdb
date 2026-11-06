/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! Resume keys as a client holds them: opaque bytes it hands back, across transactions, connections and a tampering hand.

use cucumber::{then, when};
use ultragraphdb_protocol_wire::{Begin, ErrorCode, RequestTermView, ResultView};
use ultragraphdb_tool_bdd::{Context, TestError, TestResult};

use crate::Steps;
use crate::client_reads::{
    Answer, Pattern, TENANT, batch_of, drain, edge_of, iri, read, read_whole, refusal_of,
};
use crate::cluster::{client, shared_client};
use crate::read_steps::{big_pattern, holding_big, until_rows};
use crate::shells::{TestClient, run};
use crate::transaction_steps::{committed, in_shared};

/// Commits `count` objects of `<subject> <predicate>`, then waits until a read-only transaction's snapshot holds them: the
/// objects written.
async fn objects(
    client: &mut TestClient,
    (subject, predicate): (&str, &str),
    count: u32,
) -> TestResult<Vec<String>> {
    let mut written = Vec::new();
    for at in 0..count {
        let object = format!("{subject}/object{at}");
        committed(client, subject, predicate, &iri(&object)).await?;
        written.push(object);
    }
    let _closed = until_rows(
        client,
        &Pattern::of(Some(subject), Some(predicate)),
        count as usize,
    )
    .await?;
    Ok(written)
}

#[when("a read with limit 2 matches 5 rows and is resumed twice")]
fn resumed_twice(w: &mut Steps) -> TestResult {
    let [a, p] = ["a", "p"].map(|name| w.iri(name));
    w.seen.answers = in_shared(w, async |client| {
        let _written = objects(client, (&a, &p), 5).await?;
        let pattern = Pattern::of(Some(&a), Some(&p));
        let own = client.begin(TENANT, true).await?;
        let first = read(client, &own, &pattern, (2, Vec::new())).await?;
        let second = read(client, &own, &pattern, (2, first.resume.clone())).await?;
        let third = read(client, &own, &pattern, (2, second.resume.clone())).await?;
        client.abort(own).await?;
        Ok(vec![first, second, third])
    })?;
    Ok(())
}

#[then("the three answers return rows 1–2, 3–4 and 5, the last with no resume key")]
fn three_pages(w: &mut Steps) -> TestResult {
    let [first, second, third] = w.seen.answers.as_slice() else {
        return Err(TestError::new("three answers"));
    };
    assert_eq!(first.rows.len(), 2, "{first:?}");
    assert_eq!(second.rows.len(), 2, "{second:?}");
    assert_eq!(third.rows.len(), 1, "{third:?}");
    assert!(third.resume.is_empty(), "{third:?}");
    let mut every: Vec<String> = [first, second, third]
        .iter()
        .flat_map(|page| page.column(0))
        .collect();
    every.dedup();
    every.sort();
    every.dedup();
    assert_eq!(every.len(), 5, "no row repeated: {every:?} {}", w.over());
    Ok(())
}

#[when("a read with limit 1 and its graph unbound matches one row and is resumed")]
fn resumed_to_nothing(w: &mut Steps) -> TestResult {
    let [a, p, b] = ["a", "p", "b"].map(|name| w.iri(name));
    w.seen.answers = in_shared(w, async |client| {
        committed(client, &a, &p, &iri(&b)).await?;
        let pattern = Pattern::of(Some(&a), Some(&p)).in_graph(crate::client_reads::GraphAt::Any);
        let _closed = until_rows(client, &pattern, 1).await?;
        let own = client.begin(TENANT, true).await?;
        let first = read(client, &own, &pattern, (1, Vec::new())).await?;
        let second = read(client, &own, &pattern, (1, first.resume.clone())).await?;
        client.abort(own).await?;
        Ok(vec![first, second])
    })?;
    Ok(())
}

#[then(
    "the first answer holds the row and a resume key, and the second answers one batch flagged `LAST` with no row and no key"
)]
fn found_nothing_resumed(w: &mut Steps) -> TestResult {
    let [first, second] = w.seen.answers.as_slice() else {
        return Err(TestError::new("two answers"));
    };
    assert_eq!(first.rows.len(), 1, "{first:?}");
    assert!(!first.resume.is_empty(), "{first:?}");
    assert_eq!(second, &Answer::default(), "{}", w.over());
    Ok(())
}

/// Reads with limit 1 in a read-only transaction, then the read `resumed` asks with its key: the refusal it takes.
async fn key_used_elsewhere(
    client: &mut TestClient,
    pattern: &Pattern,
    other: &Pattern,
) -> TestResult<ultragraphdb_client_compio::ClientError> {
    let own = client.begin(TENANT, true).await?;
    let first = read(client, &own, pattern, (1, Vec::new())).await?;
    let stream = client
        .read(&own, &other.read(&own, 0, first.resume.clone())?)
        .await?;
    let refused = client
        .next_batch(&stream, batch_of)
        .await
        .err()
        .context("a read with another pattern's key refused")?;
    client.abort(own).await?;
    Ok(refused)
}

#[when("a resume key from one read is used with a different pattern")]
fn key_of_another_pattern(w: &mut Steps) -> TestResult {
    let [a, p, q] = ["a", "p", "q"].map(|name| w.iri(name));
    let refused = in_shared(w, async |client| {
        let _written = objects(client, (&a, &p), 2).await?;
        key_used_elsewhere(
            client,
            &Pattern::of(Some(&a), Some(&p)),
            &Pattern::of(Some(&a), Some(&q)),
        )
        .await
    })?;
    w.seen.refusals = vec![refused];
    Ok(())
}

/// That the one refusal seen names the resume key.
fn names_the_key(w: &Steps) -> TestResult {
    let refused = w.seen.refusals.first().context("the refusal")?;
    let (_, _, message) = refusal_of(refused)?;
    assert!(message.contains("resume key"), "{message} {}", w.over());
    Ok(())
}

#[then("the read is refused with an error naming the resume key")]
fn refused_naming_key(w: &mut Steps) -> TestResult {
    names_the_key(w)
}

#[when(
    "a resume key from one read-write transaction's read is used with the same pattern in another read-write transaction"
)]
fn key_of_another_transaction(w: &mut Steps) -> TestResult {
    let [a, p] = ["a", "p"].map(|name| w.iri(name));
    let refused = in_shared(w, async |client| {
        let _written = objects(client, (&a, &p), 2).await?;
        let pattern = Pattern::of(Some(&a), Some(&p));
        let first = client.begin(TENANT, false).await?;
        let page = read(client, &first, &pattern, (1, Vec::new())).await?;
        client.abort(first).await?;
        let second = client.begin(TENANT, false).await?;
        let stream = client
            .read(&second, &pattern.read(&second, 0, page.resume)?)
            .await?;
        let refused = client
            .next_batch(&stream, batch_of)
            .await
            .err()
            .context("another transaction's key refused")?;
        Ok(refused)
    })?;
    w.seen.refusals = vec![refused];
    Ok(())
}

#[then("the read is refused with Error 1, the message naming the resume key")]
fn refused_one_naming_key(w: &mut Steps) -> TestResult {
    let refused = w.seen.refusals.first().context("the refusal")?;
    let (code, _, _) = refusal_of(refused)?;
    assert_eq!(code, ErrorCode::Protocol, "{refused:?}");
    names_the_key(w)
}

#[when("a read matches nothing")]
fn empty_result(w: &mut Steps) -> TestResult {
    let [a, p] = ["a", "p"].map(|name| w.iri(name));
    w.seen.answers = in_shared(w, async |client| {
        let own = client.begin(TENANT, true).await?;
        let answer = read_whole(client, &own, &Pattern::of(Some(&a), Some(&p))).await?;
        client.abort(own).await?;
        Ok(vec![answer])
    })?;
    Ok(())
}

#[then("it returns no rows and no resume key")]
fn no_rows_no_key(w: &mut Steps) -> TestResult {
    let answer = w.seen.answers.first().context("the answer")?;
    assert_eq!(answer, &Answer::default(), "{}", w.over());
    Ok(())
}

#[when(
    "a read-only transaction at snapshot level reads with limit 2, its connection is lost, and a new Begin names the version the first Begin answered and resumes with the key"
)]
fn named_version_after_reconnect(w: &mut Steps) -> TestResult {
    let [a, p] = ["a", "p"].map(|name| w.iri(name));
    let at = shared_client(w.carrier)?;
    let (first, rest, written) = run(async {
        let mut one = client(&at).await?;
        let written = objects(&mut one, (&a, &p), 5).await?;
        let pattern = Pattern::of(Some(&a), Some(&p));
        let reader = one.begin(TENANT, true).await?;
        let first = read(&mut one, &reader, &pattern, (2, Vec::new())).await?;
        committed(&mut one, &a, &p, &iri(&format!("{a}/later"))).await?;
        one.close().await;
        let mut two = client(&at).await?;
        let begin = Begin::new(TENANT, reader.read_version(), true);
        let again = two.begin_with(begin, None).await?;
        let rest = read(&mut two, &again, &pattern, (0, first.resume.clone())).await?;
        Ok((first, rest, written))
    })?;
    w.seen.answers = vec![first, rest];
    w.seen.facts = written;
    Ok(())
}

#[then("the read returns rows 3 onward at that version, neither repeating nor skipping a row")]
fn rest_at_the_version(w: &mut Steps) -> TestResult {
    let [first, rest] = w.seen.answers.as_slice() else {
        return Err(TestError::new("two answers"));
    };
    let mut every = [first.column(0), rest.column(0)].concat();
    assert_eq!(every.len(), w.seen.facts.len(), "{every:?} {}", w.over());
    every.sort();
    let mut written = w.seen.facts.clone();
    written.sort();
    assert_eq!(every, written, "{}", w.over());
    Ok(())
}

#[when("`<a> <knows> <b>` is held under three statement IDs, and a read with limit 2 is resumed")]
fn inside_statements(w: &mut Steps) -> TestResult {
    let [a, b, knows, since] = ["a", "b", "knows", "since"].map(|name| w.iri(name));
    w.seen.answers = in_shared(w, async |client| {
        for year in ["2018", "2019", "2020"] {
            let _edge = client
                .direct_write(edge_of((&a, &knows, &b), &since, year)?, None)
                .await?;
        }
        let pattern = Pattern::of(Some(&a), Some(&knows));
        let _closed = until_rows(client, &pattern, 3).await?;
        let own = client.begin(TENANT, true).await?;
        let first = read(client, &own, &pattern, (2, Vec::new())).await?;
        let second = read(client, &own, &pattern, (2, first.resume.clone())).await?;
        client.abort(own).await?;
        Ok(vec![first, second])
    })?;
    Ok(())
}

#[then(
    "the first answer returns the first two statements by ascending ID, the second the third, and the partition reads none of the list's prefix"
)]
fn statements_resumed(w: &mut Steps) -> TestResult {
    let [first, second] = w.seen.answers.as_slice() else {
        return Err(TestError::new("two answers"));
    };
    assert_eq!(first.rows.len(), 2, "{first:?}");
    assert_eq!(second.rows.len(), 1, "{second:?}");
    let mut reifiers = [first.column(1), second.column(1)].concat();
    reifiers.sort();
    reifiers.dedup();
    assert_eq!(reifiers.len(), 3, "{reifiers:?} {}", w.over());
    Ok(())
}

#[when(
    "`<a> <knows> <b>` and `<a> <knows> <c>` are held with no statement ID, and a read with limit 1 is resumed"
)]
fn after_whole_object(w: &mut Steps) -> TestResult {
    let [a, knows] = ["a", "knows"].map(|name| w.iri(name));
    let (pages, written) = in_shared(w, async |client| {
        let written = objects(client, (&a, &knows), 2).await?;
        let pattern = Pattern::of(Some(&a), Some(&knows));
        let own = client.begin(TENANT, true).await?;
        let first = read(client, &own, &pattern, (1, Vec::new())).await?;
        let second = read(client, &own, &pattern, (1, first.resume.clone())).await?;
        client.abort(own).await?;
        Ok((vec![first, second], written))
    })?;
    w.seen.answers = pages;
    w.seen.facts = written;
    Ok(())
}

#[then(
    "the resumed Scan's `after` names `<b>`, compute drops `<b>` from what it answers, and the read returns `<c>`"
)]
fn object_skipped_whole(w: &mut Steps) -> TestResult {
    let [first, second] = w.seen.answers.as_slice() else {
        return Err(TestError::new("two answers"));
    };
    let mut every = [first.column(0), second.column(0)].concat();
    every.sort();
    let mut written = w.seen.facts.clone();
    written.sort();
    assert_eq!(every, written, "{}", w.over());
    Ok(())
}

#[when(
    "a statement's record holds three properties, and a read of its reifier with limit 2 is resumed"
)]
fn inside_record(w: &mut Steps) -> TestResult {
    let [a, b, knows, p1, p2, p3] = ["a", "b", "knows", "p1", "p2", "p3"].map(|n| w.iri(n));
    w.seen.answers = in_shared(w, async |client| {
        let _edge = client
            .direct_write(edge_of((&a, &knows, &b), &p1, "one")?, None)
            .await?;
        let named = until_rows(client, &Pattern::of(Some(&a), Some(&knows)), 1).await?;
        let reifier = named.column(1).first().cloned().context("its reifier")?;
        for (key, value) in [(&p2, "two"), (&p3, "three")] {
            let set = crate::client_reads::property_of(&reifier, key, value)?;
            let _set = client.direct_write(set, None).await?;
        }
        let pattern = Pattern::of(Some(&reifier), None);
        let _closed = until_rows(client, &pattern, 3).await?;
        let own = client.begin(TENANT, true).await?;
        let first = read(client, &own, &pattern, (2, Vec::new())).await?;
        let second = read(client, &own, &pattern, (2, first.resume.clone())).await?;
        client.abort(own).await?;
        Ok(vec![first, second])
    })?;
    Ok(())
}

#[then("the second answer returns the third property alone")]
fn third_property(w: &mut Steps) -> TestResult {
    let [first, second] = w.seen.answers.as_slice() else {
        return Err(TestError::new("two answers"));
    };
    assert_eq!(first.rows.len(), 2, "{first:?}");
    assert_eq!(second.rows.len(), 1, "{second:?}");
    let mut keys = [first.column(0), second.column(0)].concat();
    keys.sort();
    let mut expected = [w.iri("p1"), w.iri("p2"), w.iri("p3")];
    expected.sort();
    assert_eq!(keys, expected, "{}", w.over());
    Ok(())
}

/// How a batch looks for the key's rule: its rows, and whether it held a resume key.
fn rows_and_key(batch: &ResultView<'_>) -> (u32, bool) {
    (batch.rows(), !batch.resume().is_empty())
}

#[when("a read with limit 10,000 stops at its limit after several batches")]
fn limit_after_batches(w: &mut Steps) -> TestResult {
    let batches = in_shared(w, async |client| {
        holding_big(client).await?;
        let own = client.begin(TENANT, true).await?;
        let pattern = big_pattern();
        let _closed = until_rows(client, &pattern, 14).await?;
        let stream = client
            .read(&own, &pattern.read(&own, 13, Vec::new())?)
            .await?;
        let mut batches = Vec::new();
        while let Some(batch) = client.next_batch(&stream, rows_and_key).await? {
            batches.push(batch);
        }
        client.abort(own).await?;
        Ok(batches)
    })?;
    w.seen.facts = batches.iter().map(|batch| format!("{batch:?}")).collect();
    let (last, earlier) = batches.split_last().context("a batch")?;
    w.committed = last.1 && earlier.iter().all(|(_, key)| !key) && !earlier.is_empty();
    Ok(())
}

#[then("section 6 of every batch but the last is empty, and the last holds the key")]
fn only_the_last_key(w: &mut Steps) {
    assert!(w.committed, "{:?} {}", w.seen.facts, w.over());
}

#[when("a client changes one byte of a resume key and continues its read with it")]
fn tampered_key(w: &mut Steps) -> TestResult {
    let [a, p] = ["a", "p"].map(|name| w.iri(name));
    let refused = in_shared(w, async |client| {
        let _written = objects(client, (&a, &p), 2).await?;
        let pattern = Pattern::of(Some(&a), Some(&p));
        let own = client.begin(TENANT, true).await?;
        let page = read(client, &own, &pattern, (1, Vec::new())).await?;
        let mut tampered = page.resume.clone();
        let middle = tampered.len() / 2;
        *tampered.get_mut(middle).context("a byte of the key")? ^= 1;
        let stream = client.read(&own, &pattern.read(&own, 0, tampered)?).await?;
        let answer = drain(client, &stream).await;
        answer.err().context("a tampered key refused")
    })?;
    w.seen.facts = vec![refused.to_string()];
    Ok(())
}

#[then("the read is refused with an error naming the resume key as malformed")]
fn tampered_refused(w: &mut Steps) -> TestResult {
    let refused = w.seen.facts.first().context("the refusal")?;
    assert!(refused.contains("resume key"), "{refused} {}", w.over());
    Ok(())
}

#[when(
    "a read stops at its limit after the row whose object is `http://ex.org/b`, and the server decodes the resume key it returned"
)]
fn key_of_terms(w: &mut Steps) -> TestResult {
    let [a, p] = ["a", "p"].map(|name| w.iri(name));
    let (page, objects_written) = in_shared(w, async |client| {
        let written = objects(client, (&a, &p), 2).await?;
        let own = client.begin(TENANT, true).await?;
        let page = read(
            client,
            &own,
            &Pattern::of(Some(&a), Some(&p)),
            (1, Vec::new()),
        )
        .await?;
        client.abort(own).await?;
        Ok((page, written))
    })?;
    let key = ultragraphdb_protocol_wire::ResumeKeyView::read(&page.resume)?;
    let held = (0..key.terms().len() as u32)
        .filter_map(|index| key.terms().get(index))
        .filter_map(|term| match term {
            RequestTermView::Iri(text) => Some(text.to_string()),
            RequestTermView::BlankNode(_)
            | RequestTermView::Literal { .. }
            | RequestTermView::LanguageLiteral { .. }
            | RequestTermView::Triple { .. } => None,
        })
        .collect::<Vec<_>>();
    w.seen.facts = vec![
        key.pattern().to_string(),
        held.join(" "),
        page.column(0).concat(),
    ];
    let _written = objects_written;
    Ok(())
}

#[then(
    "the key holds the pattern's hash and the text `http://ex.org/b`, and no store key or system ID"
)]
fn key_holds_terms(w: &mut Steps) -> TestResult {
    let [hash, text, object] = w.seen.facts.as_slice() else {
        return Err(TestError::new("what the key holds"));
    };
    assert_ne!(hash, "0");
    assert!(text.contains(object.as_str()), "{text} {object}");
    Ok(())
}
