/-
 SPDX-License-Identifier: Apache-2.0
 Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
-/

/-!
# Finite sets

`SCHEMA-APG.md` definition 12.1 gives `set t` the finite subsets of `V_E(t)`, two sets equal extensionally. Without
Mathlib's `Finset`, the core takes a finite set as a list up to having the same members, Lean's `Quot`: one set is one
value, whatever order or repetition wrote it, and no order on the members is needed. The executable layer holds a set
as a list without duplicates in a decidable order, which names the same set.
-/

namespace ApgModel.Core

/-- Two lists hold the same members, whatever their order and repetitions. -/
def SameMembers (α : Type) (l l' : List α) : Prop := ∀ x, x ∈ l ↔ x ∈ l'

/-- The finite sets of `α`: lists up to having the same members. -/
def FinSet (α : Type) : Type := Quot (SameMembers α)

namespace FinSet

variable {α β γ : Type}

/-- The set of a list's members. -/
def ofList (l : List α) : FinSet α := Quot.mk _ l

/-- Membership in a finite set. -/
def Mem (x : α) (s : FinSet α) : Prop :=
  Quot.liftOn s (fun l => x ∈ l) (fun _ _ h => propext (h x))

/-- Two finite sets with the same members are equal: equality of sets is extensional. -/
theorem ext {s t : FinSet α} (h : ∀ x, Mem x s ↔ Mem x t) : s = t := by
  induction s using Quot.ind with
  | mk l =>
    induction t using Quot.ind with
    | mk l' => exact Quot.sound h

/-- The image of a finite set under `f`. -/
def map (f : α → β) : FinSet α → FinSet β :=
  Quot.lift (fun l => ofList (l.map f)) (by
    intro l l' h
    apply Quot.sound
    intro y
    simp only [List.mem_map]
    constructor
    · rintro ⟨x, hx, rfl⟩
      exact ⟨x, (h x).mp hx, rfl⟩
    · rintro ⟨x, hx, rfl⟩
      exact ⟨x, (h x).mpr hx, rfl⟩)

@[simp] theorem map_ofList (f : α → β) (l : List α) : map f (ofList l) = ofList (l.map f) := rfl

theorem map_congr {f g : α → β} (h : ∀ x, f x = g x) (s : FinSet α) : map f s = map g s := by
  have : f = g := funext h
  rw [this]

theorem map_id' (s : FinSet α) : map (fun x => x) s = s := by
  induction s using Quot.ind with
  | mk l => exact congrArg ofList (List.map_id' l)

theorem map_map (f : α → β) (g : β → γ) (s : FinSet α) : map g (map f s) = map (fun x => g (f x)) s := by
  induction s using Quot.ind with
  | mk l => exact congrArg ofList (List.map_map ..)

end FinSet

end ApgModel.Core
