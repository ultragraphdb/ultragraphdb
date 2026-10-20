Feature: Packing, decoding and seeking UID lists
  A sorted UID list packs into Dgraph's UidPack blocks of group-varint deltas, decodes back in order, and a decoder
  seeks to the block holding a target.

  Scenario: Encoding matches the golden vectors
    Given the golden UID lists generated from Dgraph with block size 256
    When each golden list is encoded with block size 256
    Then every block's base, deltas and count equal Dgraph's

  Scenario: A block closes where the upper 32 bits change
    Given the UID list "1, 2, 4294967297"
    When the list is encoded with block size 256
    Then the pack has 2 blocks
    And the block bases are "1, 4294967297"

  Scenario: A one-UID block emits one empty group
    Given the UID list "7"
    When the list is encoded with block size 256
    Then the pack has 1 block
    And block 1 has base 7, count 1 and deltas "00 00 00 00 00"

  Scenario: Lists of every length round-trip
    Given every sorted UID list of length 0 to 100, below 2^32 and across 2^32
    When each list is encoded with block size 256 and decoded
    Then each decoded list equals its input

  Scenario: Decoding and seeking from a target
    Given the UIDs 10 to 10000 in steps of 10, packed with block size 256
    When the pack is decoded from 5005
    Then the result holds 500 UIDs from 5010 to 10000
    When a decoder seeks 5005 with SeekStart
    Then the result holds 12 UIDs from 5010 to 5120
    When a decoder seeks 5010 with SeekCurrent
    Then the result holds 11 UIDs from 5020 to 5120
    When the decoder moves to the next block
    Then the result holds 256 UIDs from 5130 to 7680
