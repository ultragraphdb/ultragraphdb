#
# SPDX-License-Identifier: Apache-2.0
# Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
#

Feature: Posting lists are stored in the repository's own format
  A posting list is stored in a byte format of the repository's own: a version byte, flags, the commit timestamp, the
  UID pack's blocks, the postings and the splits, little-endian, followed by an XXH3-64 checksum of the bytes before
  it. A stored value whose version is unknown, whose checksum fails or whose lengths overrun is refused as corruption
  naming its key, never decoded in part.

  Scenario: A list round-trips
    Given a list with a UID pack, value postings with statement IDs and splits
    When the list is encoded and decoded
    Then the decoded list equals the original

  Scenario: A flipped byte is corruption
    Given a list with a UID pack, value postings with statement IDs and splits, stored under the key "list-7"
    When one byte of the stored list is flipped and the list is read, for every byte in turn
    Then every read fails with a corruption error naming the key "list-7"
