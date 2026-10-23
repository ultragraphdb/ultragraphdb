/*
 * SPDX-FileCopyrightText: © 2017-2026 Istari Digital, Inc.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from Dgraph v25.5.0, posting/list.go (findPosting, findPostingWithItr, GetLength, immutableLayerLen,
 * getPostingAndLength, getPostingAndLengthNoSort, Value, ValueFor, PostingFor, ValueForTag, postingForLangs,
 * postingForTag, findValue, StaticValue, findStaticValue, AllValues, AllUntaggedValues, GetLangTags). Changed for
 * ultragraphdb: translated to Rust; where Dgraph panics on a part it cannot read (`GetLength`,
 * `getPostingAndLengthNoSort`), the port returns the error; whether an index is being rebuilt is the caller's,
 * where Dgraph asks its global schema.
 * Copyright (c) 2026. Marvin Hansen.
 */

use std::ops::ControlFlow;

use ultragraphdb_common_utils::go_farm::fingerprint64;
use ultragraphdb_core_types::{TypeId, Val, Value};
use ultragraphdb_proto_pb::pb::{Posting, PostingList, posting};

use crate::errors::posting_error::PostingError;
use crate::types::list::List;
use crate::types::mutable_layer::Rank;
use crate::types::p_iterator::{PIterator, PPosting};

impl List {
    /// The posting of `uid` at `read_ts`, from the deltas or the immutable layer: Dgraph's `FindPosting`.
    pub fn find_posting(&self, read_ts: u64, uid: u64) -> Result<Option<Posting>, PostingError> {
        let (search_further, found) = self.mutation.find_posting(read_ts, uid);
        if let Some(p) = found {
            return Ok(Some(p.clone()));
        }
        if !search_further {
            return Ok(None);
        }
        // Iteration starts after its argument, so it seeks the uid before.
        let mut pitr = PIterator::seek(self, uid.wrapping_sub(1), 0)?;
        if !pitr.valid() {
            return Ok(None);
        }
        Ok(match pitr.posting() {
            PPosting::Found(p) if p.uid == uid => Some(p.clone()),
            PPosting::Uid(u) if u == uid => Some(uid_posting(uid)),
            _ => None,
        })
    }

    /// The list's length at `read_ts` without iterating it: what the deltas add, plus the immutable layer's uids
    /// unless a delete-all hides them: Dgraph's `GetLength`.
    pub fn get_length(&self, read_ts: u64) -> Result<i64, PostingError> {
        let mut length = self.mutation.list_len(read_ts);
        if self.mutation.populate_delete_all(read_ts) == 0 {
            length += self.immutable_layer_len()? as i64;
        }
        Ok(length)
    }

    /// The number of uids in the immutable layer, every part counted.
    fn immutable_layer_len(&self) -> Result<usize, PostingError> {
        // Seeking checks that every part is held.
        PIterator::seek(self, 0, 0)?;
        Ok(self
            .immutable_parts()
            .iter()
            .filter_map(|p| p.pack.as_ref())
            .map(ultragraphdb_core_codec::exact_len)
            .sum())
    }

    /// The number of postings above `after_uid` at `read_ts`, and the posting of `uid` among them, by iterating:
    /// Dgraph's `getPostingAndLength`, which reports a length of -1 when the iteration fails.
    pub fn get_posting_and_length(
        &self,
        read_ts: u64,
        after_uid: u64,
        uid: u64,
    ) -> (i64, Option<Posting>) {
        let mut count = 0i64;
        let mut found = None;
        let result = self.iterate(read_ts, after_uid, |p| {
            if p.uid == uid {
                found = Some(p.clone());
            }
            count += 1;
            Ok::<_, PostingError>(ControlFlow::Continue(()))
        });
        match result {
            Ok(()) => (count, found),
            Err(_) => (-1, None),
        }
    }

    /// The list's length and the posting of `uid` at `read_ts` without sorting the deltas: Dgraph's
    /// `getPostingAndLengthNoSort`. While an index is being rebuilt, `indexing` is true and both are found by
    /// iterating, as [`List::get_posting_and_length`] does.
    pub fn get_posting_and_length_no_sort(
        &self,
        read_ts: u64,
        after_uid: u64,
        uid: u64,
        indexing: bool,
    ) -> Result<(i64, Option<Posting>), PostingError> {
        if indexing {
            return Ok(self.get_posting_and_length(read_ts, after_uid, uid));
        }
        let length = self.get_length(read_ts)?;
        Ok((length, self.find_posting(read_ts, uid)?))
    }

    /// The untagged value at `read_ts`: Dgraph's `Value`.
    ///
    /// # Errors
    ///
    /// [`PostingError::NoValue`] when the list holds none.
    pub fn value(&self, read_ts: u64) -> Result<Val, PostingError> {
        self.find_posting(read_ts, u64::MAX)?
            .map(|p| value_to_types_val(&p))
            .ok_or_else(PostingError::NoValue)
    }

    /// The value in the first of `langs` the list holds a value for, `.` standing for any language; with no
    /// languages, the untagged value: Dgraph's `ValueFor`.
    pub fn value_for(&self, read_ts: u64, langs: &[String]) -> Result<Val, PostingError> {
        self.posting_for(read_ts, langs)
            .map(|p| value_to_types_val(&p))
    }

    /// The posting [`List::value_for`] reads: Dgraph's `PostingFor`.
    pub fn posting_for(&self, read_ts: u64, langs: &[String]) -> Result<Posting, PostingError> {
        let mut any = false;
        for lang in langs {
            if lang == "." {
                any = true;
                break;
            }
            if let Ok(p) = self.posting_for_tag(read_ts, lang) {
                return Ok(p);
            }
        }
        if (any || langs.is_empty())
            && let Some(p) = self.find_posting(read_ts, u64::MAX)?
        {
            return Ok(p);
        }
        if any {
            // As a last resort, the value with the smallest language uid.
            let mut found = None;
            self.iterate(read_ts, 0, |p| {
                if p.posting_type == posting::PostingType::ValueLang as i32 {
                    found = Some(p.clone());
                    return Ok::<_, PostingError>(ControlFlow::Break(()));
                }
                Ok(ControlFlow::Continue(()))
            })?;
            if let Some(p) = found {
                return Ok(p);
            }
        }
        Err(PostingError::NoValue())
    }

    /// The value tagged `tag`: Dgraph's `ValueForTag`.
    pub fn value_for_tag(&self, read_ts: u64, tag: &str) -> Result<Val, PostingError> {
        self.posting_for_tag(read_ts, tag)
            .map(|p| value_to_types_val(&p))
    }

    fn posting_for_tag(&self, read_ts: u64, tag: &str) -> Result<Posting, PostingError> {
        self.find_posting(read_ts, fingerprint64(tag.as_bytes()))?
            .ok_or_else(PostingError::NoValue)
    }

    /// Every value at `read_ts`, in uid order: Dgraph's `AllValues`.
    pub fn all_values(&self, read_ts: u64) -> Result<Vec<Val>, PostingError> {
        self.collect(read_ts, |p| Some(value_to_types_val(p)))
    }

    /// Every value without a language at `read_ts`: Dgraph's `AllUntaggedValues`.
    pub fn all_untagged_values(&self, read_ts: u64) -> Result<Vec<Val>, PostingError> {
        self.collect(read_ts, |p| {
            p.lang_tag.is_empty().then(|| value_to_types_val(p))
        })
    }

    /// The language tag of every posting at `read_ts`: Dgraph's `GetLangTags`.
    pub fn get_lang_tags(&self, read_ts: u64) -> Result<Vec<String>, PostingError> {
        self.collect(read_ts, |p| {
            Some(String::from_utf8_lossy(&p.lang_tag).into_owned())
        })
    }

    fn collect<T>(
        &self,
        read_ts: u64,
        mut pick: impl FnMut(&Posting) -> Option<T>,
    ) -> Result<Vec<T>, PostingError> {
        let mut out = Vec::new();
        self.iterate(read_ts, 0, |p| {
            out.extend(pick(p));
            Ok::<_, PostingError>(ControlFlow::Continue(()))
        })?;
        Ok(out)
    }

    /// The posting list holding the list's single value at `read_ts`, unmerged: the request's own scratch layer,
    /// else the newest layer stacked by the requests before it in its batch, both at the request's read timestamp,
    /// else the newest delta at or below `read_ts`, else the complete version when it holds postings. Dgraph's
    /// `StaticValue`, which serves scalar reads and the vector index.
    pub fn static_value(&self, read_ts: u64) -> Option<&PostingList> {
        if let Some(current) = self.mutation.current_entries()
            && self.mutation.read_ts() == read_ts
        {
            return Some(current);
        }
        if let Some(stacked) = self.mutation.stacked().last()
            && self.mutation.read_ts() == read_ts
        {
            return Some(stacked.as_ref());
        }
        if self.max_ts <= read_ts
            && let Some((_, last)) = self.mutation.committed().iter().next_back()
        {
            return Some(last.as_ref());
        }
        let (_, layers) = self.mutation.live(read_ts);
        let newest = layers
            .into_iter()
            .filter(|(rank, _)| matches!(rank, Rank::Committed(_)))
            .max_by_key(|(_, pl)| pl.commit_ts)
            .map(|(_, pl)| pl);
        newest.or_else(|| (!self.plist.postings.is_empty()).then_some(self.plist.as_ref()))
    }
}

/// The uid posting the immutable layer yields for a uid it holds no posting for.
fn uid_posting(uid: u64) -> Posting {
    Posting {
        uid,
        val_type: posting::ValType::Uid as i32,
        ..Posting::default()
    }
}

/// A posting's value as stored, untyped: Dgraph's `valueToTypesVal`.
pub(crate) fn value_to_types_val(p: &Posting) -> Val {
    Val::new(TypeId::new(p.val_type), Value::Bytes(p.value.clone()))
}
