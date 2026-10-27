/-
 SPDX-License-Identifier: Apache-2.0
 Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
-/
import RoundModel.Basic
import RoundModel.Txn
import RoundModel.History
import RoundModel.Round
import RoundModel.Log
import RoundModel.Node
import RoundModel.Theorems

/-!
# The round protocol of stage 5

A model of the commit step, proved for every number of transactions, rounds and crashes: committed transactions are
strictly serializable; no acknowledged commit is lost across a crash, under the assumption that a completed sync is
durable; replaying a round is idempotent; and a session's request commits at most once. The specification is
`openspec/changes/stage-5-transactions-and-binary-protocol/specs/transaction-verification/spec.md`.

| Module | Holds |
|---|---|
| `RoundModel.Basic` | Keys, values, writes, states, and applying writes |
| `RoundModel.Txn` | Read sets with ranges, commit-time operations, requests, resolution, commits |
| `RoundModel.History` | Serial states, snapshots, `Admissible` and `Valid` histories |
| `RoundModel.Round` | Exact validation, sessions and versions: deciding a round |
| `RoundModel.Log` | Rounds, the store, replay and recovery, idempotence |
| `RoundModel.Node` | The node's steps: rounds, sync, failed sync, apply, crash; the invariant |
| `RoundModel.Theorems` | The four properties, each with the simulator invariant restating it |

The one assumption about storage is `SyncDurable`: a crash keeps at least the synced prefix of the log.
-/
