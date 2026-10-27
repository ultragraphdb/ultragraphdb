/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

mod operation_view_body_view;

use crate::types::commit_operation::CommitOperation;
use crate::types::link::Link;
use crate::types::term_table_view::TermTableView;
use crate::utils::constants::ABSENT;

/// The body of a received Operation frame, read in place; see [`crate::Operation`] for its layout.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OperationView<'a> {
    kind: u8,
    delta: i64,
    properties: u32,
    links: u32,
    args: Vec<u32>,
    terms: TermTableView<'a>,
}

impl<'a> OperationView<'a> {
    pub fn kind(&self) -> u8 {
        self.kind
    }

    pub fn delta(&self) -> i64 {
        self.delta
    }

    pub fn properties(&self) -> u32 {
        self.properties
    }

    pub fn links(&self) -> u32 {
        self.links
    }

    pub fn terms(&self) -> &TermTableView<'a> {
        &self.terms
    }

    /// The operation its args describe.
    pub fn operation(&self) -> CommitOperation {
        let arg = |i: usize| self.args.get(i).copied().unwrap_or(0);
        let graph = (arg(0) != ABSENT).then(|| arg(0));
        let pairs = |from: usize, n: u32| -> Vec<(u32, u32)> {
            (0..n as usize)
                .map(|k| (arg(from + 2 * k), arg(from + 2 * k + 1)))
                .collect()
        };
        match self.kind {
            1 => CommitOperation::InsertIfAbsent {
                graph,
                predicate: arg(1),
                value: arg(2),
                properties: pairs(3, self.properties),
            },
            2 => {
                let start = 3 + 2 * self.properties as usize;
                let links = (0..self.links as usize)
                    .map(|k| {
                        let at = start + 3 * k;
                        if arg(at + 2) == 0 {
                            Link::from_node(arg(at), arg(at + 1))
                        } else {
                            Link::to_node(arg(at), arg(at + 1))
                        }
                    })
                    .collect();
                CommitOperation::UpsertAndLink {
                    graph,
                    predicate: arg(1),
                    value: arg(2),
                    properties: pairs(3, self.properties),
                    links,
                }
            }
            3 => CommitOperation::CompareAndSet {
                graph,
                subject: arg(1),
                predicate: arg(2),
                expected: arg(3),
                new: arg(4),
            },
            _ => CommitOperation::CounterAdd {
                graph,
                subject: arg(1),
                predicate: arg(2),
                delta: self.delta,
            },
        }
    }
}
