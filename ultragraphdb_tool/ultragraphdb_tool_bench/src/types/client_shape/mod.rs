/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_cluster_placement::{NodeId, PlacementMap};
use ultragraphdb_core_ids::Epoch;
use ultragraphdb_protocol_wire::{
    DirectWrite, PatternSlot, QuadPattern, Read, RequestTerm, TermTable, Write, WriteChange,
};
use ultragraphdb_store_quads::iri_bucket;

use crate::errors::bench_error::BenchError;

/// The IRIs of the bench's nodes and predicates begin with this, as `CommitShape`'s do.
const BASE: &str = "http://bench.ultragraphdb.example/";
/// The tenant the commits write, the root, which the cluster's first start creates.
pub const CLIENT_TENANT: u64 = 0;
/// How many names a commit tries for each subject in its partition: one partition of `n` takes each name with chance
/// `1 / n`, so this many fail together with chance below `(1 - 1/n)^(64n)`, about `e^-64`.
const NAMES_PER_PARTITION: u64 = 64;

/// The commits of one connection of `client_bench`, written through the client endpoint: each commit picks a partition
/// of the cluster's even placement and writes `relations` new nodes whose buckets it owns, each with an edge `linked` to
/// one of that partition's hubs and the string `w` "x", the shape of `CommitShape`'s commits with one relation type. A
/// commit's subjects and objects lie in one partition, so no commit spans partitions and no edge registers at another
/// partition. Every name holds the run's `tag`, so runs against one cluster write distinct nodes; the hubs are the
/// run's, shared by its connections, and each connection names its own nodes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClientShape {
    prefix: String,
    relations: usize,
    placement: PlacementMap,
    /// The hubs of each partition, partition 1 first.
    hubs: Vec<Vec<String>>,
    next_node: u64,
    rng: u64,
    /// The subjects of the last commit.
    subjects: Vec<String>,
    /// The string each relation writes.
    value: String,
    /// The connection's number, which names its own predicates.
    connection: u64,
    /// The hot values and predicates written so far.
    next_hot: u64,
}

impl ClientShape {
    /// The commits of connection `connection` of the run `tag`, of `relations` relations each, over `hubs` hubs and the
    /// even placement of `partitions` partitions, drawn from `seed`.
    ///
    /// # Errors
    ///
    /// [`BenchError::usage`] for a partition count the placement refuses, or hubs leaving a partition without one.
    pub fn new(
        tag: &str,
        connection: u64,
        relations: usize,
        (partitions, hubs): (u32, u64),
        seed: u64,
    ) -> Result<Self, BenchError> {
        let placement = PlacementMap::even(partitions, NodeId::new(1))
            .map_err(|error| BenchError::usage(error.to_string()))?;
        // A partition count the placement took fits a `usize`.
        let mut grouped = vec![Vec::new(); partitions as usize];
        for hub in 0..hubs {
            let iri = format!("{BASE}{tag}/h{hub}");
            let at = partition_of(&placement, &iri)?;
            if let Some(group) = grouped.get_mut(at) {
                group.push(iri);
            }
        }
        if let Some(empty) = grouped.iter().position(Vec::is_empty) {
            return Err(BenchError::usage(format!(
                "--hubs {hubs} leaves partition {} without a hub; take more hubs",
                empty + 1
            )));
        }
        Ok(Self {
            prefix: format!("{BASE}{tag}/c{connection}/n"),
            relations,
            placement,
            hubs: grouped,
            next_node: 0,
            rng: seed.max(1)
                ^ 0x9E37_79B9_7F4A_7C15
                ^ connection.wrapping_mul(0xD1B5_4A32_D192_ED03),
            subjects: Vec::with_capacity(relations),
            value: "x".to_string(),
            connection,
            next_hot: 0,
        })
    }

    /// The commits writing a string of `bytes` bytes for each relation, in place of "x".
    #[must_use]
    pub fn with_value_bytes(mut self, bytes: usize) -> Self {
        self.value = "x".repeat(bytes);
        self
    }

    /// The IRI of `name` under the bench's base: the predicates `linked` and `w`.
    #[must_use]
    pub fn iri(name: &str) -> String {
        format!("{BASE}{name}")
    }

    /// The next commit as a DirectWrite in [`CLIENT_TENANT`].
    ///
    /// # Errors
    ///
    /// As [`ClientShape::next_changes`].
    pub fn next_direct(&mut self) -> Result<DirectWrite, BenchError> {
        let (table, changes) = self.next_changes()?;
        Ok(DirectWrite::new(CLIENT_TENANT, table, changes))
    }

    /// The next commit as the one Write of a transaction.
    ///
    /// # Errors
    ///
    /// As [`ClientShape::next_changes`].
    pub fn next_write(&mut self) -> Result<Write, BenchError> {
        let (table, changes) = self.next_changes()?;
        Ok(Write::new(table, changes))
    }

    /// The next commit's term table and changes: a partition drawn, then its subjects named and each one's hub drawn.
    ///
    /// # Errors
    ///
    /// [`BenchError::usage`] when no name of [`NAMES_PER_PARTITION`] times the partitions tried falls in the partition
    /// drawn, which a working placement makes all but impossible.
    pub fn next_changes(&mut self) -> Result<(TermTable, Vec<WriteChange>), BenchError> {
        let partitions = self.hubs.len() as u64;
        // Below the partition count, a `usize`, so it converts.
        let at = usize::try_from(self.draw() % partitions).map_err(|_| {
            BenchError::usage("a partition past this machine's indices".to_string())
        })?;
        self.changes_in(at)
    }

    /// The next commit's term table and changes in the partition at `at`, from 0: its subjects named there and each one's
    /// hub drawn.
    ///
    /// # Errors
    ///
    /// As [`ClientShape::next_changes`].
    pub fn changes_in(&mut self, at: usize) -> Result<(TermTable, Vec<WriteChange>), BenchError> {
        let partitions = self.hubs.len() as u64;
        let mut table = TermTable::new();
        let mut changes = Vec::with_capacity(2 * self.relations);
        let (linked, w) = (table.iri(&Self::iri("linked")), table.iri(&Self::iri("w")));
        let x = literal(&mut table, &self.value)?;
        self.subjects.clear();
        for _ in 0..self.relations {
            let subject = self.name_in(at, partitions)?;
            let held = self.hubs.get(at).map_or(0, Vec::len) as u64;
            // Below the partition's hub count, a `usize`, so it converts.
            let hub = usize::try_from(self.draw_below(held)).ok();
            let hubs = self.hubs.get(at).map_or(&[][..], Vec::as_slice);
            let object = table.iri(hub.and_then(|hub| hubs.get(hub)).map_or("", String::as_str));
            let node = table.iri(&subject);
            changes.push(insert(node, linked, object));
            changes.push(insert(node, w, x));
            self.subjects.push(subject);
        }
        Ok((table, changes))
    }

    /// The subjects of the last commit, in its order.
    #[must_use]
    pub fn subjects(&self) -> &[String] {
        &self.subjects
    }

    /// The Read of `<subject> <w> ?o` in the default graph, in the transaction of stream `transaction`, every row.
    #[must_use]
    pub fn read_of(subject: &str, transaction: u64) -> Read {
        let mut terms = TermTable::new();
        let pattern = QuadPattern::new(
            PatternSlot::DefaultGraph,
            PatternSlot::Bound(terms.iri(subject)),
            PatternSlot::Bound(terms.iri(&Self::iri("w"))),
            PatternSlot::Unbound,
        );
        Read::new(transaction, 0, terms, pattern, Vec::new())
    }

    /// The Read of `?s <w> ?o` in the default graph, in the transaction of stream `transaction`, every row: every node the
    /// cluster's commits wrote.
    #[must_use]
    pub fn scan_of(transaction: u64) -> Read {
        let mut terms = TermTable::new();
        let pattern = QuadPattern::new(
            PatternSlot::DefaultGraph,
            PatternSlot::Unbound,
            PatternSlot::Bound(terms.iri(&Self::iri("w"))),
            PatternSlot::Unbound,
        );
        Read::new(transaction, 0, terms, pattern, Vec::new())
    }

    /// The connection's own rare predicate, named by the run's tag so no earlier run's subjects hold it, which
    /// [`ClientShape::span_writes`] gives one subject in each partition.
    #[must_use]
    pub fn rare(&self) -> String {
        format!("{}rare", self.prefix)
    }

    /// One new subject in each partition, each holding the connection's rare predicate: one DirectWrite per partition, since
    /// a write reaches one.
    ///
    /// # Errors
    ///
    /// As [`ClientShape::next_changes`].
    pub fn span_writes(&mut self) -> Result<Vec<DirectWrite>, BenchError> {
        let partitions = self.hubs.len();
        (0..partitions)
            .map(|at| {
                let subject = self.name_in(at, partitions as u64)?;
                let mut table = TermTable::new();
                let (node, rare) = (table.iri(&subject), table.iri(&self.rare()));
                let x = literal(&mut table, "x")?;
                Ok(DirectWrite::new(
                    CLIENT_TENANT,
                    table,
                    vec![insert(node, rare, x)],
                ))
            })
            .collect()
    }

    /// The Read of `?s <rare> ?o` of this connection, in the transaction of stream `transaction`: one row from each partition.
    #[must_use]
    pub fn span_of(&self, transaction: u64) -> Read {
        let mut terms = TermTable::new();
        let pattern = QuadPattern::new(
            PatternSlot::DefaultGraph,
            PatternSlot::Unbound,
            PatternSlot::Bound(terms.iri(&self.rare())),
            PatternSlot::Unbound,
        );
        Read::new(transaction, 0, terms, pattern, Vec::new())
    }

    /// The hot subject every hot run's readers read and writers write, the run's tag left out so separate processes share
    /// it.
    #[must_use]
    pub fn hot() -> String {
        Self::iri("hot")
    }

    /// The partition the hot subject lies in, from 0.
    ///
    /// # Errors
    ///
    /// [`BenchError::usage`] when its IRI has no bucket.
    pub fn hot_partition(&self) -> Result<usize, BenchError> {
        partition_of(&self.placement, &Self::hot())
    }

    /// The hot subject's `predicates` predicates, each with the value "seed": what a hot write later adds values under.
    ///
    /// # Errors
    ///
    /// [`BenchError::usage`] when the literal is refused.
    pub fn hot_seed(predicates: u64) -> Result<DirectWrite, BenchError> {
        let mut table = TermTable::new();
        let hot = table.iri(&Self::hot());
        let seed = literal(&mut table, "seed")?;
        let changes = (0..predicates)
            .map(|p| insert(hot, table.iri(&Self::iri(&format!("p{p}"))), seed))
            .collect();
        Ok(DirectWrite::new(CLIENT_TENANT, table, changes))
    }

    /// The next hot write: a new value of the hot subject under one of its `predicates` predicates, drawn, or under a new
    /// predicate of this connection's when `fresh`.
    ///
    /// # Errors
    ///
    /// [`BenchError::usage`] when the literal is refused.
    pub fn next_hot(&mut self, predicates: u64, fresh: bool) -> Result<DirectWrite, BenchError> {
        let n = self.next_hot;
        self.next_hot += 1;
        let predicate = if fresh {
            format!("q{}-{n}", self.connection)
        } else {
            format!("p{}", self.draw_below(predicates))
        };
        let mut table = TermTable::new();
        let (hot, predicate) = (table.iri(&Self::hot()), table.iri(&Self::iri(&predicate)));
        let value = literal(&mut table, &format!("{}-{}-{n}", self.prefix, self.value))?;
        Ok(DirectWrite::new(
            CLIENT_TENANT,
            table,
            vec![insert(hot, predicate, value)],
        ))
    }

    /// The Read of `<hot> ?p ?o` in the default graph, in the transaction of stream `transaction`, at most `limit` rows.
    #[must_use]
    pub fn hot_read(transaction: u64, limit: u32) -> Read {
        let mut terms = TermTable::new();
        let pattern = QuadPattern::new(
            PatternSlot::DefaultGraph,
            PatternSlot::Bound(terms.iri(&Self::hot())),
            PatternSlot::Unbound,
            PatternSlot::Unbound,
        );
        Read::new(transaction, limit, terms, pattern, Vec::new())
    }

    /// A number drawn below `n`, from the commits' draws; 0 for an `n` of 0.
    pub fn draw_below(&mut self, n: u64) -> u64 {
        self.draw() % n.max(1)
    }

    /// A new subject in the partition at `at` of `partitions`, from 0.
    fn name_in(&mut self, at: usize, partitions: u64) -> Result<String, BenchError> {
        for _ in 0..NAMES_PER_PARTITION * partitions {
            let name = format!("{}{}", self.prefix, self.next_node);
            self.next_node += 1;
            if partition_of(&self.placement, &name)? == at {
                return Ok(name);
            }
        }
        Err(BenchError::usage(format!(
            "no name of {} tried falls in partition {}",
            NAMES_PER_PARTITION * partitions,
            at + 1
        )))
    }

    /// `CommitShape`'s xorshift.
    fn draw(&mut self) -> u64 {
        self.rng ^= self.rng << 13;
        self.rng ^= self.rng >> 7;
        self.rng ^= self.rng << 17;
        self.rng
    }
}

/// `lexical` as a plain literal of `table`.
fn literal(table: &mut TermTable, lexical: &str) -> Result<u32, BenchError> {
    table
        .intern(RequestTerm::Literal {
            lexical: lexical.to_string(),
            datatype: None,
        })
        .map_err(|error| BenchError::usage(error.to_string()))
}

/// The insert of `subject` `predicate` `object` in the default graph, each a term table's index.
fn insert(subject: u32, predicate: u32, object: u32) -> WriteChange {
    WriteChange::Insert {
        graph: None,
        subject,
        predicate,
        object,
    }
}

/// The partition `placement` gives `iri`'s bucket at the first layout, from 0.
fn partition_of(placement: &PlacementMap, iri: &str) -> Result<usize, BenchError> {
    let bucket = iri_bucket(CLIENT_TENANT, iri)
        .ok_or_else(|| BenchError::usage(format!("{iri} has no bucket")))?;
    // A partition's ID is from 1 and at most `PlacementMap::PARTITIONS_MAX`, so the index fits a `usize`.
    Ok(placement.partition_of(bucket, Epoch::ZERO).get() as usize - 1)
}
