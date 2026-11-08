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

/-- Some member of the finite set satisfies `p`. -/
def Any (p : α → Prop) (s : FinSet α) : Prop :=
  Quot.liftOn s (fun l => ∃ x, x ∈ l ∧ p x) (fun _ _ h => propext ⟨
    fun ⟨x, hx, hp⟩ => ⟨x, (h x).mp hx, hp⟩, fun ⟨x, hx, hp⟩ => ⟨x, (h x).mpr hx, hp⟩⟩)

@[simp] theorem any_ofList (p : α → Prop) (l : List α) : Any p (ofList l) ↔ ∃ x, x ∈ l ∧ p x := Iff.rfl

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

/-! ## The collection operations of corollary 12.3 -/

/-- The empty set. -/
def empty : FinSet α := ofList []

/-- The set with one more member. -/
def insert (x : α) : FinSet α → FinSet α :=
  Quot.lift (fun l => ofList (x :: l)) (by
    intro l l' h
    apply Quot.sound
    intro y
    simp only [List.mem_cons]
    exact or_congr Iff.rfl (h y))

/-- The union of two sets. -/
def union (s t : FinSet α) : FinSet α :=
  Quot.liftOn s (fun l => Quot.liftOn t (fun l' => ofList (l ++ l')) (by
      intro l' l'' h
      apply Quot.sound
      intro y
      simp only [List.mem_append]
      exact or_congr Iff.rfl (h y)))
    (by
      intro l₁ l₂ h
      induction t using Quot.ind with
      | mk l' =>
        apply Quot.sound
        intro y
        simp only [List.mem_append]
        exact or_congr (h y) Iff.rfl)

/-- The members satisfying `p`. -/
def filter (p : α → Bool) : FinSet α → FinSet α :=
  Quot.lift (fun l => ofList (l.filter p)) (by
    intro l l' h
    apply Quot.sound
    intro y
    simp only [List.mem_filter]
    exact and_congr (h y) Iff.rfl)

theorem map_empty (f : α → β) : map f (empty : FinSet α) = empty := rfl

theorem map_insert (f : α → β) (x : α) (s : FinSet α) : map f (insert x s) = insert (f x) (map f s) := by
  induction s using Quot.ind with
  | mk l => rfl

theorem map_union (f : α → β) (s t : FinSet α) : map f (union s t) = union (map f s) (map f t) := by
  induction s using Quot.ind with
  | mk l =>
    induction t using Quot.ind with
    | mk l' => exact congrArg ofList (List.map_append f l l')

/-- Filtering the images by `p` is filtering by `p ∘ f` and taking the images. -/
theorem map_filter (f : α → β) (p : β → Bool) (s : FinSet α) :
    filter p (map f s) = map f (filter (fun x => p (f x)) s) := by
  induction s using Quot.ind with
  | mk l =>
    show ofList (List.filter p (List.map f l)) = ofList (List.map f (List.filter (fun x => p (f x)) l))
    congr 1
    induction l with
    | nil => rfl
    | cons x l ih =>
      simp only [List.map_cons, List.filter_cons]
      split <;> simp [ih]

/-- Two maps commute when their components do pointwise. -/
theorem map_comm {δ : Type} {f : α → β} {h : β → δ} {k : α → γ} {g : γ → δ} (w : ∀ x, h (f x) = g (k x))
    (s : FinSet α) : map h (map f s) = map g (map k s) := by
  rw [map_map, map_map]
  exact map_congr w s

/-- Mapping after filtering by `p` is filtering by `p'` after mapping, when `p` is `p'` through the map. -/
theorem map_filter_comm (f : α → β) {p : α → Bool} {p' : β → Bool} (w : ∀ x, p x = p' (f x)) (s : FinSet α) :
    map f (filter p s) = filter p' (map f s) := by
  rw [map_filter]
  exact congrArg (fun q => map f (filter q s)) (funext w)

end FinSet

/-! ## The same laws for lists -/

theorem list_filter_map {α β : Type} (f : α → β) (p : β → Bool) (l : List α) :
    List.filter p (List.map f l) = List.map f (List.filter (fun x => p (f x)) l) := by
  induction l with
  | nil => rfl
  | cons x l ih =>
    simp only [List.map_cons, List.filter_cons]
    split <;> simp [ih]

/-- Two maps of lists commute when their components do pointwise. -/
theorem list_map_comm {α β γ δ : Type} {f : α → β} {h : β → δ} {k : α → γ} {g : γ → δ}
    (w : ∀ x, h (f x) = g (k x)) (l : List α) : List.map h (List.map f l) = List.map g (List.map k l) := by
  rw [List.map_map, List.map_map]
  exact List.map_congr_left (fun x _ => w x)

/-- Mapping a list after filtering by `p` is filtering by `p'` after mapping, when `p` is `p'` through the map. -/
theorem list_map_filter_comm {α β : Type} (f : α → β) {p : α → Bool} {p' : β → Bool} (w : ∀ x, p x = p' (f x))
    (l : List α) : List.map f (List.filter p l) = List.filter p' (List.map f l) := by
  rw [list_filter_map]
  congr 1
  exact congrArg (List.filter · l) (funext w)

end ApgModel.Core
