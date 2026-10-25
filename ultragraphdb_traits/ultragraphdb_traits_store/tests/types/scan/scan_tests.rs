/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_traits_store::{Direction, Scan};

#[test]
fn test_a_prefix_scan_ends_at_the_first_key_above_the_prefix() {
    // (prefix, end): the prefix without its trailing 0xFF bytes, its last byte incremented; none when all 0xFF.
    let cases: &[(&[u8], Option<&[u8]>)] = &[
        (b"", None),
        (b"ab", Some(b"ac")),
        (&[0x00], Some(&[0x01])),
        (&[0x00, 0x00], Some(&[0x00, 0x01])),
        (b"a\xfe", Some(b"a\xff")),
        (b"a\xff\xff", Some(b"b")),
        (&[0x01, 0xFF, 0x00], Some(&[0x01, 0xFF, 0x01])),
        (&[0xFE, 0xFF], Some(&[0xFF])),
        (&[0xFF], None),
        (&[0xFF, 0xFF], None),
    ];
    for &(prefix, end) in cases {
        let scan = Scan::prefix(prefix.to_vec());
        assert_eq!(
            (scan.start(), scan.end()),
            (prefix, end),
            "prefix {prefix:?}"
        );
    }
}

#[test]
fn test_a_prefix_scan_selects_the_keys_beginning_with_it() {
    let scan = Scan::prefix(b"ab".to_vec());
    for key in [&b"ab"[..], b"abc", b"ab\x00", b"ab\xff\xff"] {
        assert!(scan.selects(key), "{key:?}");
    }
    for key in [&b""[..], b"a", b"aa\xff", b"ac", b"b", b"\xff"] {
        assert!(!scan.selects(key), "{key:?}");
    }
}

#[test]
fn test_a_prefix_ending_in_ff_selects_exactly_its_keys() {
    let ff = Scan::prefix(b"a\xff".to_vec());
    assert!(ff.selects(b"a\xff") && ff.selects(b"a\xff\xff\xff") && ff.selects(b"a\xff\x00"));
    assert!(!ff.selects(b"a\xfe\xff") && !ff.selects(b"b"));
    let all_ff = Scan::prefix(vec![0xFF, 0xFF]);
    assert!(all_ff.selects(&[0xFF, 0xFF]) && all_ff.selects(&[0xFF, 0xFF, 0x00]));
    assert!(!all_ff.selects(&[0xFF, 0xFE]) && !all_ff.selects(&[0xFF]));
}

#[test]
fn test_an_empty_prefix_selects_every_key() {
    let scan = Scan::prefix(Vec::new());
    assert!(scan.selects(b"") && scan.selects(&[0x00]) && scan.selects(&[0xFF; 9]));
}

#[test]
fn test_a_key_scan_selects_that_key_alone() {
    let scan = Scan::key(b"ab".to_vec());
    assert_eq!(
        (scan.start(), scan.end()),
        (&b"ab"[..], Some(&b"ab\x00"[..]))
    );
    assert!(scan.selects(b"ab"));
    assert!(
        !scan.selects(b"ab\x00")
            && !scan.selects(b"abc")
            && !scan.selects(b"a")
            && !scan.selects(b"aa\xff")
    );
    let empty = Scan::key(Vec::new());
    assert_eq!((empty.start(), empty.end()), (&b""[..], Some(&b"\x00"[..])));
    assert!(empty.selects(b"") && !empty.selects(&[0x00]));
}

#[test]
fn test_a_range_includes_its_start_and_excludes_its_end() {
    let scan = Scan::range(b"b".to_vec(), Some(b"d".to_vec()));
    assert!(scan.selects(b"b") && scan.selects(b"c\xff") && scan.selects(b"c\xff\xff\xff"));
    assert!(!scan.selects(b"d") && !scan.selects(b"d\x00") && !scan.selects(b"a\xff"));
    let open = Scan::range(b"b".to_vec(), None);
    assert_eq!(open.end(), None);
    assert!(open.selects(b"b") && open.selects(&[0xFF; 3]) && !open.selects(b"a"));
}

#[test]
fn test_an_empty_or_inverted_range_selects_nothing() {
    for scan in [
        Scan::range(b"b".to_vec(), Some(b"b".to_vec())),
        Scan::range(b"c".to_vec(), Some(b"b".to_vec())),
    ] {
        for key in [&b""[..], b"a", b"b", b"b\x00", b"c", b"\xff"] {
            assert!(!scan.selects(key), "{scan:?} selects {key:?}");
        }
    }
}

#[test]
fn test_seek_skips_the_keys_before_it_in_the_scan_direction() {
    let forward = Scan::prefix(b"k".to_vec()).seek(b"k5".to_vec());
    assert!(forward.selects(b"k5") && forward.selects(b"k50") && forward.selects(b"k6"));
    assert!(!forward.selects(b"k4") && !forward.selects(b"k4\xff") && !forward.selects(b"k"));
    let reverse = Scan::prefix(b"k".to_vec()).reverse().seek(b"k5".to_vec());
    assert!(reverse.selects(b"k5") && reverse.selects(b"k4\xff") && reverse.selects(b"k"));
    assert!(!reverse.selects(b"k50") && !reverse.selects(b"k6"));
    assert_eq!(reverse.seek_key(), Some(&b"k5"[..]));
}

#[test]
fn test_a_seek_outside_the_range_never_widens_it() {
    // Before the range, a forward seek leaves it whole; past it, nothing is left.
    let before = Scan::prefix(b"k".to_vec()).seek(b"a".to_vec());
    assert!(before.selects(b"k") && before.selects(b"k\xff") && !before.selects(b"b"));
    let past = Scan::prefix(b"k".to_vec()).seek(b"z".to_vec());
    assert!(!past.selects(b"k") && !past.selects(b"k\xff") && !past.selects(b"z"));
    // In reverse, a seek past the range leaves it whole and one before it leaves nothing.
    let reverse_past = Scan::prefix(b"k".to_vec()).reverse().seek(b"z".to_vec());
    assert!(
        reverse_past.selects(b"k") && reverse_past.selects(b"k\xff") && !reverse_past.selects(b"l")
    );
    let reverse_before = Scan::prefix(b"k".to_vec()).reverse().seek(b"a".to_vec());
    assert!(!reverse_before.selects(b"k") && !reverse_before.selects(b"a"));
}

#[test]
fn test_the_builders_set_direction_versions_and_seek_and_keep_the_rest() {
    let scan = Scan::prefix(b"p".to_vec());
    assert_eq!(
        (scan.direction(), scan.is_all_versions(), scan.seek_key()),
        (Direction::Forward, false, None)
    );
    let scan = scan.seek(b"p3".to_vec()).reverse().all_versions();
    assert_eq!(
        (scan.start(), scan.end(), scan.seek_key()),
        (&b"p"[..], Some(&b"q"[..]), Some(&b"p3"[..]))
    );
    assert_eq!(
        (scan.direction(), scan.is_all_versions()),
        (Direction::Reverse, true)
    );
    // The order of the builders does not matter.
    assert_eq!(
        scan,
        Scan::prefix(b"p".to_vec())
            .all_versions()
            .reverse()
            .seek(b"p3".to_vec())
    );
    // A later seek replaces an earlier one.
    assert_eq!(
        Scan::prefix(b"p".to_vec())
            .seek(b"p1".to_vec())
            .seek(b"p2".to_vec())
            .seek_key(),
        Some(&b"p2"[..])
    );
}

#[test]
fn test_all_versions_does_not_change_which_keys_are_selected() {
    let latest = Scan::prefix(b"k".to_vec());
    let all = latest.clone().all_versions();
    for key in [&b"j"[..], b"k", b"k1", b"l"] {
        assert_eq!(all.selects(key), latest.selects(key), "{key:?}");
    }
}
