#
# SPDX-License-Identifier: Apache-2.0
# Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
#

Feature: Wire protocol: the header, checksums and validation
  Every frame begins with one 256-byte little-endian header, checksummed with XXH3-64; its body is checked in chunks
  under one root; a frame is validated in a fixed order before use and never panics; commands keep their numbers.
  These are the scenarios whose decision this crate makes. A receiver's answer, the closing of the connection and the
  connections it keeps serving are the bus's and the server's (task groups 9 and 10), whose features bind those
  clauses again; "A refused request keeps the connection" and "A large result streams" run there alone, since they
  need streams, results and prepared statements.

  Scenario: A golden header round-trips
    When the golden Hello frame's header is read
    Then each field holds its golden value, and writing the fields back yields the same 256 bytes

  Scenario: A checksum matches the specification's reference
    When a header's checksum is computed by this crate and by the xxHash reference implementation over the same bytes
    Then both give the same 64-bit value

  Scenario: A corrupted chunk is named
    When one byte of chunk 3 of a 20 KiB body is flipped
    Then the frame is refused with an error naming chunk 3 and its byte range

  Scenario: Swapped chunks fail
    When chunks 1 and 2 of a body are swapped
    Then the frame is refused naming chunk 1

  Scenario: A small body carries no table
    When a frame with a 100-byte body is built
    Then its body is 100 bytes, and `checksum_body` is the leaf of that one chunk

  Scenario: A body length with no valid split is refused
    When a frame arrives whose body is 3,841 bytes, one byte past the first chunk with no room for a table
    Then it is refused with an error naming the body's length

  Scenario: A frame of another cluster is refused
    When a frame whose `cluster` is not the server's arrives
    Then the server answers with an error naming the cluster and closes the connection

  Scenario: An unknown command is refused, not fatal
    When a frame with a valid checksum names an unknown command
    Then the connection is closed with an error naming the command, and the server keeps serving other connections

  Scenario: A retired number is refused
    When a frame carries a retired command's number
    Then it is refused as an unknown command
