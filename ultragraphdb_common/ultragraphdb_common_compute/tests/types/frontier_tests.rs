/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_common_compute::Frontier;

/// Pops `heads`' streams with each pushed once: their order by the rows they hold.
fn drained(heads: &[u32]) -> Vec<usize> {
    let order = |a: usize, b: usize| heads.get(a).cmp(&heads.get(b));
    let mut frontier = Frontier::with_capacity(heads.len());
    (0..heads.len()).for_each(|stream| frontier.push(stream, order));
    std::iter::from_fn(|| frontier.pop(order)).collect()
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
