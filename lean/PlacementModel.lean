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
to each partition in order, catalog versions and placement versions announced with a close, commits stamped exactly one
above the latest delivered close, registrations with their replies, marks and generations, referrer sets under MVCC, commits writing an
edge's forward and local reverse entries together, reverse reads at closed epochs, tombstones and reclamation, under
every interleaving, delay, duplication, reordering or loss of messages and crashes after durable steps. The
specification is `openspec/specs/transaction-verification/spec.md`, "A Lean
model of placement".

| Module | Holds |
|---|---|
| `PlacementModel.Basic` | Edge and member events, the latest event, visibility at an epoch |
| `PlacementModel.State` | The world, the protocol variants, the state, messages, the steps, the placement in effect |
| `PlacementModel.Base` | The invariants every variant keeps, P24 as written included: `BaseInv` and `PlaceInv` |
| `PlacementModel.Repaired` | The invariant of the repaired protocol: no edge outlives its referrer-set member |
| `PlacementModel.Theorems` | Rules 4, 5 and 6, 9 and 11, placement activation and its fixity, each with its simulator invariant |
| `PlacementModel.Counterexamples` | Runs refuting reclamation without each repair; the specification's stale report |

Rules 4 and 11 hold for P24 as written, and so does rule 11's analogue for placement: a placement version is committed
as pending, the next close announces the oldest pending one at one above its own number, a partition receives it with
that close, and no commit is stamped at or above its effective epoch without it. Rules 5, 6 and 9 hold for `Protocol.repaired`, which adds three rules to
reclamation: the home removes a member only at an epoch at or above the report's, only a partition holding every bucket
of a group reports for it, and a member's generations only rise. The first two are refuted without them by concrete
runs; the notes of group 6b give the third's reason.

The assumptions about the cluster are stated where they are used: a partition's latest delivered close survives its
crash, and a move hands a bucket over only once its earlier holder has closed the epochs it held it (`Handover`), a
premise of every write step. The placement map `World.place` is an oracle by epoch; `Placed` states that it is the
layout of the placement version in effect at every announced epoch, and is a premise of the theorems that connect the
two. The model proves that the version in effect at an announced epoch, and so every partition's placement at or below
its open epoch, never changes later (`placement_in_effect_is_fixed`), which is what `Handover` and `ReadReady` read; it
does not prove `Handover`, which needs the hand-over protocol modelled.
-/
