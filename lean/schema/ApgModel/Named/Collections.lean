/-
 SPDX-License-Identifier: Apache-2.0
 Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
-/
import ApgModel.Named.Apg

/-!
# Collections of references, symmetric relations and unnesting

`SCHEMA-APG.md` definitions 12.5, 12.6 and 12.7 over finite APGs.

- A set-valued role `r` of a relation `R` is reified as the role label `R_r`, a record of the instance and one
  participant (`roleLabelType`); section 14.2's constraints, no two equal elements and every instance named by one,
  complete it in `ApgModel.Catalog.Constraints`.
- A `@symmetric` relation's element is read up to the swap of its two role fields: its navigation reaches the other
  participant from either one (`symNav`), so `q` is reached from `p` exactly when `p` is reached from `q`
  (`symNav_symm`).
- The unnest of a label's collection field yields one derived element per member, each naming its origin and the
  member, and a list's members their positions (`unnest`); each derived element has exactly the origin it names, and
  its member is in that origin's collection (`unnest_origin`). `Δ` along the derived label's image is `ApgModel.Core`'s
  proposition 9.5.
-/

namespace ApgModel.Named

variable {T : Typeside} {L : Type}

/-- Definition 12.5: the type of the role label `R_r`, `rec { instance ↦ lbl R, participant ↦ ⟦U⟧ }`. -/
def roleLabelType (R : L) (U : NTy T.P L) : NTy T.P L := .record [("instance", .lbl R), ("participant", U)]

theorem roleLabelType_wf (R : L) (U : NTy T.P L) (hU : U.WF) : (roleLabelType R U).WF :=
  ⟨by show List.Pairwise (· < ·) ["instance", "participant"]; decide, trivial, hU, trivial⟩

/-- Definition 12.7: the derived label of a set field's unnest, `rec { origin ↦ lbl l, member ↦ t }`. -/
def unnestSetType (l : L) (t : NTy T.P L) : NTy T.P L := .record [("member", t), ("origin", .lbl l)]

/-- Definition 12.7: the derived label of a list field's unnest, with the member's position. -/
def unnestListType (l : L) (t : NTy T.P L) (pos : T.P) : NTy T.P L :=
  .record [("member", t), ("origin", .lbl l), ("position", .prim pos)]

theorem unnestSetType_wf (l : L) (t : NTy T.P L) (ht : t.WF) : (unnestSetType l t).WF :=
  ⟨by show List.Pairwise (· < ·) ["member", "origin"]; decide, ht, trivial, trivial⟩

theorem unnestListType_wf (l : L) (t : NTy T.P L) (pos : T.P) (ht : t.WF) : (unnestListType l t pos).WF :=
  ⟨by show List.Pairwise (· < ·) ["member", "origin", "position"]; decide, ht, trivial, trivial, trivial⟩

variable [DecidableEq L] {S : NSchema T.P L}

/-! ## Symmetric relations -/

/-- The two participants of a binary relation element's value term, in its fields' order. -/
def VTerm.pair? : VTerm T → Option (Nat × Nat)
  | .record [(_, .elem a), (_, .elem b)] => some (a, b)
  | _ => none

/-- The participant a symmetric relation's element reaches from `p`: the other field's. -/
def symOther (p : Nat) (v : VTerm T) : Option Nat :=
  (VTerm.pair? v).bind fun (a, b) => if a = p then some b else if b = p then some a else none

theorem symOther_symm (p q : Nat) (v : VTerm T) : symOther p v = some q ↔ symOther q v = some p := by
  unfold symOther
  cases VTerm.pair? v with
  | none => simp
  | some ab =>
    obtain ⟨a, b⟩ := ab
    simp only [Option.some_bind]
    by_cases ha : a = p <;> by_cases hb : b = p <;> by_cases ha' : a = q <;> by_cases hb' : b = q <;>
      simp [ha, hb, ha', hb'] <;> omega

/-- The navigation of a symmetric relation `R` from the element `p`: the other participant of every element of `R`
naming `p`. -/
def symNav (G : FinApg S) (R : L) (p : Nat) : List Nat :=
  G.elems.filterMap fun x => if x.2.1 = R then symOther p x.2.2 else none

/-- Definition 12.6: a symmetric relation is read up to the swap of its participants, so its navigation reaches `q`
from `p` exactly when it reaches `p` from `q`. -/
theorem symNav_symm (G : FinApg S) (R : L) (p q : Nat) : q ∈ symNav G R p ↔ p ∈ symNav G R q := by
  simp only [symNav, List.mem_filterMap]
  constructor
  · rintro ⟨x, hx, hf⟩
    refine ⟨x, hx, ?_⟩
    by_cases hl : x.2.1 = R
    · simp only [hl, if_true] at hf ⊢
      exact (symOther_symm p q x.2.2).mp hf
    · simp [hl] at hf
  · rintro ⟨x, hx, hf⟩
    refine ⟨x, hx, ?_⟩
    by_cases hl : x.2.1 = R
    · simp only [hl, if_true] at hf ⊢
      exact (symOther_symm p q x.2.2).mpr hf
    · simp [hl] at hf

/-! ## Unnesting -/

/-- A record term's field named `n`. -/
def VTerm.field? (n : Name) : VTerm T → Option (VTerm T)
  | .record fs => (fs.find? fun f => f.1 == n).map Prod.snd
  | _ => none

/-- A collection term's members. -/
def VTerm.members : VTerm T → List (VTerm T)
  | .set vs | .list vs => vs
  | _ => []

/-- The members of the field `n` of a value term. -/
def membersOf (v : VTerm T) (n : Name) : List (VTerm T) :=
  match v.field? n with
  | some c => c.members
  | none => []

/-- A list's members with their positions from `k`. -/
def withPositions {α : Type} : Nat → List α → List (Nat × α)
  | _, [] => []
  | k, x :: xs => (k, x) :: withPositions (k + 1) xs

theorem mem_withPositions {α : Type} {k i : Nat} {m : α} : {xs : List α} → (i, m) ∈ withPositions k xs → m ∈ xs
  | [], h => by simp [withPositions] at h
  | x :: xs, h => by
    simp only [withPositions, List.mem_cons, Prod.mk.injEq] at h
    rcases h with ⟨_, rfl⟩ | h
    · exact List.mem_cons_self _ _
    · exact List.mem_cons_of_mem _ (mem_withPositions h)

/-- Definition 12.7: the unnest of the field `n` of the label `l`: for every element of `l`, one entry per member of
its field `n`, naming the element, the member's position and the member. -/
def unnest (G : FinApg S) (l : L) (n : Name) : List (Nat × Nat × VTerm T) := go G.elems
where
  /-- The entries of a list of elements. -/
  go : List (Nat × L × VTerm T) → List (Nat × Nat × VTerm T)
    | [] => []
    | (o, l', v) :: xs =>
      (if l' = l then (withPositions 0 (membersOf v n)).map fun p => (o, p.1, p.2) else []) ++ go xs

/-- Definition 12.7: every entry of the unnest has one origin, an element of `l` whose field `n` holds the member. -/
theorem unnest_origin (G : FinApg S) (l : L) (n : Name) {o i : Nat} {m : VTerm T}
    (h : (o, i, m) ∈ unnest G l n) : ∃ v, (o, l, v) ∈ G.elems ∧ m ∈ membersOf v n :=
  go G.elems h
where
  go : (xs : List (Nat × L × VTerm T)) → (o, i, m) ∈ unnest.go l n xs → ∃ v, (o, l, v) ∈ xs ∧ m ∈ membersOf v n
    | [], h => by simp [unnest.go] at h
    | (o', l', v) :: xs, h => by
      simp only [unnest.go, List.mem_append] at h
      rcases h with h | h
      · by_cases hl : l' = l
        · simp only [hl, if_true, List.mem_map] at h
          obtain ⟨⟨i', m'⟩, hp, he⟩ := h
          simp only [Prod.mk.injEq] at he
          obtain ⟨rfl, rfl, rfl⟩ := he
          exact ⟨v, by rw [hl]; exact List.mem_cons_self _ _, mem_withPositions hp⟩
        · simp [hl] at h
      · obtain ⟨w, hw, hm⟩ := go xs h
        exact ⟨w, List.mem_cons_of_mem _ hw, hm⟩

end ApgModel.Named
