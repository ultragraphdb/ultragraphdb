/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! What the changeset tests and scenarios share: a changeset applier over a fault-free simulated store, statements
//! written as text, declarations, and reads of the quad lists the applier writes.

use oxrdf::vocab::{rdf, xsd};
use oxrdf::{BlankNode, GraphName, Literal, NamedNode, NamedOrBlankNode, Term};
use std::num::NonZeroU64;

use ultragraphdb_core_ids::SystemId;
use ultragraphdb_core_keys::{CountDirection, DEFAULT_GRAPH, Graph, QuadKey};
use ultragraphdb_core_mapping::{
    Cardinality, Change, Changeset, Declaration, PredicateDeclaration, PredicateKind, RandomSource,
    VectorIndexSpec,
};
use ultragraphdb_core_terms::{EncodedTerm, FLOAT32_VECTOR};
use ultragraphdb_core_tok::{get_tokenizer, get_tokenizer_for_lang};
use ultragraphdb_store_apply::{Applied, ChangesetApplier, ChangesetError, token_value};
use ultragraphdb_store_posting::{CachedList, Lists, ListsConfig};
use ultragraphdb_store_sim::{
    FaultConfig, Seed, SimClock, SimListCache, SimStore, SteppedExecutor, Stream,
};
use ultragraphdb_tool_bdd::{Context, TestError, TestResult};
use ultragraphdb_traits_blocks::TestBlockSource;

/// The changeset applier of the tests: over the simulated store, its cache and its stepped executor.
pub type TestChangesets =
    ChangesetApplier<SimStore, SimListCache<CachedList>, SteppedExecutor, TestBlockSource>;

/// The namespace the tests write.
pub const NS: u64 = 0;

const GEO_JSON: &str = "http://www.opengis.net/ont/geosparql#geoJSONLiteral";

/// A fault-free simulated store.
pub fn sim_store() -> SimStore {
    SimStore::new(Seed::new(1), FaultConfig::none(), SimClock::new())
}

/// A block source handing out blocks of 1,000 sequences, standing in for the metadata group.
pub fn blocks() -> TestBlockSource {
    TestBlockSource::new(NonZeroU64::MIN.saturating_add(999))
}

/// A changeset applier over `store`, with a fresh cache, leasing from `source`, its names drawn from a source seeded
/// with `seed`, its builds on a stepped executor.
pub fn changesets_with(
    store: SimStore,
    source: TestBlockSource,
    seed: u64,
) -> TestResult<TestChangesets> {
    let cache = SimListCache::new(1024, Seed::new(1).stream(Stream::Cache));
    let executor = SteppedExecutor::new(Seed::new(1).stream(Stream::Executor));
    ChangesetApplier::new(
        Lists::new(store, cache, ListsConfig::default()),
        source,
        RandomSource::new(seed),
        executor,
        1,
    )
    .context("the applier opens")
}

/// A changeset applier over `store`, with a fresh cache and blocks of its own, its builds on a stepped executor.
pub fn changesets_over(store: SimStore) -> TestResult<TestChangesets> {
    changesets_with(store, blocks(), 1)
}

/// A changeset applier over a fault-free simulated store.
pub fn changesets() -> TestResult<TestChangesets> {
    changesets_over(sim_store())
}

/// The IRI `http://ex.org/<name>`.
pub fn ex(name: &str) -> NamedNode {
    NamedNode::new_unchecked(format!("http://ex.org/{name}"))
}

/// The term `text` names: `<a>` an IRI under `http://ex.org/`, `rdf:type`, `_:b` a blank node, a quoted literal with
/// an optional `@lang` or `^^xsd:…`, `^^ugdb:float32vector` or `^^geo:geoJSONLiteral`, a bare `true` or `false`, and
/// a bare integer.
pub fn term(text: &str) -> TestResult<Term> {
    let text = text.trim();
    if text == "true" || text == "false" {
        return Ok(Literal::new_typed_literal(text, xsd::BOOLEAN).into());
    }
    if let Some(name) = text.strip_prefix('<').and_then(|t| t.strip_suffix('>')) {
        return Ok(ex(name).into());
    }
    if text == "rdf:type" {
        return Ok(rdf::TYPE.into_owned().into());
    }
    if let Some(label) = text.strip_prefix("_:") {
        return Ok(BlankNode::new_unchecked(label).into());
    }
    if let Some(rest) = text.strip_prefix('"') {
        let end = rest.rfind('"').context("a closing quote")?;
        let lexical = rest[..end].replace("\\\"", "\"");
        let suffix = &rest[end + 1..];
        let literal = if let Some(lang) = suffix.strip_prefix('@') {
            Literal::new_language_tagged_literal_unchecked(lexical, lang)
        } else if let Some(datatype) = suffix.strip_prefix("^^") {
            let iri = match datatype {
                "ugdb:float32vector" => FLOAT32_VECTOR.to_string(),
                "geo:geoJSONLiteral" => GEO_JSON.to_string(),
                other => other.replace("xsd:", "http://www.w3.org/2001/XMLSchema#"),
            };
            Literal::new_typed_literal(lexical, NamedNode::new_unchecked(iri))
        } else {
            Literal::new_simple_literal(lexical)
        };
        return Ok(literal.into());
    }
    Ok(Literal::new_typed_literal(text, xsd::INTEGER).into())
}

/// The terms of one statement's text, split at the spaces outside quotes.
fn split(text: &str) -> Vec<String> {
    let mut parts = Vec::new();
    let mut current = String::new();
    let mut quoted = false;
    let mut escaped = false;
    for c in text.chars() {
        match c {
            '\\' if quoted => {
                escaped = true;
                current.push(c);
                continue;
            }
            '"' if !escaped => quoted = !quoted,
            ' ' if !quoted => {
                if !current.is_empty() {
                    parts.push(std::mem::take(&mut current));
                }
                continue;
            }
            _ => {}
        }
        escaped = false;
        current.push(c);
    }
    if !current.is_empty() {
        parts.push(current);
    }
    parts
}

/// A statement's text: subject, predicate, object and an optional graph.
pub struct Text {
    /// The statement's graph, the default graph when the text names none.
    pub graph: GraphName,
    /// The statement's subject, an IRI or a blank node.
    pub subject: NamedOrBlankNode,
    /// The statement's predicate.
    pub predicate: NamedNode,
    /// The statement's object.
    pub object: Term,
}

/// The statement `text` writes: its terms as [`term`] reads them, separated by spaces outside quotes.
pub fn statement(text: &str) -> TestResult<Text> {
    let parts = split(text);
    let node = |t: Term| {
        match t {
            Term::NamedNode(n) => Ok(NamedOrBlankNode::from(n)),
            Term::BlankNode(b) => Ok(NamedOrBlankNode::from(b)),
            other @ (Term::Literal(_) | Term::Triple(_)) => Err(format!("{other} is no subject")),
        }
        .context("a statement's subject is an IRI or a blank node")
    };
    let named = |t: Term| {
        match t {
            Term::NamedNode(n) => Ok(n),
            other @ (Term::BlankNode(_) | Term::Literal(_) | Term::Triple(_)) => {
                Err(format!("{other} is no IRI"))
            }
        }
        .context("a statement's predicate is an IRI")
    };
    let graph = match parts.get(3) {
        Some(g) => GraphName::from(named(term(g)?)?),
        None => GraphName::default(),
    };
    Ok(Text {
        subject: node(term(&parts[0])?)?,
        predicate: named(term(&parts[1])?)?,
        object: term(&parts[2])?,
        graph,
    })
}

/// The insert of the statement `text` writes.
pub fn insert(text: &str) -> TestResult<Change> {
    let t = statement(text)?;
    Ok(Change::Insert {
        graph: t.graph,
        subject: t.subject,
        predicate: t.predicate,
        object: t.object,
    })
}

/// The delete of the statement `text` writes.
pub fn delete(text: &str) -> TestResult<Change> {
    let t = statement(text)?;
    Ok(Change::Delete {
        graph: t.graph,
        subject: t.subject,
        predicate: t.predicate,
        object: t.object,
    })
}

/// The insert of the edge `text` writes, carrying `properties` as pairs of an `http://ex.org/` name and a term's
/// text.
pub fn insert_edge(text: &str, properties: &[(&str, &str)]) -> TestResult<Change> {
    let t = statement(text)?;
    Ok(Change::InsertEdge {
        graph: t.graph,
        subject: t.subject,
        predicate: t.predicate,
        object: t.object,
        properties: properties
            .iter()
            .map(|(p, v)| Ok((ex(p), term(v)?)))
            .collect::<TestResult<_>>()?,
    })
}

/// The delete of every object of `<subject>`'s `<predicate>` in the default graph.
pub fn delete_pattern(subject: &str, predicate: &str) -> Change {
    Change::DeletePattern {
        graph: GraphName::DefaultGraph,
        subject: ex(subject).into(),
        predicate: ex(predicate),
    }
}

/// The declaration a phrase states for `predicate`: `edge` or `property`, `single-valued`, `unique`, `count index`,
/// `vector index`, with its dimension when the phrase says `of dimension N`, a backticked `xsd:` datatype, and
/// backticked tokenizers.
pub fn declaration(predicate: &str, phrase: &str) -> TestResult<PredicateDeclaration> {
    let mut d = PredicateDeclaration::new(ex(predicate));
    if phrase.contains("an edge") {
        d = d.with_kind(PredicateKind::Edge);
    }
    if phrase.contains("a property") {
        d = d.with_kind(PredicateKind::Property);
    }
    if phrase.contains("single-valued") {
        d = d.with_cardinality(Cardinality::Single);
    }
    if phrase.contains("unique") {
        d = d.with_unique(true);
    }
    if phrase.contains("count index") {
        d = d.with_count_index(true);
    }
    if phrase.contains("vector index") {
        let mut spec = VectorIndexSpec::new("hnsw", Vec::new());
        if let Some((_, after)) = phrase.split_once("of dimension ") {
            let digits: String = after.chars().take_while(char::is_ascii_digit).collect();
            spec = spec.with_dimension(digits.parse().context("a dimension")?);
        }
        d = d.with_vector_index(spec);
    }
    let quoted: Vec<&str> = phrase.split('`').skip(1).step_by(2).collect();
    let tokenizers = quoted
        .iter()
        .filter(|q| !q.contains(':'))
        .map(|q| q.to_string())
        .collect();
    if let Some(datatype) = quoted.iter().find(|q| q.starts_with("xsd:")) {
        let Term::Literal(l) = term(&format!("\"\"^^{datatype}"))? else {
            return Err(TestError::new(format!(
                "{datatype} is no literal's datatype"
            )));
        };
        d = d.with_datatype(l.datatype().into_owned());
    }
    Ok(d.with_tokenizers(tokenizers))
}

/// One changeset of the root namespace holding `changes`.
pub fn changeset(changes: Vec<Change>) -> Changeset {
    changes
        .into_iter()
        .fold(Changeset::new(NS), Changeset::with)
}

/// Applies `changes` as one changeset.
pub fn apply(a: &TestChangesets, changes: Vec<Change>) -> Result<Applied, ChangesetError> {
    a.apply(&changeset(changes))
}

/// Applies `changes` as one changeset, for a test that reads nothing of its commit.
#[track_caller]
pub fn write_changes(a: &TestChangesets, changes: Vec<Change>) -> TestResult {
    let _applied: Applied = apply(a, changes).context("the changeset applies")?;
    Ok(())
}

/// Declares `predicate` as `phrase` states.
pub fn declare(a: &TestChangesets, predicate: &str, phrase: &str) -> TestResult {
    let _applied: Applied = declared(a, predicate, phrase)?.context("the declaration applies")?;
    Ok(())
}

/// Applies the declaration of `predicate` `phrase` states: its commit or its refusal.
pub fn declared(
    a: &TestChangesets,
    predicate: &str,
    phrase: &str,
) -> TestResult<Result<Applied, ChangesetError>> {
    Ok(apply(
        a,
        vec![Change::Declare(Declaration::Predicate(declaration(
            predicate, phrase,
        )?))],
    ))
}

/// Writes `texts`, one changeset each.
pub fn write<T: AsRef<str>>(a: &TestChangesets, texts: &[T]) -> TestResult {
    for text in texts {
        let _applied: Applied =
            apply(a, vec![insert(text.as_ref())?]).context("the write applies")?;
    }
    Ok(())
}

/// Runs one pass of the vector indexer of `predicate`: the version it indexed through.
pub fn index(a: &TestChangesets, predicate: &str) -> TestResult<u64> {
    a.index_vectors(NS, ex(predicate).as_ref())
        .context("the indexer runs")
}

/// Runs one pass of the vector indexer of `predicate`, for a test that reads nothing of what it indexed.
#[track_caller]
pub fn run_indexer(a: &TestChangesets, predicate: &str) -> TestResult {
    let _indexed_through: u64 = index(a, predicate)?;
    Ok(())
}

/// Writes `texts`, one changeset each, the vector indexer of `predicate` running after each.
pub fn write_indexed<T: AsRef<str>>(
    a: &TestChangesets,
    predicate: &str,
    texts: &[T],
) -> TestResult {
    for text in texts {
        write_changes(a, vec![insert(text.as_ref())?])?;
        run_indexer(a, predicate)?;
    }
    Ok(())
}

/// Runs every build waiting, to its swap.
pub fn finish_builds(a: &TestChangesets) {
    a.executor().run_until_idle();
}

/// The ID of the IRI `<name>`.
pub fn uid(a: &TestChangesets, name: &str) -> TestResult<SystemId> {
    a.quads()
        .id_of(NS, ex(name).as_str())
        .context("the dictionary reads")?
        .ok_or_else(|| format!("<{name}> has no ID"))
        .context("the subject has an ID")
}

/// The graph `<name>`, the default graph for `""`.
pub fn graph(a: &TestChangesets, name: &str) -> TestResult<Graph> {
    if name.is_empty() {
        Ok(DEFAULT_GRAPH)
    } else {
        Ok(Graph::Named(uid(a, name)?))
    }
}

fn latest(a: &TestChangesets) -> u64 {
    a.quads().lists().applied_ts()
}

/// The objects of `subject`'s `predicate` in the default graph, as terms.
pub fn objects(a: &TestChangesets, subject: &str, predicate: &str) -> TestResult<Vec<Term>> {
    objects_in(a, "", subject, predicate)
}

/// The objects of `subject`'s `predicate` in the graph `<graph_name>`, the default graph for `""`, as terms.
pub fn objects_in(
    a: &TestChangesets,
    graph_name: &str,
    subject: &str,
    predicate: &str,
) -> TestResult<Vec<Term>> {
    let key = QuadKey::Forward {
        namespace: NS,
        graph: graph(a, graph_name)?,
        predicate: uid(a, predicate)?,
        subject: uid(a, subject)?,
    }
    .to_bytes();
    a.quads()
        .list_terms(NS, &key, latest(a))
        .context("the list reads")
}

/// The objects of `subject`'s `predicate` in the default graph, each with its statement IDs.
pub fn postings(
    a: &TestChangesets,
    subject: &str,
    predicate: &str,
) -> TestResult<Vec<(EncodedTerm, Vec<SystemId>)>> {
    let key = QuadKey::Forward {
        namespace: NS,
        graph: DEFAULT_GRAPH,
        predicate: uid(a, predicate)?,
        subject: uid(a, subject)?,
    }
    .to_bytes();
    a.quads().objects(&key, latest(a)).context("the list reads")
}

/// The IDs a list holds.
fn uids(a: &TestChangesets, key: &[u8]) -> TestResult<Vec<SystemId>> {
    Ok(a.quads()
        .objects(key, latest(a))
        .context("the list reads")?
        .iter()
        .filter_map(|(o, _)| o.as_id())
        .collect())
}

/// The subjects of the reverse list of `predicate` for the object `object`, in the default graph.
pub fn reverse(a: &TestChangesets, predicate: &str, object: &str) -> TestResult<Vec<SystemId>> {
    let object = EncodedTerm::id(uid(a, object)?);
    let key = QuadKey::Reverse {
        namespace: NS,
        graph: DEFAULT_GRAPH,
        predicate: uid(a, predicate)?,
        object,
    }
    .to_bytes();
    uids(a, &key)
}

/// The encoding of `literal`, a literal of a datatype encoded by value or a simple string.
pub fn encoded(literal: &Term) -> TestResult<EncodedTerm> {
    let Term::Literal(literal) = literal else {
        return Err(TestError::new(format!("{literal} is no literal")));
    };
    Ok(EncodedTerm::literal(literal.as_ref(), |_| {
        Err(ultragraphdb_core_terms::TermError::malformed(
            Vec::new(),
            "no datatype ID in a test",
        ))
    })?
    .into_term())
}

/// The value-index tokens of `literal` under `tokenizer`, computed as index maintenance computes them.
pub fn tokens_of(tokenizer: &str, literal: &Term) -> TestResult<Vec<(u8, Vec<u8>)>> {
    let literal = match literal {
        Term::Literal(literal) => Ok(literal),
        other @ (Term::NamedNode(_) | Term::BlankNode(_) | Term::Triple(_)) => {
            Err(format!("{other} is no literal"))
        }
    }
    .context("the term is a literal")?;
    let t = get_tokenizer(tokenizer).context("a tokenizer")?;
    let t = get_tokenizer_for_lang(&t, literal.language().unwrap_or_default());
    let value =
        token_value(literal.as_ref(), tokenizer).context("the tokenizer takes the literal")?;
    Ok(t.tokens(&value)
        .context("the literal tokenizes")?
        .into_iter()
        .map(|token| (t.identifier(), token))
        .collect())
}

/// The `int` token of `value`, written out independently of the tokenizer: Dgraph's identifier 0x06 (`tok/tok.go`)
/// and `encodeInt`, a sign byte, 1 for a value not below zero, then the value as a big-endian u64.
pub fn int_token(value: i64) -> (u8, Vec<u8>) {
    let mut token = vec![u8::from(value >= 0)];
    token.extend_from_slice(&value.to_be_bytes());
    (0x06, token)
}

/// The `term` token of `word`, already lowercase: Dgraph's identifier 0x01 (`tok/tok.go`) and the word's bytes.
pub fn term_token(word: &str) -> (u8, Vec<u8>) {
    (0x01, word.as_bytes().to_vec())
}

/// The `exact` token of `text`: Dgraph's identifier 0x02 (`tok/tok.go`) and the string's bytes.
pub fn exact_token(text: &str) -> (u8, Vec<u8>) {
    (0x02, text.as_bytes().to_vec())
}

/// The subjects the value-index key of `predicate` for `token` holds, in the default graph.
pub fn indexed(
    a: &TestChangesets,
    predicate: &str,
    (tokenizer, token): &(u8, Vec<u8>),
) -> TestResult<Vec<SystemId>> {
    let key = QuadKey::ValueIndex {
        namespace: NS,
        graph: DEFAULT_GRAPH,
        predicate: uid(a, predicate)?,
        tokenizer: *tokenizer,
        token: token.clone(),
    }
    .to_bytes();
    uids(a, &key)
}

/// The subjects the value-index key of `predicate` for the `term` token `word` holds.
pub fn indexed_term(a: &TestChangesets, predicate: &str, word: &str) -> TestResult<Vec<SystemId>> {
    let id = get_tokenizer("term").context("term")?.identifier();
    indexed(a, predicate, &(id, word.as_bytes().to_vec()))
}

/// Every value-index key of `predicate` that holds `subject`.
pub fn index_keys_holding(
    a: &TestChangesets,
    predicate: &str,
    subject: SystemId,
) -> TestResult<Vec<Vec<u8>>> {
    let p = uid(a, predicate)?;
    let mut holding = Vec::new();
    for k in stored_keys(a) {
        let of_predicate = matches!(
            QuadKey::parse(&k),
            Ok((QuadKey::ValueIndex { predicate: indexed, .. }, _)) if indexed == p
        );
        if of_predicate && uids(a, &k)?.contains(&subject) {
            holding.push(k);
        }
    }
    Ok(holding)
}

/// Every key the store holds a version of.
pub fn stored_keys(a: &TestChangesets) -> Vec<Vec<u8>> {
    a.quads()
        .store()
        .contents()
        .keys()
        .keys()
        .cloned()
        .collect()
}

/// The IDs listed under the count key of `predicate` for `count` in `direction`, in the default graph.
pub fn counted(
    a: &TestChangesets,
    predicate: &str,
    direction: CountDirection,
    count: u32,
) -> TestResult<Vec<SystemId>> {
    let key = QuadKey::CountIndex {
        namespace: NS,
        graph: DEFAULT_GRAPH,
        predicate: uid(a, predicate)?,
        direction,
        count,
    }
    .to_bytes();
    uids(a, &key)
}

/// The counts `uid` is listed under for `predicate` in `direction`, among 1 to 8.
pub fn counts_of(
    a: &TestChangesets,
    predicate: &str,
    direction: CountDirection,
    uid: SystemId,
) -> TestResult<Vec<u32>> {
    let mut counts = Vec::new();
    for c in 1..=8 {
        if counted(a, predicate, direction, c)?.contains(&uid) {
            counts.push(c);
        }
    }
    Ok(counts)
}

/// The statistics of `predicate` in the default graph, as the last commit left them.
pub fn statistics(
    a: &TestChangesets,
    predicate: &str,
) -> TestResult<ultragraphdb_store_quads::Statistics> {
    a.quads()
        .statistics(NS, DEFAULT_GRAPH, uid(a, predicate)?, latest(a))
        .context("the statistics read")
}

/// The predicates the subject-predicate list of `node` holds in the default graph, or with `objects` its
/// object-predicate list.
pub fn predicate_list(a: &TestChangesets, node: &str, objects: bool) -> TestResult<Vec<SystemId>> {
    let node = uid(a, node)?;
    let key = if objects {
        QuadKey::ObjectPredicates {
            namespace: NS,
            graph: DEFAULT_GRAPH,
            object: EncodedTerm::id(node),
        }
    } else {
        QuadKey::SubjectPredicates {
            namespace: NS,
            graph: DEFAULT_GRAPH,
            subject: node,
        }
    }
    .to_bytes();
    uids(a, &key)
}

/// Every stored key of `keyspace` that names predicate `predicate`, in any graph.
pub fn keys_of(
    a: &TestChangesets,
    keyspace: ultragraphdb_core_keys::Keyspace,
    predicate: &str,
) -> TestResult<Vec<Vec<u8>>> {
    let p = uid(a, predicate)?;
    Ok(stored_keys(a)
        .into_iter()
        .filter(|k| {
            matches!(k.first().and_then(|b| ultragraphdb_core_keys::Keyspace::from_byte(*b)), Some((ks, _)) if ks == keyspace)
                && QuadKey::parse(k).is_ok_and(|(q, _)| q.predicate() == Some(p))
        })
        .collect())
}

/// Whether `subject` holds an object of `predicate` in the default graph; `false` when either has no ID.
pub fn is_held_in_default(a: &TestChangesets, subject: &str, predicate: &str) -> TestResult<bool> {
    let known = |name: &str| -> TestResult<bool> {
        Ok(a.quads()
            .id_of(NS, ex(name).as_str())
            .context("the dictionary reads")?
            .is_some())
    };
    Ok(known(subject)? && known(predicate)? && !objects(a, subject, predicate)?.is_empty())
}

/// Whether `<name>` is the subject of no statement in the default graph: it has no ID, or its subject-predicate list
/// is empty. A refused changeset's terms may keep their IDs, which name nothing that exists.
pub fn holds_nothing(a: &TestChangesets, name: &str) -> TestResult<bool> {
    let Some(id) = a
        .quads()
        .id_of(NS, ex(name).as_str())
        .context("the dictionary reads")?
    else {
        return Ok(true);
    };
    let key = QuadKey::SubjectPredicates {
        namespace: NS,
        graph: DEFAULT_GRAPH,
        subject: id,
    }
    .to_bytes();
    Ok(uids(a, &key)?.is_empty())
}
