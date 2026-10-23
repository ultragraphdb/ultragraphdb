/*
 * SPDX-FileCopyrightText: © 2017-2026 Istari Digital, Inc.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from Dgraph v25.5.0, tok/hnsw/persistent_hnsw_test.go (TestCandidateHeapMinHeap, TestCandidateHeapMaxHeap,
 * TestCandidateHeapPushPop). Changed for ultragraphdb: translated to Rust; added: the leaf `pop_last` drops is Go's.
 * Copyright (c) 2026. Marvin Hansen.
 */

use ultragraphdb_store_hnsw::{CandidateHeap, PersistentHeapElement};

fn elements() -> Vec<PersistentHeapElement<f64>> {
    [(0.5, 1), (0.1, 2), (0.9, 3), (0.3, 4)]
        .into_iter()
        .map(|(v, i)| PersistentHeapElement::new(v, i, false))
        .collect()
}

fn drain(mut heap: CandidateHeap<f64>) -> Vec<f64> {
    std::iter::from_fn(|| heap.pop().map(|e| e.value())).collect()
}

#[test]
fn test_candidate_heap_min_heap() {
    let heap = CandidateHeap::build(elements(), false);
    assert_eq!(heap.len(), 4);
    assert_eq!(drain(heap), vec![0.1, 0.3, 0.5, 0.9]);
}

#[test]
fn test_candidate_heap_max_heap() {
    let heap = CandidateHeap::build(elements(), true);
    assert_eq!(drain(heap), vec![0.9, 0.5, 0.3, 0.1]);
}

#[test]
fn test_candidate_heap_push_pop() {
    for (max, expected) in [(false, [0.2, 0.5, 0.8]), (true, [0.8, 0.5, 0.2])] {
        let mut heap = CandidateHeap::build(Vec::new(), max);
        assert!(heap.is_empty());
        for (v, i) in [(0.5, 1), (0.2, 2), (0.8, 3)] {
            heap.push(PersistentHeapElement::new(v, i, false));
        }
        assert_eq!(drain(heap), expected.to_vec());
    }
}

/// Go's `PopLast` removes the heap array's last element, a leaf, not the worst: after building [0.5, 0.1, 0.9, 0.3]
/// into a min-heap, Go's array is [0.1, 0.3, 0.9, 0.5], so 0.5 goes and 0.9 stays.
#[test]
fn test_pop_last_drops_gos_leaf() {
    let mut heap = CandidateHeap::build(elements(), false);
    heap.pop_last();
    assert_eq!(drain(heap), vec![0.1, 0.3, 0.9]);
    let mut empty = CandidateHeap::<f64>::build(Vec::new(), false);
    empty.pop_last();
    assert!(
        empty.pop().is_none(),
        "popping an empty heap is no candidate, where Go panics"
    );
}
