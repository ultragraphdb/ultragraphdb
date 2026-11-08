/-
 SPDX-License-Identifier: Apache-2.0
 Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
-/
import ApgModel.Typeside
import ApgModel.Core.Ty
import ApgModel.Core.FinSet

/-!
# Values

`SCHEMA-APG.md` definitions 5.1, 5.2 and 12.1, and propositions 5.3 and 12.2. For a family `E` of elements, one set
per label, `V T E t` is the set of values of the type `t`: a reference to an element at a label, a primitive value at a
primitive type, built by `×`, `+`, lists and finite sets elsewhere. A morphism of families `h`, one function per label,
acts on values by `V.map`, mapping every reference and leaving everything else as it is.

The model follows the second generation of the paper's Coq development (`APG.v`'s `apg`): a family `E : L → Type`
and values typed by their type. Lean's types are the sets, so `V` computes a Lean type by recursion on `t`.
-/

namespace ApgModel.Core

variable (T : Typeside) {L : Type}

/-- `V_E(t)`, the values of the type `t` over the family `E`, definitions 5.1 and 12.1. -/
def V (E : L → Type) : Ty T.P L → Type
  | .zero => Empty
  | .one => Unit
  | .sum a b => V E a ⊕ V E b
  | .prod a b => V E a × V E b
  | .prim p => T.PV p
  | .lbl l => E l
  | .list t => List (V E t)
  | .set t => FinSet (V E t)

variable {T}

/-- `V_h(t)`, the action of a morphism of families `h : E → E'` on values, definitions 5.2 and 12.1: `h` at a
reference, the identity at a primitive value, componentwise elsewhere. -/
def V.map {E E' : L → Type} (h : ∀ l, E l → E' l) : (t : Ty T.P L) → V T E t → V T E' t
  | .zero, x => nomatch x
  | .one, x => x
  | .sum a _, .inl x => .inl (V.map h a x)
  | .sum _ b, .inr y => .inr (V.map h b y)
  | .prod a b, (x, y) => (V.map h a x, V.map h b y)
  | .prim _, x => x
  | .lbl l, x => h l x
  | .list t, xs => List.map (V.map h t) xs
  | .set t, s => FinSet.map (V.map h t) s

/-! ## Proposition 5.3: `V_(−)(t)` is a functor -/

/-- Proposition 5.3, identities: `V_id(t) = id`. -/
theorem V.map_id {E : L → Type} (t : Ty T.P L) (x : V T E t) : V.map (fun _ y => y) t x = x := by
  induction t with
  | zero => exact nomatch x
  | one | prim _ | lbl _ => rfl
  | sum a b iha ihb =>
    cases x with
    | inl x => exact congrArg Sum.inl (iha x)
    | inr y => exact congrArg Sum.inr (ihb y)
  | prod a b iha ihb =>
    obtain ⟨x, y⟩ := x
    exact Prod.ext (iha x) (ihb y)
  | list t ih =>
    show List.map (V.map (fun _ y => y) t) x = x
    rw [List.map_congr_left (fun y _ => ih y)]
    exact List.map_id' x
  | set t ih =>
    show FinSet.map (V.map (fun _ y => y) t) x = x
    rw [FinSet.map_congr ih]
    exact FinSet.map_id' x

/-- Proposition 5.3, composition: `V_{h'∘h}(t) = V_{h'}(t) ∘ V_h(t)`. -/
theorem V.map_comp {E E' E'' : L → Type} (h : ∀ l, E l → E' l) (h' : ∀ l, E' l → E'' l) (t : Ty T.P L)
    (x : V T E t) : V.map (fun l y => h' l (h l y)) t x = V.map h' t (V.map h t x) := by
  induction t with
  | zero => exact nomatch x
  | one | prim _ | lbl _ => rfl
  | sum a b iha ihb =>
    cases x with
    | inl x => exact congrArg Sum.inl (iha x)
    | inr y => exact congrArg Sum.inr (ihb y)
  | prod a b iha ihb =>
    obtain ⟨x, y⟩ := x
    exact Prod.ext (iha x) (ihb y)
  | list t ih =>
    show List.map (V.map (fun l y => h' l (h l y)) t) x = List.map (V.map h' t) (List.map (V.map h t) x)
    rw [List.map_map]
    exact List.map_congr_left (fun y _ => ih y)
  | set t ih =>
    show FinSet.map (V.map (fun l y => h' l (h l y)) t) x = FinSet.map (V.map h' t) (FinSet.map (V.map h t) x)
    rw [FinSet.map_map]
    exact FinSet.map_congr ih x

/-! ## Proposition 12.2: label-free types do not depend on the elements -/

/-- Proposition 12.2, values: over a label-free type, the values are the same whatever the family. -/
theorem V_labelFree (E E' : L → Type) (t : Ty T.P L) (ht : t.LabelFree) : V T E t = V T E' t := by
  induction t with
  | zero | one | prim _ => rfl
  | sum a b iha ihb =>
    show (V T E a ⊕ V T E b) = (V T E' a ⊕ V T E' b)
    rw [iha ht.1, ihb ht.2]
  | prod a b iha ihb =>
    show (V T E a × V T E b) = (V T E' a × V T E' b)
    rw [iha ht.1, ihb ht.2]
  | lbl _ => exact ht.elim
  | list t ih =>
    show List (V T E t) = List (V T E' t)
    rw [ih ht]
  | set t ih =>
    show FinSet (V T E t) = FinSet (V T E' t)
    rw [ih ht]

/-- Proposition 12.2, values: over a label-free type, the values of `E` are those of the empty family, `V_∅(t)`. -/
theorem V_labelFree_empty (E : L → Type) (t : Ty T.P L) (ht : t.LabelFree) :
    V T E t = V T (fun _ => Empty) t :=
  V_labelFree E _ t ht

private theorem heq_inl {A B A' B' : Type} (ea : A = A') (eb : B = B') {x : A} {x' : A'} (h : HEq x' x) :
    HEq (Sum.inl x' : A' ⊕ B') (Sum.inl x : A ⊕ B) := by
  subst ea eb
  cases eq_of_heq h
  rfl

private theorem heq_inr {A B A' B' : Type} (ea : A = A') (eb : B = B') {y : B} {y' : B'} (h : HEq y' y) :
    HEq (Sum.inr y' : A' ⊕ B') (Sum.inr y : A ⊕ B) := by
  subst ea eb
  cases eq_of_heq h
  rfl

private theorem heq_pair {A B A' B' : Type} (ea : A = A') (eb : B = B') {x : A} {x' : A'} {y : B} {y' : B'}
    (hx : HEq x' x) (hy : HEq y' y) : HEq ((x', y') : A' × B') ((x, y) : A × B) := by
  subst ea eb
  cases eq_of_heq hx
  cases eq_of_heq hy
  rfl

private theorem heq_list_map {A A' : Type} (e : A = A') (f : A → A') (hf : ∀ y, HEq (f y) y) (xs : List A) :
    HEq (xs.map f) xs := by
  subst e
  have : f = fun y => y := funext fun y => eq_of_heq (hf y)
  subst this
  rw [List.map_id']

private theorem heq_finset_map {A A' : Type} (e : A = A') (f : A → A') (hf : ∀ y, HEq (f y) y) (s : FinSet A) :
    HEq (FinSet.map f s) s := by
  subst e
  have : f = fun y => y := funext fun y => eq_of_heq (hf y)
  subst this
  rw [FinSet.map_id']

/-- Proposition 12.2, the action: over a label-free type, `V_h(t)` is the identity, every value mapped to itself
across the equality of `V_labelFree`. -/
theorem V.map_labelFree {E E' : L → Type} (h : ∀ l, E l → E' l) (t : Ty T.P L) (ht : t.LabelFree)
    (x : V T E t) : HEq (V.map h t x) x := by
  induction t with
  | zero => exact nomatch x
  | one | prim _ => rfl
  | lbl _ => exact ht.elim
  | sum a b iha ihb =>
    cases x with
    | inl x => exact heq_inl (V_labelFree E E' a ht.1) (V_labelFree E E' b ht.2) (iha ht.1 x)
    | inr y => exact heq_inr (V_labelFree E E' a ht.1) (V_labelFree E E' b ht.2) (ihb ht.2 y)
  | prod a b iha ihb =>
    obtain ⟨x, y⟩ := x
    exact heq_pair (V_labelFree E E' a ht.1) (V_labelFree E E' b ht.2) (iha ht.1 x) (ihb ht.2 y)
  | list t ih =>
    exact heq_list_map (V_labelFree E E' t ht) (V.map h t) (ih ht) x
  | set t ih =>
    exact heq_finset_map (V_labelFree E E' t ht) (V.map h t) (ih ht) x

/-- Proposition 12.2, the action on one family: an endomorphism of `E` acts as the identity on a label-free type. -/
theorem V.map_labelFree_self {E : L → Type} (h : ∀ l, E l → E l) (t : Ty T.P L) (ht : t.LabelFree)
    (x : V T E t) : V.map h t x = x :=
  eq_of_heq (V.map_labelFree h t ht x)

/-- Proposition 12.2, independence: over a label-free type, every morphism `E → E'` acts alike. -/
theorem V.map_labelFree_indep {E E' : L → Type} (h h' : ∀ l, E l → E' l) (t : Ty T.P L) (ht : t.LabelFree)
    (x : V T E t) : V.map h t x = V.map h' t x :=
  eq_of_heq ((V.map_labelFree h t ht x).trans (V.map_labelFree h' t ht x).symm)

end ApgModel.Core
