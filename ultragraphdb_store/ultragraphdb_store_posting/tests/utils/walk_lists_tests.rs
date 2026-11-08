/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The read kernel's walk, step by step: what each answer of its visit does to where it goes and how it ends.

use std::num::NonZeroUsize;

use ultragraphdb_error_store::StoreError;
use ultragraphdb_store_posting::{
    PostingError, PostingErrorEnum, WalkBounds, WalkEnd, WalkStep, walk_lists,
};
use ultragraphdb_tool_bdd::{Context, TestError, TestResult};
use ultragraphdb_traits_store::{Item, Scan, Store, StoreSnapshot};

use crate::utils::helpers::{TestLists, commit, del_uid, key, new_lists, set_uid};

/// Lists holding `count` keys, in key order.
fn held(count: u64) -> TestResult<(TestLists, Vec<Vec<u8>>)> {
    let lists = new_lists();
    let keys: Vec<Vec<u8>> = (1..=count).map(|n| key("walk", n)).collect();
    for k in &keys {
        commit(&lists, k, &[set_uid(1)])?;
    }
    Ok((lists, keys))
}

/// The walk of every key of `keys`, `steps` answering each key's visit in turn, and the keys it visited.
fn walked(
    lists: &TestLists,
    keys: &[Vec<u8>],
    limit: Option<NonZeroUsize>,
    steps: &[WalkStep],
) -> TestResult<(WalkEnd, Vec<Vec<u8>>)> {
    walked_within(lists, keys, WalkBounds::rows(limit), steps)
}

/// As [`walked`], within `bounds`.
fn walked_within(
    lists: &TestLists,
    keys: &[Vec<u8>],
    bounds: WalkBounds,
    steps: &[WalkStep],
) -> TestResult<(WalkEnd, Vec<Vec<u8>>)> {
    let version = lists.applied_ts();
    let snapshot = lists.store().snapshot(version)?;
    let first = keys.first().context("a first key")?.clone();
    let mut past = keys.last().context("a last key")?.clone();
    past.push(0);
    let mut visited = Vec::new();
    let mut answers = steps.iter().copied();
    let end = walk_lists(
        &snapshot,
        &Scan::range(first, Some(past)),
        lists,
        version,
        bounds,
        |k, _| {
            visited.push(k.to_vec());
            Ok::<_, PostingError>(answers.next().unwrap_or(WalkStep::Row))
        },
    )?;
    Ok((end, visited))
}

#[test]
fn test_a_walk_visits_every_key_in_order_to_the_range_s_end() -> TestResult {
    let (lists, keys) = held(3)?;
    let (end, visited) = walked(&lists, &keys, None, &[])?;
    assert_eq!(end, WalkEnd::Exhausted);
    assert_eq!(visited, keys);
    Ok(())
}

#[test]
fn test_a_limit_stops_before_the_key_past_it_and_a_key_with_no_row_does_not_count() -> TestResult {
    let (lists, keys) = held(3)?;
    let (end, visited) = walked(
        &lists,
        &keys,
        Some(NonZeroUsize::MIN),
        &[WalkStep::Nothing, WalkStep::Row],
    )?;
    let [first, second, third] = keys.as_slice() else {
        return Err(TestError::new("three keys"));
    };
    assert_eq!(
        end,
        WalkEnd::Stopped {
            last: second.clone(),
            next: third.clone(),
        }
    );
    assert_eq!(visited, vec![first.clone(), second.clone()]);
    Ok(())
}

#[test]
fn test_a_limit_the_range_fills_exactly_ends_at_its_end() -> TestResult {
    let (lists, keys) = held(2)?;
    let limit = NonZeroUsize::new(keys.len()).context("a limit")?;
    let (end, _) = walked(&lists, &keys, Some(limit), &[])?;
    assert_eq!(end, WalkEnd::Exhausted);
    Ok(())
}

#[test]
fn test_a_pause_inside_a_key_makes_it_the_last_and_the_next() -> TestResult {
    let (lists, keys) = held(3)?;
    let (end, visited) = walked(
        &lists,
        &keys,
        None,
        &[WalkStep::Row, WalkStep::PausedInside],
    )?;
    let second = keys.get(1).context("a second key")?;
    assert_eq!(
        end,
        WalkEnd::Paused {
            last: Some(second.clone()),
            next: second.clone(),
        }
    );
    assert_eq!(visited.len(), 2, "it stops there");
    Ok(())
}

#[test]
fn test_a_pause_before_a_key_keeps_the_last_row() -> TestResult {
    let (lists, keys) = held(3)?;
    let (end, _) = walked(
        &lists,
        &keys,
        None,
        &[WalkStep::Row, WalkStep::Nothing, WalkStep::PausedBefore],
    )?;
    let [first, _, third] = keys.as_slice() else {
        return Err(TestError::new("three keys"));
    };
    assert_eq!(
        end,
        WalkEnd::Paused {
            last: Some(first.clone()),
            next: third.clone(),
        }
    );
    Ok(())
}

#[test]
fn test_a_pause_having_given_nothing_has_no_last() -> TestResult {
    let (lists, keys) = held(2)?;
    let (end, _) = walked(
        &lists,
        &keys,
        None,
        &[WalkStep::Nothing, WalkStep::PausedBefore],
    )?;
    assert_eq!(
        end,
        WalkEnd::Paused {
            last: None,
            next: keys.get(1).context("a second key")?.clone(),
        }
    );
    Ok(())
}

/// A walk handing back after its bound of keys giving nothing goes on just past the last it read, whatever rows came before.
#[test]
fn test_a_bound_of_keys_giving_nothing_hands_the_walk_back_past_the_last_read() -> TestResult {
    let (lists, keys) = held(4)?;
    let idle = NonZeroUsize::new(2).context("a bound")?;
    let bounds = WalkBounds::rows(None).with_idle(Some(idle));
    let (end, visited) = walked_within(
        &lists,
        &keys,
        bounds,
        &[WalkStep::Row, WalkStep::Nothing, WalkStep::Nothing],
    )?;
    let third = keys.get(2).context("a third key")?;
    let next = [third.as_slice(), &[0]].concat();
    assert_eq!(end.past_last(), Some(next.clone()), "what it covered");
    assert_eq!(end, WalkEnd::Yielded { next });
    assert_eq!(visited.len(), 3, "it reads no further");
    Ok(())
}

/// Past its rows a walk looks for a key holding something within that bound too, and then stops there as at the limit.
#[test]
fn test_past_its_rows_a_walk_stops_looking_at_the_bound() -> TestResult {
    let (lists, keys) = held(4)?;
    for k in keys.iter().skip(1) {
        commit(&lists, k, &[del_uid(1)])?;
    }
    let idle = NonZeroUsize::new(2).context("a bound")?;
    let bounds = WalkBounds::rows(Some(NonZeroUsize::MIN)).with_idle(Some(idle));
    let (end, visited) = walked_within(&lists, &keys, bounds, &[WalkStep::Row])?;
    let [first, _, third, _] = keys.as_slice() else {
        return Err(TestError::new("four keys"));
    };
    assert_eq!(
        end,
        WalkEnd::Stopped {
            last: first.clone(),
            next: third.clone(),
        }
    );
    assert_eq!(visited.len(), 1, "the rest are only looked at");
    let unbounded = WalkBounds::rows(Some(NonZeroUsize::MIN));
    let (whole, _) = walked_within(&lists, &keys, unbounded, &[WalkStep::Row])?;
    assert_eq!(whole, WalkEnd::Exhausted, "with no bound it reads them all");
    Ok(())
}

/// A snapshot whose every scan fails at its first item with `failure`; its other reads are `inner`'s.
struct FailingScan<T> {
    inner: T,
    failure: StoreError,
}

impl<T: StoreSnapshot> StoreSnapshot for FailingScan<T> {
    type Scan = std::vec::IntoIter<Result<Item, StoreError>>;

    fn read_ts(&self) -> u64 {
        self.inner.read_ts()
    }

    fn get(&self, key: &[u8]) -> Result<Option<Item>, StoreError> {
        self.inner.get(key)
    }

    fn get_many<K: AsRef<[u8]>>(&self, keys: &[K]) -> Result<Vec<Option<Item>>, StoreError> {
        self.inner.get_many(keys)
    }

    fn scan(&self, _scan: &Scan) -> Self::Scan {
        vec![Err(self.failure.clone())].into_iter()
    }

    fn get_versionless(&self, key: &[u8]) -> Result<Option<Vec<u8>>, StoreError> {
        self.inner.get_versionless(key)
    }
}

/// A scan the store fails ends the walk with the store's error, before any key is visited.
#[test]
fn test_a_failed_scan_ends_the_walk_with_the_store_s_error() -> TestResult {
    let (lists, keys) = held(1)?;
    let version = lists.applied_ts();
    let failure = StoreError::read_failed(keys.concat(), "a fault".into());
    let snapshot = FailingScan {
        inner: lists.store().snapshot(version)?,
        failure: failure.clone(),
    };
    let mut visited = 0usize;
    let walked = walk_lists(
        &snapshot,
        &Scan::prefix(Vec::new()),
        &lists,
        version,
        WalkBounds::rows(None),
        |_, _| {
            visited += 1;
            Ok::<_, PostingError>(WalkStep::Row)
        },
    );
    let error = walked.err().context("an error")?;
    assert_eq!(error.0, PostingErrorEnum::Store(failure));
    assert_eq!(visited, 0);
    Ok(())
}
