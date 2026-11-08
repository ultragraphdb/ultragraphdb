/-
 SPDX-License-Identifier: Apache-2.0
 Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
-/
import ApgModel.Named.Collections
import ApgModel.Core.DeltaCompose
import ApgModel.Catalog.Constraints

/-!
# The specification's examples

`SCHEMA-APG.md` examples 6.7 and 9.6. Example 6.7: people, organisations, a membership with its own attributes, and provenance on the
membership and on a person, as a finite APG on its named schema, checked by the kernel. The membership is a
hyperedge label with attributes of its own, `AssertedBy`'s statement field a role whose type is a union of labels,
and `AssertedBy` a meta-edge label, so `a1` is a relation over a relation.

The schema's records list their fields in increasing name order, so the membership's fields read `collection`,
`confidence`, `member`, `role`, and its role enumeration `contractor`, `employee`: one finite map, written one way.
Example 9.6 renames `MemberOf` to `Membership` and drops its `confidence`: a mapping from the target schema to example
6.7's, along which `Δ` carries example 6.7's APG to the target. Every term the mapping holds is checked by Lean
against its type `Φ̂(σ'(l))`, `AssertedBy`'s identity term among them, which type-checks because `Φ̂(σ'(AssertedBy))`
is example 6.7's `σ(AssertedBy)`, as the specification states.
-/

namespace ApgModel.Examples

open Named

/-- The example's primitive types. -/
inductive Prim where
  | string
  | float64
  deriving DecidableEq, Repr

/-- The example's typeside: strings and floats, no functions. -/
def typeside : Typeside where
  P := Prim
  PV
    | .string => String
    | .float64 => Float
  F := Empty
  dom := fun f => nomatch f
  cod := fun f => nomatch f
  interp := fun f => nomatch f

instance : DecidableEq typeside.P := inferInstanceAs (DecidableEq Prim)

/-- Example 6.7's labels. -/
inductive Label where
  | person
  | organization
  | source
  | memberOf
  | assertedBy
  deriving DecidableEq, Repr

/-- Example 6.7's schema, every record's fields in increasing name order. -/
def schema : NSchema typeside.P Label where
  σ
    | .person => .record [("name", .prim .string)]
    | .organization => .record [("label", .prim .string)]
    | .source => .record [("label", .prim .string)]
    | .memberOf => .record [
        ("collection", .lbl .organization),
        ("confidence", NTy.optional (.prim .float64)),
        ("member", .lbl .person),
        ("role", NTy.enum ["contractor", "employee"])]
    | .assertedBy => .record [
        ("asserter", .lbl .source),
        ("statement", .variant [("membership", .lbl .memberOf), ("person", .lbl .person)])]

/-- Example 6.7's elements: Ada 1, Acme 2, the HR system 3, the membership `m1` 4, and the provenance `a1` 5 and
`a2` 6. -/
def apg : FinApg schema where
  elems := [
    (1, .person, .record [("name", .prim (T := typeside) .string "Ada")]),
    (2, .organization, .record [("label", .prim (T := typeside) .string "Acme")]),
    (3, .source, .record [("label", .prim (T := typeside) .string "HR system")]),
    (4, .memberOf, .record [
      ("collection", .elem 2),
      ("confidence", .inj "some" (.prim (T := typeside) .float64 (show Float from 0.98))),
      ("member", .elem 1),
      ("role", .inj "employee" (.record []))]),
    (5, .assertedBy, .record [("asserter", .elem 3), ("statement", .inj "membership" (.elem 4))]),
    (6, .assertedBy, .record [("asserter", .elem 3), ("statement", .inj "person" (.elem 1))])]

/-- Example 6.7 is a valid APG: distinct identifiers, every value of its label's type. -/
theorem apg_valid : apg.valid = true := by decide

/-- `a1` is a relation over a relation: its value references the membership `m1`. -/
theorem a1_references_m1 : 4 ∈ ((apg.entry 5).map fun x => x.2.refs).getD [] := by decide

/-- A membership naming a person as its collection does not check: the reference's label is not the role's. -/
def misfiled : FinApg schema where
  elems := apg.elems ++ [(7, .memberOf, .record [
    ("collection", .elem 1),
    ("confidence", .inj "none" (.record [])),
    ("member", .elem 1),
    ("role", .inj "contractor" (.record []))])]

theorem misfiled_invalid : misfiled.valid = false := by decide

/-- Two elements with one identifier are no APG. -/
def duplicated : FinApg schema where
  elems := apg.elems ++ [(1, .person, .record [("name", .prim (T := typeside) .string "Ada Lovelace")])]

theorem duplicated_invalid : duplicated.valid = false := by decide

/-- Example 6.7 as an APG on the core's binary schema, through proposition 3.3. -/
def coreApg : Core.Apg schema.bin := (apg.toNApg apg_valid).toCore

/-! ## Example 9.6 -/

/-- Example 9.6's target labels: example 6.7's with `Membership` for `MemberOf`. -/
inductive TargetLabel where
  | person
  | organization
  | source
  | membership
  | assertedBy
  deriving DecidableEq, Repr

/-- Example 9.6's target schema: `Membership` without `confidence`, `AssertedBy`'s statement naming it. -/
def targetSchema : NSchema typeside.P TargetLabel where
  σ
    | .person => .record [("name", .prim .string)]
    | .organization => .record [("label", .prim .string)]
    | .source => .record [("label", .prim .string)]
    | .membership => .record [
        ("collection", .lbl .organization),
        ("member", .lbl .person),
        ("role", NTy.enum ["contractor", "employee"])]
    | .assertedBy => .record [
        ("asserter", .lbl .source),
        ("statement", .variant [("membership", .lbl .membership), ("person", .lbl .person)])]

/-- Each target label's image: the source label it reads. -/
def migrationImage : TargetLabel → Core.Ty typeside.P Label
  | .person => .lbl .person
  | .organization => .lbl .organization
  | .source => .lbl .source
  | .membership => .lbl .memberOf
  | .assertedBy => .lbl .assertedBy

/-- Example 9.6's terms, each checked against `Φ̂(σ'(l))`. -/
def migrationTerm : (l : TargetLabel) →
    Core.Tm schema.bin [migrationImage l] (((targetSchema.bin).σ l).subst migrationImage)
  | .person => Core.Tm.deref Label.person (Core.Tm.var Core.Var.here)
  | .organization => Core.Tm.deref Label.organization (Core.Tm.var Core.Var.here)
  | .source => Core.Tm.deref Label.source (Core.Tm.var Core.Var.here)
  | .membership =>
    Core.Tm.pair (Core.Tm.fst (Core.Tm.deref Label.memberOf (Core.Tm.var Core.Var.here)))
      (Core.Tm.pair (Core.Tm.fst (Core.Tm.snd (Core.Tm.snd (Core.Tm.deref Label.memberOf (Core.Tm.var Core.Var.here)))))
        (Core.Tm.pair (Core.Tm.fst (Core.Tm.snd (Core.Tm.snd (Core.Tm.snd (Core.Tm.deref Label.memberOf (Core.Tm.var Core.Var.here))))))
          Core.Tm.unit))
  | .assertedBy => Core.Tm.deref Label.assertedBy (Core.Tm.var Core.Var.here)

/-- Example 9.6's mapping, from the target to the source, since `Δ` carries source data along it: `Membership` reads
a `MemberOf`'s `collection`, `member` and `role`; every other label is the identity, `deref x`. -/
def migration : Core.Mapping targetSchema.bin schema.bin := ⟨migrationImage, migrationTerm⟩

/-- Example 9.6 applied to example 6.7. -/
def migrated : Core.Apg targetSchema.bin := migration.delta coreApg

/-- One `Membership` per `MemberOf`: the image's elements are the origins (proposition 9.5). -/
theorem migrated_memberships : migrated.E .membership = coreApg.E .memberOf := rfl

/-- The membership `m1`. -/
def m1 : coreApg.E .memberOf := ⟨4, by decide⟩

/-- `m1`'s image carries Acme as its collection. -/
theorem m1_collection : (migrated.υ .membership m1).1.1 = 2 := by decide

/-- `m1`'s image carries Ada as its member. -/
theorem m1_member : (migrated.υ .membership m1).2.1.1 = 1 := by decide

/-- `m1`'s image carries the role `employee`, the second member of the enumeration. -/
theorem m1_role : (migrated.υ .membership m1).2.2.1 = Sum.inr (Sum.inl ()) := rfl

/-- The provenance `a1`. -/
def a1 : coreApg.E .assertedBy := ⟨5, by decide⟩

/-- Every `AssertedBy` whose statement names `m1` names its image: `a1`'s statement is `m1` as a `Membership`. -/
theorem a1_names_m1 : (migrated.υ .assertedBy a1).2.1 = Sum.inl m1 := rfl

/-! ## Sections 13 and 14 -/

/-- Example 6.7 as a catalog: the abstract `Agent` above `Person` and `Organization`, and the relation `MemberOf`,
prefixes 1 to 4. -/
def catalog : Catalog.Catalog typeside where
  types := [
    ⟨"Agent", .entity, none, true, 1, []⟩,
    ⟨"Person", .entity, some "Agent", false, 2, [.attr "name" (.prim .string) false]⟩,
    ⟨"Organization", .entity, some "Agent", false, 3, [.attr "label" (.prim .string) false]⟩,
    ⟨"MemberOf", .relation, none, false, 4, [
      .role "member" ["Person"] false none,
      .role "collection" ["Organization"] false none,
      .attr "role" (NTy.enum ["contractor", "employee"]) false,
      .attr "confidence" (.prim .float64) true]⟩]

/-- `conc(Agent)` holds the two concrete agents. -/
theorem catalog_conc : catalog.conc "Agent" = ["Person", "Organization"] := by decide

/-- `Agent`'s prefix set, its own and its descendants', compresses to one range. -/
theorem catalog_ranges : catalog.ranges "Agent" = [(1, 4)] := by decide

/-- `S(K)` gives `MemberOf` example 6.7's record, its fields in increasing name order. -/
theorem catalog_schema : catalog.schema.σ "MemberOf" = .record [
    ("collection", .lbl "Organization"),
    ("confidence", NTy.optional (.prim .float64)),
    ("member", .lbl "Person"),
    ("role", NTy.enum ["contractor", "employee"])] := by decide

/-- Every person is a member of something: participation `+` of `MemberOf.member`. -/
theorem apg_members : Catalog.participationViolations apg [.memberOf] "member" [.person] 1 none = [] := by decide

/-- Nothing is asserted twice per person: participation `?` of `AssertedBy.statement` on persons holds, and
`{2, 2}` fails at Ada. -/
theorem apg_assertions : Catalog.participationViolations apg [.assertedBy] "statement" [.person] 0 (some 1) = [] ∧
    Catalog.participationViolations apg [.assertedBy] "statement" [.person] 2 (some 2) = [1] := by decide

/-- A containment cycle is found, and a chain is not one. -/
theorem cycles : Catalog.cycleViolations [(1, 2), (2, 1)] ≠ [] ∧ Catalog.cycleViolations [(1, 2), (2, 3)] = [] ∧
    Catalog.depthViolations [(1, 2), (2, 3)] 1 = [1] := by decide

end ApgModel.Examples
