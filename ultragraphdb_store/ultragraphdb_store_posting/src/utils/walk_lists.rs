/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::num::NonZeroUsize;

use ultragraphdb_traits_store::{Scan, StoreSnapshot};

use crate::errors::posting_error::PostingError;
use crate::traits::list_source::ListSource;
use crate::types::walk_end::WalkEnd;

/// The read kernel's one walk: the keys of `range` the snapshot holds merged with those `source` adds, lowest first and each
/// once, every one given to `visit`, which says whether it made a row. It stops before the key past `limit` rows; with no limit
/// it reads the range through.
///
/// # Errors
///
/// The store's error; whatever `visit` fails with.
pub fn walk_lists<T, L, E, V>(
    snapshot: &T,
    range: &Scan,
    source: &L,
    limit: Option<NonZeroUsize>,
    mut visit: V,
) -> Result<WalkEnd, E>
where
    T: StoreSnapshot,
    L: ListSource,
    E: From<PostingError>,
    V: FnMut(&[u8], &L) -> Result<bool, E>,
{
    let mut held = snapshot.scan(range).peekable();
    let mut added = source.added_keys(range.start(), range.end()).peekable();
    let mut rows = 0usize;
    let mut last = None;
    loop {
        let from_store = match held.peek() {
            Some(Ok(item)) => Some(item.key().to_vec()),
            Some(Err(_)) => match held.next() {
                Some(Err(error)) => return Err(PostingError::from(error).into()),
                Some(Ok(_)) | None => None,
            },
            None => None,
        };
        let key = match (from_store, added.peek().copied()) {
            (None, None) => return Ok(WalkEnd::Exhausted),
            (Some(store), None) => {
                let _taken = held.next();
                store
            }
            (None, Some(own)) => {
                let _taken = added.next();
                own.to_vec()
            }
            (Some(store), Some(own)) => {
                if store.as_slice() <= own {
                    let _taken = held.next();
                    if store.as_slice() == own {
                        let _same = added.next();
                    }
                    store
                } else {
                    let _taken = added.next();
                    own.to_vec()
                }
            }
        };
        if let (Some(most), Some(row)) = (limit, last.as_ref())
            && rows == most.get()
        {
            return Ok(WalkEnd::Stopped {
                last: Vec::clone(row),
                next: key,
            });
        }
        if visit(&key, source)? {
            rows += 1;
            last = Some(key);
        }
    }
}
