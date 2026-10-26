/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

mod canonical_dataset_apply;

use std::collections::BTreeMap;

use oxrdf::{GraphName, NamedNode, NamedOrBlankNode, Term};

use crate::types::statement::Statement;
use crate::types::statement_id::StatementId;

/// The canonical statements of one namespace, in memory: what a store holds, by the contract's rules, with no keys,
/// terms or catalog beneath it. Writes apply a [`crate::Changeset`]; reads list the statements, which the views
/// present. It is the reference a store is checked against.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CanonicalDataset {
    /// The triples by their N-Quads text, which is unique and orders them.
    triples: BTreeMap<String, Held>,
    next_id: u64,
}

/// A triple and the statements that hold it.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Held {
    graph: GraphName,
    subject: NamedOrBlankNode,
    predicate: NamedNode,
    object: Term,
    records: Vec<Record>,
}

/// One statement of a triple: its ID, when it has one, and its properties.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Record {
    id: Option<StatementId>,
    properties: Vec<(NamedNode, Term)>,
}

impl CanonicalDataset {
    pub fn new() -> Self {
        Self::default()
    }

    /// Every statement, a triple held by several statements once for each, in the order of their triples' N-Quads
    /// text and then of their statement IDs.
    pub fn statements(&self) -> impl Iterator<Item = Statement> + '_ {
        self.triples.values().flat_map(|held| {
            held.records.iter().map(move |record| {
                let statement = Statement::new(
                    held.graph.clone(),
                    held.subject.clone(),
                    held.predicate.clone(),
                    held.object.clone(),
                )
                .with_properties(record.properties.clone());
                match record.id {
                    Some(id) => statement.with_id(id),
                    None => statement,
                }
            })
        })
    }

    /// The number of triples held.
    pub fn triple_count(&self) -> usize {
        self.triples.len()
    }

    /// The statement IDs allocated so far.
    pub fn allocated_ids(&self) -> u64 {
        self.next_id
    }
}

/// The key a triple is held under: its N-Quads text.
fn key(
    graph: &GraphName,
    subject: &NamedOrBlankNode,
    predicate: &NamedNode,
    object: &Term,
) -> String {
    match graph {
        GraphName::DefaultGraph => format!("{subject} {predicate} {object}"),
        named => format!("{subject} {predicate} {object} {named}"),
    }
}
