/-
 SPDX-License-Identifier: Apache-2.0
 Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
-/
import PlacementModel.Theorems

/-!
# Two repairs P24's reclamation needs, each refuted without it

Each section is a concrete run, checked step by step, of the protocol with every repair but one, ending in a state
where an edge to an entity is visible at an epoch its group is not a member of the entity's referrer set: the
reclamation theorem fails, and a reverse read at a closed epoch misses the edge. The notes of group 6b give both runs
as traces for group 7's simulator.

1. `removal_below_the_report_epoch_loses_an_edge` — without `removalAwaitsReportEpoch`. A home whose delivered closes
   lag behind the reporter removes the member at its own open epoch, below the report's epoch, so the member vanishes
   at epochs where the reporter's last edge was still present.
2. `split_group_report_loses_an_edge` — without `reportNeedsWholeGroup`. A bucket group whose buckets two partitions
   hold shares one member and one generation: the second registrant's registration is a repeat, and the first
   partition's report of that generation removes the member while the second still holds an edge.
-/

namespace PlacementModel

namespace Counterexample1

/-! ## 1. Removal below the report's epoch

Partition 0 is the home of entity 5; partition 1 holds every subject, and subject 0 is bucket group 0. Six closes are
issued; partition 1 receives five of them, the home none. -/

/-- Every subject is its own group, held by partition 1; every entity's home is partition 0. -/
def W : World := { grp := fun s => s, place := fun _ _ => 1, home := fun _ => 0 }

/-- Every repair but the removal waiting for the report's epoch. -/
def P : Protocol := ⟨false, true, true⟩

def c6 : State := State.init.close.close.close.close.close.close
def d5 : State := (((((c6.deliver 1).deliver 1).deliver 1).deliver 1).deliver 1)
/-- Partition 1 registers subject 0's group for entity 5. -/
def s1 : State := d5.register 1 5 0
/-- The home, at open epoch 1, adds group 0 at generation 1 and replies with close 0. -/
def s2 : State := s1.accept W P 1 5 0 0 0
/-- Partition 1, at open epoch 6, commits the edge `0 → 5` under the reply. -/
def s3 : State := s2.insertReply W 1 0 5 1 0
/-- Partition 1 receives close 6. -/
def s4 : State := s3.deliver 1
/-- At epoch 7 it deletes the edge… -/
def s5 : State := s4.delete 1 0 5
/-- …and reports no edges as of epoch 7 at generation 1. -/
def s6 : State := s5.report 1 5 0 ⟨1, 1⟩
/-- The home, still at open epoch 1, applies the report: generation 1 is current, so it removes group 0 at epoch 1. -/
def s7 : State := s6.onReport W P 5 0 1 7
/-- The home receives closes 1 to 6, so a reverse read at epoch 6 can answer. -/
def s8 : State := (((((s7.deliver 0).deliver 0).deliver 0).deliver 0).deliver 0).deliver 0

theorem reach_c6 : Reachable W P c6 := by
  unfold c6
  exact .tail (.tail (.tail (.tail (.tail (.tail (.refl _) (.close _)) (.close _)) (.close _)) (.close _))
    (.close _)) (.close _)

theorem reach_d5 : Reachable W P d5 := by
  unfold d5
  exact .tail (.tail (.tail (.tail (.tail reach_c6 (.deliver _ 1 (by decide))) (.deliver _ 1 (by decide)))
    (.deliver _ 1 (by decide))) (.deliver _ 1 (by decide))) (.deliver _ 1 (by decide))

theorem reach_s8 : Reachable W P s8 := by
  have h1 : Reachable W P s1 := .tail reach_d5 (.register _ 1 5 0 (by decide))
  have h2 : Reachable W P s2 := .tail h1 (.accept _ 1 5 0 0 0 (by decide))
  have h3 : Reachable W P s3 := .tail h2 (.insertReply _ 1 0 5 1 0 0 rfl (by decide) (by decide) (by decide)
    (by decide) (fun t ht => by
      show t ≤ s2.delivered 1
      have : s2.delivered 1 = 5 := by decide
      have : s2.openEpoch 1 = 6 := by decide
      omega))
  have h4 : Reachable W P s4 := .tail h3 (.deliver _ 1 (by decide))
  have h5 : Reachable W P s5 := .tail h4 (.delete _ 1 0 5 rfl (fun t ht => by
    show t ≤ s4.delivered 1
    have : s4.delivered 1 = 6 := by decide
    have : s4.openEpoch 1 = 7 := by decide
    omega))
  have h6 : Reachable W P s6 := .tail h5 (.report _ 1 5 0 ⟨1, 1⟩ (by decide) (fun s hs _ => by
    change s = 0 at hs; subst hs; decide) (fun _ _ _ _ => rfl))
  have h7 : Reachable W P s7 := .tail h6 (.reclaim _ 1 5 0 1 7 (by decide))
  exact .tail (.tail (.tail (.tail (.tail (.tail h7 (.deliver _ 0 (by decide))) (.deliver _ 0 (by decide)))
    (.deliver _ 0 (by decide))) (.deliver _ 0 (by decide))) (.deliver _ 0 (by decide))) (.deliver _ 0 (by decide))

/-- No member of `R(5)` is visible at epoch 6 after the removal: group 0's latest event at or below 6 is the removal,
and no other group has an event. -/
theorem no_member_at_6 (g : Nat) : memAt s8.mems 5 g 6 = false := by
  have hm : s8.mems = [⟨5, 0, 1, 1, true⟩, ⟨5, 0, 1, 1, false⟩] := by decide
  rw [hm]
  by_cases hg : g = 0
  · subst hg; decide
  · simp [memAt, visible, lastWhere, memKey, hg, Ne.symm hg]

end Counterexample1

/-- **Without `removalAwaitsReportEpoch`, reclamation loses an edge.** In a reachable state of the protocol with the
other two repairs, the edge `0 → 5` is visible at epoch 6, its group is no member of `R(5)` at 6, and a reverse read of
entity 5 at the closed epoch 6 misses it. -/
theorem removal_below_the_report_epoch_loses_an_edge :
    Reachable Counterexample1.W Counterexample1.P Counterexample1.s8 ∧
      edgeAt Counterexample1.s8.edges 0 5 6 = true ∧
      memAt Counterexample1.s8.mems 5 (Counterexample1.W.grp 0) 6 = false ∧
      ReadReady Counterexample1.W Counterexample1.s8 5 6 ∧
      reverseRead Counterexample1.W Counterexample1.s8 5 6 0 = false := by
  refine ⟨Counterexample1.reach_s8, by decide, Counterexample1.no_member_at_6 0, ⟨by decide, ?_⟩, ?_⟩
  · intro s hs; rw [Counterexample1.no_member_at_6] at hs; cases hs
  · simp [reverseRead, Counterexample1.no_member_at_6]

namespace Counterexample2

/-! ## 2. A report from a partition holding part of a group

Partition 0 is the home of entity 5. Every subject is in bucket group 0; partition 1 holds subject 0 and partition 2
every other subject, as a range boundary inside a group gives. Every partition's open epoch stays 1. -/

/-- One group, split between partitions 1 and 2; every entity's home is partition 0. -/
def W : World := { grp := fun _ => 0, place := fun _ s => if s = 0 then 1 else 2, home := fun _ => 0 }

/-- Every repair but the whole-group requirement on reports. -/
def P : Protocol := ⟨true, false, true⟩

/-- Partition 1 registers group 0 for entity 5; the home adds it at generation 1. -/
def s2 : State := (State.init.register 1 5 0).accept W P 1 5 0 0 0
/-- Partition 2 registers group 0 naming generation 0: a repeat, answered with generation 1. -/
def s4 : State := (s2.register 2 5 0).accept W P 2 5 0 0 0
/-- Both commit an edge to 5 under their replies: `0 → 5` at partition 1, `1 → 5` at partition 2. -/
def s6 : State := (s4.insertReply W 1 0 5 1 0).insertReply W 2 1 5 1 0
/-- Partition 1 deletes its edge and reports no edges from its subjects of group 0 at generation 1. -/
def s8 : State := (s6.delete 1 0 5).report 1 5 0 ⟨1, 1⟩
/-- The home applies the report: generation 1 is current, so it removes group 0. -/
def s9 : State := s8.onReport W P 5 0 1 1

theorem reach_s9 : Reachable W P s9 := by
  have h1 : Reachable W P (State.init.register 1 5 0) := .tail (.refl _) (.register _ 1 5 0 (by decide))
  have h2 : Reachable W P s2 := .tail h1 (.accept _ 1 5 0 0 0 (by decide))
  have h3 : Reachable W P (s2.register 2 5 0) := .tail h2 (.register _ 2 5 0 (by decide))
  have h4 : Reachable W P s4 := .tail h3 (.accept _ 2 5 0 0 0 (by decide))
  have h5 : Reachable W P (s4.insertReply W 1 0 5 1 0) :=
    .tail h4 (.insertReply _ 1 0 5 1 0 0 rfl (by decide) (by decide) (by decide) (by decide)
      (fun t ht => by have : s4.openEpoch 1 = 1 := by decide
                      omega))
  have h6 : Reachable W P s6 :=
    .tail h5 (.insertReply _ 2 1 5 1 0 0 rfl (by decide) (by decide) (by decide) (by decide)
      (fun t ht => by have : (s4.insertReply W 1 0 5 1 0).openEpoch 2 = 1 := by decide
                      omega))
  have h7 : Reachable W P (s6.delete 1 0 5) :=
    .tail h6 (.delete _ 1 0 5 rfl (fun t ht => by have : s6.openEpoch 1 = 1 := by decide
                                                  omega))
  have h8 : Reachable W P s8 := .tail h7 (.report _ 1 5 0 ⟨1, 1⟩ (by decide) (fun s _ hp => by
    have hs : s = 0 := by
      by_cases h : s = 0
      · exact h
      · change (if s = 0 then 1 else 2) = 1 at hp; rw [if_neg h] at hp; cases hp
    subst hs; decide) (fun h => by cases h))
  exact .tail h8 (.reclaim _ 1 5 0 1 1 (by decide))

/-- No member of `R(5)` is visible at epoch 1 after the removal. -/
theorem no_member_at_1 (g : Nat) : memAt s9.mems 5 g 1 = false := by
  have hm : s9.mems = [⟨5, 0, 1, 1, true⟩, ⟨5, 0, 1, 1, false⟩] := by decide
  rw [hm]
  by_cases hg : g = 0
  · subst hg; decide
  · simp [memAt, visible, lastWhere, memKey, hg, Ne.symm hg]

end Counterexample2

/-- **Without `reportNeedsWholeGroup`, reclamation loses an edge.** In a reachable state of the protocol with the
other two repairs, partition 2's edge `1 → 5` is visible at epoch 1, its group is no member of `R(5)` at 1, and a
reverse read of entity 5 at epoch 1, once every partition has closed it, misses it. -/
theorem split_group_report_loses_an_edge :
    Reachable Counterexample2.W Counterexample2.P Counterexample2.s9 ∧
      edgeAt Counterexample2.s9.edges 1 5 1 = true ∧
      memAt Counterexample2.s9.mems 5 (Counterexample2.W.grp 1) 1 = false ∧
      reverseRead Counterexample2.W Counterexample2.s9 5 1 1 = false := by
  refine ⟨Counterexample2.reach_s9, by decide, Counterexample2.no_member_at_1 0, ?_⟩
  simp [reverseRead, Counterexample2.no_member_at_1]

namespace StaleReport

/-! ## The stale report of the specification's scenario, in the repaired protocol

Partition 0 is the home of entity 5; partition 1 holds subject 0, bucket group 0. Every open epoch stays 1. Partition 1
raises the member to generation 3 through three registrations, deletes its edge and reports generation 3, then
re-registers before the report arrives; the home raises the member to 4, partition 1 commits a new edge, and the stale
report arrives. A later report of generation 4 does remove the member. -/

def W : World := { grp := fun s => s, place := fun _ _ => 1, home := fun _ => 0 }

/-- Before its first commit, every partition's open epoch is 1, so the hand-over is vacuous. -/
theorem handover_at_one {σ : State} {p s : Nat} (hd : σ.delivered p = 0) : Handover W σ.delivered s (σ.openEpoch p) :=
  fun t ht => by
    have : t = 0 := by simp only [State.openEpoch, hd] at ht; omega
    subst this; exact Nat.zero_le _

/-- One round: partition 1 registers naming what it saw, the home accepts, and partition 1 commits under the reply. -/
def round (σ : State) (gen : Nat) : State :=
  ((σ.register 1 5 0).accept W Protocol.repaired 1 5 0 (σ.seen 1 5 0) (σ.mver 1 5 0)).insertReply W 1 0 5 gen 0

def g1 : State := round State.init 1
def g2 : State := round g1 2
def g3 : State := round g2 3
/-- Partition 1 deletes its edge and reports generation 3. -/
def rep3 : State := (g3.delete 1 0 5).report 1 5 0 ⟨3, 1⟩
/-- It re-registers naming 3 before the report arrives; the home raises the member to 4; it commits a new edge. -/
def g4 : State := round rep3 4
/-- The stale report arrives. -/
def stale : State := g4.onReport W Protocol.repaired 5 0 3 1
/-- Later partition 1 deletes the new edge and reports generation 4, which the home applies. -/
def rep4 : State := (g4.delete 1 0 5).report 1 5 0 ⟨4, 1⟩
def removed : State := rep4.onReport W Protocol.repaired 5 0 4 1

theorem reach_round {σ : State} (h : Reachable W Protocol.repaired σ) (gen : Nat)
    (hreply : Msg.reply 1 5 0 gen 0 (σ.mver 1 5 0) false ∈
      ((σ.register 1 5 0).accept W Protocol.repaired 1 5 0 (σ.seen 1 5 0) (σ.mver 1 5 0)).msgs)
    (hmv : σ.mver 1 5 0 =
      ((σ.register 1 5 0).accept W Protocol.repaired 1 5 0 (σ.seen 1 5 0) (σ.mver 1 5 0)).mver 1 5 0)
    (hd' : ((σ.register 1 5 0).accept W Protocol.repaired 1 5 0 (σ.seen 1 5 0) (σ.mver 1 5 0)).delivered 1 = 0) :
    Reachable W Protocol.repaired (round σ gen) := by
  have h1 := Steps.tail h (.register σ 1 5 0 (by decide))
  have h2 := Steps.tail h1 (.accept _ 1 5 0 (σ.seen 1 5 0) (σ.mver 1 5 0)
    (List.mem_append_right _ (List.mem_singleton_self _)))
  exact .tail h2 (.insertReply _ 1 0 5 gen 0 (σ.mver 1 5 0) rfl (by decide) hreply hmv (Nat.zero_le _)
    (handover_at_one hd'))

theorem reach_g4 : Reachable W Protocol.repaired g4 := by
  have h1 : Reachable W Protocol.repaired g1 := reach_round (.refl _) 1 (by decide) (by decide) (by decide)
  have h2 : Reachable W Protocol.repaired g2 := reach_round h1 2 (by decide) (by decide) (by decide)
  have h3 : Reachable W Protocol.repaired g3 := reach_round h2 3 (by decide) (by decide) (by decide)
  have h4 : Reachable W Protocol.repaired (g3.delete 1 0 5) :=
    .tail h3 (.delete _ 1 0 5 rfl (handover_at_one (by decide)))
  have h5 : Reachable W Protocol.repaired rep3 := .tail h4 (.report _ 1 5 0 ⟨3, 1⟩ (by decide)
    (fun s hs _ => by change s = 0 at hs; subst hs; decide) (fun _ _ _ _ => rfl))
  exact reach_round h5 4 (by decide) (by decide) (by decide)

theorem reach_removed : Reachable W Protocol.repaired removed := by
  have h1 : Reachable W Protocol.repaired (g4.delete 1 0 5) :=
    .tail reach_g4 (.delete _ 1 0 5 rfl (handover_at_one (by decide)))
  have h2 : Reachable W Protocol.repaired rep4 := .tail h1 (.report _ 1 5 0 ⟨4, 1⟩ (by decide)
    (fun s hs _ => by change s = 0 at hs; subst hs; decide) (fun _ _ _ _ => rfl))
  exact .tail h2 (.reclaim _ 1 5 0 4 1 (by decide))

end StaleReport

/-- **The specification's stale report, as a run.** Partition 1 reports generation 3, re-registers before the report
arrives, and the home raises the member to generation 4; when the report arrives, the home keeps the member, and the
partition's new edge `0 → 5` is visible at epoch 1 with its group in `R(5)`. The later report of generation 4, made
once the new edge is deleted, removes the member: reclamation is reachable in the repaired protocol. -/
theorem a_stale_reclamation_report_removes_nothing_trace :
    Reachable StaleReport.W Protocol.repaired StaleReport.g4 ∧
      Msg.report 1 5 0 3 1 ∈ StaleReport.g4.msgs ∧
      StaleReport.g4.curGen 5 0 = 4 ∧
      StaleReport.stale = StaleReport.g4 ∧
      edgeAt StaleReport.stale.edges 0 5 1 = true ∧
      memAt StaleReport.stale.mems 5 0 1 = true ∧
      Reachable StaleReport.W Protocol.repaired StaleReport.removed ∧
      memAt StaleReport.removed.mems 5 0 1 = false ∧
      edgeAt StaleReport.removed.edges 0 5 1 = false :=
  ⟨StaleReport.reach_g4, by decide, by decide,
    (a_stale_reclamation_report_removes_nothing (r := 1)
      (show memCur StaleReport.g4.mems 5 0 = some ⟨5, 0, 1, 4, true⟩ by decide) (by decide)).1 _,
    by decide, by decide, StaleReport.reach_removed, by decide, by decide⟩

end PlacementModel
