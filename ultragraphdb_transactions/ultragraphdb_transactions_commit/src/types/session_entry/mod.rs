/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_core_ids::{Epoch, SessionToken};
use ultragraphdb_error_store::StoreError;
use ultragraphdb_protocol_wire::{ABSENT, OperationResult, RequestTerm, TermTable};

use crate::types::stored_reply::StoredReply;

/// The fixed part: request, version, partition, 4 zero bytes, position, epoch, result count and node count.
const FIXED: usize = 48;
/// A result's size.
const RESULT: usize = 16;

/// A session's entry in a partition's keyspace without versions (7.8): its last request committed in the partition and
/// that request's whole reply, the commit version, the session token and every commit-time operation's result with the
/// reply's term table, which names each node by its IRI or skolem IRI, so a retry after a restart is answered as the
/// original was.
///
/// The entry sits at [`SessionEntry::key`] under the system prefix, outside every namespace, so no drop removes it;
/// [`SessionEntry::INDEX_KEY`] holds the identities of the sessions held, ascending, 16 big-endian bytes each. An entry's
/// bytes, little-endian: `request` u64, `version` u64, the token's `partition` u32 and 4 zero bytes, its `position`
/// u64 and its `epoch` u64, the result count u32 and the term table's length u32, then each result's 16 bytes as a
/// Commit reply holds them (`PROTOCOL.md` §6.3), then the term table's records and text (§4.4).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionEntry {
    request: u64,
    reply: StoredReply,
}

impl SessionEntry {
    /// The key holding the identities of the sessions held.
    pub const INDEX_KEY: &'static [u8] = b"\xffultragraphdb/sessions";

    /// The prefix of every session's key.
    pub const KEY_PREFIX: &'static [u8] = b"\xffultragraphdb/session/";

    /// The entry of a session whose last committed request in the partition is `request`, answered with `reply`.
    pub fn new(request: u64, reply: StoredReply) -> Self {
        Self { request, reply }
    }

    /// The key of session `session`'s entry: the prefix, then its identity's 16 big-endian bytes, so keys sort by
    /// identity.
    pub fn key(session: u128) -> Vec<u8> {
        let mut key = Vec::with_capacity(Self::KEY_PREFIX.len() + 16);
        key.extend_from_slice(Self::KEY_PREFIX);
        key.extend_from_slice(&session.to_be_bytes());
        key
    }

    /// The session's last committed request number in the partition.
    pub fn request(&self) -> u64 {
        self.request
    }

    /// That request's stored reply.
    pub fn reply(&self) -> &StoredReply {
        &self.reply
    }

    /// The reply, taken out of the entry.
    pub fn into_reply(self) -> StoredReply {
        self.reply
    }

    /// The entry's bytes.
    pub fn encode(&self) -> Vec<u8> {
        let reply = &self.reply;
        let nodes: Vec<&str> = node_texts(reply.terms());
        let mut bytes = Vec::with_capacity(
            FIXED
                + RESULT * reply.results().len()
                + nodes.iter().map(|n| 4 + n.len()).sum::<usize>(),
        );
        bytes.extend_from_slice(&self.request.to_le_bytes());
        bytes.extend_from_slice(&reply.version().to_le_bytes());
        bytes.extend_from_slice(&reply.token().partition().to_le_bytes());
        bytes.extend_from_slice(&[0; 4]);
        bytes.extend_from_slice(&reply.token().position().to_le_bytes());
        bytes.extend_from_slice(&reply.token().epoch().get().to_le_bytes());
        // A reply's results and nodes are one transaction's operations', far below u32.
        bytes.extend_from_slice(&(reply.results().len() as u32).to_le_bytes());
        bytes.extend_from_slice(&(nodes.len() as u32).to_le_bytes());
        for result in reply.results() {
            bytes.extend_from_slice(&result.encode());
        }
        for node in nodes {
            bytes.extend_from_slice(&(node.len() as u32).to_le_bytes());
            bytes.extend_from_slice(node.as_bytes());
        }
        debug_assert!(bytes.len() >= FIXED);
        bytes
    }

    /// The entry `bytes` hold, stored at `key`.
    ///
    /// # Errors
    ///
    /// [`StoreError::corrupted`] naming `key` for bytes shorter than the fixed part, a padding byte not zero, a request
    /// 0, a version 0 or an odd one, an epoch past 2^42 − 1, results or a term table not filling the rest exactly, a
    /// result that does not read, or a result naming a node the term table does not hold.
    pub fn decode(key: &[u8], bytes: &[u8]) -> Result<Self, StoreError> {
        let corrupted = |reason: &str| StoreError::corrupted(key.to_vec(), reason.to_string());
        let Some((fixed, rest)) = bytes.split_first_chunk::<FIXED>() else {
            return Err(corrupted("a session entry shorter than its fixed part"));
        };
        let word = |at: usize| -> u64 {
            let mut le = [0; 8];
            if let Some(src) = fixed.get(at..at + 8) {
                le.copy_from_slice(src);
            }
            u64::from_le_bytes(le)
        };
        let half = |at: usize| -> u32 {
            let mut le = [0; 4];
            if let Some(src) = fixed.get(at..at + 4) {
                le.copy_from_slice(src);
            }
            u32::from_le_bytes(le)
        };
        let (request, version, partition, padding) = (word(0), word(8), half(16), half(20));
        let (position, epoch, results, nodes) = (word(24), word(32), half(40), half(44));
        if padding != 0 {
            return Err(corrupted("a session entry's padding is not zero"));
        }
        if request == 0 {
            return Err(corrupted("a session entry of request 0"));
        }
        if version == 0 || !version.is_multiple_of(2) {
            return Err(corrupted(
                "a session entry whose version is no commit version",
            ));
        }
        let epoch = Epoch::new(epoch)
            .map_err(|_| corrupted("a session entry whose epoch is past 2^42 − 1"))?;
        let (results, terms) =
            decode_results(rest, results as usize, nodes as usize).map_err(corrupted)?;
        let token = SessionToken::new(partition, position, epoch);
        Ok(Self::new(
            request,
            StoredReply::new(version, token, results, terms),
        ))
    }
}

/// The texts of `terms`' entries, in table order: each an IRI, as a results' term table holds them.
fn node_texts(terms: &TermTable) -> Vec<&str> {
    terms
        .terms()
        .iter()
        .filter_map(|t| match t {
            RequestTerm::Iri(text) => Some(text.as_str()),
            RequestTerm::BlankNode(_)
            | RequestTerm::Literal { .. }
            | RequestTerm::LanguageLiteral { .. }
            | RequestTerm::Triple { .. } => None,
        })
        .collect()
}

/// The `results` results and `nodes` node texts `bytes` hold, filling it exactly, each result naming a node the table
/// holds; the reason for anything else.
fn decode_results(
    bytes: &[u8],
    results: usize,
    nodes: usize,
) -> Result<(Vec<OperationResult>, TermTable), &'static str> {
    const FILL: &str = "a session entry whose results and nodes do not fill it";
    let Some((records, mut rest)) = results
        .checked_mul(RESULT)
        .and_then(|len| bytes.split_at_checked(len))
    else {
        return Err(FILL);
    };
    let (records, _) = records.as_chunks::<RESULT>();
    let mut terms = TermTable::new();
    for _ in 0..nodes {
        let Some((len, tail)) = rest.split_first_chunk::<4>() else {
            return Err(FILL);
        };
        let Some((text, tail)) = tail.split_at_checked(u32::from_le_bytes(*len) as usize) else {
            return Err(FILL);
        };
        let text = std::str::from_utf8(text).map_err(|_| "a session entry's node is not UTF-8")?;
        let _index: u32 = terms.iri(text);
        rest = tail;
    }
    if !rest.is_empty() {
        return Err(FILL);
    }
    let decoded = records
        .iter()
        .map(|r| decode_result(r, terms.len()))
        .collect::<Result<Vec<_>, _>>()?;
    Ok((decoded, terms))
}

/// The result `bytes` hold, §6.3's layout, naming a node below `nodes`.
fn decode_result(bytes: &[u8; RESULT], nodes: usize) -> Result<OperationResult, &'static str> {
    let [
        v0,
        v1,
        v2,
        v3,
        v4,
        v5,
        v6,
        v7,
        n0,
        n1,
        n2,
        n3,
        created,
        p0,
        p1,
        p2,
    ] = *bytes;
    if [p0, p1, p2] != [0; 3] || created > 1 {
        return Err("a session entry's result has padding or a created above 1");
    }
    let value = i64::from_le_bytes([v0, v1, v2, v3, v4, v5, v6, v7]);
    match u32::from_le_bytes([n0, n1, n2, n3]) {
        ABSENT if created == 0 && value != 0 => Ok(OperationResult::counter(value)),
        ABSENT if created == 0 => Ok(OperationResult::compare_and_set()),
        ABSENT => Err("a session entry's result creates no node"),
        node if value != 0 => Err(if (node as usize) < nodes {
            "a session entry's result names a node and a value"
        } else {
            "a session entry's result names a node the table does not hold"
        }),
        node if (node as usize) < nodes => Ok(OperationResult::for_node(node, created == 1)),
        _ => Err("a session entry's result names a node the table does not hold"),
    }
}
