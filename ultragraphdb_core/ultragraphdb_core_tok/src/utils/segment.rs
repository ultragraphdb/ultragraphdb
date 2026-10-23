/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use crate::bleve::segment_words;
use crate::types::segment_type::SegmentType;

/// The word segments of `input` as blevesearch/segment v0.9.1 (Unicode 8.0.0 word boundaries) finds them, each with
/// its type, and the error the segmenter stopped on. Segmentation stops early, without an error, where segment
/// does: at an invalid UTF-8 byte after a Hangul, Han, Hiragana, Katakana or non-word segment.
///
/// ```
/// use ultragraphdb_core_tok::{SegmentType, word_segments};
///
/// let (segments, err) = word_segments(b"3.5 cats");
/// assert_eq!(segments, vec![(&b"3.5"[..], SegmentType::Number), (&b" "[..], SegmentType::None), (&b"cats"[..], SegmentType::Letter)]);
/// assert!(err.is_none());
/// ```
pub fn word_segments(input: &[u8]) -> (Vec<(&[u8], SegmentType)>, Option<&'static str>) {
    let (segments, err) = segment_words::word_segments(input);
    (
        segments
            .into_iter()
            .map(|(pos, len, typ)| (&input[pos..pos + len], typ))
            .collect(),
        err,
    )
}
