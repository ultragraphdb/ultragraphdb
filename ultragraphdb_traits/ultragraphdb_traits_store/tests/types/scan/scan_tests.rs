/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_traits_store::{Direction, Scan};

#[test]
fn test_a_prefix_scan_selects_the_keys_beginning_with_it() {
    let scan = Scan::prefix(b"ab".to_vec());
    assert_eq!((scan.start(), scan.end()), (&b"ab"[..], Some(&b"ac"[..])));
    assert!(scan.selects(b"ab") && scan.selects(b"abc") && scan.selects(b"ab\xff\xff"));
    assert!(!scan.selects(b"a") && !scan.selects(b"ac") && !scan.selects(b"b"));
}

#[test]
fn test_a_prefix_ending_in_ff_ends_above_its_last_other_byte() {
    assert_eq!(Scan::prefix(b"a\xff\xff".to_vec()).end(), Some(&b"b"[..]));
    let all_ff = Scan::prefix(vec![0xFF, 0xFF]);
    assert_eq!(all_ff.end(), None);
    assert!(all_ff.selects(&[0xFF, 0xFF, 0x00]) && !all_ff.selects(&[0xFF, 0xFE]));
}

#[test]
fn test_an_empty_prefix_selects_every_key() {
    let scan = Scan::prefix(Vec::new());
    assert!(scan.selects(b"") && scan.selects(&[0xFF; 9]));
}

#[test]
fn test_a_key_scan_selects_that_key_alone() {
    let scan = Scan::key(b"ab".to_vec());
    assert!(scan.selects(b"ab"));
    assert!(!scan.selects(b"ab\x00") && !scan.selects(b"abc") && !scan.selects(b"a"));
}

#[test]
fn test_a_range_includes_its_start_and_excludes_its_end() {
    let scan = Scan::range(b"b".to_vec(), Some(b"d".to_vec()));
    assert!(scan.selects(b"b") && scan.selects(b"c\xff"));
    assert!(!scan.selects(b"d") && !scan.selects(b"a"));
    assert!(Scan::range(b"b".to_vec(), None).selects(&[0xFF; 3]));
}

#[test]
fn test_seek_skips_the_keys_before_it_in_the_scan_direction() {
    let forward = Scan::prefix(b"k".to_vec()).seek(b"k5".to_vec());
    assert!(forward.selects(b"k5") && forward.selects(b"k6") && !forward.selects(b"k4"));
    let reverse = Scan::prefix(b"k".to_vec()).reverse().seek(b"k5".to_vec());
    assert!(reverse.selects(b"k5") && reverse.selects(b"k4") && !reverse.selects(b"k6"));
    assert_eq!(reverse.seek_key(), Some(&b"k5"[..]));
}

#[test]
fn test_the_builders_set_direction_and_versions() {
    let scan = Scan::prefix(b"p".to_vec());
    assert_eq!(scan.direction(), Direction::Forward);
    assert!(!scan.is_all_versions());
    let scan = scan.reverse().all_versions();
    assert_eq!(scan.direction(), Direction::Reverse);
    assert!(scan.is_all_versions());
    assert_eq!(scan.seek_key(), None);
}
