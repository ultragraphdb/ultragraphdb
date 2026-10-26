/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use oxrdf::vocab::xsd;
use oxrdf::{GraphName, Literal, NamedNode, Term};
use ultragraphdb_core_mapping::{
    Change, Changeset, Declaration, PredicateDeclaration, PredicateKind,
};

/// The IRIs of the bench's nodes and predicates begin with this.
const BASE: &str = "http://bench.ultragraphdb.example/";

/// The commits of the Dgraph write runs (scrith's `dgraph-bench.md` §5–§7, `dgload` over `singlefs.py`'s schema), as
/// changesets of quads in the default graph of the root namespace: each commit picks one of the relation types
/// `rel_t<j>` and writes `relations` new nodes of it, each with an edge `rel_t<j>_org` to one of the hubs and the
/// string `rel_t<j>_w` "x". The hubs `h<i>` carry `name`, indexed `exact`. Every edge keeps its reverse list, as every
/// quad does; no predicate has a count index. New nodes are named in order; types and hubs are drawn from a seeded
/// xorshift, as `dgload` draws them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommitShape {
    relations: usize,
    hubs: u64,
    types: u64,
    next_node: u64,
    rng: u64,
}

impl CommitShape {
    /// Commits of `relations` records over `hubs` hubs and `types` relation types, drawn from `seed`.
    pub fn new(relations: usize, hubs: u64, types: u64, seed: u64) -> Self {
        Self {
            relations,
            hubs,
            types,
            next_node: 1,
            rng: seed.max(1) ^ 0x9E37_79B9_7F4A_7C15,
        }
    }

    /// The IRI of `name` under the bench's base.
    pub fn iri(name: &str) -> NamedNode {
        NamedNode::new_unchecked(format!("{BASE}{name}"))
    }

    /// The IRI of hub `i`, from 0.
    pub fn hub(i: u64) -> NamedNode {
        Self::iri(&format!("h{i}"))
    }

    /// The declarations of the Dgraph runs' schema over `types` relation types: each `rel_t<j>_org` an edge, each
    /// `rel_t<j>_w` a property of strings, and `name` a property of strings indexed `exact`.
    pub fn declarations(types: u64) -> Changeset {
        let property = |name: &str| {
            PredicateDeclaration::new(Self::iri(name))
                .with_kind(PredicateKind::Property)
                .with_datatype(xsd::STRING.into_owned())
        };
        let mut declarations: Vec<PredicateDeclaration> = (0..types)
            .flat_map(|j| {
                [
                    PredicateDeclaration::new(Self::iri(&format!("rel_t{j}_org")))
                        .with_kind(PredicateKind::Edge),
                    property(&format!("rel_t{j}_w")),
                ]
            })
            .collect();
        declarations.push(property("name").with_tokenizers(vec!["exact".to_string()]));
        declarations.into_iter().fold(Changeset::new(0), |c, d| {
            c.with(Change::Declare(Declaration::Predicate(d)))
        })
    }

    /// The hubs, each named `h<i>`, in one changeset.
    pub fn hubs(&self) -> Changeset {
        let name = Self::iri("name");
        (0..self.hubs).fold(Changeset::new(0), |c, i| {
            c.with(Change::Insert {
                graph: GraphName::DefaultGraph,
                subject: Self::hub(i).into(),
                predicate: name.clone(),
                object: Term::from(Literal::new_simple_literal(format!("h{i}"))),
            })
        })
    }

    /// The next commit.
    pub fn next_commit(&mut self) -> Changeset {
        let j = self.draw() % self.types;
        let org = Self::iri(&format!("rel_t{j}_org"));
        let w = Self::iri(&format!("rel_t{j}_w"));
        let mut changeset = Changeset::new(0);
        for _ in 0..self.relations {
            let node = Self::iri(&format!("r{}", self.next_node));
            self.next_node += 1;
            let hub = self.draw() % self.hubs;
            changeset = changeset
                .with(Change::Insert {
                    graph: GraphName::DefaultGraph,
                    subject: node.clone().into(),
                    predicate: org.clone(),
                    object: Self::hub(hub).into(),
                })
                .with(Change::Insert {
                    graph: GraphName::DefaultGraph,
                    subject: node.into(),
                    predicate: w.clone(),
                    object: Term::from(Literal::new_simple_literal("x")),
                });
        }
        changeset
    }

    /// The relations each commit writes.
    pub fn relations(&self) -> usize {
        self.relations
    }

    /// `dgload`'s xorshift.
    fn draw(&mut self) -> u64 {
        self.rng ^= self.rng << 13;
        self.rng ^= self.rng >> 7;
        self.rng ^= self.rng << 17;
        self.rng
    }
}
