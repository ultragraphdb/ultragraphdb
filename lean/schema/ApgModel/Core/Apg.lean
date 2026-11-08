/-
 SPDX-License-Identifier: Apache-2.0
 Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
-/
import ApgModel.Core.Value

/-!
# Algebraic property graphs

`SCHEMA-APG.md` definitions 6.1, 6.2 and 6.5, and proposition 6.6. An APG on a schema is a family of elements, one
set per label, with a value for every element of its label's type (`Apg`). Equivalently it is one set of elements
with a labelling and a value for each, typed by its label (`SingleSet`); `ofSingleSet` and `toSingleSet` pass between
the two, and `ApgModel.Core.Hom` proves them inverse up to isomorphism, which is proposition 6.3.

A value references elements; `V.Ref` says which, each with the label the type at the reference names. The element
graph has an arc from an element to every element its value references (definition 6.5), and every arc ends at an
element of the graph with that label (proposition 6.6), since a reference typed `lbl l` is an element of `E l`.
-/

namespace ApgModel.Core

variable {T : Typeside} {L : Type}

/-- An APG on the schema `S`, definition 6.1: the elements of each label, and the value of each element, of its
label's type. -/
structure Apg (S : Schema T.P L) where
  /-- The elements of each label. -/
  E : L → Type
  /-- The value of each element. -/
  υ : (l : L) → E l → V T E (S.σ l)

/-- An APG in single-set form, definition 6.2: a set of elements, a labelling, and a value for each element of its
label's type, whose references are elements carrying the labels the type names. -/
structure SingleSet (S : Schema T.P L) where
  /-- The elements. -/
  X : Type
  /-- The labelling `λ`. -/
  lam : X → L
  /-- The value of each element, its references typed by `λ⁻¹`. -/
  υ : (x : X) → V T (fun l => { y : X // lam y = l }) (S.σ (lam x))

variable {S : Schema T.P L}

/-- The elements of label `l`, `λ⁻¹(l)`, and the values read off the labelling: definition 6.2 to definition 6.1. -/
def ofSingleSet (H : SingleSet S) : Apg S where
  E l := { y : H.X // H.lam y = l }
  υ _ y := y.2 ▸ H.υ y.1

/-- An element `e` of label `l` as an element of the disjoint union, carrying `l`. -/
def toSigma (E : L → Type) (l : L) (e : E l) : { x : (Σ l, E l) // x.1 = l } := ⟨⟨l, e⟩, rfl⟩

/-- The disjoint union of the labels' elements, labelled by the summand: definition 6.1 to definition 6.2. -/
def toSingleSet (G : Apg S) : SingleSet S where
  X := Σ l, G.E l
  lam x := x.1
  υ x := V.map (toSigma G.E) (S.σ x.1) (G.υ x.1 x.2)

/-! ## References and the element graph -/

/-- `V.Ref t v r`: the value `v` of type `t` references the element `r`, at a position whose type is `lbl r.1`
(definition 5.5). -/
def V.Ref {E : L → Type} : (t : Ty T.P L) → V T E t → (Σ l, E l) → Prop
  | .zero, x, _ => nomatch x
  | .one, _, _ | .prim _, _, _ => False
  | .sum a _, .inl x, r => V.Ref a x r
  | .sum _ b, .inr y, r => V.Ref b y r
  | .prod a b, (x, y), r => V.Ref a x r ∨ V.Ref b y r
  | .lbl l, e, r => (⟨l, e⟩ : Σ l, E l) = r
  | .list t, xs, r => ∃ y, List.Mem y xs ∧ V.Ref t y r
  | .set t, s, r => FinSet.Any (fun y => V.Ref t y r) s

/-- The element graph's arcs, definition 6.5: an arc from `e` to every element `e`'s value references. -/
def Apg.Arc (G : Apg S) (e e' : Σ l, G.E l) : Prop := V.Ref (S.σ e.1) (G.υ e.1 e.2) e'

/-- An arc of the element graph of a single-set APG: `x`'s value references `y` at a position typed `lbl l`. -/
def SingleSet.Arc (H : SingleSet S) (x : H.X) (l : L) (y : { y : H.X // H.lam y = l }) : Prop :=
  V.Ref (S.σ (H.lam x)) (H.υ x) ⟨l, y⟩

/-- Proposition 6.6, referential integrity: every arc of the element graph ends at an element of the graph that
carries the label the type at the reference names, since `V_E(lbl l) = E(l)`. -/
theorem SingleSet.arc_target_label (H : SingleSet S) {x : H.X} {l : L} {y : { y : H.X // H.lam y = l }}
    (_ : H.Arc x l y) : H.lam y.1 = l :=
  y.2

/-- Proposition 6.6 in the family form: an arc ends at an element of the graph, of the label it is filed under. -/
theorem Apg.arc_target (G : Apg S) {e e' : Σ l, G.E l} (_ : G.Arc e e') : ∃ l, ∃ x : G.E l, e' = ⟨l, x⟩ :=
  ⟨e'.1, e'.2, rfl⟩

end ApgModel.Core
