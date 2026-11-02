/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_core_ids::IdError;

// No Dgraph text applies: the system ID is the repository's own. Each text names the field refused and its value; the
// limits are hand-computed: 2^24 − 1 = 16,777,215, 2^32 − 1 = 4,294,967,295, 2^42 − 1 = 4,398,046,511,103 and
// 2^22 − 2 = 4,194,302.

#[test]
fn test_invalid_id_texts_name_the_field() {
    assert_eq!(
        IdError::reserved_bits(0x00_0001).to_string(),
        "invalid system ID: its reserved bits are 0x000001, not zero"
    );
    assert_eq!(
        IdError::reserved_bits(0x80_0000).to_string(),
        "invalid system ID: its reserved bits are 0x800000, not zero"
    );
    assert_eq!(
        IdError::zero_type_prefix().to_string(),
        "invalid system ID: its type prefix is 0"
    );
    assert_eq!(
        IdError::zero_sequence().to_string(),
        "invalid system ID: its sequence is 0"
    );
}

#[test]
fn test_length_text() {
    assert_eq!(
        IdError::length("a version", 8, 7).to_string(),
        "a version is 8 bytes long, not 7"
    );
}

#[test]
fn test_range_texts_name_the_limit() {
    assert_eq!(
        IdError::bucket_range(16_777_216).to_string(),
        "bucket 16777216 does not fit in 24 bits; the largest is 16777215"
    );
    assert_eq!(
        IdError::iri_too_long(4_294_967_296).to_string(),
        "an IRI of 4294967296 bytes is longer than a canonical key's 4-byte length counts, 4294967295 bytes"
    );
    assert_eq!(
        IdError::epoch_range(4_398_046_511_104).to_string(),
        "epoch 4398046511104 does not fit in 42 bits; the largest is 4398046511103"
    );
    assert_eq!(
        IdError::commit_sequence_range(4_194_304).to_string(),
        "commit sequence 4194304 does not fit in 22 bits as an even number; the largest is 4194302"
    );
    assert_eq!(
        IdError::odd_commit_sequence(3).to_string(),
        "commit sequence 3 is odd; commit versions are even"
    );
}
