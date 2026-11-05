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
use oxrdf::{GraphName, NamedNode, Term};
use ultragraphdb_core_ids::{SystemId, TypePrefix};
use ultragraphdb_core_keys::{CountDirection, DEFAULT_GRAPH, Keyspace, QuadKey};
use ultragraphdb_core_mapping::{Change, PredicateDeclaration, RandomSource, StatementRef};
use ultragraphdb_core_terms::EncodedTerm;
use ultragraphdb_store_apply::{Applied, ChangesetApplier, ChangesetError};
use ultragraphdb_store_posting::{Lists, ListsConfig};
use ultragraphdb_store_quads::QuadStore;
use ultragraphdb_store_sim::{Seed, SimListCache, SteppedExecutor, Stream};

use crate::Steps;
use crate::utils::changesets::{
    TENANT, TestChangesets, apply, changeset, changesets, changesets_over, counts_of, delete,
    delete_pattern, ex, exact_token, index_keys_holding, indexed, insert, insert_edge, int_token,
    objects, objects_in, postings, reverse, sim_store, statement, term, term_token, tokens_of, uid,
};
use ultragraphdb_tool_bdd::{Context, TestResult};

/// The changeset applier of the scenario, created on first use.
pub(crate) fn cs(w: &mut Steps) -> TestResult<Arc<TestChangesets>> {
    if let Some(a) = &w.changesets {
        return Ok(a.clone());
    }
    let a = Arc::new(changesets()?);
    w.changesets = Some(a.clone());
    Ok(a)
}

/// The backticked items of a step's text, in order.
pub(crate) fn quoted(text: &str) -> Vec<String> {
    text.split('`')
        .skip(1)
        .step_by(2)
        .map(str::to_string)
        .collect()
}

/// The last changeset's result.
pub(crate) fn last(w: &Steps) -> TestResult<&Result<Applied, ChangesetError>> {
    w.results.last().context("a changeset was applied")
}

fn applied(w: &Steps) -> TestResult<&Applied> {
    last(w)?.as_ref().context("the changeset applied")
}

/// Whether the statement `text` names is held.
pub(crate) fn is_held(a: &TestChangesets, text: &str) -> TestResult<bool> {
    let t = statement(text)?;
    let graph = match &t.graph {
        GraphName::NamedNode(n) => n.as_str().trim_start_matches("http://ex.org/").to_string(),
        GraphName::BlankNode(_) | GraphName::DefaultGraph => String::new(),
    };
    let known = |iri: &str| -> TestResult<bool> {
        Ok(a.quads()
            .id_of(TENANT, iri)
            .context("the dictionary reads")?
            .is_some())
    };
    let subject = t.subject.to_string();
    let subject = subject
        .trim_start_matches("<http://ex.org/")
        .trim_end_matches('>');
    let predicate = t.predicate.as_str().trim_start_matches("http://ex.org/");
    if !known(ex(subject).as_str())?
        || !known(t.predicate.as_str())?
        || (!graph.is_empty() && !known(ex(&graph).as_str())?)
    {
        return Ok(false);
    }
    Ok(objects_in(a, &graph, subject, predicate)?.contains(&t.object))
}

/// The token a step's item names under `tokenizer`, written out independently of the tokenizer: an `int` token of an
/// integer, a `term` token of a word, an `exact` token of a string.
fn named_token(tokenizer: &str, item: &str) -> TestResult<(u8, Vec<u8>)> {
    match tokenizer {
        "int" => Ok(int_token(item.parse().context("an integer")?)),
        "term" => Ok(term_token(item)),
        "exact" => Ok(exact_token(item)),
        other => Err(format!("no written-out token of the {other} tokenizer")),
    }
    .context("the tokenizer's token is written out")
}

// Givens.

#[given(regex = r"^`(\w+)` is declared (.+)$")]
fn declared(w: &mut Steps, args: &[String]) -> TestResult {
    let predicate = &args[0];
    let phrase = &args[1];
    let a = cs(w)?;
    crate::utils::changesets::declare(&a, predicate, phrase)?;
    Ok(())
}

#[given(regex = r"^(`.+`) (?:is|are) held$")]
fn held(w: &mut Steps, args: &[String]) -> TestResult {
    let statements = &args[0];
    let a = cs(w)?;
    for text in quoted(statements) {
        let _applied: Applied =
            apply(&a, vec![insert(&text)?]).context("the statement is written")?;
    }
    Ok(())
}

#[given("every literal's fingerprint is 7")]
fn constant_fingerprint(w: &mut Steps) -> TestResult {
    let cache = SimListCache::new(1024, Seed::new(1).stream(Stream::Cache));
    let quads = QuadStore::open(
        Lists::new(sim_store(), cache, ListsConfig::default()),
        crate::utils::changesets::blocks(),
        RandomSource::new(1),
    )
    .context("the quads open")?
    .with_fingerprint(|_| 7);
    let executor = SteppedExecutor::new(Seed::new(1).stream(Stream::Executor));
    let applier = ChangesetApplier::over(quads, executor, 1).context("the applier opens")?;
    w.changesets = Some(Arc::new(applier));
    Ok(())
}

#[given(regex = r"^`(.+)` is held with the property `(\S+) (.+)`$")]
fn held_with_property(w: &mut Steps, args: &[String]) -> TestResult {
    let text = &args[0];
    let property = &args[1];
    let value = &args[2];
    let a = cs(w)?;
    let property = property.trim_start_matches('<').trim_end_matches('>');
    let _applied: Applied = apply(&a, vec![insert_edge(text, &[(property, value)])?])
        .context("the statement is written")?;
    Ok(())
}

// Writes.

#[when(regex = r"^(`[^`]+`(?:(?:, | and |, and )`[^`]+`)*) (?:is|are) written$")]
fn written(w: &mut Steps, args: &[String]) -> TestResult {
    let statements = &args[0];
    let a = cs(w)?;
    for text in quoted(statements) {
        let result = apply(&a, vec![insert(&text)?]);
        w.results.push(result);
    }
    Ok(())
}

#[when(regex = r"^(`.+`) (?:is|are) written in one changeset$")]
fn written_together(w: &mut Steps, args: &[String]) -> TestResult {
    let statements = &args[0];
    let a = cs(w)?;
    let changes = quoted(statements)
        .iter()
        .map(|t| insert(t))
        .collect::<TestResult<_>>()?;
    let result = apply(&a, changes);
    w.results.push(result);
    Ok(())
}

#[when(
    regex = r"^`(\w+)` has an exact count index, `(\w+)` holds two objects of it, and a third is written$"
)]
fn third_object_written(w: &mut Steps, args: &[String]) -> TestResult {
    let (predicate, subject) = (&args[0], &args[1]);
    let a = cs(w)?;
    crate::utils::changesets::declare(&a, predicate, "with an exact count index")?;
    for object in ["b", "c"] {
        let _held: Applied = apply(
            &a,
            vec![insert(&format!("<{subject}> <{predicate}> <{object}>"))?],
        )
        .context("an object is held")?;
    }
    w.names = vec![predicate.clone()];
    let result = apply(&a, vec![insert(&format!("<{subject}> <{predicate}> <d>"))?]);
    w.results.push(result);
    Ok(())
}

#[when(regex = r"^an object `(\w+)` already holds is written again$")]
fn held_object_written_again(w: &mut Steps, args: &[String]) -> TestResult {
    let subject = &args[0];
    let a = cs(w)?;
    crate::utils::changesets::declare(&a, "friend", "with an exact count index")?;
    let text = format!("<{subject}> <friend> <b>");
    let _held: Applied = apply(&a, vec![insert(&text)?]).context("the object is held")?;
    w.names = vec![subject.clone(), "b".to_string(), "friend".to_string()];
    w.counts = count_snapshot(&a, &w.names)?;
    let result = apply(&a, vec![insert(&text)?]);
    w.results.push(result);
    Ok(())
}

#[when(regex = r"^`(\w+)` has an exact count index and gains `(.+)`$")]
fn gains_counted(w: &mut Steps, args: &[String]) -> TestResult {
    let (predicate, text) = (&args[0], &args[1]);
    let a = cs(w)?;
    crate::utils::changesets::declare(&a, predicate, "with an exact count index")?;
    let t = statement(text)?;
    let object = t.object.to_string();
    let object = object
        .trim_start_matches("<http://ex.org/")
        .trim_end_matches('>')
        .to_string();
    // The object's reverse counts before the edge: none when it is new to the store.
    let before = match a
        .quads()
        .id_of(TENANT, ex(&object).as_str())
        .context("the dictionary reads")?
    {
        Some(id) => counts_of(&a, predicate, CountDirection::Reverse, id)?,
        None => Vec::new(),
    };
    w.names = vec![predicate.clone(), object];
    w.counts = vec![before];
    let result = apply(&a, vec![insert(text)?]);
    w.results.push(result);
    Ok(())
}

/// The counts the subject and object are listed under, forward and reverse.
fn count_snapshot(a: &TestChangesets, names: &[String]) -> TestResult<Vec<Vec<u32>>> {
    let predicate = names[2].trim_start_matches("http://ex.org/");
    let mut counts = Vec::new();
    for (n, d) in [
        (&names[0], CountDirection::Forward),
        (&names[1], CountDirection::Reverse),
    ] {
        counts.push(counts_of(a, predicate, d, uid(a, n)?)?);
        counts.push(counts_of(a, predicate, flip(d), uid(a, n)?)?);
    }
    Ok(counts)
}

fn flip(d: CountDirection) -> CountDirection {
    match d {
        CountDirection::Forward => CountDirection::Reverse,
        CountDirection::Reverse => CountDirection::Forward,
    }
}

#[when(regex = r"^(`.+`) (?:is|are) deleted$")]
fn deleted(w: &mut Steps, args: &[String]) -> TestResult {
    let statements = &args[0];
    let a = cs(w)?;
    for text in quoted(statements) {
        let result = apply(&a, vec![delete(&text)?]);
        w.results.push(result);
    }
    Ok(())
}

#[when(regex = r"^the pattern `(\w+)`, `(\w+)` is deleted$")]
fn pattern_deleted(w: &mut Steps, args: &[String]) -> TestResult {
    let subject = &args[0];
    let predicate = &args[1];
    let a = cs(w)?;
    let result = apply(&a, vec![delete_pattern(subject, predicate)]);
    w.results.push(result);
    Ok(())
}

#[when(regex = r"^`(.+)` gains the property `(\S+) (.+)`$")]
fn gains_property(w: &mut Steps, args: &[String]) -> TestResult {
    let text = &args[0];
    let property = &args[1];
    let value = &args[2];
    let a = cs(w)?;
    let t = statement(text)?;
    let change = Change::SetProperty {
        statement: StatementRef::Triple {
            graph: t.graph,
            subject: t.subject,
            predicate: t.predicate,
            object: t.object,
        },
        predicate: ex(property.trim_start_matches('<').trim_end_matches('>')),
        value: term(value)?,
    };
    let result = apply(&a, vec![change]);
    w.results.push(result);
    Ok(())
}

#[when("a statement with a statement ID is deleted")]
fn statement_deleted(w: &mut Steps) -> TestResult {
    let a = cs(w)?;
    let _applied: Applied = apply(
        &a,
        vec![insert_edge("<a> <knows> <b>", &[("since", "2020")])?],
    )
    .context("the statement is written")?;
    let ids = postings(&a, "a", "knows")?
        .first()
        .context("a posting")?
        .1
        .clone();
    let id = *ids.first().context("one statement ID")?;
    assert_eq!(ids.len(), 1, "one statement ID");
    w.statement = Some(id);
    let change = Change::DeleteStatement {
        graph: GraphName::DefaultGraph,
        reifier: reifier_of(&a, id)?,
    };
    let result = apply(&a, vec![change]);
    w.results.push(result);
    Ok(())
}

/// The reifier naming statement `id`.
pub(crate) fn reifier_of(a: &TestChangesets, id: SystemId) -> TestResult<NamedNode> {
    Ok(NamedNode::new(
        a.quads().iri_of(TENANT, id).context("the reifier reads")?.0,
    )?)
}

#[when("`<a> <knows> <b>` is held without a statement ID and gains the property `<since> 2020`")]
fn held_then_property(w: &mut Steps) -> TestResult {
    let a = cs(w)?;
    let _applied: Applied =
        apply(&a, vec![insert("<a> <knows> <b>")?]).context("the statement is written")?;
    assert!(
        postings(&a, "a", "knows")?
            .iter()
            .all(|(_, ids)| ids.is_empty()),
        "no statement ID yet"
    );
    gains_property(
        w,
        &["<a> <knows> <b>".into(), "<since>".into(), "2020".into()],
    )
}

#[then(
    "the statement has a statement ID of type prefix 2 and the bucket of `<a>`, its forward posting carries it, and its record holds `<since> 2020`"
)]
fn statement_id_allocated(w: &mut Steps) -> TestResult {
    let a = cs(w)?;
    assert!(last(w)?.is_ok(), "{:?}", last(w)?);
    let held = postings(&a, "a", "knows")?;
    let ids = &held.first().context("a posting")?.1;
    let id = *ids.first().context("a statement ID")?;
    assert_eq!(ids.len(), 1);
    assert_eq!(id.type_prefix(), TypePrefix::STATEMENT);
    assert_eq!(id.type_prefix().get(), 2);
    assert_eq!(id.bucket(), uid(&a, "a")?.bucket());
    let record = stored_record(&a, id)?.context("a record")?;
    let properties = record.properties();
    assert_eq!(properties.len(), 1);
    let (p, v) = properties.first().context("a property")?;
    assert_eq!(*p, uid(&a, "since")?);
    assert_eq!(a.quads().term_of(TENANT, v)?, term("2020")?);
    w.statement = Some(id);
    Ok(())
}

#[when("one changeset is applied to two copies of one store")]
fn two_copies(w: &mut Steps) -> TestResult {
    let copies = [changesets_over(sim_store())?, changesets_over(sim_store())?];
    for a in &copies {
        crate::utils::changesets::declare(a, "name", "single-valued with a `term` index")?;
        let _applied: Applied = apply(
            a,
            vec![
                insert(r#"<a> <name> "big apple""#)?,
                insert("<a> <knows> <b>")?,
            ],
        )?;
    }
    let changes = vec![
        insert(r#"<a> <name> "red apple""#)?,
        insert_edge("<a> <knows> <b>", &[("since", "2020")])?,
        insert("<c> <knows> <a>")?,
        delete_pattern("a", "name"),
    ];
    for a in &copies {
        let result = apply(a, changes.clone());
        w.results.push(result);
    }
    w.copies = copies.into_iter().map(Arc::new).collect();
    Ok(())
}

#[when(
    regex = r"^two changesets read before either commits, the first writing `(.+)` and the second `(.+)`$"
)]
fn concurrent(w: &mut Steps, first: String, second: String) -> TestResult {
    let a = cs(w)?;
    // Both execute from one watermark, then commit in one round.
    let mut changes = Vec::with_capacity(2);
    for text in [first, second] {
        changes.push(changeset(vec![insert(&text)?]));
    }
    let executed: Vec<_> = changes.iter().map(|c| a.execute(c)).collect();
    let next = a.quads().lists().next_version();
    let round: Vec<(u64, ultragraphdb_store_apply::Executed)> = executed
        .into_iter()
        .zip([next, next + 2])
        .map(|(e, v)| Ok((v, e.context("executes")?)))
        .collect::<TestResult<_>>()?;
    let applied = a.apply_round(round).context("the round is written")?;
    w.results.extend(applied.into_iter().map(Ok));
    Ok(())
}

// Outcomes.

#[then(regex = r"^`(\w+)`'s `(\w+)` objects are (.+)$")]
fn objects_are(w: &mut Steps, args: &[String]) -> TestResult {
    let subject = &args[0];
    let predicate = &args[1];
    let terms = &args[2];
    let a = cs(w)?;
    let held: BTreeSet<String> = objects(&a, subject, predicate)?
        .iter()
        .map(Term::to_string)
        .collect();
    let expected: BTreeSet<String> = quoted(terms)
        .iter()
        .map(|t| Ok(term(t)?.to_string()))
        .collect::<TestResult<_>>()?;
    assert_eq!(held, expected);
    Ok(())
}

#[then(regex = r#"^after `(.+)` is deleted, `(\w+)`'s `(\w+)` objects are (.+)$"#)]
fn after_delete(w: &mut Steps, args: &[String]) -> TestResult {
    let text = &args[0];
    let a = cs(w)?;
    let _applied: Applied = apply(&a, vec![delete(text)?]).context("the delete applies")?;
    // The subject, predicate and terms, as `objects_are` reads them.
    objects_are(w, &args[1..])?;
    Ok(())
}

#[then(regex = r"^`(\w+)` holds no `(\w+)`$")]
fn holds_none(w: &mut Steps, args: &[String]) -> TestResult {
    let subject = &args[0];
    let predicate = &args[1];
    let a = cs(w)?;
    assert!(objects(&a, subject, predicate)?.is_empty());
    Ok(())
}

#[then(regex = r"^(`.+`) (?:is|are) held$")]
fn are_held(w: &mut Steps, args: &[String]) -> TestResult {
    let statements = &args[0];
    let a = cs(w)?;
    for text in quoted(statements) {
        assert!(is_held(&a, &text)?, "{text} is held");
    }
    Ok(())
}

#[then(regex = r"^`(.+)` is not held$")]
fn not_held(w: &mut Steps, args: &[String]) -> TestResult {
    let text = &args[0];
    let a = cs(w)?;
    assert!(!is_held(&a, text)?, "{text} is not held");
    Ok(())
}

#[then(regex = r"^the write fails with an error naming ((?:`[^`]+`(?:, | and )?)+)$")]
fn fails_naming(w: &mut Steps, args: &[String]) -> TestResult {
    let names = &args[0];
    let error = last(w)?
        .as_ref()
        .err()
        .context("the write fails")?
        .to_string();
    for name in quoted(names) {
        let shown = if name.chars().all(|c| c.is_alphanumeric()) {
            format!("<{}>", ex(&name).as_str())
        } else {
            name
        };
        assert!(error.contains(&shown), "{error} names {shown}");
    }
    Ok(())
}

#[then("the write succeeds")]
fn succeeds(w: &mut Steps) -> TestResult {
    assert!(last(w)?.is_ok(), "{:?}", last(w)?);
    Ok(())
}

#[then("both commit")]
fn both_commit(w: &mut Steps) -> TestResult {
    assert_eq!(w.results.len(), 2);
    let ts: Vec<u64> = w
        .results
        .iter()
        .map(|r| Ok(r.as_ref().context("the changeset commits")?.commit_ts()))
        .collect::<TestResult<_>>()?;
    assert!(ts[0] < ts[1], "each commits at its own timestamp: {ts:?}");
    Ok(())
}

#[then(regex = r"^the reverse list of `(\w+)` for `(\w+)` (holds|does not hold) `(\w+)`$")]
fn reverse_list(w: &mut Steps, args: &[String]) -> TestResult {
    let predicate = &args[0];
    let object = &args[1];
    let holds = &args[2];
    let subject = &args[3];
    let a = cs(w)?;
    let listed = reverse(&a, predicate, object)?.contains(&uid(&a, subject)?);
    assert_eq!(listed, holds == "holds");
    Ok(())
}

#[then(regex = r"^the `(\w+)` index of `(\w+)` (lists|does not list) (.+) under (.+)$")]
fn index_lists(w: &mut Steps, args: &[String]) -> TestResult {
    let tokenizer = &args[0];
    let predicate = &args[1];
    let lists = &args[2];
    let subjects = &args[3];
    let under = &args[4];
    let a = cs(w)?;
    let tokens: Vec<(u8, Vec<u8>)> = match under.strip_prefix("the tokens of ") {
        Some(literal) => tokens_of(tokenizer, &term(&quoted(literal)[0])?)?,
        None => quoted(under)
            .iter()
            .map(|item| named_token(tokenizer, item))
            .collect::<TestResult<_>>()?,
    };
    assert!(!tokens.is_empty());
    for subject in quoted(subjects) {
        for token in &tokens {
            let listed = indexed(&a, predicate, token)?.contains(&uid(&a, &subject)?);
            assert_eq!(listed, lists == "lists", "{subject} under {token:?}");
        }
        // Under a literal's tokens, exactly those: the subject holds that one literal, so no other value-index key of
        // the predicate lists it. The tokens themselves are pinned by hand in the unit tests.
        if lists == "lists" && under.starts_with("the tokens of ") {
            assert_eq!(
                index_keys_holding(&a, predicate, uid(&a, &subject)?)?.len(),
                tokens.len(),
                "{subject}"
            );
        }
    }
    Ok(())
}

#[then(regex = r"^a range read of the `int` index of `(\w+)` over (\d+) to (\d+) returns `(\w+)`$")]
fn range_read(w: &mut Steps, args: &[String]) -> TestResult {
    let predicate = &args[0];
    let low: i64 = args[1].parse().context("the step names a number")?;
    let high: i64 = args[2].parse().context("the step names a number")?;
    let subject = &args[3];
    let a = cs(w)?;
    let mut found = BTreeSet::new();
    for v in low..=high {
        found.extend(indexed(&a, predicate, &int_token(v))?);
    }
    assert_eq!(found, BTreeSet::from([uid(&a, subject)?]));
    Ok(())
}

#[then(regex = r"^no value-index key of `(\w+)` holds `(\w+)`$")]
fn no_index_key(w: &mut Steps, args: &[String]) -> TestResult {
    let predicate = &args[0];
    let subject = &args[1];
    let a = cs(w)?;
    assert!(index_keys_holding(&a, predicate, uid(&a, subject)?)?.is_empty());
    Ok(())
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
fn listed_under(w: &mut Steps, args: &[String]) -> TestResult {
    let node = &args[0];
    let dir = &args[1];
    let count: u32 = args[2].parse().context("the step names a number")?;
    let predicate = &args[3];
    let not: u32 = args[4].parse().context("the step names a number")?;
    let a = cs(w)?;
    let counts = counts_of(&a, predicate, direction(dir), uid(&a, node)?)?;
    assert!(
        counts.contains(&count) && !counts.contains(&not),
        "{counts:?}"
    );
    Ok(())
}

#[then(regex = r"^`(\w+)` is listed under no (forward|reverse) count key of `(\w+)`$")]
fn listed_nowhere(w: &mut Steps, args: &[String]) -> TestResult {
    let node = &args[0];
    let dir = &args[1];
    let predicate = &args[2];
    let a = cs(w)?;
    assert_eq!(
        counts_of(&a, predicate, direction(dir), uid(&a, node)?)?,
        Vec::<u32>::new()
    );
    Ok(())
}

#[then(regex = r"^`(\w+)` is listed under (forward|reverse) count (\d+) and not under (\d+)$")]
fn listed_under_counted(w: &mut Steps, args: &[String]) -> TestResult {
    let (node, dir) = (&args[0], &args[1]);
    let count: u32 = args[2].parse().context("the step names a number")?;
    let not: u32 = args[3].parse().context("the step names a number")?;
    let predicate = w.names.first().cloned().context("the counted predicate")?;
    let a = cs(w)?;
    assert!(last(w)?.is_ok());
    let counts = counts_of(&a, &predicate, direction(dir), uid(&a, node)?)?;
    assert!(
        counts.contains(&count) && !counts.contains(&not),
        "{counts:?}"
    );
    Ok(())
}

#[then(regex = r"^the count keys of `(\w+)` do not change$")]
fn counts_unchanged(w: &mut Steps, args: &[String]) -> TestResult {
    let subject = &args[0];
    assert_eq!(w.names.first(), Some(subject));
    let a = cs(w)?;
    assert!(last(w)?.is_ok());
    assert_eq!(count_snapshot(&a, &w.names)?, w.counts);
    assert_eq!(w.counts[0], vec![1], "one object counted before and after");
    Ok(())
}

#[then(
    regex = r"^the reverse count keys of `(\w+)` in `<(\w+)>`'s partition reflect one more subject$"
)]
fn reverse_count_one_more(w: &mut Steps, args: &[String]) -> TestResult {
    let object = &args[0];
    let predicate = w.names.first().cloned().context("the counted predicate")?;
    assert_eq!(w.names.get(1), Some(object));
    let before = w.counts.first().cloned().context("the counts before")?;
    let a = cs(w)?;
    assert!(last(w)?.is_ok());
    let held_before = before.iter().copied().max().unwrap_or(0);
    let after = counts_of(&a, &predicate, CountDirection::Reverse, uid(&a, object)?)?;
    assert_eq!(
        after,
        vec![held_before + 1],
        "listed under one more subject alone"
    );
    Ok(())
}

#[when(regex = r"^a node holds only `(\w+)`, and a read asks for its `(\w+)`$")]
fn read_of_a_lacking_predicate(w: &mut Steps, args: &[String]) -> TestResult {
    let (held, asked) = (&args[0], &args[1]);
    let a = cs(w)?;
    // Another node holds the asked predicate, so its lists exist for some subject.
    crate::utils::changesets::write(
        &a,
        &[format!("<a> <{held}> \"Ann\""), format!("<b> <{asked}> 30")],
    )?;
    let mut execution = a
        .begin(TENANT, a.quads().lists().applied_ts())
        .context("begun")?;
    let mut out = vec![Term::from(ex("stale"))];
    let (subject, predicate) = (ex("a"), ex(asked));
    execution
        .objects_into(
            GraphName::DefaultGraph.as_ref(),
            subject.as_ref().into(),
            predicate.as_ref(),
            &mut out,
        )
        .context("the read")?;
    let executed = execution.finish().context("finished")?;
    w.flags = vec![out.is_empty()];
    w.keys = executed.reads().lists().to_vec();
    w.names = vec!["a".to_string(), asked.clone()];
    Ok(())
}

#[then(
    regex = r"^the read answers no `(\w+)` after reading the node's subject-predicates list, and reads no `(\w+)` list$"
)]
fn lacking_predicate_not_read(w: &mut Steps, args: &[String]) -> TestResult {
    let asked = &args[0];
    assert_eq!(&args[1], asked);
    assert_eq!(w.names.get(1), Some(asked));
    assert_eq!(w.flags, vec![true], "no {asked} is answered");
    let a = cs(w)?;
    let node = uid(&a, "a")?;
    let subject_predicates = QuadKey::SubjectPredicates {
        tenant: TENANT,
        graph: DEFAULT_GRAPH,
        subject: node,
    }
    .to_bytes();
    assert!(
        w.keys.contains(&subject_predicates),
        "the subject-predicates list is read"
    );
    let forward = QuadKey::Forward {
        tenant: TENANT,
        graph: DEFAULT_GRAPH,
        predicate: uid(&a, asked)?,
        subject: node,
    }
    .to_bytes();
    assert!(!w.keys.contains(&forward), "no {asked} list is read");
    Ok(())
}

#[when(
    "`<a> <name> \"Ann\"` is held, `tag` is multi-valued with a `term` index and no count index, and `<a> <tag> \"x\"` is written"
)]
fn insert_on_held_subject(w: &mut Steps) -> TestResult {
    let a = cs(w)?;
    crate::utils::changesets::declare(&a, "tag", "with `term`")?;
    crate::utils::changesets::write(&a, &["<a> <name> \"Ann\""])?;
    let result = apply(&a, vec![insert("<a> <tag> \"x\"")?]);
    w.results.push(result);
    Ok(())
}

#[then(
    "the write reads the forward list of `tag` for `a` and no other list, and writes the forward, reverse, predicate-list, index and statistics deltas"
)]
fn reads_subject_list_only(w: &mut Steps) -> TestResult {
    let a = cs(w)?;
    let tag = uid(&a, "tag")?;
    let read: Vec<_> = applied(w)?.read().iter().cloned().collect();
    assert_eq!(applied(w)?.lists_read(), 1, "{read:?}");
    assert!(
        read.iter()
            .all(|r| r.keyspace() == Keyspace::Forward && r.predicate() == Some(tag)),
        "{read:?}"
    );
    writes_insert_regions(w, &["tag".to_string()])?;
    Ok(())
}

#[when("`<a> <tag> \"x\"` is written and `a` is new to the store")]
fn insert_on_new_subject(w: &mut Steps) -> TestResult {
    let a = cs(w)?;
    crate::utils::changesets::declare(&a, "tag", "with `term`")?;
    let result = apply(&a, vec![insert("<a> <tag> \"x\"")?]);
    w.results.push(result);
    Ok(())
}

#[when("`<a> <knows> <b>` is written in one commit and again in a later one")]
fn written_twice(w: &mut Steps) -> TestResult {
    let a = cs(w)?;
    crate::utils::changesets::write(&a, &["<a> <knows> <b>", "<a> <knows> <b>"])?;
    Ok(())
}

#[then("the statistics of `knows` report 1 statement")]
fn counted_once(w: &mut Steps) -> TestResult {
    let a = cs(w)?;
    let ts = a.quads().lists().applied_ts();
    let statistics = a
        .quads()
        .statistics(TENANT, DEFAULT_GRAPH, uid(&a, "knows")?, ts)
        .context("the statistics read")?;
    assert_eq!(statistics.statements(), 1);
    Ok(())
}

#[then("the write reads no list")]
fn reads_nothing(w: &mut Steps) -> TestResult {
    assert!(applied(w)?.read().is_empty(), "{:?}", applied(w)?.read());
    Ok(())
}

#[then(regex = r"^the write reads the forward list of `(\w+)`$")]
fn reads_forward(w: &mut Steps, args: &[String]) -> TestResult {
    let predicate = &args[0];
    let a = cs(w)?;
    let p = uid(&a, predicate)?;
    assert!(
        applied(w)?
            .read()
            .iter()
            .any(|r| r.keyspace() == Keyspace::Forward && r.predicate() == Some(p))
    );
    Ok(())
}

/// Whether the last changeset wrote the region of `keyspace` in the default graph, for `predicate` when it names one.
fn wrote(w: &Steps, keyspace: Keyspace, predicate: Option<SystemId>) -> TestResult<bool> {
    Ok(applied(w)?.written().iter().any(|r| {
        r.keyspace() == keyspace
            && r.tenant() == TENANT
            && r.graph() == Some(DEFAULT_GRAPH)
            && r.predicate() == predicate
    }))
}

#[then(
    regex = r"^the write writes the forward, reverse, predicate-list, index and statistics regions of `(\w+)`$"
)]
fn writes_insert_regions(w: &mut Steps, args: &[String]) -> TestResult {
    let predicate = &args[0];
    let a = cs(w)?;
    let p = Some(uid(&a, predicate)?);
    for (keyspace, region_predicate) in [
        (Keyspace::Forward, p),
        (Keyspace::Reverse, p),
        (Keyspace::SubjectPredicates, None),
        (Keyspace::ObjectPredicates, None),
        (Keyspace::ValueIndex, p),
        (Keyspace::Statistics, p),
    ] {
        assert!(wrote(w, keyspace, region_predicate)?, "{keyspace:?}");
    }
    Ok(())
}

#[then(regex = r"^the write writes the forward and reverse regions of `(\w+)`$")]
fn writes_forward_reverse(w: &mut Steps, args: &[String]) -> TestResult {
    let predicate = &args[0];
    let a = cs(w)?;
    let p = Some(uid(&a, predicate)?);
    assert!(wrote(w, Keyspace::Forward, p)? && wrote(w, Keyspace::Reverse, p)?);
    Ok(())
}

#[then("the write writes the subject- and object-predicate regions of the default graph")]
fn writes_predicate_lists(w: &mut Steps) -> TestResult {
    assert!(
        wrote(w, Keyspace::SubjectPredicates, None)? && wrote(w, Keyspace::ObjectPredicates, None)?
    );
    Ok(())
}

#[when(
    regex = r"^a changeset inserting `(.+)` is executed and `(\w+)` holds other objects of `(\w+)`$"
)]
fn insert_beside_others(w: &mut Steps, args: &[String]) -> TestResult {
    let text = &args[0];
    let subject = &args[1];
    let predicate = &args[2];
    let a = cs(w)?;
    crate::utils::changesets::write(&a, &[format!("<{subject}> <{predicate}> <other>")])?;
    let result = apply(&a, vec![insert(text)?]);
    w.results.push(result);
    Ok(())
}

#[then(
    regex = r"^its read set holds the statement `(\w+)`, `(\w+)`, `(\w+)`, not the whole list of `(\w+)` of `(\w+)`$"
)]
fn read_set_holds_statement(w: &mut Steps, args: &[String]) -> TestResult {
    let subject = &args[0];
    let predicate = &args[1];
    let object = &args[2];
    let list_predicate = &args[3];
    let list_subject = &args[4];
    let a = cs(w)?;
    let reads = applied(w)?.reads();
    let key = |p: &str, s: &str| -> TestResult<Vec<u8>> {
        Ok(ultragraphdb_core_keys::QuadKey::Forward {
            tenant: TENANT,
            graph: DEFAULT_GRAPH,
            predicate: uid(&a, p)?,
            subject: uid(&a, s)?,
        }
        .to_bytes())
    };
    let tested = (
        key(predicate, subject)?,
        EncodedTerm::id(uid(&a, object)?).as_bytes().to_vec(),
    );
    assert!(
        reads.statements().contains(&tested),
        "{:?}",
        reads.statements()
    );
    assert!(
        !reads.lists().contains(&key(list_predicate, list_subject)?),
        "{:?}",
        reads.lists()
    );
    Ok(())
}

#[when(
    "a transaction creates `<new>`, and another transaction writing `<new>` executes before the first commits"
)]
fn two_writers_of_new(w: &mut Steps) -> TestResult {
    let a = cs(w)?;
    let first = a
        .execute(&changeset(vec![insert("<new> <p> <q>")?]))
        .context("the first executes")?;
    let second = a
        .execute(&changeset(vec![insert("<new> <r> <s>")?]))
        .context("the second executes")?;
    let id_in = |e: &ultragraphdb_store_apply::Executed| {
        e.terms()
            .written()
            .find(|(iri, _)| *iri == ex("new").as_str())
            .map(|(_, id)| id)
    };
    let (one, two) = (
        id_in(&first).context("the first names <new>")?,
        id_in(&second).context("the second names it")?,
    );
    w.names = vec![format!("{one:?}"), format!("{two:?}")];
    assert_eq!(
        a.quads().id_of(TENANT, ex("new").as_str())?,
        None,
        "neither has committed"
    );
    // The second commits; the first, refused by the commit step, is released.
    let version = a.quads().lists().next_version();
    let applied = a
        .apply_round(vec![(version, second)])
        .context("the second commits")?;
    a.release(&first);
    w.results.extend(applied.into_iter().map(Ok));
    w.statement = Some(two);
    Ok(())
}

#[then(
    "both receive the same system ID for `<new>` while executing, and whichever commits holds `<new>` under it"
)]
fn same_id_for_new(w: &mut Steps) -> TestResult {
    let a = cs(w)?;
    assert_eq!(w.names.first(), w.names.get(1), "one ID while executing");
    assert_eq!(Some(uid(&a, "new")?), w.statement);
    assert!(is_held(&a, "<new> <r> <s>")?);
    assert!(
        !is_held(&a, "<new> <p> <q>")?,
        "the refused write wrote no statement"
    );
    Ok(())
}

#[when(
    regex = r"^`a` holds three objects of `nick` and a changeset deletes the pattern `a`, `nick` in the default graph$"
)]
fn three_nicks_deleted(w: &mut Steps) -> TestResult {
    let a = cs(w)?;
    crate::utils::changesets::write(
        &a,
        &[r#"<a> <nick> "Al""#, "<a> <nick> <n1>", "<a> <nick> <n2>"],
    )?;
    let result = apply(&a, vec![delete_pattern("a", "nick")]);
    w.results.push(result);
    Ok(())
}

#[then("`a` holds no `nick`, and the reverse lists of the three objects no longer hold `a`")]
fn nicks_gone(w: &mut Steps) -> TestResult {
    let a = cs(w)?;
    assert!(last(w)?.is_ok(), "{:?}", last(w)?);
    assert!(objects(&a, "a", "nick")?.is_empty());
    let subject = uid(&a, "a")?;
    for object in ["n1", "n2"] {
        assert!(!reverse(&a, "nick", object)?.contains(&subject), "{object}");
    }
    let al = QuadKey::Reverse {
        tenant: TENANT,
        graph: DEFAULT_GRAPH,
        predicate: uid(&a, "nick")?,
        object: crate::utils::changesets::encoded(&term(r#""Al""#)?)?,
    }
    .to_bytes();
    assert!(
        a.quads()
            .objects(&al, a.quads().lists().applied_ts())?
            .is_empty(),
        "the literal's reverse list"
    );
    Ok(())
}

#[when(regex = r"^a changeset inserting `(.+)` is executed$")]
fn insert_executed(w: &mut Steps, args: &[String]) -> TestResult {
    let a = cs(w)?;
    let result = apply(&a, vec![insert(&args[0])?]);
    w.results.push(result);
    Ok(())
}

#[then(
    "the reported write regions include the forward and reverse regions of `knows` and the subject- and object-predicate regions of the default graph"
)]
fn reported_regions(w: &mut Steps) -> TestResult {
    let a = cs(w)?;
    let p = Some(uid(&a, "knows")?);
    assert!(wrote(w, Keyspace::Forward, p)? && wrote(w, Keyspace::Reverse, p)?);
    assert!(
        wrote(w, Keyspace::SubjectPredicates, None)? && wrote(w, Keyspace::ObjectPredicates, None)?
    );
    Ok(())
}

#[when("a changeset inserting `<new> <knows> <b>` is executed and `<new>` had no ID")]
fn new_term_executed(w: &mut Steps) -> TestResult {
    let a = cs(w)?;
    crate::utils::changesets::write(&a, &["<b> <knows> <c>"])?;
    assert_eq!(a.quads().id_of(TENANT, ex("new").as_str())?, None);
    let result = apply(&a, vec![insert("<new> <knows> <b>")?]);
    w.results.push(result);
    Ok(())
}

#[then("its read set holds the absent term `<new>` and no dictionary key")]
fn absent_new(w: &mut Steps) -> TestResult {
    let a = cs(w)?;
    let reads = applied(w)?.reads();
    assert_eq!(reads.absent(), &[uid(&a, "new")?]);
    let dictionary = |key: &[u8]| key.first() == Some(&Keyspace::Dictionary.byte());
    assert!(
        !reads.lists().iter().any(|k| dictionary(k)),
        "{:?}",
        reads.lists()
    );
    assert!(!reads.statements().iter().any(|(k, _)| dictionary(k)));
    assert!(!reads.ranges().iter().any(|(k, _)| dictionary(k)));
    Ok(())
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
fn catalog_default(w: &mut Steps, args: &[String]) -> TestResult {
    let predicate = &args[0];
    let a = cs(w)?;
    let catalog = a.catalog(TENANT).context("the catalog is held")?;
    assert_eq!(
        catalog.predicate(uid(&a, predicate)?),
        Some(&PredicateDeclaration::new(ex(predicate)))
    );
    Ok(())
}

#[then(regex = r"^the catalog does not hold `(\w+)`$")]
fn catalog_lacks(w: &mut Steps, args: &[String]) -> TestResult {
    let predicate = &args[0];
    let a = cs(w)?;
    crate::schema_storage_steps::send_defaults(w, &a)?;
    let recorded = w.source.defaults(TENANT);
    assert!(
        recorded
            .iter()
            .all(|e| e.predicate() != ex(predicate).as_str()),
        "{recorded:?}"
    );
    let catalog = a.catalog(TENANT).context("the catalog is held")?;
    let declared = a
        .quads()
        .id_of(TENANT, ex(predicate).as_str())
        .context("the dictionary reads")?
        .and_then(|p| catalog.predicate(p).cloned());
    assert_eq!(declared, None);
    Ok(())
}

#[then(regex = r"^the forward posting of `(.+)` carries one statement ID$")]
fn carries_id(w: &mut Steps, args: &[String]) -> TestResult {
    let text = &args[0];
    let a = cs(w)?;
    let held = postings(&a, "a", "knows")?;
    let object = EncodedTerm::id(uid(&a, "b")?);
    let ids = &held.iter().find(|(o, _)| *o == object).context(text)?.1;
    assert_eq!(ids.len(), 1);
    w.statement = Some(ids[0]);
    Ok(())
}

#[then(regex = r"^the record of that ID holds `(.+)` and the property `(\S+) (.+)`$")]
fn record_holds(w: &mut Steps, args: &[String]) -> TestResult {
    let text = &args[0];
    let property = &args[1];
    let value = &args[2];
    let a = cs(w)?;
    let record = stored_record(&a, w.statement.context("a statement ID")?)?.context("a record")?;
    let t = statement(text)?;
    let name = |s: String| {
        s.trim_start_matches("<http://ex.org/")
            .trim_end_matches('>')
            .to_string()
    };
    assert_eq!(record.subject(), uid(&a, &name(t.subject.to_string()))?);
    assert_eq!(record.predicate(), uid(&a, &name(t.predicate.to_string()))?);
    assert_eq!(
        record.object(),
        &EncodedTerm::id(uid(&a, &name(t.object.to_string()))?)
    );
    let (p, v) = match record.properties() {
        [one] => Ok(one),
        more => Err(format!("one property: {more:?}")),
    }
    .context("the record holds one property")?;
    assert_eq!(
        *p,
        uid(&a, property.trim_start_matches('<').trim_end_matches('>'))?
    );
    assert_eq!(
        a.quads().term_of(TENANT, v).context("the value decodes")?,
        term(value)?
    );
    Ok(())
}

#[then("no statement record of that ID remains")]
fn no_record(w: &mut Steps) -> TestResult {
    let a = cs(w)?;
    assert!(last(w)?.is_ok(), "{:?}", last(w)?);
    assert_eq!(
        stored_record(&a, w.statement.context("a statement ID")?)?,
        None
    );
    Ok(())
}

/// The record of statement `id` in the default graph, as the last commit left it.
fn stored_record(
    a: &TestChangesets,
    id: SystemId,
) -> TestResult<Option<ultragraphdb_store_quads::StatementRecord>> {
    a.quads()
        .statement(TENANT, DEFAULT_GRAPH, id, a.quads().lists().applied_ts())
        .context("the record reads")
}

#[when("`<a> <knows> <b>` gains the property `<since> 2020` and so receives a statement ID")]
fn gains_property_and_id(w: &mut Steps) -> TestResult {
    let a = cs(w)?;
    let _applied: Applied =
        apply(&a, vec![insert("<a> <knows> <b>")?]).context("the statement is written")?;
    gains_property(
        w,
        &["<a> <knows> <b>".into(), "<since>".into(), "2020".into()],
    )?;
    let ids = postings(&a, "a", "knows")?
        .first()
        .context("a posting")?
        .1
        .clone();
    w.statement = Some(*ids.first().context("a statement ID")?);
    Ok(())
}

#[then(
    "the dictionary holds a skolem IRI `…/.well-known/genid/{bucket}-{random}` for it, whose bucket is `<a>`'s, mapping to the statement ID and back, and a write naming that IRI as the reifier adds properties to the statement"
)]
fn reifier_is_skolem(w: &mut Steps) -> TestResult {
    let a = cs(w)?;
    let id = w.statement.context("a statement ID")?;
    let (reifier, blank) = a.quads().iri_of(TENANT, id)?;
    assert!(!blank, "a reifier is no blank node");
    let tail = reifier
        .strip_prefix(&format!(
            "{}.well-known/genid/",
            ultragraphdb_store_quads::tenant_base(TENANT)
        ))
        .context("under the tenant's skolem base")?;
    let (bucket, random) = tail.split_once('-').context("a bucket and random digits")?;
    assert_eq!((bucket.len(), random.len()), (6, 26), "{reifier}");
    assert_eq!(
        u32::from_str_radix(bucket, 16)?,
        uid(&a, "a")?.bucket().get(),
        "<a>'s bucket"
    );
    assert_eq!(
        a.quads().id_of(TENANT, &reifier)?,
        Some(id),
        "the IRI maps back to the statement ID"
    );
    let change = Change::SetProperty {
        statement: StatementRef::Reifier {
            graph: GraphName::DefaultGraph,
            reifier: NamedNode::new(reifier)?,
        },
        predicate: ex("weight"),
        value: term("1")?,
    };
    let _applied: Applied =
        apply(&a, vec![change]).context("the property is added through the reifier")?;
    let record = stored_record(&a, id)?.context("the record")?;
    assert_eq!(record.properties().len(), 2, "{record:?}");
    Ok(())
}

#[when(
    "two transactions in `<new>`'s home read before either commits, both write `<new> <p> \"x\"`, not held, and both send their commit"
)]
fn two_creators(w: &mut Steps) -> TestResult {
    let a = cs(w)?;
    let first = a.execute(&changeset(vec![insert(r#"<new> <p> "x""#)?]));
    let second = a.execute(&changeset(vec![insert(r#"<new> <p> "x""#)?]));
    w.flags = vec![first.is_ok(), second.is_ok()];
    let (first, second) = (
        first.context("the first executes")?,
        second.context("the second executes")?,
    );
    let id_in = |e: &ultragraphdb_store_apply::Executed| {
        e.terms()
            .written()
            .find(|(iri, _)| *iri == ex("new").as_str())
            .map(|(_, id)| id)
    };
    let (one, two) = (
        id_in(&first).context("the first names <new>")?,
        id_in(&second).context("the second names it")?,
    );
    // What the commit step validates by: each one's read set against the other's writes.
    let wrote = |e: &ultragraphdb_store_apply::Executed, key: &[u8]| {
        e.writes().written_keys().any(|k| k == key)
    };
    let names_new = |e: &ultragraphdb_store_apply::Executed| {
        e.writes().written_keys().any(|k| {
            matches!(QuadKey::parse(k), Ok((QuadKey::Forward { subject, .. }, _)) if subject == one)
        })
    };
    let second_tested_what_first_wrote = second
        .reads()
        .statements()
        .iter()
        .any(|(key, _)| wrote(&first, key));
    let first_absent_what_second_wrote =
        first.reads().absent().contains(&one) && names_new(&second);
    w.flags.extend([
        one == two,
        second_tested_what_first_wrote,
        first_absent_what_second_wrote,
    ]);
    let version = a.quads().lists().next_version();
    let applied = a
        .apply_round(vec![(version, first)])
        .context("the first commits")?;
    a.release(&second);
    w.results.extend(applied.into_iter().map(Ok));
    w.statement = Some(one);
    Ok(())
}

#[then(
    "both receive one system ID for `<new>` and neither aborts for creating it; the two still conflict on the statement (transactions, \"What a transaction's reads record and what conflicts with them\"), so exactly one commits, and `<new>` has one system ID"
)]
fn creators_share(w: &mut Steps) -> TestResult {
    let a = cs(w)?;
    assert_eq!(
        w.flags.get(..2),
        Some(&[true, true][..]),
        "neither aborts for creating <new>"
    );
    assert_eq!(w.flags.get(2), Some(&true), "one system ID for <new>");
    assert_eq!(
        w.flags.get(3),
        Some(&true),
        "the second tested the statement the first wrote"
    );
    assert_eq!(
        w.flags.get(4),
        Some(&true),
        "the first found <new> absent, which the second wrote"
    );
    let id = w.statement.context("the ID")?;
    assert_eq!(uid(&a, "new")?, id);
    assert_eq!(
        a.quads().iri_of(TENANT, id)?,
        (ex("new").as_str().to_string(), false)
    );
    assert!(is_held(&a, r#"<new> <p> "x""#)?);
    Ok(())
}
