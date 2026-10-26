/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::collections::BTreeSet;

use oxrdf::LiteralRef;
use ultragraphdb_core_keys::{CountDirection, QuadKey};
use ultragraphdb_core_mapping::PredicateDeclaration;
use ultragraphdb_core_terms::EncodedTerm;
use ultragraphdb_core_tok::{get_tokenizer, get_tokenizer_for_lang};
use ultragraphdb_proto_pb::pb::Posting;
use ultragraphdb_store_posting::{CachedList, DEL, SET};
use ultragraphdb_store_quads::object_posting;
use ultragraphdb_traits_store::{ListCache, Store};

use crate::errors::changeset_error::ChangesetError;
use crate::types::changeset_write::ChangesetWrite;
use crate::utils::token_values::token_value;

impl<S, C> ChangesetWrite<'_, S, C>
where
    S: Store,
    C: ListCache<Value = CachedList>,
{
    /// The value-index tokens of `literal` under each tokenizer `declaration` names, each with its tokenizer's
    /// identifier: computed from the literal's value, by its language's tokenizer when it is tagged. A literal of a
    /// datatype a tokenizer does not take gives that tokenizer no token.
    pub(crate) fn tokens(
        &self,
        declaration: &PredicateDeclaration,
        literal: LiteralRef<'_>,
    ) -> Result<BTreeSet<(u8, Vec<u8>)>, ChangesetError> {
        let lang = literal.language().unwrap_or_default();
        let mut tokens = BTreeSet::new();
        for name in declaration.tokenizers() {
            let (Some(tokenizer), Some(value)) = (get_tokenizer(name), token_value(literal, name))
            else {
                continue;
            };
            let tokenizer = get_tokenizer_for_lang(&tokenizer, lang);
            for token in tokenizer.tokens(&value)? {
                tokens.insert((tokenizer.identifier(), token));
            }
        }
        Ok(tokens)
    }

    /// Adds the subject to, or removes it from, the value-index list of `token`, without reading it.
    pub(crate) fn index(
        &mut self,
        graph: u64,
        subject: u64,
        predicate: u64,
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
        let posting = Posting {
            uid: subject,
            op: if add { SET } else { DEL },
            ..Posting::default()
        };
        self.write
            .request()
            .get_from_delta(&key)
            .add_posting(posting, false);
    }

    /// The number of objects the subject holds for the predicate, and of subjects holding the object: reads of the
    /// subject's list and of the object's reverse list.
    pub(crate) fn counts(
        &mut self,
        graph: u64,
        subject: u64,
        predicate: u64,
        object: &EncodedTerm,
    ) -> Result<(u32, u32), ChangesetError> {
        let objects = self.write.objects(graph, subject, predicate)?.len();
        let subjects = self.write.subjects(graph, predicate, object)?.len();
        Ok((count(objects), count(subjects)))
    }

    /// Moves the subject between the forward count keys `objects` names, from its first count to its second, and the
    /// object between the reverse count keys `subjects` names; a count of zero has no key. A vector has no reverse
    /// count.
    pub(crate) fn move_counts(
        &mut self,
        graph: u64,
        subject: u64,
        predicate: u64,
        object: &EncodedTerm,
        objects: (u32, u32),
        subjects: (u32, u32),
    ) {
        let fp = self.write.quads().fingerprint();
        let subject_posting = |op| Posting {
            uid: subject,
            op,
            ..Posting::default()
        };
        self.count(
            graph,
            predicate,
            CountDirection::Forward,
            objects,
            subject_posting,
        );
        if !object.is_vector() {
            self.count(graph, predicate, CountDirection::Reverse, subjects, |op| {
                object_posting(object, op, &[], fp)
            });
        }
    }

    fn count(
        &mut self,
        graph: u64,
        predicate: u64,
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
