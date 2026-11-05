/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The steps of transactions' "What a transaction's reads record and what conflicts with them", on the recording side:
//! each transaction executes against the snapshot at the newest timestamp, records its reads, and is judged against the
//! writes of the transactions committed after its read version by the rules of that requirement, written out here as
//! the commit step's resolver is to apply them: a list read whole conflicts with any write to it, a tested statement
//! with a write adding or removing that object in that list, a scanned range with a write of a key inside it, an absent
//! term, by its ID, with a commit writing a statement of it.

use std::collections::{BTreeMap, BTreeSet};

use cucumber::{then, when};
use ultragraphdb_core_ids::SystemId;
use ultragraphdb_core_keys::{DEFAULT_GRAPH, QuadKey};
use ultragraphdb_core_terms::EncodedTerm;
use ultragraphdb_store_posting::CachedList;
use ultragraphdb_store_quads::{QuadWrite, WriteTerms, posting_object};
use ultragraphdb_store_sim::{SimListCache, SimStore};
use ultragraphdb_traits_blocks::TestBlockSource;

use ultragraphdb_store_apply::Applied;

use crate::Steps;
use crate::changeset_steps::{cs, is_held};
use crate::utils::changesets::{
    NS, TestChangesets, apply, declare, insert, int_token, objects, statement, term, uid, write,
};
use ultragraphdb_tool_bdd::{Context, TestResult};

type Write<'a> = QuadWrite<'a, SimStore, SimListCache<CachedList>, TestBlockSource>;

/// What one transaction read, as it recorded it.
#[derive(Debug, Default)]
struct Read {
    lists: BTreeSet<Vec<u8>>,
    statements: BTreeSet<(Vec<u8>, Vec<u8>)>,
    ranges: Vec<(Vec<u8>, Option<Vec<u8>>)>,
    absent: BTreeSet<SystemId>,
}

/// What one committed transaction wrote: each list key with the objects it added or removed, `None` for a key whose
/// objects are not known, and the terms its statements name, by ID.
#[derive(Debug, Default)]
struct Written {
    keys: BTreeMap<Vec<u8>, Option<BTreeSet<Vec<u8>>>>,
    named: BTreeSet<SystemId>,
}

/// Whether a transaction that read `read` conflicts with a commit that wrote `written` after its read version.
fn conflicts(read: &Read, written: &Written) -> bool {
    let list = read.lists.iter().any(|k| written.keys.contains_key(k));
    let statement = read
        .statements
        .iter()
        .any(|(k, o)| match written.keys.get(k) {
            Some(Some(objects)) => objects.contains(o),
            Some(None) => true,
            None => false,
        });
    let range = read.ranges.iter().any(|(start, end)| {
        written
            .keys
            .keys()
            .any(|k| k >= start && end.as_ref().is_none_or(|e| k < e))
    });
    let absent = read.absent.iter().any(|id| written.named.contains(id));
    list || statement || range || absent
}

fn read_of(write: &mut Write<'_>) -> Read {
    let request = write.request();
    let mut read = Read {
        lists: request.read_keys().map(<[u8]>::to_vec).collect(),
        statements: request
            .tested()
            .map(|(k, o)| (k.to_vec(), o.to_vec()))
            .collect(),
        ranges: request
            .ranges()
            .map(|(s, e)| (s.to_vec(), e.map(<[u8]>::to_vec)))
            .collect(),
        absent: BTreeSet::new(),
    };
    let (terms, _) = write.terms();
    read.absent = terms.absent().collect();
    read
}

fn written_of(write: &mut Write<'_>) -> TestResult<Written> {
    let deltas = write.request().deltas();
    let keys = deltas
        .iter()
        .map(|(key, delta)| {
            let objects = delta
                .postings()
                .iter()
                .map(|p| {
                    Ok(posting_object(key, p)
                        .context("a posting names an object")?
                        .as_bytes()
                        .to_vec())
                })
                .collect::<TestResult<_>>()?;
            Ok((key.clone(), Some(objects)))
        })
        .collect::<TestResult<_>>()?;
    // The subjects of the forward keys written and the ID objects of their postings: the terms a statement names.
    let mut named = BTreeSet::new();
    for (key, delta) in &deltas {
        if let Ok((QuadKey::Forward { subject, .. }, _)) = QuadKey::parse(key) {
            let _subject: bool = named.insert(subject);
            for p in delta.postings() {
                if let Some(id) = posting_object(key, p)
                    .context("a posting names an object")?
                    .as_id()
                {
                    let _object: bool = named.insert(id);
                }
            }
        }
    }
    Ok(Written { keys, named })
}

/// A transaction reading at `ts` that inserts the statement `text` when its test finds it not held.
fn inserting<'a>(a: &'a TestChangesets, ts: u64, text: &str) -> TestResult<Write<'a>> {
    let mut w = QuadWrite::new(a.quads(), NS, a.quads().lists().request(ts));
    insert_into(&mut w, text)?;
    Ok(w)
}

/// Inserts `text` through `w`, unless its test finds it held.
fn insert_into(w: &mut Write<'_>, text: &str) -> TestResult {
    let t = statement(text)?;
    let (terms, quads): (&mut WriteTerms, _) = w.terms();
    let subject = terms
        .node(quads, t.subject.as_ref())
        .context("the subject resolves")?;
    let predicate = terms
        .predicate(quads, t.predicate.as_ref())
        .context("the predicate resolves")?;
    let object = terms
        .term(quads, t.object.as_ref())
        .context("the object resolves")?;
    let kind = match &t.object {
        oxrdf::Term::Literal(literal) => literal.datatype().as_str().to_string(),
        oxrdf::Term::NamedNode(_) | oxrdf::Term::BlankNode(_) | oxrdf::Term::Triple(_) => {
            "iri".to_string()
        }
    };
    if w.holds(DEFAULT_GRAPH, subject, predicate, &object)
        .context("the test reads")?
        .is_none()
    {
        w.insert(DEFAULT_GRAPH, subject, predicate, &object, &[], &kind);
    }
    Ok(())
}

/// Commits `w` on its own.
fn commit(a: &TestChangesets, w: Write<'_>) -> TestResult {
    let (request, terms) = w.finish();
    let mut batch = a.quads().lists().batch();
    batch.push(request);
    let _committed_at: Vec<u64> = a
        .quads()
        .commit(batch, &[terms])
        .context("the batch is written")?;
    Ok(())
}

/// Runs `writes`, all read at one timestamp, in order: each commits unless it conflicts with a transaction committed
/// before it. Whether each committed.
fn decide(a: &TestChangesets, writes: Vec<Write<'_>>) -> TestResult<Vec<bool>> {
    let mut committed: Vec<Written> = Vec::new();
    let mut outcomes = Vec::new();
    for mut w in writes {
        let read = read_of(&mut w);
        if committed.iter().any(|written| conflicts(&read, written)) {
            outcomes.push(false);
            continue;
        }
        committed.push(written_of(&mut w)?);
        commit(a, w)?;
        outcomes.push(true);
    }
    Ok(outcomes)
}

#[when(regex = r"^two transactions concurrently insert `([^`]+)` and `([^`]+)`$")]
fn two_inserts(w: &mut Steps, args: &[String]) -> TestResult {
    let first = &args[0];
    let second = &args[1];
    let a = cs(w)?;
    write(&a, &["<a> <knows> <z>", "<b> <p> <c>"])?;
    let ts = a.quads().lists().applied_ts();
    let writes = vec![inserting(&a, ts, first)?, inserting(&a, ts, second)?];
    w.flags = decide(&a, writes)?;
    Ok(())
}

#[when(regex = r"^two transactions concurrently insert `([^`]+)`, which is not held$")]
fn two_same_inserts(w: &mut Steps, text: String) -> TestResult {
    let a = cs(w)?;
    write(&a, &["<a> <knows> <z>", "<b> <p> <q>"])?;
    assert!(!is_held(&a, &text)?);
    let ts = a.quads().lists().applied_ts();
    let writes = vec![inserting(&a, ts, &text)?, inserting(&a, ts, &text)?];
    w.flags = decide(&a, writes)?;
    w.names = vec![text];
    Ok(())
}

#[when(
    regex = r"^a transaction scans the `int` index of `(\w+)` for values from (\d+) to (\d+) and writes `([^`]+)`, and a transaction inserting `([^`]+)` commits after its read version$"
)]
fn scan_and_insert(w: &mut Steps, args: &[String]) -> TestResult {
    let predicate = &args[0];
    let low: i64 = args[1].parse().context("the step names a number")?;
    let high: i64 = args[2].parse().context("the step names a number")?;
    let own = &args[3];
    let other = &args[4];
    let a = cs(w)?;
    declare(&a, predicate, "with an `int` index")?;
    write(&a, &[format!("<b> <{predicate}> 20")])?;
    let ts = a.quads().lists().applied_ts();
    let predicate_uid = uid(&a, predicate)?;
    let key = |value: i64| {
        let (tokenizer, token) = int_token(value);
        QuadKey::ValueIndex {
            namespace: NS,
            graph: DEFAULT_GRAPH,
            predicate: predicate_uid,
            tokenizer,
            token,
        }
        .to_bytes()
    };
    let mut scanning = QuadWrite::new(a.quads(), NS, a.quads().lists().request(ts));
    let mut end = key(high);
    end.push(0);
    let _exhausted: Option<Vec<u8>> = scanning
        .request()
        .span(key(low), Some(end), None, |_, _| {
            Ok::<bool, ultragraphdb_store_posting::PostingError>(true)
        })
        .context("the index scans")?;
    insert_into(&mut scanning, own)?;
    let read = read_of(&mut scanning);

    let _applied: Applied = apply(&a, vec![insert(other)?]).context("the insert commits")?;
    let after = a.quads().store().contents();
    let written = Written {
        keys: after
            .keys()
            .iter()
            .filter(|(_, versions)| versions.iter().any(|v| v.version() > ts))
            .map(|(k, _)| (k.clone(), None))
            .collect(),
        named: after
            .keys()
            .iter()
            .filter(|(_, versions)| versions.iter().any(|v| v.version() > ts))
            .filter_map(|(k, _)| match QuadKey::parse(k) {
                Ok((QuadKey::Forward { subject, .. }, _)) => Some(subject),
                _ => None,
            })
            .collect(),
    };
    let commits = !conflicts(&read, &written);
    if commits {
        commit(&a, scanning)?;
    }
    w.flags = vec![commits];
    w.names = vec![own.clone()];
    Ok(())
}

#[then(regex = r"^both commit, and `(\w+)` knows `(\w+)` and `(\w+)`$")]
fn both_commit(w: &mut Steps, args: &[String]) -> TestResult {
    let subject = &args[0];
    let first = &args[1];
    let second = &args[2];
    assert_eq!(w.flags, vec![true, true]);
    let a = cs(w)?;
    let held = objects(&a, subject, "knows")?;
    for o in [first, second] {
        assert!(held.contains(&term(&format!("<{o}>"))?), "{o} in {held:?}");
    }
    Ok(())
}

#[then("one commits and the other aborts with a conflict, and its retry finds the statement held")]
fn one_aborts(w: &mut Steps) -> TestResult {
    assert_eq!(w.flags, vec![true, false]);
    let a = cs(w)?;
    let text = w.names[0].clone();
    let t = statement(&text)?;
    let ts = a.quads().lists().applied_ts();
    let mut retry = QuadWrite::new(a.quads(), NS, a.quads().lists().request(ts));
    let (terms, quads) = retry.terms();
    let subject = terms.node(quads, t.subject.as_ref())?;
    let predicate = terms.predicate(quads, t.predicate.as_ref())?;
    let object: EncodedTerm = terms.term(quads, t.object.as_ref())?;
    assert!(
        retry
            .holds(DEFAULT_GRAPH, subject, predicate, &object)?
            .is_some()
    );
    Ok(())
}

#[then("the scanning transaction aborts with a conflict")]
fn scanning_aborts(w: &mut Steps) -> TestResult {
    assert_eq!(w.flags, vec![false]);
    let a = cs(w)?;
    assert!(!is_held(&a, &w.names[0])?);
    Ok(())
}

#[then("the scanning transaction commits")]
fn scanning_commits(w: &mut Steps) -> TestResult {
    assert_eq!(w.flags, vec![true]);
    let a = cs(w)?;
    assert!(is_held(&a, &w.names[0])?);
    Ok(())
}
