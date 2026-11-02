/*
 * SPDX-FileCopyrightText: © 2017-2026 Istari Digital, Inc.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from Dgraph v25.5.0, algo/heap_test.go. Changed for ultragraphdb: translated to Rust over system IDs, Go's
 * numbers standing for IDs as in `uidlist_tests.rs`.
 * Copyright (c) 2026. Marvin Hansen.
 */

use ultragraphdb_core_algo::{Elem, IdHeap};
use ultragraphdb_tool_bdd::{Context, TestResult};

use crate::uidlist_tests::{idn, nums};

fn top(h: &IdHeap) -> TestResult<u64> {
    let id = h.get(0).context("the heap is not empty")?.val();
    nums(&[id]).first().copied().context("a number")
}

/// The number an element's ID stands for.
fn num(e: &Elem) -> u64 {
    e.val().sequence().get() - 1
}

#[test]
fn test_push() -> TestResult {
    let mut h = IdHeap::new(Vec::new());

    h.push(Elem::new(idn(5)?, 0));
    h.push(Elem::new(idn(3)?, 0));
    h.push(Elem::new(idn(4)?, 0));

    assert_eq!(h.len(), 3);
    assert_eq!(top(&h)?, 3);

    assert_eq!(
        h.set(0, Elem::new(idn(10)?, 0)),
        Some(Elem::new(idn(3)?, 0))
    );
    h.fix(0);
    assert_eq!(top(&h)?, 4);

    assert_eq!(
        h.set(0, Elem::new(idn(11)?, 0)),
        Some(Elem::new(idn(4)?, 0))
    );
    h.fix(0);
    assert_eq!(top(&h)?, 5);

    let e = h.pop().context("an element")?;
    assert_eq!(num(&e), 5);

    let second = h.pop().context("an element")?;
    assert_eq!(num(&second), 10);

    let third = h.pop().context("an element")?;
    assert_eq!(num(&third), 11);

    assert_eq!(h.len(), 0);
    assert!(h.is_empty());
    assert!(h.pop().is_none());
    Ok(())
}

#[test]
fn new_orders_its_elements() -> TestResult {
    let mut h = IdHeap::new(
        [9, 2, 7, 2, 5]
            .iter()
            .enumerate()
            .map(|(i, &v)| Ok(Elem::new(idn(v)?, i)))
            .collect::<TestResult<_>>()?,
    );
    let mut out = Vec::new();
    while let Some(e) = h.pop() {
        out.push((num(&e), e.list_idx()));
    }
    assert_eq!(
        out.iter().map(|e| e.0).collect::<Vec<_>>(),
        vec![2, 2, 5, 7, 9]
    );
    assert_eq!(out.get(2), Some(&(5, 4)));
    Ok(())
}

#[test]
fn set_and_fix_out_of_range_change_nothing() -> TestResult {
    let mut h = IdHeap::new(vec![Elem::new(idn(1)?, 0), Elem::new(idn(2)?, 1)]);
    assert_eq!(h.set(2, Elem::new(idn(0)?, 2)), None);
    h.fix(2);
    assert_eq!(h.len(), 2);
    assert_eq!(top(&h)?, 1);
    assert_eq!(h.set(0, Elem::new(idn(3)?, 0)), Some(Elem::new(idn(1)?, 0)));
    h.fix(0);
    assert_eq!(top(&h)?, 2);
    Ok(())
}

/// The heap's elements in their array order, as (value, list index).
fn layout(h: &IdHeap) -> TestResult<Vec<(u64, usize)>> {
    (0..h.len())
        .map(|i| {
            let e = h.get(i).context("in range")?;
            Ok((num(e), e.list_idx()))
        })
        .collect()
}

#[test]
fn push_sifts_each_element_up_past_its_parents() -> TestResult {
    // Go's container/heap, which Dgraph's uint64Heap implements, after Push of 5, 4, 3, 2, 1, 0 with list indexes 0
    // to 5 (run in Go 1.27): [{0 5} {2 3} {1 4} {5 0} {3 2} {4 1}].
    let mut h = IdHeap::new(Vec::new());
    for (i, v) in [5, 4, 3, 2, 1, 0].into_iter().enumerate() {
        h.push(Elem::new(idn(v)?, i));
    }
    assert!(!h.is_empty());
    assert_eq!(
        layout(&h)?,
        vec![(0, 5), (2, 3), (1, 4), (5, 0), (3, 2), (4, 1)]
    );
    Ok(())
}

#[test]
fn push_leaves_an_equal_element_below_its_parent() -> TestResult {
    // Less is strict, so an element equal to its parent stays where it is: Go's container/heap after Push of three
    // 7s with list indexes 0, 1, 2 (run in Go 1.27): [{7 0} {7 1} {7 2}].
    let mut h = IdHeap::new(Vec::new());
    for i in 0..3 {
        h.push(Elem::new(idn(7)?, i));
    }
    assert_eq!(layout(&h)?, vec![(7, 0), (7, 1), (7, 2)]);
    Ok(())
}

#[test]
fn fix_moves_a_decreased_leaf_up_to_the_root() -> TestResult {
    // Go's container/heap: Init of [{1 0} {5 1} {9 2} {6 3}], the leaf at 3 set to {0 3}, then Fix(3) (run in Go
    // 1.27): [{0 3} {1 0} {9 2} {5 1}].
    let mut h = IdHeap::new(vec![
        Elem::new(idn(1)?, 0),
        Elem::new(idn(5)?, 1),
        Elem::new(idn(9)?, 2),
        Elem::new(idn(6)?, 3),
    ]);
    assert_eq!(h.set(3, Elem::new(idn(0)?, 3)), Some(Elem::new(idn(6)?, 3)));
    h.fix(3);
    assert_eq!(layout(&h)?, vec![(0, 3), (1, 0), (9, 2), (5, 1)]);
    Ok(())
}
