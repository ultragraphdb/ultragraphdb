/*
 * SPDX-FileCopyrightText: © 2017-2026 Istari Digital, Inc.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from Dgraph v25.5.0, algo/heap_test.go. Changed for ultragraphdb: translated to Rust.
 * Copyright (c) 2026. Marvin Hansen.
 */

use ultragraphdb_core_algo::{Elem, Uint64Heap};

fn top(h: &Uint64Heap) -> u64 {
    h.get(0).expect("the heap is not empty").val()
}

#[test]
fn test_push() {
    let mut h = Uint64Heap::new(Vec::new());

    h.push(Elem::new(5, 0));
    h.push(Elem::new(3, 0));
    h.push(Elem::new(4, 0));

    assert_eq!(h.len(), 3);
    assert_eq!(top(&h), 3);

    h.set(0, Elem::new(10, 0));
    h.fix(0);
    assert_eq!(top(&h), 4);

    h.set(0, Elem::new(11, 0));
    h.fix(0);
    assert_eq!(top(&h), 5);

    let e = h.pop().expect("an element");
    assert_eq!(e.val(), 5);

    let e = h.pop().expect("an element");
    assert_eq!(e.val(), 10);

    let e = h.pop().expect("an element");
    assert_eq!(e.val(), 11);

    assert_eq!(h.len(), 0);
    assert!(h.is_empty());
    assert!(h.pop().is_none());
}

#[test]
fn new_orders_its_elements() {
    let mut h = Uint64Heap::new(
        [9, 2, 7, 2, 5]
            .iter()
            .enumerate()
            .map(|(i, &v)| Elem::new(v, i))
            .collect(),
    );
    let mut out = Vec::new();
    while let Some(e) = h.pop() {
        out.push((e.val(), e.list_idx()));
    }
    assert_eq!(
        out.iter().map(|e| e.0).collect::<Vec<_>>(),
        vec![2, 2, 5, 7, 9]
    );
    assert_eq!(out[2], (5, 4));
}

#[test]
fn set_and_fix_out_of_range_change_nothing() {
    let mut h = Uint64Heap::new(vec![Elem::new(1, 0), Elem::new(2, 1)]);
    assert_eq!(h.set(2, Elem::new(0, 2)), None);
    h.fix(2);
    assert_eq!(h.len(), 2);
    assert_eq!(top(&h), 1);
    assert_eq!(h.set(0, Elem::new(3, 0)), Some(Elem::new(1, 0)));
    h.fix(0);
    assert_eq!(top(&h), 2);
}
