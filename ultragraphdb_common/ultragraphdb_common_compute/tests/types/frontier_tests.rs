/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_common_compute::Frontier;

/// Pops `heads`' streams with each pushed once: their order by the rows they hold, and one pop past them, which a frontier
/// emptied answers with nothing.
fn drained(heads: &[u32]) -> Vec<usize> {
    let order = |a: usize, b: usize| heads.get(a).cmp(&heads.get(b));
    let mut frontier = Frontier::with_capacity(heads.len());
    (0..heads.len()).for_each(|stream| frontier.push(stream, order));
    std::iter::from_fn(|| frontier.pop(order))
        .take(heads.len() + 1)
        .collect()
}

/// The rows of `heads` in the order [`drained`] pops their streams.
fn rows_popped(heads: &[u32]) -> Vec<u32> {
    drained(heads)
        .iter()
        .filter_map(|stream| heads.get(*stream).copied())
        .collect()
}

#[test]
fn test_streams_come_out_lowest_row_first() {
    let heads = [5, 1, 9, 3, 7, 2, 8, 4, 6, 0];
    let popped = drained(&heads);
    let rows: Vec<u32> = popped
        .iter()
        .filter_map(|stream| heads.get(*stream).copied())
        .collect();
    let mut sorted = heads.to_vec();
    sorted.sort_unstable();
    assert_eq!(rows, sorted);
}

#[test]
fn test_an_empty_frontier_pops_nothing() {
    let mut frontier = Frontier::default();
    assert_eq!(frontier.pop(|a, b| a.cmp(&b)), None);
    assert!(frontier.streams().is_empty());
}

#[test]
fn test_a_stream_pushed_back_takes_its_place_by_its_next_row() {
    let mut heads = [2, 4];
    let mut frontier = Frontier::with_capacity(2);
    (0..2).for_each(|stream| frontier.push(stream, |a, b| heads.get(a).cmp(&heads.get(b))));
    assert_eq!(
        frontier.pop(|a, b| heads.get(a).cmp(&heads.get(b))),
        Some(0)
    );
    heads = [5, 4];
    frontier.push(0, |a, b| heads.get(a).cmp(&heads.get(b)));
    assert_eq!(
        frontier.pop(|a, b| heads.get(a).cmp(&heads.get(b))),
        Some(1)
    );
    assert_eq!(frontier.streams(), [0]);
}

/// Sixteen streams pushed highest row first, each climbing past every parent on its way up: the frontier still pops them lowest
/// row first, each once.
#[test]
fn test_streams_pushed_highest_first_come_out_lowest_first() {
    let heads: Vec<u32> = (0..16).rev().collect();
    let mut sorted = heads.clone();
    sorted.sort_unstable();
    assert_eq!(rows_popped(&heads), sorted);
}

/// Each pop takes the lowest row of those left, the one moved up in its place sinking below a lower child first.
#[test]
fn test_each_pop_takes_the_lowest_row_left() {
    let heads = [1, 2, 3, 4];
    let order = |a: usize, b: usize| heads.get(a).cmp(&heads.get(b));
    let mut frontier = Frontier::with_capacity(heads.len());
    (0..heads.len()).for_each(|stream| frontier.push(stream, order));
    for stream in 0..heads.len() {
        assert_eq!(frontier.pop(order), Some(stream));
    }
    assert_eq!(frontier.pop(order), None);
}
