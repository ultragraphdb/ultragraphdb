/*
 * Copyright (c) 2014 Couchbase, Inc.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from github.com/blevesearch/segment v0.9.1, segment_words_test.go (TestAdhocSegmentsWithType,
 * TestWordSegmentLongInputSlowReader). Changed for ultragraphdb: translated to Rust over the direct segmenter.
 * Copyright (c) 2026. Marvin Hansen.
 */

use ultragraphdb_core_tok::{SegmentType, word_segments};

#[test]
fn test_adhoc_segments_with_type() {
    use SegmentType::{Ideo, Letter, None, Number};
    let tests: Vec<(&str, Vec<(&str, SegmentType)>)> = vec![
        (
            "Now  is the.\n End.",
            vec![
                ("Now", Letter),
                (" ", None),
                (" ", None),
                ("is", Letter),
                (" ", None),
                ("the", Letter),
                (".", None),
                ("\n", None),
                (" ", None),
                ("End", Letter),
                (".", None),
            ],
        ),
        ("3.5", vec![("3.5", Number)]),
        ("cat3.5", vec![("cat3.5", Letter)]),
        ("c", vec![("c", Letter)]),
        (
            "こんにちは世界",
            vec![
                ("こ", Ideo),
                ("ん", Ideo),
                ("に", Ideo),
                ("ち", Ideo),
                ("は", Ideo),
                ("世", Ideo),
                ("界", Ideo),
            ],
        ),
        (
            "你好世界",
            vec![("你", Ideo), ("好", Ideo), ("世", Ideo), ("界", Ideo)],
        ),
        ("サッカ", vec![("サッカ", Ideo)]),
        // WB7b/WB7c.
        ("א\"א", vec![("א\"א", Letter)]),
        // The empty input has no segment.
        ("", vec![]),
    ];
    for (input, want) in tests {
        assert_eq!(word_segments(input), (want, Option::None), "{input:?}");
    }
}

#[test]
fn test_word_segment_long_input() {
    let text = format!("{} cat", "abcdefghijklmnop".repeat(26));
    let (segs, err) = word_segments(&text);
    assert_eq!(err, None);
    assert_eq!(
        segs,
        vec![
            (&text[..416], SegmentType::Letter),
            (" ", SegmentType::None),
            ("cat", SegmentType::Letter)
        ]
    );
}
