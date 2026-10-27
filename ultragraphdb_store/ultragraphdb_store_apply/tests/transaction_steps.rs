/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The steps of transactions' "What a transaction's reads record and what conflicts with them", on the recording side:
//! each transaction executes against the snapshot at the newest timestamp, records its reads, and is judged against the
//! writes of the transactions committed after its read version by the rules of that requirement, written out here as
//! the commit step's resolver is to apply them: a list read whole conflicts with any write to it, a tested statement
//! with a write adding or removing that object in that list, a scanned range with a write of a key inside it, an absent
//! dictionary key with a commit creating it.

use std::collections::{BTreeMap, BTreeSet};

use cucumber::{then, when};
use ultragraphdb_core_keys::{DEFAULT_GRAPH, QuadKey};
use ultragraphdb_core_terms::EncodedTerm;
use ultragraphdb_store_posting::CachedList;
use ultragraphdb_store_quads::{QuadWrite, WriteTerms, posting_object};
use ultragraphdb_store_sim::{SimListCache, SimStore};

use crate::Steps;
use crate::changeset_steps::{cs, is_held};
use crate::utils::changesets::{
    NS, TestChangesets, apply, declare, insert, int_token, objects, statement, term, uid, write,
};

type Write<'a> = QuadWrite<'a, SimStore, SimListCache<CachedList>>;

/// What one transaction read, as it recorded it.
#[derive(Debug, Default)]
struct Read {
    lists: BTreeSet<Vec<u8>>,
    statements: BTreeSet<(Vec<u8>, Vec<u8>)>,
    ranges: Vec<(Vec<u8>, Option<Vec<u8>>)>,
    absent: BTreeSet<Vec<u8>>,
}

/// What one committed transaction wrote: each list key with the objects it added or removed, `None` for a key whose
/// objects are not known, and the dictionary keys it created.
#[derive(Debug, Default)]
struct Written {
    keys: BTreeMap<Vec<u8>, Option<BTreeSet<Vec<u8>>>>,
    dictionary: BTreeSet<Vec<u8>>,
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
    let absent = read.absent.iter().any(|k| written.dictionary.contains(k));
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
    read.absent = terms.absent().map(<[u8]>::to_vec).collect();
    read
}

fn written_of(write: &mut Write<'_>) -> Written {
    let deltas = write.request().deltas();
    let keys = deltas
        .iter()
        .map(|(key, delta)| {
            let objects = delta
                .postings()
                .iter()
                .map(|p| {
                    posting_object(key, p)
                        .expect("a posting names an object")
                        .as_bytes()
                        .to_vec()
                })
                .collect();
            (key.clone(), Some(objects))
        })
        .collect();
    let (terms, _) = write.terms();
    let dictionary = terms.entries().into_iter().map(|(k, _)| k).collect();
    Written { keys, dictionary }
}

/// A transaction reading at `ts` that inserts the statement `text` when its test finds it not held.
fn inserting<'a>(a: &'a TestChangesets, ts: u64, text: &str) -> Write<'a> {
    let mut w = QuadWrite::new(a.quads(), NS, a.quads().lists().request(ts));
    insert_into(&mut w, text);
    w
}

/// Inserts `text` through `w`, unless its test finds it held.
fn insert_into(w: &mut Write<'_>, text: &str) {
    let t = statement(text);
    let (terms, quads): (&mut WriteTerms, _) = w.terms();
    let (s, l) = (quads.store(), quads.leases());
    let subject = terms
        .node(s, l, t.subject.as_ref())
        .expect("the subject resolves");
    let predicate = terms
        .predicate(s, l, t.predicate.as_ref())
        .expect("the predicate resolves");
    let object = terms
        .term(s, l, t.object.as_ref())
        .expect("the object resolves");
    let kind = match &t.object {
        oxrdf::Term::Literal(literal) => literal.datatype().as_str().to_string(),
        _ => "iri".to_string(),
    };
    if w.holds(DEFAULT_GRAPH, subject, predicate, &object)
        .expect("the test reads")
        .is_none()
    {
        w.insert(DEFAULT_GRAPH, subject, predicate, &object, &[], &kind);
    }
}

/// Commits `w` on its own.
fn commit(a: &TestChangesets, w: Write<'_>) {
    let (request, terms) = w.finish();
    let mut batch = a.quads().lists().batch();
    batch.push(request);
    a.quads()
        .commit(batch, &[terms])
        .expect("the batch is written")
        .remove(0)
        .expect("the transaction commits");
}

/// Runs `writes`, all read at one timestamp, in order: each commits unless it conflicts with a transaction committed
/// before it. Whether each committed.
fn decide(a: &TestChangesets, writes: Vec<Write<'_>>) -> Vec<bool> {
    let mut committed: Vec<Written> = Vec::new();
    let mut outcomes = Vec::new();
    for mut w in writes {
        let read = read_of(&mut w);
        if committed.iter().any(|written| conflicts(&read, written)) {
            outcomes.push(false);
            continue;
        }
        committed.push(written_of(&mut w));
        commit(a, w);
        outcomes.push(true);
    }
    outcomes
}

#[when(regex = r"^two transactions concurrently insert `([^`]+)` and `([^`]+)`$")]
fn two_inserts(w: &mut Steps, first: String, second: String) {
    let a = cs(w);
    write(&a, &["<a> <knows> <z>", "<b> <p> <c>"]);
    let ts = a.quads().lists().applied_ts();
    let writes = vec![inserting(&a, ts, &first), inserting(&a, ts, &second)];
    w.flags = decide(&a, writes);
}

#[when(regex = r"^two transactions concurrently insert `([^`]+)`, which is not held$")]
fn two_same_inserts(w: &mut Steps, text: String) {
    let a = cs(w);
    write(&a, &["<a> <knows> <z>", "<b> <p> <q>"]);
    assert!(!is_held(&a, &text));
    let ts = a.quads().lists().applied_ts();
    let writes = vec![inserting(&a, ts, &text), inserting(&a, ts, &text)];
    w.flags = decide(&a, writes);
    w.names = vec![text];
}

#[when(
    regex = r"^a transaction scans the `int` index of `(\w+)` for values from (\d+) to (\d+) and writes `([^`]+)`, and a transaction inserting `([^`]+)` commits after its read version$"
)]
fn scan_and_insert(
    w: &mut Steps,
    predicate: String,
    low: i64,
    high: i64,
    own: String,
    other: String,
) {
    let a = cs(w);
    declare(&a, &predicate, "with an `int` index");
    write(&a, &[format!("<b> <{predicate}> 20")]);
    let ts = a.quads().lists().applied_ts();
    let key = |value: i64| {
        let (tokenizer, token) = int_token(value);
        QuadKey::ValueIndex {
            namespace: NS,
            graph: DEFAULT_GRAPH,
            predicate: uid(&a, &predicate),
            tokenizer,
            token,
        }
        .to_bytes()
    };
    let mut scanning = QuadWrite::new(a.quads(), NS, a.quads().lists().request(ts));
    let mut end = key(high);
    end.push(0);
    scanning
        .request()
        .scan(key(low), Some(end))
        .expect("the index scans");
    insert_into(&mut scanning, &own);
    let read = read_of(&mut scanning);

    let before = a.quads().store().contents();
    apply(&a, vec![insert(&other)]).expect("the insert commits");
    let after = a.quads().store().contents();
    let written = Written {
        keys: after
            .keys()
            .iter()
            .filter(|(_, versions)| versions.iter().any(|v| v.version() > ts))
            .map(|(k, _)| (k.clone(), None))
            .collect(),
        dictionary: after
            .versionless()
            .keys()
            .filter(|k| !before.versionless().contains_key(*k))
            .cloned()
            .collect(),
    };
    let commits = !conflicts(&read, &written);
    if commits {
        commit(&a, scanning);
    }
    w.flags = vec![commits];
    w.names = vec![own];
}

#[then(regex = r"^both commit, and `(\w+)` knows `(\w+)` and `(\w+)`$")]
fn both_commit(w: &mut Steps, subject: String, first: String, second: String) {
    assert_eq!(w.flags, vec![true, true]);
    let a = cs(w);
    let held = objects(&a, &subject, "knows");
    for o in [first, second] {
        assert!(held.contains(&term(&format!("<{o}>"))), "{o} in {held:?}");
    }
}

#[then("one commits and the other aborts with a conflict, and its retry finds the statement held")]
fn one_aborts(w: &mut Steps) {
    assert_eq!(w.flags, vec![true, false]);
    let a = cs(w);
    let text = w.names[0].clone();
    let t = statement(&text);
    let ts = a.quads().lists().applied_ts();
    let mut retry = QuadWrite::new(a.quads(), NS, a.quads().lists().request(ts));
    let (terms, quads) = retry.terms();
    let (s, l) = (quads.store(), quads.leases());
    let subject = terms.node(s, l, t.subject.as_ref()).unwrap();
    let predicate = terms.predicate(s, l, t.predicate.as_ref()).unwrap();
    let object: EncodedTerm = terms.term(s, l, t.object.as_ref()).unwrap();
    assert!(
        retry
            .holds(DEFAULT_GRAPH, subject, predicate, &object)
            .unwrap()
            .is_some()
    );
}

#[then("the scanning transaction aborts with a conflict")]
fn scanning_aborts(w: &mut Steps) {
    assert_eq!(w.flags, vec![false]);
    let a = cs(w);
    assert!(!is_held(&a, &w.names[0]));
}

#[then("the scanning transaction commits")]
fn scanning_commits(w: &mut Steps) {
    assert_eq!(w.flags, vec![true]);
    let a = cs(w);
    assert!(is_held(&a, &w.names[0]));
}
