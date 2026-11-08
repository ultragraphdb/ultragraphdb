/-
 SPDX-License-Identifier: Apache-2.0
 Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
-/

/-!
# The core's types and schemas

`SCHEMA-APG.md` definitions 3.2, 4.1 and 12.1, in the binary form the paper states its results over:
`0 | 1 | t + t | t × t | prim p | lbl l`, with part 2's `list t` and `set t` among them from the start, so every result
of part 1 proved here holds with collections, which is corollary 12.3. Records and variants with field names are the
named layer's, translated to this form by proposition 3.3.

A label inside a type is a reference to an element, a leaf of the grammar: a schema may name its own labels, and its
recursion runs through element identity, never through the unfolding of a type.
-/

namespace ApgModel.Core

/-- The types over primitive types `P` and labels `L`: definition 3.2 in binary form, with the collection constructors
of definition 12.1. -/
inductive Ty (P L : Type) where
  /-- `0`, the empty type. -/
  | zero
  /-- `1`, the unit. -/
  | one
  /-- `a + b`, a value of `a` or a value of `b`. -/
  | sum (a b : Ty P L)
  /-- `a × b`, a value of `a` and a value of `b`. -/
  | prod (a b : Ty P L)
  /-- A primitive type. -/
  | prim (p : P)
  /-- A reference to an element labelled `l`. -/
  | lbl (l : L)
  /-- A finite sequence. -/
  | list (t : Ty P L)
  /-- A finite set. -/
  | set (t : Ty P L)
  deriving DecidableEq, Repr

namespace Ty

variable {P L : Type}

/-- `lbls(t)`, the labels occurring in `t`, with repetitions. -/
def lbls : Ty P L → List L
  | .zero | .one | .prim _ => []
  | .sum a b | .prod a b => a.lbls ++ b.lbls
  | .lbl l => [l]
  | .list t | .set t => t.lbls

/-- A type is label-free when no label occurs in it (definition 3.2). -/
def LabelFree : Ty P L → Prop
  | .zero | .one | .prim _ => True
  | .sum a b | .prod a b => a.LabelFree ∧ b.LabelFree
  | .lbl _ => False
  | .list t | .set t => t.LabelFree

theorem labelFree_iff_lbls_nil (t : Ty P L) : t.LabelFree ↔ t.lbls = [] := by
  induction t with
  | zero | one | prim _ => simp [LabelFree, lbls]
  | sum a b iha ihb | prod a b iha ihb => simp [LabelFree, lbls, iha, ihb]
  | lbl _ => simp [LabelFree, lbls]
  | list t ih | set t ih => simp [LabelFree, lbls, ih]

/-- A type in which no collection occurs. -/
def CollectionFree : Ty P L → Prop
  | .zero | .one | .prim _ | .lbl _ => True
  | .sum a b | .prod a b => a.CollectionFree ∧ b.CollectionFree
  | .list _ | .set _ => False

/-- An enumeration in binary form: `1 + (1 + (… + 0))`, the translation of `enum c₁ … cₙ` (definition 3.2). -/
def IsEnum : Ty P L → Prop
  | .zero => True
  | .sum .one b => b.IsEnum
  | _ => False

/-- The types definition 12.1 admits: `list t` over a label-free `t` without a collection in it, and `set t` over a
primitive type or an enumeration, at every depth. -/
def Admitted : Ty P L → Prop
  | .zero | .one | .prim _ | .lbl _ => True
  | .sum a b | .prod a b => a.Admitted ∧ b.Admitted
  | .list t => t.LabelFree ∧ t.CollectionFree ∧ t.Admitted
  | .set t => (∃ p, t = .prim p) ∨ t.IsEnum

theorem IsEnum.labelFree {t : Ty P L} (h : t.IsEnum) : t.LabelFree := by
  induction t with
  | zero => trivial
  | sum a b _ ihb =>
    cases a <;> simp only [IsEnum] at h
    exact ⟨trivial, ihb h⟩
  | _ => simp [IsEnum] at h

/-- The element of an admitted collection type is label-free: what definition 12.1 requires, and what makes a
collection a constant of the coalgebra functor (corollary 12.3). -/
theorem Admitted.list_labelFree {t : Ty P L} (h : (Ty.list t).Admitted) : t.LabelFree := h.1

theorem Admitted.set_labelFree {t : Ty P L} (h : (Ty.set t).Admitted) : t.LabelFree := by
  rcases h with ⟨p, rfl⟩ | he
  · trivial
  · exact he.labelFree

end Ty

/-- An APG schema, definition 4.1: a type for every label, which may name labels, its own included. -/
structure Schema (P L : Type) where
  /-- Each label's type. -/
  σ : L → Ty P L

/-- A schema is finite when a list enumerates its labels (definition 4.1). -/
def Schema.Finite {P L : Type} (_ : Schema P L) : Prop := ∃ ls : List L, ∀ l, l ∈ ls

end ApgModel.Core
