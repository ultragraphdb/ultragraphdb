/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_core_keys::attr_in_root_namespace;
use ultragraphdb_proto_pb::pb::{DirectedEdge, Mutations, directed_edge, posting};

/// The commits the Dgraph write runs made (scrith's `dgraph-bench.md` §5–§7, `dgload` over `singlefs.py`'s
/// schema): each commit picks one of the relation types `rel_t<j>` and writes `relations` new nodes of it, each
/// with an edge `rel_t<j>_org` to one of the hubs, kept as an `@reverse` edge, and the string `rel_t<j>_w` "x".
/// The hubs carry `name`, indexed `exact`. No predicate has `@count`. New nodes take uids above the hubs', in
/// order, as Zero's leases hand them out; types and hubs are drawn from a seeded xorshift, as `dgload` draws them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommitShape {
    relations: usize,
    hubs: u64,
    types: u64,
    next_uid: u64,
    rng: u64,
    org: Vec<String>,
    w: Vec<String>,
}

impl CommitShape {
    /// Commits of `relations` records over `hubs` hubs and `types` relation types, drawn from `seed`.
    pub fn new(relations: usize, hubs: u64, types: u64, seed: u64) -> Self {
        Self {
            relations,
            hubs,
            types,
            next_uid: hubs + 1,
            rng: seed.max(1) ^ 0x9E37_79B9_7F4A_7C15,
            org: (0..types)
                .map(|j| attr_in_root_namespace(&format!("rel_t{j}_org")))
                .collect(),
            w: (0..types)
                .map(|j| attr_in_root_namespace(&format!("rel_t{j}_w")))
                .collect(),
        }
    }

    /// The Dgraph runs' schema over `types` relation types.
    pub fn schema(types: u64) -> String {
        let mut lines: Vec<String> = (0..types)
            .flat_map(|j| {
                [
                    format!("rel_t{j}_org: uid @reverse ."),
                    format!("rel_t{j}_w: string ."),
                ]
            })
            .collect();
        lines.push("name: string @index(exact) .".to_string());
        lines.join("\n")
    }

    /// The hubs, uids 1 to `hubs`, each named `h<i>`, in one mutation reading at `start_ts`.
    pub fn hubs(&self, start_ts: u64) -> Mutations {
        let name = attr_in_root_namespace("name");
        Mutations {
            start_ts,
            edges: (0..self.hubs)
                .map(|i| DirectedEdge {
                    entity: i + 1,
                    attr: name.clone(),
                    value: format!("h{i}").into_bytes(),
                    op: directed_edge::Op::Set as i32,
                    ..DirectedEdge::default()
                })
                .collect(),
            ..Mutations::default()
        }
    }

    /// The next commit, reading at `start_ts`.
    pub fn next(&mut self, start_ts: u64) -> Mutations {
        let j = (self.draw() % self.types) as usize;
        let mut edges = Vec::with_capacity(2 * self.relations);
        for _ in 0..self.relations {
            let uid = self.next_uid;
            self.next_uid += 1;
            let hub = self.draw() % self.hubs + 1;
            edges.push(DirectedEdge {
                entity: uid,
                attr: self.org[j].clone(),
                value_id: hub,
                value_type: posting::ValType::Uid as i32,
                op: directed_edge::Op::Set as i32,
                ..DirectedEdge::default()
            });
            edges.push(DirectedEdge {
                entity: uid,
                attr: self.w[j].clone(),
                value: b"x".to_vec(),
                op: directed_edge::Op::Set as i32,
                ..DirectedEdge::default()
            });
        }
        Mutations {
            start_ts,
            edges,
            ..Mutations::default()
        }
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
