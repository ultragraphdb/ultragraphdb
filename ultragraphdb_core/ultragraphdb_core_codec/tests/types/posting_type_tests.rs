/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_core_codec::PostingType;

#[test]
fn a_posting_is_a_reference_by_default() {
    // Dgraph's PostingType: REF = 0, the protobuf default.
    assert_eq!(PostingType::default(), PostingType::Ref);
    assert_ne!(PostingType::Ref, PostingType::Value);
}
