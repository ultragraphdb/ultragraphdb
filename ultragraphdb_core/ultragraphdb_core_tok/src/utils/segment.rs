/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use crate::bleve::segment_words;
use crate::types::segment_type::SegmentType;

/// The word segments of `input` as blevesearch/segment v0.9.1 (Unicode 8.0.0 word boundaries) finds them, each with
/// its type, and the error the segmenter stopped on.
///
/// ```
/// use ultragraphdb_core_tok::{SegmentType, word_segments};
///
/// let (segments, err) = word_segments("3.5 cats");
/// assert_eq!(segments, vec![("3.5", SegmentType::Number), (" ", SegmentType::None), ("cats", SegmentType::Letter)]);
/// assert!(err.is_none());
/// ```
pub fn word_segments(input: &str) -> (Vec<(&str, SegmentType)>, Option<&'static str>) {
    segment_words::word_segments(input)
}
