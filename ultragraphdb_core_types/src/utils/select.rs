/*
 * SPDX-FileCopyrightText: © 2017-2026 Istari Digital, Inc.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from Dgraph v25.5.0, types/select.go (quickselect, after Go's sort/zsortinterface.go). Changed for
 * ultragraphdb: translated to Rust.
 * Copyright (c) 2026. Marvin Hansen.
 */

use crate::traits::sort_data::SortData;

fn insertion_sort<D: SortData>(data: &mut D, a: usize, b: usize) {
    for i in a + 1..b {
        let mut j = i;
        while j > a && data.less(j, j - 1) {
            data.swap(j, j - 1);
            j -= 1;
        }
    }
}

fn order2<D: SortData>(data: &D, a: usize, b: usize) -> (usize, usize) {
    if data.less(b, a) { (b, a) } else { (a, b) }
}

fn median<D: SortData>(data: &D, a: usize, b: usize, c: usize) -> usize {
    let (a, b) = order2(data, a, b);
    let (b, _) = order2(data, b, c);
    let (_, b) = order2(data, a, b);
    b
}

fn choose_pivot<D: SortData>(data: &D, a: usize, b: usize) -> usize {
    const SHORTEST_NINTHER: usize = 50;
    let l = b - a;
    let mut i = a + l / 4;
    let mut j = a + l / 4 * 2;
    let mut k = a + l / 4 * 3;
    if l >= 8 {
        if l >= SHORTEST_NINTHER {
            i = median(data, i - 1, i, i + 1);
            j = median(data, j - 1, j, j + 1);
            k = median(data, k - 1, k, k + 1);
        }
        j = median(data, i, j, k);
    }
    j
}

fn partition<D: SortData>(data: &mut D, a: usize, b: usize, pivot: usize) -> usize {
    let mut index = a;
    data.swap(pivot, b);
    for i in a..b {
        if data.less(i, b) {
            data.swap(i, index);
            index += 1;
        }
    }
    data.swap(index, b);
    index
}

/// Dgraph's `quickSelect`: moves the `k` smallest elements of `[low, high]` before position `k`.
pub(crate) fn quick_select<D: SortData>(data: &mut D, low: usize, high: usize, k: usize) {
    let (mut low, mut high) = (low as isize, high as isize);
    let k = k as isize;
    loop {
        if low >= high {
            return;
        } else if high - low <= 8 {
            insertion_sort(data, low as usize, high as usize + 1);
            return;
        }
        let pivot = choose_pivot(data, low as usize, high as usize);
        let pivot = partition(data, low as usize, high as usize, pivot) as isize;
        if k < pivot {
            high = pivot - 1;
        } else if k > pivot {
            low = pivot + 1;
        } else {
            return;
        }
    }
}
