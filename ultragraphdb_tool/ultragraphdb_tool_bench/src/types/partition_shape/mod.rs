/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use oxrdf::{GraphName, Literal, NamedNode};
use ultragraphdb_cluster_placement::{PartitionId, PlacementMap};
use ultragraphdb_core_ids::{Bucket, Epoch};
use ultragraphdb_core_mapping::{Change, Changeset};
use ultragraphdb_store_quads::iri_bucket;

use crate::types::commit_shape::CommitShape;

/// The relation types a commit's edges and strings take.
const TYPES: u64 = 10;
/// The hubs each partition homes, the entities most edges point at.
const HUBS_PER_PARTITION: usize = 64;
/// How far a name's search goes: a hash places about one name in `partitions` in each.
const NAME_SEARCH: u64 = 1 << 24;
/// The tenant the commits write, the root.
const TENANT: u64 = 0;

/// One partition's generator of commits (partitions, "The benchmark measures N partitions in one process"), a function
/// of the run's seed and the partition: each commit writes `relations` new nodes of the partition's own buckets, each
/// with an edge of one relation type to a hub, one another partition homes for `cross_percent` of edges and otherwise
/// one of its own, and that type's string. The hubs are named `h<i>`; a node's name, `n<partition>-<k>`, is the next
/// that falls in the partition's buckets.
#[derive(Debug, Clone)]
pub struct PartitionShape {
    partition: PartitionId,
    placement: PlacementMap,
    relations: usize,
    cross_percent: u8,
    own_hubs: Vec<String>,
    other_hubs: Vec<String>,
    state: u64,
    next_node: u64,
}

impl PartitionShape {
    /// The generator of `partition` of `placement`, seeded from `seed`; `None` when the map names no partition's hubs.
    pub fn new(
        seed: u64,
        partition: PartitionId,
        placement: &PlacementMap,
        relations: usize,
        cross_percent: u8,
    ) -> Option<Self> {
        let mut holders: Vec<PartitionId> = placement
            .ranges_at(Epoch::ZERO)
            .into_iter()
            .map(|(_, owner)| owner)
            .collect();
        holders.sort();
        holders.dedup();
        let (mut own_hubs, mut other_hubs) = (Vec::new(), Vec::new());
        let mut remaining: Vec<(PartitionId, usize)> =
            holders.iter().map(|p| (*p, HUBS_PER_PARTITION)).collect();
        for i in 0..NAME_SEARCH {
            if remaining.iter().all(|(_, left)| *left == 0) {
                break;
            }
            let name = format!("h{i}");
            let owner = placement.partition_of(bucket_of(&name)?, Epoch::ZERO);
            if let Some((_, left)) = remaining
                .iter_mut()
                .find(|(p, left)| *p == owner && *left > 0)
            {
                *left -= 1;
                if owner == partition {
                    own_hubs.push(name);
                } else {
                    other_hubs.push(name);
                }
            }
        }
        // xorshift is stuck at zero; the partition's number keeps two generators' streams apart.
        let state = (seed ^ u64::from(partition.get()).wrapping_mul(0x9E37_79B9_7F4A_7C15)) | 1;
        Some(Self {
            partition,
            placement: placement.clone(),
            relations,
            cross_percent,
            own_hubs,
            other_hubs,
            state,
            next_node: 0,
        })
    }

    /// The next commit.
    pub fn next_commit(&mut self) -> Changeset {
        let mut commit = Changeset::new(TENANT);
        for _ in 0..self.relations {
            let node = CommitShape::iri(&self.next_node_name());
            let relation = self.draw() % TYPES;
            let cross = self.draw() % 100 < u64::from(self.cross_percent);
            let hub_draw = self.draw();
            let hubs = if cross && !self.other_hubs.is_empty() {
                &self.other_hubs
            } else {
                &self.own_hubs
            };
            // A hub of each partition is found before the first commit, unless the search above ran out.
            let at = usize::try_from(hub_draw % hubs.len().max(1) as u64).unwrap_or(0);
            let hub = hubs
                .get(at)
                .map_or_else(|| CommitShape::iri("h0"), |name| CommitShape::iri(name));
            commit = commit
                .with(edge(
                    &node,
                    &CommitShape::iri(&format!("rel_t{relation}_org")),
                    hub,
                ))
                .with(string(
                    &node,
                    &CommitShape::iri(&format!("rel_t{relation}_w")),
                ));
        }
        commit
    }

    /// The partition's next node's name: the next number whose name is of its buckets.
    fn next_node_name(&mut self) -> String {
        let id = self.partition.get();
        for _ in 0..NAME_SEARCH {
            let name = format!("n{id}-{}", self.next_node);
            self.next_node += 1;
            let owned = bucket_of(&name)
                .is_some_and(|b| self.placement.partition_of(b, Epoch::ZERO) == self.partition);
            if owned {
                return name;
            }
        }
        // A map giving the partition no bucket of the tenant's run: the run's routing refuses the write.
        format!("n{id}-{}", self.next_node)
    }

    /// The next draw of the xorshift `dgload` draws from, as [`CommitShape`] draws.
    fn draw(&mut self) -> u64 {
        let mut x = self.state;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.state = x;
        x
    }
}

/// The bucket of the bench's IRI `name`.
fn bucket_of(name: &str) -> Option<Bucket> {
    iri_bucket(TENANT, CommitShape::iri(name).as_str())
}

/// The edge `<node> <predicate> <hub>`.
fn edge(node: &NamedNode, predicate: &NamedNode, hub: NamedNode) -> Change {
    Change::Insert {
        graph: GraphName::DefaultGraph,
        subject: node.clone().into(),
        predicate: predicate.clone(),
        object: hub.into(),
    }
}

/// The string `<node> <predicate> "x"`.
fn string(node: &NamedNode, predicate: &NamedNode) -> Change {
    Change::Insert {
        graph: GraphName::DefaultGraph,
        subject: node.clone().into(),
        predicate: predicate.clone(),
        object: Literal::new_simple_literal("x").into(),
    }
}
