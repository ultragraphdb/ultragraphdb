/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_transactions_log::AlignedBuf;

fn aligned(buf: &AlignedBuf) -> bool {
    (buf.as_slice().as_ptr() as usize).is_multiple_of(4_096)
}

#[test]
fn test_zeroed_bytes_are_aligned_zeros_of_the_length() {
    for len in [0, 1, 255, 4_095, 4_096, 4_097, 1 << 20] {
        let buf = AlignedBuf::zeroed(len);
        assert_eq!(buf.len(), len);
        assert_eq!(buf.is_empty(), len == 0);
        assert_eq!(buf.as_slice().len(), len);
        assert!(buf.as_slice().iter().all(|b| *b == 0));
        assert!(aligned(&buf), "length {len}");
    }
}

#[test]
fn test_from_slice_copies_into_aligned_bytes() {
    let source: Vec<u8> = (0..5_000u32).map(|i| (i * 7) as u8).collect();
    let buf = AlignedBuf::from_slice(&source);
    assert_eq!(buf.as_slice(), source.as_slice());
    assert!(aligned(&buf));
    assert!(AlignedBuf::from_slice(&[]).is_empty());
}

#[test]
fn test_writes_through_the_mutable_slice_read_back() {
    let mut buf = AlignedBuf::zeroed(8_192);
    buf.as_mut_slice()[0] = 1;
    buf.as_mut_slice()[8_191] = 2;
    assert_eq!(buf.as_slice()[0], 1);
    assert_eq!(buf.as_slice()[8_191], 2);
    assert_eq!(
        buf.as_slice()[1..8_191].iter().filter(|b| **b != 0).count(),
        0
    );
}

#[test]
fn test_a_clone_is_equal_and_aligned_on_its_own() {
    let mut buf = AlignedBuf::zeroed(4_096);
    buf.as_mut_slice()[17] = 0xEE;
    let copy = buf.clone();
    assert_eq!(copy.as_slice(), buf.as_slice());
    assert!(aligned(&copy));
}

#[test]
fn test_alignment_is_four_kibibytes() {
    assert_eq!(AlignedBuf::ALIGNMENT, 4_096);
}
