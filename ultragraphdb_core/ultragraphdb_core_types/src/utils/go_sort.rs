/*
 * Copyright 2009 The Go Authors. All rights reserved.
 * SPDX-License-Identifier: BSD-3-Clause
 *
 * Ported from Go 1.27, src/sort/sort.go and zsortinterface.go (Sort: pattern-defeating quicksort). Changed for
 * ultragraphdb: translated to Rust. Go's sort is not stable; equal elements end where Go's algorithm leaves them,
 * which Dgraph's result order depends on.
 * Copyright (c) 2026. Marvin Hansen.
 */

use crate::traits::sort_data::SortData;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Hint {
    Unknown,
    Increasing,
    Decreasing,
}

fn bits_len(n: usize) -> usize {
    (usize::BITS - n.leading_zeros()) as usize
}

/// Go's `sort.Sort`.
pub(crate) fn sort<D: SortData>(data: &mut D) {
    let n = data.len();
    if n <= 1 {
        return;
    }
    pdqsort(data, 0, n, bits_len(n));
}

fn insertion_sort<D: SortData>(data: &mut D, a: usize, b: usize) {
    for i in a + 1..b {
        let mut j = i;
        while j > a && data.less(j, j - 1) {
            data.swap(j, j - 1);
            j -= 1;
        }
    }
}

fn sift_down<D: SortData>(data: &mut D, lo: usize, hi: usize, first: usize) {
    let mut root = lo;
    loop {
        let mut child = 2 * root + 1;
        if child >= hi {
            break;
        }
        if child + 1 < hi && data.less(first + child, first + child + 1) {
            child += 1;
        }
        if !data.less(first + root, first + child) {
            return;
        }
        data.swap(first + root, first + child);
        root = child;
    }
}

fn heap_sort<D: SortData>(data: &mut D, a: usize, b: usize) {
    let first = a;
    let hi = b - a;
    for i in (0..=(hi as isize - 1) / 2).rev() {
        if i >= 0 {
            sift_down(data, i as usize, hi, first);
        }
    }
    for i in (0..hi).rev() {
        data.swap(first, first + i);
        sift_down(data, 0, i, first);
    }
}

fn pdqsort<D: SortData>(data: &mut D, a: usize, b: usize, limit: usize) {
    const MAX_INSERTION: usize = 12;
    let (mut a, mut b, mut limit) = (a, b, limit);
    let mut was_balanced = true;
    let mut was_partitioned = true;
    loop {
        let length = b - a;
        if length <= MAX_INSERTION {
            insertion_sort(data, a, b);
            return;
        }
        if limit == 0 {
            heap_sort(data, a, b);
            return;
        }
        if !was_balanced {
            break_patterns(data, a, b);
            limit -= 1;
        }
        let (mut pivot, mut hint) = choose_pivot(data, a, b);
        if hint == Hint::Decreasing {
            reverse_range(data, a, b);
            pivot = (b - 1) - (pivot - a);
            hint = Hint::Increasing;
        }
        if was_balanced
            && was_partitioned
            && hint == Hint::Increasing
            && partial_insertion_sort(data, a, b)
        {
            return;
        }
        if a > 0 && !data.less(a - 1, pivot) {
            a = partition_equal(data, a, b, pivot);
            continue;
        }
        let (mid, already) = partition(data, a, b, pivot);
        was_partitioned = already;
        let (left, right) = (mid - a, b - mid);
        let threshold = length / 8;
        if left < right {
            was_balanced = left >= threshold;
            pdqsort(data, a, mid, limit);
            a = mid + 1;
        } else {
            was_balanced = right >= threshold;
            pdqsort(data, mid + 1, b, limit);
            b = mid;
        }
    }
}

fn partition<D: SortData>(data: &mut D, a: usize, b: usize, pivot: usize) -> (usize, bool) {
    data.swap(a, pivot);
    let (mut i, mut j) = (a as isize + 1, b as isize - 1);
    while i <= j && data.less(i as usize, a) {
        i += 1;
    }
    while i <= j && !data.less(j as usize, a) {
        j -= 1;
    }
    if i > j {
        data.swap(j as usize, a);
        return (j as usize, true);
    }
    data.swap(i as usize, j as usize);
    i += 1;
    j -= 1;
    loop {
        while i <= j && data.less(i as usize, a) {
            i += 1;
        }
        while i <= j && !data.less(j as usize, a) {
            j -= 1;
        }
        if i > j {
            break;
        }
        data.swap(i as usize, j as usize);
        i += 1;
        j -= 1;
    }
    data.swap(j as usize, a);
    (j as usize, false)
}

fn partition_equal<D: SortData>(data: &mut D, a: usize, b: usize, pivot: usize) -> usize {
    data.swap(a, pivot);
    let (mut i, mut j) = (a as isize + 1, b as isize - 1);
    loop {
        while i <= j && !data.less(a, i as usize) {
            i += 1;
        }
        while i <= j && data.less(a, j as usize) {
            j -= 1;
        }
        if i > j {
            break;
        }
        data.swap(i as usize, j as usize);
        i += 1;
        j -= 1;
    }
    i as usize
}

fn partial_insertion_sort<D: SortData>(data: &mut D, a: usize, b: usize) -> bool {
    const MAX_STEPS: usize = 5;
    const SHORTEST_SHIFTING: usize = 50;
    let mut i = a + 1;
    for _ in 0..MAX_STEPS {
        while i < b && !data.less(i, i - 1) {
            i += 1;
        }
        if i == b {
            return true;
        }
        if b - a < SHORTEST_SHIFTING {
            return false;
        }
        data.swap(i, i - 1);
        if i - a >= 2 {
            let mut j = i - 1;
            while j >= 1 {
                if !data.less(j, j - 1) {
                    break;
                }
                data.swap(j, j - 1);
                j -= 1;
            }
        }
        if b - i >= 2 {
            let mut j = i + 1;
            while j < b {
                if !data.less(j, j - 1) {
                    break;
                }
                data.swap(j, j - 1);
                j += 1;
            }
        }
    }
    false
}

fn break_patterns<D: SortData>(data: &mut D, a: usize, b: usize) {
    let length = b - a;
    if length >= 8 {
        let mut random = length as u64;
        let modulus = 1usize << bits_len(length);
        let start = a + (length / 4) * 2 - 1;
        for idx in start..=a + (length / 4) * 2 + 1 {
            random ^= random << 13;
            random ^= random >> 7;
            random ^= random << 17;
            let mut other = (random as usize) & (modulus - 1);
            if other >= length {
                other -= length;
            }
            data.swap(idx, a + other);
        }
    }
}

fn choose_pivot<D: SortData>(data: &mut D, a: usize, b: usize) -> (usize, Hint) {
    const SHORTEST_NINTHER: usize = 50;
    const MAX_SWAPS: usize = 4 * 3;
    let l = b - a;
    let mut swaps = 0;
    let mut i = a + l / 4;
    let mut j = a + l / 4 * 2;
    let mut k = a + l / 4 * 3;
    if l >= 8 {
        if l >= SHORTEST_NINTHER {
            i = median(data, i - 1, i, i + 1, &mut swaps);
            j = median(data, j - 1, j, j + 1, &mut swaps);
            k = median(data, k - 1, k, k + 1, &mut swaps);
        }
        j = median(data, i, j, k, &mut swaps);
    }
    match swaps {
        0 => (j, Hint::Increasing),
        MAX_SWAPS => (j, Hint::Decreasing),
        _ => (j, Hint::Unknown),
    }
}

fn order2<D: SortData>(data: &D, a: usize, b: usize, swaps: &mut usize) -> (usize, usize) {
    if data.less(b, a) {
        *swaps += 1;
        return (b, a);
    }
    (a, b)
}

fn median<D: SortData>(data: &D, a: usize, b: usize, c: usize, swaps: &mut usize) -> usize {
    let (a, b) = order2(data, a, b, swaps);
    let (b, _c) = order2(data, b, c, swaps);
    let (_a, b) = order2(data, a, b, swaps);
    b
}

fn reverse_range<D: SortData>(data: &mut D, a: usize, b: usize) {
    let (mut i, mut j) = (a, b - 1);
    while i < j {
        data.swap(i, j);
        i += 1;
        j -= 1;
    }
}
