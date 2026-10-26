/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The steps of the scenarios run over a changeset applier: changesets, index maintenance, posting lists, the
//! catalog's write checks, schema storage's writes, statement IDs and vector literals on write. Statements are written
//! as text, `<a> <name> "big apple"`, its IRIs under `http://ex.org/`.

use std::collections::BTreeSet;
use std::sync::Arc;

use cucumber::{given, then, when};
use oxrdf::{GraphName, Literal, Term};
use ultragraphdb_core_keys::{CountDirection, DEFAULT_GRAPH, Keyspace};
use ultragraphdb_core_mapping::{Change, PredicateDeclaration, StatementId, StatementRef};
use ultragraphdb_core_terms::EncodedTerm;
use ultragraphdb_store_apply::{Applied, ChangesetApplier, ChangesetError};
use ultragraphdb_store_posting::{Lists, ListsConfig};
use ultragraphdb_store_quads::QuadStore;
use ultragraphdb_store_sim::{Seed, SimListCache, Stream};

use crate::Steps;
use crate::utils::changesets::{
    NS, TestChangesets, apply, changeset, changesets, changesets_over, counts_of, delete,
    delete_pattern, ex, index_keys_holding, indexed, insert, insert_edge, objects, objects_in,
    postings, reverse, sim_store, statement, term, tokens_of, uid,
};

/// The changeset applier of the scenario, created on first use.
fn cs(w: &mut Steps) -> Arc<TestChangesets> {
    w.changesets
        .get_or_insert_with(|| Arc::new(changesets()))
        .clone()
}

/// The backticked items of a step's text, in order.
fn quoted(text: &str) -> Vec<String> {
    text.split('`')
        .skip(1)
        .step_by(2)
        .map(str::to_string)
        .collect()
}

/// The last changeset's result.
fn last(w: &Steps) -> &Result<Applied, ChangesetError> {
    w.results.last().expect("a changeset was applied")
}

fn applied(w: &Steps) -> &Applied {
    last(w).as_ref().expect("the changeset applied")
}

/// Whether the statement `text` names is held.
fn is_held(a: &TestChangesets, text: &str) -> bool {
    let t = statement(text);
    let graph = match &t.graph {
        GraphName::NamedNode(n) => n.as_str().trim_start_matches("http://ex.org/").to_string(),
        _ => String::new(),
    };
    let known = |iri: &str| {
        a.quads()
            .uid_of(NS, iri)
            .expect("the dictionary reads")
            .is_some()
    };
    let subject = t.subject.to_string();
    let subject = subject
        .trim_start_matches("<http://ex.org/")
        .trim_end_matches('>');
    let predicate = t.predicate.as_str().trim_start_matches("http://ex.org/");
    if !known(ex(subject).as_str())
        || !known(t.predicate.as_str())
        || (!graph.is_empty() && !known(ex(&graph).as_str()))
    {
        return false;
    }
    objects_in(a, &graph, subject, predicate).contains(&t.object)
}

/// The term a token item names: an integer, or a simple literal.
fn token_term(item: &str) -> Term {
    if item.parse::<i64>().is_ok() {
        term(item)
    } else {
        Literal::new_simple_literal(item).into()
    }
}

// Givens.

#[given(regex = r"^`(\w+)` is declared (.+)$")]
fn declared(w: &mut Steps, predicate: String, phrase: String) {
    let a = cs(w);
    crate::utils::changesets::declare(&a, &predicate, &phrase);
}

#[given(regex = r"^(`.+`) (?:is|are) held$")]
fn held(w: &mut Steps, statements: String) {
    let a = cs(w);
    for text in quoted(&statements) {
        apply(&a, vec![insert(&text)]).expect("the statement is written");
    }
}

#[given("every literal's fingerprint is 7")]
fn constant_fingerprint(w: &mut Steps) {
    let cache = SimListCache::new(1024, Seed::new(1).stream(Stream::Cache));
    let quads = QuadStore::open(Lists::new(sim_store(), cache, ListsConfig::default()))
        .expect("the quads open")
        .with_fingerprint(|_| 7);
    w.changesets = Some(Arc::new(ChangesetApplier::over(quads)));
}

#[given(regex = r"^`(.+)` is held with the property `(\S+) (.+)`$")]
fn held_with_property(w: &mut Steps, text: String, property: String, value: String) {
    let a = cs(w);
    let property = property.trim_start_matches('<').trim_end_matches('>');
    apply(&a, vec![insert_edge(&text, &[(property, &value)])]).expect("the statement is written");
}

// Writes.

#[when(regex = r"^(`.+`) (?:is|are) written$")]
fn written(w: &mut Steps, statements: String) {
    let a = cs(w);
    for text in quoted(&statements) {
        let result = apply(&a, vec![insert(&text)]);
        w.results.push(result);
    }
}

#[when(regex = r"^(`.+`) (?:is|are) written in one changeset$")]
fn written_together(w: &mut Steps, statements: String) {
    let a = cs(w);
    let changes = quoted(&statements).iter().map(|t| insert(t)).collect();
    let result = apply(&a, changes);
    w.results.push(result);
}

#[when(regex = r"^`(.+)` is written again$")]
fn written_again(w: &mut Steps, text: String) {
    let a = cs(w);
    let t = statement(&text);
    let (subject, object) = (t.subject.to_string(), t.object.to_string());
    let name = |s: &str| {
        s.trim_start_matches("<http://ex.org/")
            .trim_end_matches('>')
            .to_string()
    };
    w.names = vec![name(&subject), name(&object), name(t.predicate.as_str())];
    w.counts = count_snapshot(&a, &w.names);
    let result = apply(&a, vec![insert(&text)]);
    w.results.push(result);
}

/// The counts the subject and object are listed under, forward and reverse.
fn count_snapshot(a: &TestChangesets, names: &[String]) -> Vec<Vec<u32>> {
    let predicate = names[2].trim_start_matches("http://ex.org/");
    [
        (&names[0], CountDirection::Forward),
        (&names[1], CountDirection::Reverse),
    ]
    .iter()
    .flat_map(|(n, d)| {
        [
            counts_of(a, predicate, *d, uid(a, n)),
            counts_of(a, predicate, flip(*d), uid(a, n)),
        ]
    })
    .collect()
}

fn flip(d: CountDirection) -> CountDirection {
    match d {
        CountDirection::Forward => CountDirection::Reverse,
        CountDirection::Reverse => CountDirection::Forward,
    }
}

#[when(regex = r"^(`.+`) (?:is|are) deleted$")]
fn deleted(w: &mut Steps, statements: String) {
    let a = cs(w);
    for text in quoted(&statements) {
        let result = apply(&a, vec![delete(&text)]);
        w.results.push(result);
    }
}

#[when(regex = r"^the pattern `(\w+)`, `(\w+)` is deleted$")]
fn pattern_deleted(w: &mut Steps, subject: String, predicate: String) {
    let a = cs(w);
    let result = apply(&a, vec![delete_pattern(&subject, &predicate)]);
    w.results.push(result);
}

#[when(regex = r"^`(.+)` gains the property `(\S+) (.+)`$")]
fn gains_property(w: &mut Steps, text: String, property: String, value: String) {
    let a = cs(w);
    let t = statement(&text);
    let change = Change::SetProperty {
        statement: StatementRef::Triple {
            graph: t.graph,
            subject: t.subject,
            predicate: t.predicate,
            object: t.object,
        },
        predicate: ex(property.trim_start_matches('<').trim_end_matches('>')),
        value: term(&value),
    };
    let result = apply(&a, vec![change]);
    w.results.push(result);
}

#[when(regex = r"^the statement of `(.+)` is deleted by its ID$")]
fn statement_deleted(w: &mut Steps, text: String) {
    let a = cs(w);
    let t = statement(&text);
    let name = |s: String| {
        s.trim_start_matches("<http://ex.org/")
            .trim_end_matches('>')
            .to_string()
    };
    let (subject, predicate) = (name(t.subject.to_string()), name(t.predicate.to_string()));
    let ids = postings(&a, &subject, &predicate)[0].1.clone();
    assert_eq!(ids.len(), 1, "one statement ID");
    w.statement = Some(ids[0]);
    let change = Change::DeleteStatement {
        graph: t.graph,
        id: StatementId::new(ids[0]),
    };
    let result = apply(&a, vec![change]);
    w.results.push(result);
}

#[when("one changeset is applied to two copies of one store")]
fn two_copies(w: &mut Steps) {
    let copies = [changesets_over(sim_store()), changesets_over(sim_store())];
    for a in &copies {
        crate::utils::changesets::declare(a, "name", "single-valued with a `term` index");
        apply(
            a,
            vec![
                insert(r#"<a> <name> "big apple""#),
                insert("<a> <knows> <b>"),
            ],
        )
        .unwrap();
    }
    let changes = vec![
        insert(r#"<a> <name> "red apple""#),
        insert_edge("<a> <knows> <b>", &[("since", "2020")]),
        insert("<c> <knows> <a>"),
        delete_pattern("a", "name"),
    ];
    for a in &copies {
        let result = apply(a, changes.clone());
        w.results.push(result);
    }
    w.copies = copies.into_iter().map(Arc::new).collect();
}

#[when(
    regex = r"^two changesets read before either commits, the first writing `(.+)` and the second `(.+)`$"
)]
fn concurrent(w: &mut Steps, first: String, second: String) {
    let a = cs(w);
    let results = a
        .apply_batch(&[
            changeset(vec![insert(&first)]),
            changeset(vec![insert(&second)]),
        ])
        .expect("the batch is written");
    w.results.extend(results);
}

// Outcomes.

#[then(regex = r"^`(\w+)`'s `(\w+)` objects are (.+)$")]
fn objects_are(w: &mut Steps, subject: String, predicate: String, terms: String) {
    let a = cs(w);
    let held: BTreeSet<String> = objects(&a, &subject, &predicate)
        .iter()
        .map(Term::to_string)
        .collect();
    let expected: BTreeSet<String> = quoted(&terms).iter().map(|t| term(t).to_string()).collect();
    assert_eq!(held, expected);
}

#[then(regex = r#"^after `(.+)` is deleted, `(\w+)`'s `(\w+)` objects are (.+)$"#)]
fn after_delete(w: &mut Steps, text: String, subject: String, predicate: String, terms: String) {
    let a = cs(w);
    apply(&a, vec![delete(&text)]).expect("the delete applies");
    objects_are(w, subject, predicate, terms);
}

#[then(regex = r"^`(\w+)` holds no `(\w+)`$")]
fn holds_none(w: &mut Steps, subject: String, predicate: String) {
    let a = cs(w);
    assert!(objects(&a, &subject, &predicate).is_empty());
}

#[then(regex = r"^(`.+`) (?:is|are) held$")]
fn are_held(w: &mut Steps, statements: String) {
    let a = cs(w);
    for text in quoted(&statements) {
        assert!(is_held(&a, &text), "{text} is held");
    }
}

#[then(regex = r"^`(.+)` is not held$")]
fn not_held(w: &mut Steps, text: String) {
    let a = cs(w);
    assert!(!is_held(&a, &text), "{text} is not held");
}

#[then(regex = r"^the write fails with an error naming (.+)$")]
fn fails_naming(w: &mut Steps, names: String) {
    let error = last(w).as_ref().expect_err("the write fails").to_string();
    for name in quoted(&names) {
        let shown = if name.chars().all(|c| c.is_alphanumeric()) {
            format!("<{}>", ex(&name).as_str())
        } else {
            name
        };
        assert!(error.contains(&shown), "{error} names {shown}");
    }
}

#[then("the write succeeds")]
fn succeeds(w: &mut Steps) {
    assert!(last(w).is_ok(), "{:?}", last(w));
}

#[then("both commit")]
fn both_commit(w: &mut Steps) {
    assert_eq!(w.results.len(), 2);
    assert!(w.results.iter().all(Result::is_ok), "{:?}", w.results);
}

#[then(regex = r"^the reverse list of `(\w+)` for `(\w+)` (holds|does not hold) `(\w+)`$")]
fn reverse_list(w: &mut Steps, predicate: String, object: String, holds: String, subject: String) {
    let a = cs(w);
    let listed = reverse(&a, &predicate, &object).contains(&uid(&a, &subject));
    assert_eq!(listed, holds == "holds");
}

#[then(regex = r"^the `(\w+)` index of `(\w+)` (lists|does not list) (.+) under (.+)$")]
fn index_lists(
    w: &mut Steps,
    tokenizer: String,
    predicate: String,
    lists: String,
    subjects: String,
    under: String,
) {
    let a = cs(w);
    let tokens: Vec<(u8, Vec<u8>)> = match under.strip_prefix("the tokens of ") {
        Some(literal) => tokens_of(&tokenizer, &term(&quoted(literal)[0])),
        None => quoted(&under)
            .iter()
            .flat_map(|item| tokens_of(&tokenizer, &token_term(item)))
            .collect(),
    };
    assert!(!tokens.is_empty());
    for subject in quoted(&subjects) {
        for token in &tokens {
            let listed = indexed(&a, &predicate, token).contains(&uid(&a, &subject));
            assert_eq!(listed, lists == "lists", "{subject} under {token:?}");
        }
    }
}

#[then(regex = r"^a range read of the `int` index of `(\w+)` over (\d+) to (\d+) returns `(\w+)`$")]
fn range_read(w: &mut Steps, predicate: String, low: i64, high: i64, subject: String) {
    let a = cs(w);
    let found: BTreeSet<u64> = (low..=high)
        .flat_map(|v| tokens_of("int", &term(&v.to_string())))
        .flat_map(|token| indexed(&a, &predicate, &token))
        .collect();
    assert_eq!(found, BTreeSet::from([uid(&a, &subject)]));
}

#[then(regex = r"^no value-index key of `(\w+)` holds `(\w+)`$")]
fn no_index_key(w: &mut Steps, predicate: String, subject: String) {
    let a = cs(w);
    assert!(index_keys_holding(&a, &predicate, uid(&a, &subject)).is_empty());
}

fn direction(text: &str) -> CountDirection {
    if text == "forward" {
        CountDirection::Forward
    } else {
        CountDirection::Reverse
    }
}

#[then(
    regex = r"^`(\w+)` is listed under (forward|reverse) count (\d+) of `(\w+)` and not under (\d+)$"
)]
fn listed_under(w: &mut Steps, node: String, dir: String, count: u32, predicate: String, not: u32) {
    let a = cs(w);
    let counts = counts_of(&a, &predicate, direction(&dir), uid(&a, &node));
    assert!(
        counts.contains(&count) && !counts.contains(&not),
        "{counts:?}"
    );
}

#[then(regex = r"^`(\w+)` is listed under no (forward|reverse) count key of `(\w+)`$")]
fn listed_nowhere(w: &mut Steps, node: String, dir: String, predicate: String) {
    let a = cs(w);
    assert_eq!(
        counts_of(&a, &predicate, direction(&dir), uid(&a, &node)),
        Vec::<u32>::new()
    );
}

#[then(regex = r"^the count keys of `(\w+)` and `(\w+)` for `(\w+)` do not change$")]
fn counts_unchanged(w: &mut Steps, _subject: String, _object: String, _predicate: String) {
    let a = cs(w);
    assert!(last(w).is_ok());
    assert_eq!(count_snapshot(&a, &w.names), w.counts);
    assert_eq!(w.counts[0], vec![1]);
}

#[then("the write reads no list")]
fn reads_nothing(w: &mut Steps) {
    assert!(applied(w).read().is_empty(), "{:?}", applied(w).read());
}

#[then(regex = r"^the write reads the forward list of `(\w+)`$")]
fn reads_forward(w: &mut Steps, predicate: String) {
    let a = cs(w);
    let p = uid(&a, &predicate);
    assert!(
        applied(w)
            .read()
            .iter()
            .any(|r| r.keyspace() == Keyspace::Forward && r.predicate() == Some(p))
    );
}

/// Whether the last changeset wrote the region of `keyspace` in the default graph, for `predicate` when it names one.
fn wrote(w: &Steps, keyspace: Keyspace, predicate: Option<u64>) -> bool {
    applied(w).written().iter().any(|r| {
        r.keyspace() == keyspace
            && r.namespace() == NS
            && r.graph() == Some(DEFAULT_GRAPH)
            && r.predicate() == predicate
    })
}

#[then(
    regex = r"^the write writes the forward, reverse, predicate-list, index and statistics regions of `(\w+)`$"
)]
fn writes_insert_regions(w: &mut Steps, predicate: String) {
    let a = cs(w);
    let p = Some(uid(&a, &predicate));
    for (keyspace, predicate) in [
        (Keyspace::Forward, p),
        (Keyspace::Reverse, p),
        (Keyspace::SubjectPredicates, None),
        (Keyspace::ObjectPredicates, None),
        (Keyspace::ValueIndex, p),
        (Keyspace::Statistics, p),
    ] {
        assert!(wrote(w, keyspace, predicate), "{keyspace:?}");
    }
}

#[then(regex = r"^the write writes the forward and reverse regions of `(\w+)`$")]
fn writes_forward_reverse(w: &mut Steps, predicate: String) {
    let a = cs(w);
    let p = Some(uid(&a, &predicate));
    assert!(wrote(w, Keyspace::Forward, p) && wrote(w, Keyspace::Reverse, p));
}

#[then("the write writes the subject- and object-predicate regions of the default graph")]
fn writes_predicate_lists(w: &mut Steps) {
    assert!(
        wrote(w, Keyspace::SubjectPredicates, None) && wrote(w, Keyspace::ObjectPredicates, None)
    );
}

#[then("both stores hold the same keys, versions and values")]
fn same_bytes(w: &mut Steps) {
    assert!(w.results.iter().all(Result::is_ok));
    let [one, two] = [&w.copies[0], &w.copies[1]];
    let contents = one.quads().store().contents();
    assert!(!contents.keys().is_empty());
    assert_eq!(contents, two.quads().store().contents());
}

#[then(regex = r"^the catalog holds `(\w+)` with the default entry$")]
fn catalog_default(w: &mut Steps, predicate: String) {
    let a = cs(w);
    a.forget_catalogs();
    let catalog = a.catalog(NS).expect("the catalog loads");
    assert_eq!(
        catalog.predicate(uid(&a, &predicate)),
        Some(&PredicateDeclaration::new(ex(&predicate)))
    );
}

#[then(regex = r"^the catalog does not hold `(\w+)`$")]
fn catalog_lacks(w: &mut Steps, predicate: String) {
    let a = cs(w);
    a.forget_catalogs();
    let catalog = a.catalog(NS).expect("the catalog loads");
    let declared = a
        .quads()
        .uid_of(NS, ex(&predicate).as_str())
        .expect("the dictionary reads")
        .and_then(|p| catalog.predicate(p).cloned());
    assert_eq!(declared, None);
}

#[then(regex = r"^the forward posting of `(.+)` carries one statement ID$")]
fn carries_id(w: &mut Steps, text: String) {
    let a = cs(w);
    let held = postings(&a, "a", "knows");
    let object = EncodedTerm::uid(uid(&a, "b"));
    let ids = &held.iter().find(|(o, _)| *o == object).expect(&text).1;
    assert_eq!(ids.len(), 1);
    w.statement = Some(ids[0]);
}

#[then(regex = r"^the record of that ID holds `(.+)` and the property `(\S+) (.+)`$")]
fn record_holds(w: &mut Steps, text: String, property: String, value: String) {
    let a = cs(w);
    let record = stored_record(&a, w.statement.expect("a statement ID")).expect("a record");
    let t = statement(&text);
    let name = |s: String| {
        s.trim_start_matches("<http://ex.org/")
            .trim_end_matches('>')
            .to_string()
    };
    assert_eq!(record.subject(), uid(&a, &name(t.subject.to_string())));
    assert_eq!(record.predicate(), uid(&a, &name(t.predicate.to_string())));
    assert_eq!(
        record.object(),
        &EncodedTerm::uid(uid(&a, &name(t.object.to_string())))
    );
    let [(p, v)] = record.properties() else {
        panic!("one property: {:?}", record.properties())
    };
    assert_eq!(
        *p,
        uid(&a, property.trim_start_matches('<').trim_end_matches('>'))
    );
    assert_eq!(
        a.quads().term_of(NS, v).expect("the value decodes"),
        term(&value)
    );
}

#[then("no statement record of that ID remains")]
fn no_record(w: &mut Steps) {
    let a = cs(w);
    assert!(last(w).is_ok(), "{:?}", last(w));
    assert_eq!(
        stored_record(&a, w.statement.expect("a statement ID")),
        None
    );
}

/// The record of statement `id` in the default graph, as the last commit left it.
fn stored_record(a: &TestChangesets, id: u64) -> Option<ultragraphdb_store_quads::StatementRecord> {
    a.quads()
        .statement(NS, DEFAULT_GRAPH, id, a.quads().lists().latest_ts())
        .expect("the record reads")
}
