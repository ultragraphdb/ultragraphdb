/-
 SPDX-License-Identifier: Apache-2.0
 Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
-/
import ApgModel.Named.Collections

/-!
# The schema of a catalog

`SCHEMA-APG.md` section 13 and proposition 13.1: a catalog version `K` denotes an APG schema `S(K)`.

A catalog (`Catalog`) lists its declared types in declaration order: each a name, a kind, a parent, whether it is
abstract, its type prefix (`SCHEMA.md` section 7.1), its key, whether it is `@bitemporal`, `@multi` or `@symmetric`,
its `@colocate` role, and its own fields: attributes with the view the RDF view writes them in, single-valued roles with
their navigation names, and set-valued roles, a role naming the participant types it admits. The model takes field
types already translated to named types (`SCHEMA-APG.md` section 13.2); the compiler's translation of UGS types is the
compiler's, checked against this model when the compiler exports its catalogs (`openspec/notes/prepare-apg-lean.md`,
task A.6).

- `conc(T)` is the concrete types at or below `T` (section 13.1); a role's participant type `⟦U⟧` is the one label
  when the union admits one concrete type and the variant over them otherwise (`Catalog.participantType`).
- `S(K)` (`Catalog.schema`) has a label per concrete type, the record of its fields, own and inherited, a specialised
  role replacing its super role, `asserted` on a `@bitemporal` type; and a role label `R_r` per set-valued role
  (section 12.3).
- A relation's encoding is `SCHEMA.md` section 11.1's: edge-encoded with exactly two roles, neither set-valued, and no
  key (`Catalog.edgeEncoded`), anchored at its `@colocate` role or its first role (`Catalog.anchorRole`), allocating a
  reifier when it carries attributes, is `@multi`, or a role names it (`Catalog.allocatesReifier`).
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

/-- How the RDF view writes an attribute (`SCHEMA.md` section 11.2): its literals, an interval's bounds and
precisions, a measure with its unit on the value, or a catalog type's class IRI. -/
inductive AttrView where
  | plain
  | interval
  | measure
  | typeValue
  deriving DecidableEq, Repr

/-- A field a type declares, its type already a named type. -/
inductive Field (T : Typeside) where
  /-- An attribute of a label-free type, optional or required, with its view. -/
  | attr (name : Name) (t : NTy T.P Name) (optional : Bool) (view : AttrView)
  /-- A single-valued role admitting the participant types listed, optional or required, with its navigation name,
  specialising a super role when named. -/
  | role (name : Name) (participants : List Name) (optional : Bool) (nav : Option Name) (specializes : Option Name)
  /-- A set-valued role, reified as a role label. -/
  | setRole (name : Name) (participants : List Name) (specializes : Option Name)

/-- A field's name. -/
def Field.name {T : Typeside} : Field T → Name
  | .attr n _ _ _ | .role n _ _ _ _ | .setRole n _ _ => n

/-- The super role a field specialises. -/
def Field.specializes {T : Typeside} : Field T → Option Name
  | .attr _ _ _ _ => none
  | .role _ _ _ _ s | .setRole _ _ s => s

/-- Whether a field is a role. -/
def Field.isRole {T : Typeside} : Field T → Bool
  | .attr _ _ _ _ => false
  | .role _ _ _ _ _ | .setRole _ _ _ => true

/-- The participant types a role admits. -/
def Field.participants {T : Typeside} : Field T → List Name
  | .attr _ _ _ _ => []
  | .role _ ps _ _ _ | .setRole _ ps _ => ps

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
  /-- Its key attributes, empty when it declares no `@key`. -/
  key : List Name := []
  /-- Whether it is `@bitemporal`. -/
  bitemporal : Bool := false
  /-- Whether it is a `@multi` relation. -/
  multi : Bool := false
  /-- Whether it is a `@symmetric` relation. -/
  symmetric : Bool := false
  /-- The role its `@colocate` names. -/
  colocate : Option Name := none

/-- A type with direct instances: a concrete entity or relation type; a statement type is always abstract. -/
def CType.concrete {T : Typeside} (c : CType T) : Bool := !c.abstract && c.kind != .statement

/-- A catalog version's types, in declaration order. -/
structure Catalog (T : Typeside) where
  /-- The declared types. -/
  types : List (CType T)
  /-- The typeside's `DateTime`, the type of `asserted`'s bounds. -/
  dateTime : T.P

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

/-- A type's fields with its ancestors', nearest first, each with the type declaring it, each super role a field
specialises left out. -/
def ownedFields (n : Name) : List (Name × Field T) :=
  (K.chain n).foldl (fun acc m =>
    let own := ((K.find m).map CType.fields).getD []
    let replaced := acc.filterMap fun f => f.2.specializes
    acc ++ (own.filter fun f => !replaced.contains f.name).map fun f => (m, f)) []

/-- A type's fields with its ancestors', nearest first, each super role a field specialises left out. -/
def allFields (n : Name) : List (Field T) := (K.ownedFields n).map Prod.snd

/-- Whether a property holds of `n` or an ancestor: `@bitemporal`, `@multi` and `@symmetric` are fixed at the root of
a tree. -/
def inherits (p : CType T → Bool) (n : Name) : Bool := (K.chain n).any fun m => ((K.find m).map p).getD false

/-- The key `n` declares or inherits. -/
def keyOf (n : Name) : List Name :=
  ((K.chain n).findSome? fun m => (K.find m).bind fun c => if c.key.isEmpty then none else some c.key).getD []

/-- The `asserted` field of a `@bitemporal` type (`SCHEMA-APG.md` section 13.2). -/
def assertedType : NTy T.P Name := .record [("end", NTy.optional (.prim K.dateTime)), ("start", .prim K.dateTime)]

/-- The name of the role label of `R`'s set-valued role `r`. -/
def roleLabelName (R r : Name) : Name := R ++ "_" ++ r

/-- The role label named `n`: its concrete relation, the type declaring the role, the role and its participants. -/
def roleLabelOf (n : Name) : Option (Name × Name × Name × List Name) :=
  K.types.findSome? fun c =>
    if c.concrete then
      (K.ownedFields c.name).findSome? fun f =>
        match f with
        | (owner, .setRole r ps _) => if roleLabelName c.name r = n then some (c.name, owner, r, ps) else none
        | _ => none
    else none

/-- A field of a concrete type's record: an attribute with its view, a single-valued role with the type declaring it, or
`asserted`. -/
inductive Slot (T : Typeside) where
  | attr (optional : Bool) (t : NTy T.P Name) (view : AttrView)
  | role (participants : List Name) (optional : Bool) (owner : Name)
  | asserted

/-- A concrete type's record fields, `asserted` with them on a `@bitemporal` type, in increasing name order. -/
def slots (n : Name) : List (Name × Slot T) :=
  sortBy (fun a b => decide (a.1 < b.1))
    (((K.ownedFields n).filterMap fun x =>
      match x with
      | (_, .attr a t opt view) => some (a, .attr opt t view)
      | (owner, .role r ps opt _ _) => some (r, .role ps opt owner)
      | (_, .setRole _ _ _) => none) ++
      if K.inherits CType.bitemporal n then [("asserted", .asserted)] else [])

/-- A slot's type in `S(K)`. -/
def slotType : Slot T → NTy T.P Name
  | .attr opt t _ => if opt then NTy.optional t else t
  | .role ps opt _ => if opt then NTy.optional (K.participantType ps) else K.participantType ps
  | .asserted => K.assertedType

/-- `S(K)`: a concrete type's record of its slots, and a role label's record of its instance and participant
(sections 12.3, 13.1 to 13.3). Any other name has the unit type and is no label. -/
def schema : NSchema T.P Name where
  σ n :=
    match K.find n with
    | some c => if c.concrete then .record ((K.slots n).map fun x => (x.1, K.slotType x.2)) else NTy.unit
    | none =>
      match K.roleLabelOf n with
      | some (R, _, _, ps) => roleLabelType R (K.participantType ps)
      | none => NTy.unit

/-- The labels of `S(K)`: every concrete type, then every role label. -/
def labels : List Name :=
  (K.types.filter CType.concrete).map CType.name ++
    (K.types.filter CType.concrete).foldr (fun c acc =>
      ((K.allFields c.name).filterMap fun f =>
        match f with
        | .setRole r _ _ => some (roleLabelName c.name r)
        | _ => none) ++ acc) []

/-! ## Relation encodings, `SCHEMA.md` section 11.1 -/

/-- The role named `r` at its most specific: the role of `fs` specialising it, followed down, else `r`. -/
def resolveRole (fs : List (Field T)) (r : Name) : Name := go fs.length r
where
  /-- Within `fuel` steps. -/
  go : Nat → Name → Name
    | 0, r => r
    | k + 1, r => match fs.find? fun f => f.specializes == some r with
      | some f => go k f.name
      | none => r

/-- The roles of `n` in declaration order, root first, each super role at its most specific. -/
def rolesInOrder (n : Name) : List Name :=
  let fs := (K.chain n).reverse.flatMap fun m => ((K.find m).map CType.fields).getD []
  (fs.filter fun f => f.isRole && f.specializes.isNone).map fun f => resolveRole fs f.name

/-- The anchor role: the `@colocate` role `n` declares or inherits from the nearest supertype declaring one, else the
first role in declaration order (`SCHEMA.md` section 9.1). -/
def anchorRole (n : Name) : Option Name :=
  let fs := (K.chain n).reverse.flatMap fun m => ((K.find m).map CType.fields).getD []
  match (K.chain n).findSome? fun m => (K.find m).bind CType.colocate with
  | some r => some (resolveRole fs r)
  | none => (K.rolesInOrder n).head?

/-- A relation is edge-encoded when it has exactly two roles, both required and single-valued, and no key. -/
def edgeEncoded (n : Name) : Bool :=
  let roles := (K.allFields n).filter Field.isRole
  ((K.find n).map fun c => c.kind == .relation).getD false && roles.length == 2 &&
    roles.all (fun f => match f with | .role _ _ false _ _ => true | _ => false) && (K.keyOf n).isEmpty

/-- A role's navigation name. -/
def navOf (n r : Name) : Option Name :=
  (K.allFields n).findSome? fun f =>
    match f with
    | .role m _ _ nav _ => if m == r then nav else none
    | _ => none

/-- An edge-encoded relation's other role, beside its anchor. -/
def otherRole (n : Name) : Option Name :=
  ((K.allFields n).filter Field.isRole).findSome? fun f => if some f.name == K.anchorRole n then none else some f.name

/-- The root of `n`'s tree. -/
def root (n : Name) : Name := (K.chain n).getLastD n

/-- An edge-encoded relation allocates a reifier when its instances carry attributes, `asserted` among them, it is
`@multi`, its tree holds more than one concrete type, or a role of any type names it, a supertype or a statement type
covering it. -/
def allocatesReifier (n : Name) : Bool :=
  (K.allFields n).any (fun f => !f.isRole) || K.inherits CType.bitemporal n || K.inherits CType.multi n ||
    decide (1 < (K.conc (K.root n)).length) ||
    K.types.any fun c => c.fields.any fun f => f.participants.any fun p => (K.chain n).contains p

end Catalog

end ApgModel.Catalog
