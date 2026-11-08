/-
 SPDX-License-Identifier: Apache-2.0
 Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
-/
import ApgModel.Core.Term

/-!
# Named types and schemas

`SCHEMA-APG.md` definitions 3.1, 3.2 and 4.2 with field names, the form part 3 reads: a record or a variant is a list of
fields, each a name and a type. The model fixes `String`'s order as the total order on names that proposition 3.3
names, and a well-formed type lists its fields in strictly increasing order (`NTy.WF`), so one finite map of fields is
one type. Decidable equality is written by hand, since Lean 4.14 derives none for a type nested through a list.

`NTy.bin` is proposition 3.3's translation `⌜·⌝` to the core's binary form: a record `rec {n₁ ↦ t₁, …, nₖ ↦ tₖ}`
becomes `⌜t₁⌝ × (⌜t₂⌝ × ( … × (⌜tₖ⌝ × 1)))` and a variant the same with `+` and `0`.
-/

namespace ApgModel.Named

/-- Field names, definition 3.1, ordered as strings. -/
abbrev Name : Type := String

/-- The named types of definition 3.2, with the collections of definition 12.1. -/
inductive NTy (P L : Type) where
  /-- A primitive type. -/
  | prim (p : P)
  /-- A reference to an element labelled `l`. -/
  | lbl (l : L)
  /-- A record: one value for every field. -/
  | record (fs : List (Name × NTy P L))
  /-- A variant: one field, with a value for it. -/
  | variant (fs : List (Name × NTy P L))
  /-- A finite sequence. -/
  | list (t : NTy P L)
  /-- A finite set. -/
  | set (t : NTy P L)
  deriving Repr

namespace NTy

variable {P L : Type}

/-! ## Decidable equality, written by hand -/

/-- Decidable equality of named types, by structural recursion through the field lists. -/
def decEq [DecidableEq P] [DecidableEq L] : (a b : NTy P L) → Decidable (a = b)
  | .prim p, .prim q => if h : p = q then isTrue (h ▸ rfl) else isFalse (fun e => by cases e; exact h rfl)
  | .lbl l, .lbl m => if h : l = m then isTrue (h ▸ rfl) else isFalse (fun e => by cases e; exact h rfl)
  | .record fs, .record gs =>
    match decFields fs gs with
    | isTrue h => isTrue (h ▸ rfl)
    | isFalse h => isFalse (fun e => by cases e; exact h rfl)
  | .variant fs, .variant gs =>
    match decFields fs gs with
    | isTrue h => isTrue (h ▸ rfl)
    | isFalse h => isFalse (fun e => by cases e; exact h rfl)
  | .list a, .list b =>
    match decEq a b with
    | isTrue h => isTrue (h ▸ rfl)
    | isFalse h => isFalse (fun e => by cases e; exact h rfl)
  | .set a, .set b =>
    match decEq a b with
    | isTrue h => isTrue (h ▸ rfl)
    | isFalse h => isFalse (fun e => by cases e; exact h rfl)
  | .prim _, .lbl _ | .prim _, .record _ | .prim _, .variant _ | .prim _, .list _ | .prim _, .set _
  | .lbl _, .prim _ | .lbl _, .record _ | .lbl _, .variant _ | .lbl _, .list _ | .lbl _, .set _
  | .record _, .prim _ | .record _, .lbl _ | .record _, .variant _ | .record _, .list _ | .record _, .set _
  | .variant _, .prim _ | .variant _, .lbl _ | .variant _, .record _ | .variant _, .list _ | .variant _, .set _
  | .list _, .prim _ | .list _, .lbl _ | .list _, .record _ | .list _, .variant _ | .list _, .set _
  | .set _, .prim _ | .set _, .lbl _ | .set _, .record _ | .set _, .variant _ | .set _, .list _ =>
    isFalse (fun e => by cases e)
where
  /-- Decidable equality of field lists. -/
  decFields : (fs gs : List (Name × NTy P L)) → Decidable (fs = gs)
    | [], [] => isTrue rfl
    | [], _ :: _ => isFalse (fun e => by cases e)
    | _ :: _, [] => isFalse (fun e => by cases e)
    | (n, t) :: fs, (m, u) :: gs =>
      if hn : n = m then
        match decEq t u, decFields fs gs with
        | isTrue ht, isTrue hfs => isTrue (by rw [hn, ht, hfs])
        | isFalse ht, _ => isFalse (fun e => by cases e; exact ht rfl)
        | _, isFalse hfs => isFalse (fun e => by cases e; exact hfs rfl)
      else isFalse (fun e => by cases e; exact hn rfl)

instance [DecidableEq P] [DecidableEq L] : DecidableEq (NTy P L) := decEq

/-! ## Well-formed types -/

/-- The names of a field list, in order. -/
def names (fs : List (Name × NTy P L)) : List Name := fs.map Prod.fst

/-- A well-formed named type: every record's and variant's field names strictly increasing, at every depth, so a
type's fields are a finite map written one way. -/
def WF : NTy P L → Prop
  | .prim _ | .lbl _ => True
  | .record fs | .variant fs => (names fs).Pairwise (· < ·) ∧ WFFields fs
  | .list t | .set t => t.WF
where
  /-- Every field's type well-formed. -/
  WFFields : List (Name × NTy P L) → Prop
    | [] => True
    | (_, t) :: fs => t.WF ∧ WFFields fs

/-! ## Definition 3.2's abbreviations -/

/-- `1 := rec ∅`, the unit. -/
def unit : NTy P L := .record []

/-- `0 := var ∅`, the empty type. -/
def empty : NTy P L := .variant []

/-- `t₁ × t₂ := rec { fst ↦ t₁, snd ↦ t₂ }`. -/
def prod (a b : NTy P L) : NTy P L := .record [("fst", a), ("snd", b)]

/-- `t₁ + t₂ := var { inl ↦ t₁, inr ↦ t₂ }`. -/
def sum (a b : NTy P L) : NTy P L := .variant [("inl", a), ("inr", b)]

/-- `optional t := var { none ↦ 1, some ↦ t }`. -/
def optional (t : NTy P L) : NTy P L := .variant [("none", unit), ("some", t)]

/-- `enum c₁ … cₙ := var { c₁ ↦ 1, …, cₙ ↦ 1 }`, the members given in increasing order. -/
def enum (cs : List Name) : NTy P L := .variant (cs.map fun c => (c, unit))

/-- `enum false true`, the Booleans. -/
def bool : NTy P L := enum ["false", "true"]

/-! ## Proposition 3.3's translation to the binary form -/

/-- `⌜t⌝`, the binary form of a named type: a record as a right-nested product ending in `1`, a variant as a
right-nested sum ending in `0`, in the order of the fields. -/
def bin : NTy P L → Core.Ty P L
  | .prim p => .prim p
  | .lbl l => .lbl l
  | .record fs => binRec fs
  | .variant fs => binVar fs
  | .list t => .list t.bin
  | .set t => .set t.bin
where
  /-- A record's fields as a right-nested product. -/
  binRec : List (Name × NTy P L) → Core.Ty P L
    | [] => .one
    | (_, t) :: fs => .prod t.bin (binRec fs)
  /-- A variant's fields as a right-nested sum. -/
  binVar : List (Name × NTy P L) → Core.Ty P L
    | [] => .zero
    | (_, t) :: fs => .sum t.bin (binVar fs)

/-- The named Booleans translate to the core's `Ty.bool`. -/
theorem bin_bool : (bool : NTy P L).bin = Core.Ty.bool := rfl

/-- A type with no label in it. -/
def LabelFree (t : NTy P L) : Prop := t.bin.LabelFree

/-- A label or an optional label, `lbl a` or `var { none ↦ 1, some ↦ lbl a }`. -/
def isLabelOrOptionalLabel : NTy P L → Bool
  | .lbl _ => true
  | .variant [("none", .record []), ("some", .lbl _)] => true
  | _ => false

/-- A two-field record translates to `t₁ × (t₂ × 1)`, as proposition 3.3 states. -/
example : (NTy.record [("a", .prim 0), ("b", .lbl ())] : NTy Nat Unit).bin =
    .prod (.prim 0) (.prod (.lbl ()) .one) := rfl

/-- The hand-written equality decides equal types equal and different ones different. -/
example : (NTy.optional (.lbl ()) : NTy Nat Unit) = NTy.optional (.lbl ()) := by decide
example : (NTy.record [("a", .prim 0)] : NTy Nat Unit) ≠ NTy.record [("b", .prim 0)] := by decide
example : (NTy.list (.prim 1) : NTy Nat Unit) ≠ NTy.set (.prim 1) := by decide

end NTy

/-- A named APG schema: a named type for every label (definition 4.1 with field names). -/
structure NSchema (P L : Type) where
  /-- Each label's type. -/
  σ : L → NTy P L

/-- The core schema of a named schema: each label's type in binary form. -/
def NSchema.bin {P L : Type} (S : NSchema P L) : Core.Schema P L := ⟨fun l => (S.σ l).bin⟩

/-! ## Definition 4.2: the kinds of labels -/

namespace NSchema

variable {P L : Type} (S : NSchema P L)

/-- A vertex label: its type is `1`. -/
def IsVertex (l : L) : Prop := S.σ l = .record []

/-- An edge label: a record of exactly two fields, each a reference to a vertex label. -/
def IsEdge (l : L) : Prop :=
  ∃ n₁ n₂ a b, S.σ l = .record [(n₁, .lbl a), (n₂, .lbl b)] ∧ S.IsVertex a ∧ S.IsVertex b

/-- A vertex property: a record of exactly two fields, one a reference to a vertex label and one label-free. -/
def IsVertexProperty (l : L) : Prop :=
  ∃ n₁ n₂ a t, (S.σ l = .record [(n₁, .lbl a), (n₂, t)] ∨ S.σ l = .record [(n₁, t), (n₂, .lbl a)]) ∧
    S.IsVertex a ∧ t.LabelFree

/-- An edge property or a meta-property: as a vertex property, the reference to an edge or a property label. -/
def IsEdgeProperty (l : L) : Prop :=
  ∃ n₁ n₂ a t, (S.σ l = .record [(n₁, .lbl a), (n₂, t)] ∨ S.σ l = .record [(n₁, t), (n₂, .lbl a)]) ∧
    (S.IsEdge a ∨ S.IsVertexProperty a) ∧ t.LabelFree

/-- A meta-edge label: a field of a label that is no vertex label. -/
def IsMetaEdge (l : L) : Prop :=
  ∃ fs, S.σ l = .record fs ∧ ∃ n a, (n, NTy.lbl a) ∈ fs ∧ ¬ S.IsVertex a

/-- A hyperedge label, generalising the edge: a record whose fields include two or more labels or optional labels. -/
def IsHyperedge (l : L) : Prop :=
  ∃ fs, S.σ l = .record fs ∧ 2 ≤ (fs.filter fun f => f.2.isLabelOrOptionalLabel).length

end NSchema

end ApgModel.Named
