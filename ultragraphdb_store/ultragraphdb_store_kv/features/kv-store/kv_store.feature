#
# SPDX-License-Identifier: Apache-2.0
# Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
#

Feature: The versioned store over fjall
  The store keeps every version of a key as a stored key of its own, reads at a timestamp through one snapshot,
  writes atomic batches, drops prefixes physically, bans namespaces and collects versions below the discard timestamp.

  Scenario: A key's versions iterate newest first
    When a key is written at versions 3, 7 and 5
    Then iterating its versions yields 7, 5 and 3, each with its meta byte

  Scenario: A version below the newest can be written
    When a key holds version 9 and a write at version 6 follows
    Then both versions are kept, and the key's versions iterate as 9 then 6

  Scenario: Rewriting a version replaces it
    When the same key and version are written twice with different values
    Then a read at that version returns the second value

  Scenario: A key that prefixes another sorts first
    When index keys for the terms ab and abc are each written at versions 3 and 5
    Then a scan yields ab before abc, each key's versions contiguous and newest first

  Scenario: A user key over Badger's limit is refused
    When a batch writes a key whose user key is 65,001 bytes long
    Then the batch fails with an error beginning "Key with size 65001 exceeded 65000 limit", and none of its keys is found

  Scenario: A stored key over fjall's limit is refused
    When a batch writes a key whose user key is 40,000 zero bytes, 80,010 bytes once escaped and versioned
    Then the batch fails with an error beginning "Key with size 80010 exceeded 65535 limit", and none of its keys is found

  Scenario: A read takes the newest version at or below its timestamp
    When a key holds versions 3 and 7 and is read at 5
    Then the read returns version 3

  Scenario: A deletion hides the key
    When a key holds a value at 3 and a deletion at 6
    Then a read at 7 finds nothing and a read at 4 finds the value

  Scenario: A read below the discard timestamp is refused
    When the discard timestamp is 10 and a key is read at 5
    Then the read fails with "Transaction is too old"

  Scenario: A scan is unaffected by a later batch
    When a scan is open and a batch writes keys inside its range
    Then the scan returns none of the batch's keys

  Scenario: A scan reports each key once
    When three keys under a prefix hold several versions each and the prefix is scanned at a timestamp
    Then each key appears once, with its newest version at or below that timestamp

  Scenario: A scan yields every version when asked
    When the same prefix is scanned for all versions
    Then each key's versions at or below the timestamp appear newest first

  Scenario: A reader sees a batch whole or not at all
    When a batch writing 10,000 keys commits while readers read those keys repeatedly
    Then every reader sees either none or all of the batch's keys

  Scenario: A crash during a batch leaves nothing of it
    When the process is killed while a batch is written and the store is reopened
    Then either the whole batch or none of it is present

  Scenario: A batch holds entries at several versions
    When one batch writes key a at version 12 and key b at version 13
    Then a read at 12 finds a and not b, and a read at 13 finds both

  Scenario: A dropped prefix is gone after reopening
    When a prefix is dropped and the store is closed and reopened
    Then no key under the prefix is found

  Scenario: A drop interrupted by a crash finishes at open
    When a prefix drop has synced its record and written part of its removals, and the process is killed
    Then on reopening, the store finishes the drop before it serves a read, and no key under the prefix is found

  Scenario: A read below a drop finds nothing of it
    When a key under a prefix holds a version at 3, the prefix is dropped at 8, and the key is read at 5
    Then the read finds nothing

  Scenario: Dropping everything keeps only its entries
    When everything is dropped with one entry to write at timestamp 1, and the store is closed and reopened
    Then that entry is the only key found, and the applied timestamp and the discard timestamp are 0

  Scenario: A banned namespace ignores writes
    When namespace 5 is banned and a batch writes a key in it and a key in namespace 6
    Then the batch succeeds, the key in namespace 5 is not found and the key in namespace 6 is

  Scenario: Dropping everything clears the banned set
    When namespace 5 is banned, everything is dropped, and a batch then writes a key in namespace 5
    Then the key is found

  Scenario: The initial schema a drop writes survives garbage collection
    When everything in a namespace is dropped, its initial schema is written at timestamp 1, the discard timestamp is raised above the drop and the store compacts
    Then the namespace's initial schema is still read at the discard timestamp

  Scenario: A collected version stays collected after a restart
    When a version below the discard timestamp is collected, its deletion is swept and the store is closed and reopened
    Then a read above the deletion's timestamp finds no version of the key

  Scenario: Versions below a discard point are collected
    When a key holds deltas at 2 and 4, a complete version with the discard flag at 5 and a delta at 8, the discard timestamp is 6, and the memtables are flushed and a full compaction runs
    Then versions 2 and 4 are gone, and versions 5 and 8 remain

  Scenario: A deletion outlives an older version in another table
    When a key's value at 2 and its deletion at 4 lie in two different last-level tables, the discard timestamp is 6, and a compaction merges the table holding the deletion without the other
    Then the deletion remains, and a read at 7 finds nothing

  Scenario: The sweep removes a deletion with nothing below it
    When a key holds a value at 2 and a deletion at 4, the discard timestamp and the tombstone horizon are 6, and a full compaction and the sweep run
    Then no version of the key remains

  Scenario: A deletion above the tombstone horizon is kept
    When a key holds a value at 2 and a deletion at 5, the discard timestamp is 6 and the tombstone horizon is 4, and a full compaction and the sweep run
    Then the value at 2 is gone and the deletion at 5 remains

  Scenario: An undecodable item survives compaction
    When a stored value too short to hold the version prefix lies in a compaction
    Then the compaction completes, the store is not poisoned, and the item remains

  Scenario: An open reader holds the discard timestamp back
    When a reader at timestamp 3 is open and a discard timestamp of 6 is requested
    Then the discard timestamp used is at most 3 until the reader closes
