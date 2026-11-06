# SPDX-License-Identifier: Apache-2.0
# Copyright (c) 2026. Marvin Hansen. All Rights Reserved.

Feature: Wire protocol: a parked request holds no role connection
  openspec/changes/stage-5-5-three-role-refactor/specs/wire-protocol/spec.md, through a real storage process beside a real
  frontend, a home played where it must hold its answer.

  Rule: A parked request holds no role connection

    Scenario: A write parked on a home's Reference holds no role connection
      When a Write on a Bind stream waits on a home's answer, and a second request arrives on its connection
      Then the second request is answered while the Write waits, and the Write is answered Held on its stream once the home answers

    Scenario: A one-frame write parked on a home's Reference holds no role connection
      When a DirectWrite waits on a home's answer, and a second request arrives on its connection
      Then the second request is answered while the DirectWrite waits, and its Commit reply follows once the home answers

    Scenario: A write behind a read's wait holds no role connection
      When a Scan of a transaction waits on a home's answer, a Write follows on the transaction's stream, and a second request arrives on the connection
      Then the second request is answered while the Write waits its turn, and the Write is answered Held once the home answers

    Scenario: What follows a parked write on its stream keeps its order
      When a Write waits on a home's answer, and a second Write and the Commit follow on its stream
      Then nothing is answered on the stream until the home answers, and then Held, Held and the Commit, in that order, and the commit holds both writes

    Scenario: A home's refusal drops what is parked behind the write
      When a Write waits on a home's answer, a second Write follows on its stream, and the home refuses the registration
      Then the stream ends with the home's refusal alone, and the transaction holds nothing

    Scenario: A Cancel reaches a parked write
      When a Write waits on a home's answer, and its stream is cancelled
      Then the partition holds no such transaction by the time a later request on the connection is answered, and no Held follows once the home answers

    Scenario: A connection closing with a write parked aborts its transaction
      When a Write waits on a home's answer, and its connection closes
      Then the partition no longer holds the transaction

    Scenario: A connection at its parked bound stops reading
      When a Write's frame takes its connection's parked bytes to `storage.held_bytes`, the Write waiting on a home's answer, and a second request follows on the connection
      Then the second request is unanswered until the home answers and is then answered, and nothing is refused

    Scenario: Writes parked on each other's homes both answer
      When both storage processes hold `storage.held_bytes` of 1, and a Write on partition 1 names an entity partition 3 owns while a Write on partition 3 names an entity partition 1 owns
      Then both are answered Held and both transactions commit

    Scenario: A stop with a write parked answers or closes
      When a storage process stops while a Write on a Bind stream waits on a home's answer
      Then the stream is answered with code 17 or the connection closes
