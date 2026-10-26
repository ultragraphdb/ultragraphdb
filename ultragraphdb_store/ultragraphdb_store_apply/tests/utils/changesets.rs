/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! What the changeset tests and scenarios share: a changeset applier over a fault-free simulated store, statements
//! written as text, declarations, and reads of the quad lists the applier writes.

use oxrdf::vocab::{rdf, xsd};
use oxrdf::{BlankNode, GraphName, Literal, NamedNode, NamedOrBlankNode, Term};
use ultragraphdb_core_keys::{CountDirection, DEFAULT_GRAPH, QuadKey};
use ultragraphdb_core_mapping::{
    Cardinality, Change, Changeset, Declaration, PredicateDeclaration, PredicateKind,
    VectorIndexSpec,
};
use ultragraphdb_core_terms::{EncodedTerm, FLOAT32_VECTOR};
use ultragraphdb_core_tok::{get_tokenizer, get_tokenizer_for_lang};
use ultragraphdb_store_apply::{Applied, ChangesetApplier, ChangesetError, token_value};
use ultragraphdb_store_posting::{CachedList, Lists, ListsConfig};
use ultragraphdb_store_sim::{
    FaultConfig, Seed, SimClock, SimListCache, SimStore, SteppedExecutor, Stream,
};

pub type TestChangesets = ChangesetApplier<SimStore, SimListCache<CachedList>, SteppedExecutor>;

/// The namespace the tests write.
pub const NS: u64 = 0;

const GEO_JSON: &str = "http://www.opengis.net/ont/geosparql#geoJSONLiteral";

/// A fault-free simulated store.
pub fn sim_store() -> SimStore {
    SimStore::new(Seed::new(1), FaultConfig::none(), SimClock::new())
}

/// A changeset applier over `store`, with a fresh cache, its builds on a stepped executor.
pub fn changesets_over(store: SimStore) -> TestChangesets {
    let cache = SimListCache::new(1024, Seed::new(1).stream(Stream::Cache));
    let executor = SteppedExecutor::new(Seed::new(1).stream(Stream::Executor));
    ChangesetApplier::new(
        Lists::new(store, cache, ListsConfig::default()),
        executor,
        1,
    )
    .expect("the applier opens")
}

/// A changeset applier over a fault-free simulated store.
pub fn changesets() -> TestChangesets {
    changesets_over(sim_store())
}

pub fn ex(name: &str) -> NamedNode {
    NamedNode::new_unchecked(format!("http://ex.org/{name}"))
}

/// The term `text` names: `<a>` an IRI under `http://ex.org/`, `rdf:type`, `_:b` a blank node, a quoted literal with
/// an optional `@lang` or `^^xsd:…`, `^^ugdb:float32vector` or `^^geo:geoJSONLiteral`, and a bare integer.
pub fn term(text: &str) -> Term {
    let text = text.trim();
    if let Some(name) = text.strip_prefix('<').and_then(|t| t.strip_suffix('>')) {
        return ex(name).into();
    }
    if text == "rdf:type" {
        return rdf::TYPE.into_owned().into();
    }
    if let Some(label) = text.strip_prefix("_:") {
        return BlankNode::new_unchecked(label).into();
    }
    if let Some(rest) = text.strip_prefix('"') {
        let end = rest.rfind('"').expect("a closing quote");
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
        return literal.into();
    }
    Literal::new_typed_literal(text, xsd::INTEGER).into()
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
    pub graph: GraphName,
    pub subject: NamedOrBlankNode,
    pub predicate: NamedNode,
    pub object: Term,
}

pub fn statement(text: &str) -> Text {
    let parts = split(text);
    let node = |t: Term| match t {
        Term::NamedNode(n) => NamedOrBlankNode::from(n),
        Term::BlankNode(b) => NamedOrBlankNode::from(b),
        other => panic!("{other} is no subject"),
    };
    let named = |t: Term| match t {
        Term::NamedNode(n) => n,
        other => panic!("{other} is no IRI"),
    };
    Text {
        subject: node(term(&parts[0])),
        predicate: named(term(&parts[1])),
        object: term(&parts[2]),
        graph: parts
            .get(3)
            .map(|g| GraphName::from(named(term(g))))
            .unwrap_or_default(),
    }
}

pub fn insert(text: &str) -> Change {
    let t = statement(text);
    Change::Insert {
        graph: t.graph,
        subject: t.subject,
        predicate: t.predicate,
        object: t.object,
    }
}

pub fn delete(text: &str) -> Change {
    let t = statement(text);
    Change::Delete {
        graph: t.graph,
        subject: t.subject,
        predicate: t.predicate,
        object: t.object,
    }
}

pub fn insert_edge(text: &str, properties: &[(&str, &str)]) -> Change {
    let t = statement(text);
    Change::InsertEdge {
        graph: t.graph,
        subject: t.subject,
        predicate: t.predicate,
        object: t.object,
        properties: properties.iter().map(|(p, v)| (ex(p), term(v))).collect(),
    }
}

pub fn delete_pattern(subject: &str, predicate: &str) -> Change {
    Change::DeletePattern {
        graph: GraphName::DefaultGraph,
        subject: ex(subject).into(),
        predicate: ex(predicate),
    }
}

/// The declaration a phrase states for `predicate`: `edge` or `property`, `single-valued`, `unique`, `count index`,
/// `vector index`, a backticked `xsd:` datatype, and backticked tokenizers.
pub fn declaration(predicate: &str, phrase: &str) -> PredicateDeclaration {
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
        d = d.with_vector_index(VectorIndexSpec::new("hnsw", Vec::new()));
    }
    let quoted: Vec<&str> = phrase.split('`').skip(1).step_by(2).collect();
    let tokenizers = quoted
        .iter()
        .filter(|q| !q.contains(':'))
        .map(|q| q.to_string())
        .collect();
    if let Some(datatype) = quoted.iter().find(|q| q.starts_with("xsd:")) {
        let Term::Literal(l) = term(&format!("\"\"^^{datatype}")) else {
            unreachable!()
        };
        d = d.with_datatype(l.datatype().into_owned());
    }
    d.with_tokenizers(tokenizers)
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

/// Declares `predicate` as `phrase` states.
pub fn declare(a: &TestChangesets, predicate: &str, phrase: &str) {
    declared(a, predicate, phrase).expect("the declaration applies");
}

/// Applies the declaration of `predicate` `phrase` states: its commit or its refusal.
pub fn declared(
    a: &TestChangesets,
    predicate: &str,
    phrase: &str,
) -> Result<Applied, ChangesetError> {
    apply(
        a,
        vec![Change::Declare(Declaration::Predicate(declaration(
            predicate, phrase,
        )))],
    )
}

/// Writes `texts`, one changeset each.
pub fn write<T: AsRef<str>>(a: &TestChangesets, texts: &[T]) {
    for text in texts {
        apply(a, vec![insert(text.as_ref())]).expect("the write applies");
    }
}

/// Runs every build waiting, to its swap.
pub fn finish_builds(a: &TestChangesets) {
    a.executor().run_until_idle();
}

/// The UID of the IRI `<name>`.
pub fn uid(a: &TestChangesets, name: &str) -> u64 {
    a.quads()
        .uid_of(NS, ex(name).as_str())
        .expect("the dictionary reads")
        .unwrap_or_else(|| panic!("<{name}> has a UID"))
}

/// The graph UID of `<name>`, the default graph for `""`.
pub fn graph(a: &TestChangesets, name: &str) -> u64 {
    if name.is_empty() {
        DEFAULT_GRAPH
    } else {
        uid(a, name)
    }
}

fn latest(a: &TestChangesets) -> u64 {
    a.quads().lists().latest_ts()
}

/// The objects of `subject`'s `predicate` in the default graph, as terms.
pub fn objects(a: &TestChangesets, subject: &str, predicate: &str) -> Vec<Term> {
    objects_in(a, "", subject, predicate)
}

pub fn objects_in(
    a: &TestChangesets,
    graph_name: &str,
    subject: &str,
    predicate: &str,
) -> Vec<Term> {
    let key = QuadKey::Forward {
        namespace: NS,
        graph: graph(a, graph_name),
        predicate: uid(a, predicate),
        subject: uid(a, subject),
    }
    .to_bytes();
    a.quads()
        .list_terms(NS, &key, latest(a))
        .expect("the list reads")
}

/// The objects of `subject`'s `predicate` in the default graph, each with its statement IDs.
pub fn postings(
    a: &TestChangesets,
    subject: &str,
    predicate: &str,
) -> Vec<(EncodedTerm, Vec<u64>)> {
    let key = QuadKey::Forward {
        namespace: NS,
        graph: DEFAULT_GRAPH,
        predicate: uid(a, predicate),
        subject: uid(a, subject),
    }
    .to_bytes();
    a.quads().objects(&key, latest(a)).expect("the list reads")
}

/// The UIDs a list holds.
fn uids(a: &TestChangesets, key: &[u8]) -> Vec<u64> {
    a.quads()
        .objects(key, latest(a))
        .expect("the list reads")
        .iter()
        .filter_map(|(o, _)| o.as_uid())
        .collect()
}

/// The subjects of the reverse list of `predicate` for the object `object`, in the default graph.
pub fn reverse(a: &TestChangesets, predicate: &str, object: &str) -> Vec<u64> {
    let object = EncodedTerm::uid(uid(a, object));
    let key = QuadKey::Reverse {
        namespace: NS,
        graph: DEFAULT_GRAPH,
        predicate: uid(a, predicate),
        object,
    }
    .to_bytes();
    uids(a, &key)
}

/// The value-index tokens of `literal` under `tokenizer`, computed as index maintenance computes them.
pub fn tokens_of(tokenizer: &str, literal: &Term) -> Vec<(u8, Vec<u8>)> {
    let Term::Literal(literal) = literal else {
        panic!("{literal} is no literal")
    };
    let t = get_tokenizer(tokenizer).expect("a tokenizer");
    let t = get_tokenizer_for_lang(&t, literal.language().unwrap_or_default());
    let value = token_value(literal.as_ref(), tokenizer).expect("the tokenizer takes the literal");
    t.tokens(&value)
        .expect("the literal tokenizes")
        .into_iter()
        .map(|token| (t.identifier(), token))
        .collect()
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
) -> Vec<u64> {
    let key = QuadKey::ValueIndex {
        namespace: NS,
        graph: DEFAULT_GRAPH,
        predicate: uid(a, predicate),
        tokenizer: *tokenizer,
        token: token.clone(),
    }
    .to_bytes();
    uids(a, &key)
}

/// The subjects the value-index key of `predicate` for the `term` token `word` holds.
pub fn indexed_term(a: &TestChangesets, predicate: &str, word: &str) -> Vec<u64> {
    let id = get_tokenizer("term").expect("term").identifier();
    indexed(a, predicate, &(id, word.as_bytes().to_vec()))
}

/// Every value-index key of `predicate` that holds `subject`.
pub fn index_keys_holding(a: &TestChangesets, predicate: &str, subject: u64) -> Vec<Vec<u8>> {
    let p = uid(a, predicate);
    stored_keys(a)
        .into_iter()
        .filter(|k| {
            matches!(
                QuadKey::parse(k),
                Ok((QuadKey::ValueIndex { predicate, .. }, _)) if predicate == p
            )
        })
        .filter(|k| uids(a, k).contains(&subject))
        .collect()
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

/// The UIDs listed under the count key of `predicate` for `count` in `direction`, in the default graph.
pub fn counted(
    a: &TestChangesets,
    predicate: &str,
    direction: CountDirection,
    count: u32,
) -> Vec<u64> {
    let key = QuadKey::CountIndex {
        namespace: NS,
        graph: DEFAULT_GRAPH,
        predicate: uid(a, predicate),
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
    uid: u64,
) -> Vec<u32> {
    (1..=8)
        .filter(|c| counted(a, predicate, direction, *c).contains(&uid))
        .collect()
}

/// The statistics of `predicate` in the default graph, as the last commit left them.
pub fn statistics(a: &TestChangesets, predicate: &str) -> ultragraphdb_store_quads::Statistics {
    a.quads()
        .statistics(NS, DEFAULT_GRAPH, uid(a, predicate), latest(a))
        .expect("the statistics read")
}

/// The predicates the subject-predicate list of `node` holds in the default graph, or with `objects` its
/// object-predicate list.
pub fn predicate_list(a: &TestChangesets, node: &str, objects: bool) -> Vec<u64> {
    let node = uid(a, node);
    let key = if objects {
        QuadKey::ObjectPredicates {
            namespace: NS,
            graph: DEFAULT_GRAPH,
            object: EncodedTerm::uid(node),
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
) -> Vec<Vec<u8>> {
    let p = uid(a, predicate);
    stored_keys(a)
        .into_iter()
        .filter(|k| {
            matches!(ultragraphdb_core_keys::Keyspace::from_byte(k[0]), Some((ks, _)) if ks == keyspace)
                && QuadKey::parse(k).is_ok_and(|(q, _)| q.predicate() == Some(p))
        })
        .collect()
}

/// Whether `subject` holds an object of `predicate` in the default graph; `false` when either has no UID.
pub fn is_held_in_default(a: &TestChangesets, subject: &str, predicate: &str) -> bool {
    let known = |name: &str| {
        a.quads()
            .uid_of(NS, ex(name).as_str())
            .expect("the dictionary reads")
            .is_some()
    };
    known(subject) && known(predicate) && !objects(a, subject, predicate).is_empty()
}
