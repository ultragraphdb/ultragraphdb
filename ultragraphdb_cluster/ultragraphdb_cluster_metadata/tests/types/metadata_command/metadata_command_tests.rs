/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_cluster_metadata::MetadataCommand;

#[test]
fn test_the_limits() {
    // Chunks of 128 KiB fit a test log's 256 KiB segment; the map holds 2^18 ranges; the sequencer's close limits are
    // 256 versions and 4,096 evictions.
    assert_eq!(MetadataCommand::CATALOG_BYTES_MAX, 523_000);
    assert_eq!(MetadataCommand::COMMAND_BYTES_MAX, 33_554_432);
    assert_eq!(MetadataCommand::CHUNK_BYTES, 131_072);
    assert_eq!(MetadataCommand::PREDICATE_BYTES_MAX, 4_096);
    assert_eq!(MetadataCommand::ENTRY_BYTES_MAX, 65_536);
    assert_eq!(MetadataCommand::ASSIGNMENTS_MAX, 262_144);
    assert_eq!(MetadataCommand::NAME_BYTES_MAX, 255);
    assert_eq!(MetadataCommand::SALT_BYTES_MAX, 255);
    assert_eq!(MetadataCommand::VERSIONS_MAX, 256);
    assert_eq!(MetadataCommand::EVICTIONS_MAX, 4_096);
}
