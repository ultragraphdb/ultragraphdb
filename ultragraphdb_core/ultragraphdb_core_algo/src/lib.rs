/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! Ported from Dgraph v25.5.0: `algo`: sorted-set operations over UID lists and packed lists.
//!
//! Intersection, merge, difference and index-of over sorted [`List`](ultragraphdb_core_codec::List)s and over
//! [`UidPack`](ultragraphdb_core_codec::UidPack)s, with the heap the merges use. Where Dgraph accepts a nil pack,
//! the port takes an `Option`. The count-min sketch (`algo/cm-sketch.go`) is not ported.

mod packed;
mod types;
mod uidlist;

pub use crate::packed::{
    apply_filter_packed, difference_packed, index_of_packed, intersect_sorted_packed,
    intersect_with_lin_packed, merge_sorted_packed,
};
pub use crate::types::elem::Elem;
pub use crate::types::uint64_heap::Uint64Heap;
pub use crate::uidlist::{
    apply_filter, difference, index_of, intersect_compressed_with, intersect_compressed_with_bin,
    intersect_compressed_with_lin_jump, intersect_sorted, intersect_with, intersect_with_bin,
    intersect_with_jump, intersect_with_lin, merge_sorted, merge_sorted_more_mem,
};
