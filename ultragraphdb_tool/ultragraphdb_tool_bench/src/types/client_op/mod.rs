/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

/// What each request of a `client_bench` run is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClientOp {
    /// A DirectWrite: the commit's changes in one frame, with no Begin.
    Direct,
    /// A read-write transaction: Begin, one Write and Commit.
    Transaction,
    /// A read-only transaction: Begin, one Read drained to its end, and Abort.
    Read,
    /// A read-only transaction reading every `w` of the cluster, `?s <w> ?o`, drained to its end: a Scan taking many of
    /// its partitions' turns.
    Scan,
    /// A read-only transaction reading `?s <rare> ?o`, a predicate of the connection's own, which it gave one subject in
    /// each partition: a read reaching every partition for one row from each.
    Span,
    /// A read-write transaction that pipelines `--writes` Writes without waiting and holds them to the run's end: what a
    /// stalled partition leaves the frontend holding.
    Stall,
    /// A read-write transaction reading the hot subject's every statement, `<hot> ?p ?o`, at most `--limit` rows, then
    /// writing a new node of its own and committing: a reader whose commit a write of the hot subject may abort.
    ListRead,
    /// A DirectWrite of a new value of the hot subject, under a predicate it holds, or a new one with `--fresh on`.
    Hot,
}

impl ClientOp {
    /// Every op, in the order `--op` lists them.
    pub const ALL: [ClientOp; 8] = [
        ClientOp::Direct,
        ClientOp::Transaction,
        ClientOp::Read,
        ClientOp::Scan,
        ClientOp::Span,
        ClientOp::Stall,
        ClientOp::ListRead,
        ClientOp::Hot,
    ];

    /// The name `--op` takes and the report states.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::Direct => "direct",
            Self::Transaction => "transaction",
            Self::Read => "read",
            Self::Scan => "scan",
            Self::Span => "span",
            Self::Stall => "stall",
            Self::ListRead => "list-read",
            Self::Hot => "hot",
        }
    }

    /// The op `--op` names `name`; `None` for a name no op has.
    #[must_use]
    pub fn of_name(name: &str) -> Option<ClientOp> {
        Self::ALL.into_iter().find(|op| op.name() == name)
    }

    /// Whether each request is a read-only transaction's read, whose level `--level` names.
    #[must_use]
    pub fn reads_only(self) -> bool {
        matches!(self, Self::Read | Self::Scan | Self::Span)
    }
}
