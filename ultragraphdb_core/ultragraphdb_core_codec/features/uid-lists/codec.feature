# SPDX-License-Identifier: Apache-2.0
# Copyright (c) 2026. Marvin Hansen. All Rights Reserved.

Feature: Packing, decoding and seeking ID lists
  Lists of 128-bit system IDs in the repository's own packed format
  (openspec/changes/stage-5-5-three-role-refactor/specs/uid-lists/spec.md): the scenarios of the requirements "ID
  lists pack into the repository's own format", "Packed lists decode, seek and round-trip" and "Decoders seek without
  panicking or hanging" that the codec decides alone, verbatim; "Intersecting from past a pack's last ID returns
  nothing" runs in ultragraphdb_core_algo.

  Rule: ID lists pack into the repository's own format

    Scenario: A block closes at the block size
      When 300 increasing IDs are encoded with block size 256
      Then the encoding holds two blocks, of 256 and 44 IDs, each beginning with its first ID whole

    Scenario: A delta is written in LEB128
      When the IDs (type 1, bucket 0, sequence 1) and (type 1, bucket 0, sequence 129) are encoded
      Then the second is written as the difference of their first 104 bits, 128 × 2^24 since the sequence sits above the 24-bit bucket, in LEB128 the bytes `80 80 80 80 08`

    Scenario: A list out of order is refused
      When a list whose third ID is not greater than its second is encoded
      Then encoding fails with an error naming position 2

  Rule: Packed lists decode, seek and round-trip

    Scenario: Lists of every length round-trip
      When each sorted list of length 0 to 100, including lists spanning two types and two buckets, is encoded and decoded
      Then the decoded list equals the input

    Scenario: Decoding and seeking from a target
      When a packed list of the 1,000 IDs of type 1, bucket 0 and sequences 10, 20, …, 10,000 is decoded from sequence 5,005 and a decoder is sought to sequence 5,005
      Then decoding returns the 500 IDs from sequence 5,010 on, and the seek returns the rest of the target's block from sequence 5,010

  Rule: Decoders seek without panicking or hanging

    Scenario: A backward seek returns the target's block
      When a decoder over sequences 1 to 40 of type 1, bucket 0, in blocks of 10, seeks to sequence 35 and then to 5
      Then the second seek returns the block of sequences 1 to 10

    Scenario: An empty block contributes nothing to a seek
      When a pack holds a block whose ID count is 0 and a seek targets it
      Then the seek moves on to the next block and nothing panics

    Scenario: A seek to the largest target finds nothing past the end
      When a decoder over sequences 1 to 3 of type 1, bucket 0, in blocks of 2, seeks to `u128::MAX`
      Then the seek returns nothing and nothing panics

    Scenario: A linear seek to the largest target stays on the last block
      When a decoder over sequences 1 to 3 of type 1, bucket 0, in blocks of 2, is linearly sought to `u128::MAX`
      Then it returns the block holding sequence 3
