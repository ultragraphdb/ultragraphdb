/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

/// XXH3-64 of `bytes` with seed 0, as `doc/xxhash_spec.md` of the xxHash repository specifies it: the header's
/// `checksum` over its bytes 8 to 256, and `checksum_body` over a chunk table.
pub fn xxh3_64(bytes: &[u8]) -> u64 {
    xxhash_rust::xxh3::xxh3_64(bytes)
}

/// XXH3-64 of `bytes` with `seed`: a chunk's leaf is seeded with the chunk's index.
pub fn xxh3_64_with_seed(bytes: &[u8], seed: u64) -> u64 {
    xxhash_rust::xxh3::xxh3_64_with_seed(bytes, seed)
}
