/-
 SPDX-License-Identifier: Apache-2.0
 Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
-/
import ApgModel.Named.Collections

/-!
# The specification's examples

`SCHEMA-APG.md` example 6.7: people, organisations, a membership with its own attributes, and provenance on the
membership and on a person, as a finite APG on its named schema, checked by the kernel. The membership is a
hyperedge label with attributes of its own, `AssertedBy`'s statement field a role whose type is a union of labels,
and `AssertedBy` a meta-edge label, so `a1` is a relation over a relation.

The schema's records list their fields in increasing name order, so the membership's fields read `collection`,
`confidence`, `member`, `role`, and its role enumeration `contractor`, `employee`: one finite map, written one way.
Example 9.6, a migration along `Δ`, joins this module with wave L3.
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

end ApgModel.Examples
