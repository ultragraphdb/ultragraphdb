/-
 SPDX-License-Identifier: Apache-2.0
 Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
-/
import ApgModel.Named.Collections

/-!
# The schema of a catalog

`SCHEMA-APG.md` section 13 and proposition 13.1: a catalog version `K` denotes an APG schema `S(K)`.

A catalog (`Catalog`) lists its declared types in declaration order: each a name, a kind, a parent, whether it is
abstract, its type prefix (`SCHEMA.md` section 7.1), and its own fields, attributes, single-valued roles and
set-valued roles, a role naming the participant types it admits. The model takes field types already translated to
named types (`SCHEMA-APG.md` section 13.2); the compiler's translation of UGS types is the compiler's, checked against
this model when the compiler exports its catalogs (`openspec/notes/prepare-apg-lean.md`, task A.6).

- `conc(T)` is the concrete types at or below `T` (section 13.1); a role's participant type `⟦U⟧` is the one label
  when the union admits one concrete type and the variant over them otherwise (`Catalog.participantType`).
- `S(K)` (`Catalog.schema`) has a label per concrete type, the record of its fields, own and inherited, a specialised
  role replacing its super role; and a role label `R_r` per set-valued role (section 12.3).
- A type's prefix set is its own prefix and its descendants', kept as sorted half-open ranges (`Catalog.ranges`); a type
  test is a check against those ranges (`inRanges`). Proposition 13.1: an element's label, its concrete type, is in
  `conc(T)` exactly when its prefix is in `T`'s ranges (`Catalog.is_iff_inRanges`).
-/

namespace ApgModel.Catalog

open Named

/-! ## Sorting and ranges -/

/-- Insertion into a list sorted by `lt`. -/
def insertBy {α : Type} (lt : α → α → Bool) (x : α) : List α → List α
  | [] => [x]
  | y :: ys => if lt y x then y :: insertBy lt x ys else x :: y :: ys

/-- Insertion sort by `lt`. -/
def sortBy {α : Type} (lt : α → α → Bool) : List α → List α
  | [] => []
  | x :: xs => insertBy lt x (sortBy lt xs)

theorem mem_insertBy {α : Type} (lt : α → α → Bool) (x z : α) : (l : List α) → (z ∈ insertBy lt x l ↔ z = x ∨ z ∈ l)
  | [] => by simp [insertBy]
  | y :: ys => by
    simp only [insertBy]
    split
    · simp only [List.mem_cons, mem_insertBy lt x z ys]
      constructor
      · rintro (h | h | h) <;> simp [h]
      · rintro (h | h | h) <;> simp [h]
    · simp

theorem mem_sortBy {α : Type} (lt : α → α → Bool) (z : α) : (l : List α) → (z ∈ sortBy lt l ↔ z ∈ l)
  | [] => Iff.rfl
  | x :: xs => by
    simp only [sortBy, mem_insertBy, List.mem_cons, mem_sortBy lt z xs]

/-- A list of numbers as half-open ranges `[a, b)`, consecutive numbers joined into one range. -/
def compress (l : List Nat) : List (Nat × Nat) := go none l
where
  /-- The ranges of `l` after the open range `cur`. -/
  go : Option (Nat × Nat) → List Nat → List (Nat × Nat)
    | none, [] => []
    | some r, [] => [r]
    | none, x :: xs => go (some (x, x + 1)) xs
    | some (a, b), x :: xs => if x = b then go (some (a, b + 1)) xs else (a, b) :: go (some (x, x + 1)) xs

/-- Whether `p` lies in one of the ranges. -/
def inRanges (p : Nat) (rs : List (Nat × Nat)) : Bool := rs.any fun r => decide (r.1 ≤ p ∧ p < r.2)

theorem inRanges_go_some (p : Nat) : (l : List Nat) → (a b : Nat) → a < b →
    (inRanges p (compress.go (some (a, b)) l) = true ↔ (a ≤ p ∧ p < b) ∨ p ∈ l)
  | [], a, b, _ => by simp [compress.go, inRanges]
  | x :: xs, a, b, hab => by
    simp only [compress.go]
    split
    · next hx =>
      rw [inRanges_go_some p xs a (b + 1) (by omega), List.mem_cons]
      by_cases hm : p ∈ xs <;> simp [hm] <;> omega
    · next hx =>
      show (decide (a ≤ p ∧ p < b) || inRanges p (compress.go (some (x, x + 1)) xs)) = true ↔ _
      rw [Bool.or_eq_true, decide_eq_true_eq, inRanges_go_some p xs x (x + 1) (by omega), List.mem_cons]
      by_cases hm : p ∈ xs <;> simp [hm] <;> omega

/-- The ranges hold exactly the numbers of the list. -/
theorem inRanges_compress (p : Nat) (l : List Nat) : inRanges p (compress l) = true ↔ p ∈ l := by
  cases l with
  | nil => simp [compress, compress.go, inRanges]
  | cons x xs =>
    show inRanges p (compress.go (some (x, x + 1)) xs) = true ↔ _
    rw [inRanges_go_some p xs x (x + 1) (by omega), List.mem_cons]
    by_cases hm : p ∈ xs <;> simp [hm] <;> omega

/-! ## Catalogs -/

/-- The kinds of declared types. -/
inductive TypeKind where
  | statement
  | entity
  | relation
  deriving DecidableEq, Repr

/-- A field a type declares, its type already a named type. -/
inductive Field (T : Typeside) where
  /-- An attribute of a label-free type, optional or required. -/
  | attr (name : Name) (t : NTy T.P Name) (optional : Bool)
  /-- A single-valued role admitting the participant types listed, optional or required, specialising a super role
  when named. -/
  | role (name : Name) (participants : List Name) (optional : Bool) (specializes : Option Name)
  /-- A set-valued role, reified as a role label. -/
  | setRole (name : Name) (participants : List Name) (specializes : Option Name)

/-- A field's name. -/
def Field.name {T : Typeside} : Field T → Name
  | .attr n _ _ | .role n _ _ _ | .setRole n _ _ => n

/-- The super role a field specialises. -/
def Field.specializes {T : Typeside} : Field T → Option Name
  | .attr _ _ _ => none
  | .role _ _ _ s | .setRole _ _ s => s

/-- A declared type. -/
structure CType (T : Typeside) where
  /-- Its name. -/
  name : Name
  /-- Its kind. -/
  kind : TypeKind
  /-- Its parent, a single inheritance. -/
  parent : Option Name
  /-- Whether it has no direct instances. -/
  abstract : Bool
  /-- Its type prefix. -/
  pfx : Nat
  /-- Its own fields. -/
  fields : List (Field T)

/-- A type with direct instances: a concrete entity or relation type; a statement type is always abstract. -/
def CType.concrete {T : Typeside} (c : CType T) : Bool := !c.abstract && c.kind != .statement

/-- A catalog version's types, in declaration order. -/
structure Catalog (T : Typeside) where
  /-- The declared types. -/
  types : List (CType T)

namespace Catalog

variable {T : Typeside} (K : Catalog T)

/-- The type named `n`. -/
def find (n : Name) : Option (CType T) := K.types.find? fun c => c.name == n

/-- `n` and its ancestors, nearest first, at most one per declared type. -/
def chain (n : Name) : List Name := go (K.types.length + 1) n
where
  /-- The chain within `fuel` steps. -/
  go : Nat → Name → List Name
    | 0, _ => []
    | k + 1, n => n :: match (K.find n).bind CType.parent with
      | some p => go k p
      | none => []

/-- `c` lies at or below `t`. -/
def isSub (c t : Name) : Bool := (K.chain c).contains t

/-- `conc(t)`: the concrete types at or below `t`, in declaration order. -/
def conc (t : Name) : List Name := (K.types.filter fun c => c.concrete && K.isSub c.name t).map CType.name

/-- A type's prefix set: its own prefix and its descendants'. -/
def prefixSet (t : Name) : List Nat := (K.types.filter fun c => K.isSub c.name t).map CType.pfx

/-- A type's prefix set as sorted half-open ranges (`SCHEMA.md` section 7.1). -/
def ranges (t : Name) : List (Nat × Nat) := compress (sortBy (fun a b => decide (a < b)) (K.prefixSet t))

/-! ## Proposition 13.1 -/

theorem mem_conc (n t : Name) :
    n ∈ K.conc t ↔ ∃ c ∈ K.types, c.name = n ∧ c.concrete = true ∧ K.isSub n t = true := by
  simp only [conc, List.mem_map, List.mem_filter, Bool.and_eq_true]
  constructor
  · rintro ⟨c, ⟨hc, hconc, hsub⟩, rfl⟩
    exact ⟨c, hc, rfl, hconc, hsub⟩
  · rintro ⟨c, hc, rfl, hconc, hsub⟩
    exact ⟨c, ⟨hc, hconc, hsub⟩, rfl⟩

/-- Proposition 13.1, the prefix-range check: a declared type's prefix is in `t`'s ranges exactly when the type lies
at or below `t`, prefixes being unique to their types. -/
theorem inRanges_iff_isSub (t : Name) (c : CType T) (hc : c ∈ K.types)
    (hpfx : ∀ d ∈ K.types, d.pfx = c.pfx → d.name = c.name) :
    inRanges c.pfx (K.ranges t) = true ↔ K.isSub c.name t = true := by
  rw [ranges, inRanges_compress, mem_sortBy, prefixSet, List.mem_map]
  constructor
  · rintro ⟨d, hd, hdp⟩
    rw [List.mem_filter] at hd
    rw [← hpfx d hd.1 hdp]
    exact hd.2
  · intro h
    exact ⟨c, List.mem_filter.mpr ⟨hc, h⟩, rfl⟩

/-- Proposition 13.1: an element carries one label, its concrete type `c`, and `is t` holds of it, its label being in
`conc(t)`, exactly when the prefix-range check finds its prefix in `t`'s ranges. -/
theorem is_iff_inRanges (t : Name) (c : CType T) (hc : c ∈ K.types) (hconc : c.concrete = true)
    (hpfx : ∀ d ∈ K.types, d.pfx = c.pfx → d.name = c.name) :
    c.name ∈ K.conc t ↔ inRanges c.pfx (K.ranges t) = true := by
  rw [K.inRanges_iff_isSub t c hc hpfx, mem_conc]
  constructor
  · rintro ⟨_, _, _, _, hsub⟩
    exact hsub
  · intro h
    exact ⟨c, hc, rfl, hconc, h⟩

/-! ## `S(K)` -/

/-- The names without repetitions, first occurrence kept. -/
def dedup : List Name → List Name
  | [] => []
  | n :: ns => if ns.contains n then dedup ns else n :: dedup ns

/-- `⟦U⟧`, a role's participant type: the one label when the union admits one concrete type, else the variant over
them, its fields named by the types in increasing order. -/
def participantType (ps : List Name) : NTy T.P Name :=
  match dedup (ps.foldr (fun p acc => K.conc p ++ acc) []) with
  | [c] => .lbl c
  | cs => .variant (sortBy (fun a b => decide (a.1 < b.1)) (cs.map fun c => (c, .lbl c)))

/-- A type's fields with its ancestors', nearest first, each super role a field specialises left out. -/
def allFields (n : Name) : List (Field T) :=
  (K.chain n).foldl (fun acc m =>
    let own := ((K.find m).map CType.fields).getD []
    let replaced := acc.filterMap Field.specializes
    acc ++ own.filter fun f => !replaced.contains f.name) []

/-- A field's place in the record of its type: an attribute or a single-valued role; a set-valued role is a role
label's. -/
def fieldType : Field T → Option (Name × NTy T.P Name)
  | .attr n t opt => some (n, if opt then NTy.optional t else t)
  | .role n ps opt _ => some (n, if opt then NTy.optional (K.participantType ps) else K.participantType ps)
  | .setRole _ _ _ => none

/-- The name of the role label of `R`'s set-valued role `r`. -/
def roleLabelName (R r : Name) : Name := R ++ "_" ++ r

/-- The relation and participants of the role label named `n`. -/
def roleLabelOf (n : Name) : Option (Name × List Name) :=
  K.types.findSome? fun c =>
    if c.concrete then
      (K.allFields c.name).findSome? fun f =>
        match f with
        | .setRole r ps _ => if roleLabelName c.name r = n then some (c.name, ps) else none
        | _ => none
    else none

/-- `S(K)`: a concrete type's record of its fields, in increasing name order, and a role label's record of its
instance and participant (sections 12.3, 13.1 to 13.3). Any other name has the unit type and is no label. -/
def schema : NSchema T.P Name where
  σ n :=
    match K.find n with
    | some c =>
      if c.concrete then .record (sortBy (fun a b => decide (a.1 < b.1)) ((K.allFields n).filterMap K.fieldType))
      else NTy.unit
    | none =>
      match K.roleLabelOf n with
      | some (R, ps) => roleLabelType R (K.participantType ps)
      | none => NTy.unit

/-- The labels of `S(K)`: every concrete type, then every role label. -/
def labels : List Name :=
  (K.types.filter CType.concrete).map CType.name ++
    (K.types.filter CType.concrete).foldr (fun c acc =>
      ((K.allFields c.name).filterMap fun f =>
        match f with
        | .setRole r _ _ => some (roleLabelName c.name r)
        | _ => none) ++ acc) []

end Catalog

end ApgModel.Catalog
