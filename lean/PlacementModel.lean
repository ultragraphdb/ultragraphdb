/-
 SPDX-License-Identifier: Apache-2.0
 Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
-/
import PlacementModel.Basic
import PlacementModel.State
import PlacementModel.Base
import PlacementModel.Repaired
import PlacementModel.Theorems
import PlacementModel.Counterexamples

/-!
# The placement protocol of stage 5.5 (P24, one owner per fact)

A model of partitions over bucket groups with a placement map that changes by epoch, the sequencer's closes delivered
to each partition in order, catalog versions announced with a close, commits stamped exactly one above the latest
delivered close, registrations with their replies, marks and generations, referrer sets under MVCC, commits writing an
edge's forward and local reverse entries together, reverse reads at closed epochs, tombstones and reclamation, under
every interleaving, delay, duplication, reordering or loss of messages and crashes after durable steps. The
specification is `openspec/changes/stage-5-5-three-role-refactor/specs/transaction-verification/spec.md`, "A Lean
model of placement".

| Module | Holds |
|---|---|
| `PlacementModel.Basic` | Edge and member events, the latest event, visibility at an epoch |
| `PlacementModel.State` | The world, the protocol variants, the state, messages, the steps |
| `PlacementModel.Base` | The invariant every variant keeps, P24 as written included |
| `PlacementModel.Repaired` | The invariant of the repaired protocol: no edge outlives its referrer-set member |
| `PlacementModel.Theorems` | Rules 4, 5 and 6, 9 and 11, each with the simulator invariant restating it |
| `PlacementModel.Counterexamples` | Runs refuting reclamation without each repair; the specification's stale report |

Rules 4 and 11 hold for P24 as written. Rules 5, 6 and 9 hold for `Protocol.repaired`, which adds three rules to
reclamation: the home removes a member only at an epoch at or above the report's, only a partition holding every bucket
of a group reports for it, and a member's generations only rise. The first two are refuted without them by concrete
runs; the notes of group 6b give the third's reason.

The assumptions about the cluster are stated where they are used: a partition's latest delivered close survives its
crash, and a move hands a bucket over only once its earlier holder has closed the epochs it held it (`Handover`).
-/
