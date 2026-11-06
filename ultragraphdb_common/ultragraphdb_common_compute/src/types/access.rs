/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_core_ids::SystemId;
use ultragraphdb_core_keys::{Graph, Keyspace, QuadKey, key_prefix};
use ultragraphdb_core_terms::EncodedTerm;

use crate::utils::keys::{past_alone, prefix_end};

/// The keyspace a read takes in a partition, by its bound positions (`PROTOCOL.md` §11.12, quad-reads "Access paths follow the
/// bound positions"), each by the IDs that partition resolved.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Access {
    /// A bound subject and predicate: the one forward list.
    ForwardKey {
        predicate: SystemId,
        subject: SystemId,
    },
    /// An unbound subject and no reverse key: the forward lists of a predicate, or of every predicate.
    ForwardRange { predicate: Option<SystemId> },
    /// A bound subject alone: its predicates' list, then each forward list.
    SubjectPredicates { subject: SystemId },
    /// A bound object and predicate: the one reverse list.
    ReverseKey {
        predicate: SystemId,
        object: EncodedTerm,
    },
    /// A bound object alone: its predicates' list, then each reverse list.
    ObjectPredicates { object: EncodedTerm },
    /// A bound subject that is a reifier: the statement's record.
    Statement { statement: SystemId },
}

/// One range a read has yet to Scan.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RangeSpec {
    pub(crate) first: Vec<u8>,
    pub(crate) past: Vec<u8>,
    pub(crate) after: Vec<u8>,
}

impl RangeSpec {
    /// The range holding `key` alone, resuming its list at `after`.
    pub(crate) fn alone(key: Vec<u8>, after: Vec<u8>) -> Self {
        let past = past_alone(&key);
        Self {
            first: key,
            past,
            after,
        }
    }

    /// Whether the range holds one key alone.
    pub(crate) fn is_alone(&self) -> bool {
        self.past.strip_prefix(self.first.as_slice()) == Some(&[0][..])
    }
}

impl Access {
    /// The keyspace the access reads, which an unbound graph's walk finds its graphs in.
    pub(crate) fn keyspace(&self) -> Keyspace {
        match self {
            Self::ForwardKey { .. } | Self::ForwardRange { .. } => Keyspace::Forward,
            Self::SubjectPredicates { .. } => Keyspace::SubjectPredicates,
            Self::ReverseKey { .. } => Keyspace::Reverse,
            Self::ObjectPredicates { .. } => Keyspace::ObjectPredicates,
            Self::Statement { .. } => Keyspace::Statements,
        }
    }

    /// The range holding what the access reads in `graph`: for a predicates' list, that list.
    pub(crate) fn range(&self, tenant: u64, graph: Graph) -> RangeSpec {
        match self {
            Self::ForwardKey { predicate, subject } => RangeSpec::alone(
                QuadKey::Forward {
                    tenant,
                    graph,
                    predicate: *predicate,
                    subject: *subject,
                }
                .to_bytes(),
                Vec::new(),
            ),
            Self::ForwardRange { predicate } => {
                let first = key_prefix(Keyspace::Forward, false, tenant, Some(graph), *predicate);
                let past = prefix_end(&first);
                RangeSpec {
                    first,
                    past,
                    after: Vec::new(),
                }
            }
            Self::SubjectPredicates { subject } => RangeSpec::alone(
                QuadKey::SubjectPredicates {
                    tenant,
                    graph,
                    subject: *subject,
                }
                .to_bytes(),
                Vec::new(),
            ),
            Self::ReverseKey { predicate, object } => RangeSpec::alone(
                QuadKey::Reverse {
                    tenant,
                    graph,
                    predicate: *predicate,
                    object: object.clone(),
                }
                .to_bytes(),
                Vec::new(),
            ),
            Self::ObjectPredicates { object } => RangeSpec::alone(
                QuadKey::ObjectPredicates {
                    tenant,
                    graph,
                    object: object.clone(),
                }
                .to_bytes(),
                Vec::new(),
            ),
            Self::Statement { statement } => RangeSpec::alone(
                QuadKey::Statement {
                    tenant,
                    graph,
                    statement: *statement,
                }
                .to_bytes(),
                Vec::new(),
            ),
        }
    }

    /// The key a predicate's list names, in `graph`, for an access reading a predicates' list.
    pub(crate) fn listed_key(
        &self,
        tenant: u64,
        graph: Graph,
        predicate: SystemId,
    ) -> Option<Vec<u8>> {
        match self {
            Self::SubjectPredicates { subject } => Some(
                QuadKey::Forward {
                    tenant,
                    graph,
                    predicate,
                    subject: *subject,
                }
                .to_bytes(),
            ),
            Self::ObjectPredicates { object } => Some(
                QuadKey::Reverse {
                    tenant,
                    graph,
                    predicate,
                    object: object.clone(),
                }
                .to_bytes(),
            ),
            Self::ForwardKey { .. }
            | Self::ForwardRange { .. }
            | Self::ReverseKey { .. }
            | Self::Statement { .. } => None,
        }
    }

    /// Whether the access reads a predicates' list first.
    pub(crate) fn lists_predicates(&self) -> bool {
        matches!(
            self,
            Self::SubjectPredicates { .. } | Self::ObjectPredicates { .. }
        )
    }
}
