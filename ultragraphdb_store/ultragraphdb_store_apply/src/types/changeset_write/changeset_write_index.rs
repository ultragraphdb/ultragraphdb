/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::collections::BTreeSet;

use oxrdf::LiteralRef;
use ultragraphdb_core_codec::Posting;
use ultragraphdb_core_ids::SystemId;
use ultragraphdb_core_keys::{CountDirection, Graph, QuadKey};
use ultragraphdb_core_mapping::PredicateDeclaration;
use ultragraphdb_core_terms::EncodedTerm;
use ultragraphdb_store_posting::{CachedList, DEL, SET};
use ultragraphdb_store_quads::object_posting;
use ultragraphdb_traits_blocks::BlockSource;
use ultragraphdb_traits_store::{ListCache, Store};

use crate::errors::changeset_error::ChangesetError;
use crate::types::changeset_write::ChangesetWrite;
use crate::utils::literal_tokens::literal_tokens;

impl<S, C, B> ChangesetWrite<'_, S, C, B>
where
    S: Store,
    C: ListCache<Value = CachedList>,
    B: BlockSource,
{
    /// The value-index tokens of `literal` under each tokenizer `declaration` names, each with its tokenizer's
    /// identifier.
    pub(crate) fn tokens(
        &self,
        declaration: &PredicateDeclaration,
        literal: LiteralRef<'_>,
    ) -> Result<BTreeSet<(u8, Vec<u8>)>, ChangesetError> {
        Ok(literal_tokens(declaration.tokenizers(), literal)?)
    }

    /// Adds the subject to, or removes it from, the value-index list of `token`, without reading it.
    pub(crate) fn index(
        &mut self,
        graph: Graph,
        subject: SystemId,
        predicate: SystemId,
        tokenizer: u8,
        token: Vec<u8>,
        add: bool,
    ) {
        let key = QuadKey::ValueIndex {
            namespace: self.namespace(),
            graph,
            predicate,
            tokenizer,
            token,
        }
        .to_bytes();
        let posting = Posting::default()
            .with_uid(subject.to_u128())
            .with_op(if add { SET } else { DEL });
        self.write
            .request()
            .get_from_delta(&key)
            .add_posting(posting, false);
    }

    /// The number of objects the subject holds for the predicate, and of subjects holding the object: reads of the
    /// subject's list and of the object's reverse list.
    pub(crate) fn counts(
        &mut self,
        graph: Graph,
        subject: SystemId,
        predicate: SystemId,
        object: &EncodedTerm,
    ) -> Result<(u32, u32), ChangesetError> {
        let objects = self.write.objects(graph, subject, predicate)?.len();
        let subjects = self.write.subjects(graph, predicate, object)?.len();
        Ok((count(objects), count(subjects)))
    }

    /// Moves the subject between the forward count keys `objects` names, from its first count to its second, and the
    /// object between the reverse count keys `subjects` names. A vector has no reverse count. The move is recorded, and
    /// [`ChangesetWrite::write_counts`] writes each subject's and object's moves as one, so that a count moved and moved
    /// back writes no key and a count passed through writes none.
    pub(crate) fn move_counts(
        &mut self,
        graph: Graph,
        subject: SystemId,
        predicate: SystemId,
        object: &EncodedTerm,
        objects: (u32, u32),
        subjects: (u32, u32),
    ) {
        let fp = self.write.quads().fingerprint();
        self.count_move(
            (graph, predicate, CountDirection::Forward),
            subject.to_be_bytes().to_vec(),
            objects,
            Posting::default().with_uid(subject.to_u128()),
        );
        if !object.is_vector() {
            self.count_move(
                (graph, predicate, CountDirection::Reverse),
                object.as_bytes().to_vec(),
                subjects,
                object_posting(object, SET, &[], fp),
            );
        }
    }

    /// Records a move of the member `identity` names, whose posting is `posting`, between the count keys of `list`:
    /// from the count it held before the write's first move to the count of its last.
    fn count_move(
        &mut self,
        list: (Graph, SystemId, CountDirection),
        identity: Vec<u8>,
        (from, to): (u32, u32),
        posting: Posting,
    ) {
        let (graph, predicate, direction) = list;
        let moved: &mut (u32, u32, Posting) = self
            .counts
            .entry((graph, predicate, direction, identity))
            .or_insert((from, to, posting));
        moved.1 = to;
    }

    /// Writes every count the write moved: each subject and object leaves the count key of the count it held before
    /// the write and joins the key of the count it holds after it. A count of zero has no key.
    pub(crate) fn write_counts(&mut self) {
        for ((graph, predicate, direction, _), (from, to, posting)) in
            std::mem::take(&mut self.counts)
        {
            self.count(graph, predicate, direction, (from, to), |op| {
                posting.clone().with_op(op)
            });
        }
    }

    fn count(
        &mut self,
        graph: Graph,
        predicate: SystemId,
        direction: CountDirection,
        (from, to): (u32, u32),
        posting: impl Fn(u32) -> Posting,
    ) {
        if from == to {
            return;
        }
        let namespace = self.namespace();
        let key = |count| {
            QuadKey::CountIndex {
                namespace,
                graph,
                predicate,
                direction,
                count,
            }
            .to_bytes()
        };
        if from > 0 {
            self.write
                .request()
                .get_from_delta(&key(from))
                .add_posting(posting(DEL), false);
        }
        if to > 0 {
            self.write
                .request()
                .get_from_delta(&key(to))
                .add_posting(posting(SET), false);
        }
    }
}

/// A list's length as a count key's count, which saturates.
fn count(len: usize) -> u32 {
    u32::try_from(len).unwrap_or(u32::MAX)
}
