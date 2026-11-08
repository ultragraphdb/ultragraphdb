/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::iter::Peekable;
use std::num::NonZeroUsize;

use ultragraphdb_error_store::StoreError;
use ultragraphdb_traits_store::{Item, Scan, StoreSnapshot};

use crate::errors::posting_error::PostingError;
use crate::traits::list_source::ListSource;
use crate::types::walk_bounds::WalkBounds;
use crate::types::walk_end::WalkEnd;
use crate::types::walk_step::WalkStep;

/// The read kernel's one walk: the keys of `range` the snapshot holds merged with those `source` adds, lowest first and each
/// once, every one given to `visit`, which says what it made of the key. Past `bounds`' rows it stops before the next key whose
/// list holds anything at `version`, so a range holding exactly that many rows is exhausted, or before the key its bound of keys
/// giving nothing runs out at; it stops too where `visit` pauses it. Below its rows, that bound hands the walk back with all it
/// read covered. With none of these it reads the range through.
///
/// # Errors
///
/// The store's error; whatever `visit` fails with.
pub fn walk_lists<T, L, E, V>(
    snapshot: &T,
    range: &Scan,
    source: &L,
    version: u64,
    bounds: WalkBounds,
    mut visit: V,
) -> Result<WalkEnd, E>
where
    T: StoreSnapshot,
    L: ListSource,
    E: From<PostingError>,
    V: FnMut(&[u8], &L) -> Result<WalkStep, E>,
{
    let mut held = snapshot.scan(range).peekable();
    let mut added = source.added_keys(range.start(), range.end()).peekable();
    let (mut rows, mut idle) = (0usize, 0usize);
    let idle_most = bounds.most_idle().map_or(usize::MAX, NonZeroUsize::get);
    let mut last = None;
    // Ends: each turn takes the next key of `held` and `added`, both finite, or returns.
    loop {
        let Some(key) = next_key::<_, _, E>(&mut held, &mut added)? else {
            return Ok(WalkEnd::Exhausted);
        };
        if let (Some(most), Some(row)) = (bounds.most_rows(), last.as_ref())
            && rows == most.get()
        {
            if source.with_list(&key, version, |list| list.is_empty(version, 0))?? {
                idle += 1;
                if idle < idle_most {
                    continue;
                }
            }
            return Ok(WalkEnd::Stopped {
                last: Vec::clone(row),
                next: key,
            });
        }
        match visit(&key, source)? {
            WalkStep::Nothing => {
                idle += 1;
                if idle == idle_most {
                    let mut next = key;
                    next.push(0);
                    return Ok(WalkEnd::Yielded { next });
                }
            }
            WalkStep::Row => {
                rows += 1;
                last = Some(key);
            }
            WalkStep::PausedInside => {
                return Ok(WalkEnd::Paused {
                    last: Some(key.clone()),
                    next: key,
                });
            }
            WalkStep::PausedBefore => return Ok(WalkEnd::Paused { last, next: key }),
        }
    }
}

/// The lowest key `held` and `added` hold next, taken from both when both hold it.
fn next_key<'a, H, A, E>(
    held: &mut Peekable<H>,
    added: &mut Peekable<A>,
) -> Result<Option<Vec<u8>>, E>
where
    H: Iterator<Item = Result<Item, StoreError>>,
    A: Iterator<Item = &'a [u8]>,
    E: From<PostingError>,
{
    let from_store = match held.peek() {
        Some(Ok(item)) => Some(item.key().to_vec()),
        Some(Err(_)) => match held.next() {
            Some(Err(error)) => return Err(PostingError::from(error).into()),
            Some(Ok(_)) | None => None,
        },
        None => None,
    };
    Ok(match (from_store, added.peek().copied()) {
        (None, None) => None,
        (Some(store), None) => {
            let _taken = held.next();
            Some(store)
        }
        (None, Some(own)) => {
            let _taken = added.next();
            Some(own.to_vec())
        }
        (Some(store), Some(own)) => {
            if store.as_slice() <= own {
                let _taken = held.next();
                if store.as_slice() == own {
                    let _same = added.next();
                }
                Some(store)
            } else {
                let _taken = added.next();
                Some(own.to_vec())
            }
        }
    })
}
