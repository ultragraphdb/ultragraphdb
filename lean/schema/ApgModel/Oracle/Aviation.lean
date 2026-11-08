/-
 SPDX-License-Identifier: Apache-2.0
 Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
-/
import ApgModel.Oracle.Parse
import ApgModel.Catalog.Constraints

/-!
# The aviation example

`SCHEMA.md` section 16's aviation schema as a catalog over the oracle's typeside (decision L8), and a generator of its
valid instance sets.

The catalog follows the compiler's report in section 16: prefixes 3 to 15 in declaration order; `Owns` and `Certifies`
edge-encoded with reifiers for their attributes, anchored at `asset` and `contract`; `PartOf` edge-encoded without one,
anchored at `part`; `Supply` and `Lease` node-encoded below the abstract `@multi` `Agreement`, their roles specializing
`party` and `counterparty`. `embedding` is left out: `Vector` waits for stage 8 (decision L6). Value checks, indexes,
derivations and functions shape no element, and the generator honours them by construction.

`instances n` is the `n`th instance set: `n + 2` companies and an authority, and per index `i < n + 1` an aircraft, an
engine and an avionics unit, an ownership, the engine's and the avionics' assembly into the aircraft, a supply of the
engine, a lease of the aircraft and the supply's certification. Keys, `tail_number`, `@range`, `@values` and
`@pattern` hold; each supply's value is positive and no buyer's sum reaches `@limit`'s bound; `PartOf` is acyclic with
depth one; each asset has one owner, each part one assembly, each contract one certification.
-/

namespace ApgModel.Oracle.Aviation

open Named Catalog Views

/-- A date's interval: optional bounds, each with its optional precision (`SCHEMA-APG.md` section 13.2). -/
def dateInterval : NTy typeside.P Name :=
  .record [("end", NTy.optional (.prim Prim.date)), ("end_precision", NTy.optional (.prim Prim.precision)),
    ("start", NTy.optional (.prim Prim.date)), ("start_precision", NTy.optional (.prim Prim.precision))]

/-- The aviation catalog. -/
def catalog : Catalog typeside where
  dateTime := Prim.dateTime
  types := [
    { name := "Asset", kind := .entity, parent := none, abstract := true, pfx := 3, key := ["serial"],
      fields := [
        .attr "serial" (.prim Prim.string) false .plain,
        .attr "commissioned" (.prim Prim.date) false .plain,
        .attr "status" (NTy.enum ["active", "retired", "stored"]) false .plain,
        .attr "tags" (.set (.prim Prim.string)) false .plain,
        .attr "notes" (.prim Prim.text) true .plain] },
    { name := "Aircraft", kind := .entity, parent := some "Asset", abstract := false, pfx := 4,
      fields := [.attr "tail_number" (.prim Prim.string) false .plain] },
    { name := "Engine", kind := .entity, parent := some "Asset", abstract := false, pfx := 5,
      fields := [.attr "thrust_kn" (.prim Prim.float64) false .plain] },
    { name := "Avionics", kind := .entity, parent := some "Asset", abstract := false, pfx := 6,
      fields := [.attr "firmware" (.prim Prim.string) true .plain] },
    { name := "Organization", kind := .entity, parent := none, abstract := true, pfx := 7, key := ["name"],
      fields := [.attr "name" (.prim Prim.string) false .plain] },
    { name := "Company", kind := .entity, parent := some "Organization", abstract := false, pfx := 8, fields := [] },
    { name := "Authority", kind := .entity, parent := some "Organization", abstract := false, pfx := 9,
      fields := [.attr "jurisdiction" (.prim Prim.string) false .plain] },
    { name := "Owns", kind := .relation, parent := none, abstract := false, pfx := 10, colocate := some "asset",
      fields := [
        .role "owner" ["Organization"] false (some "assets") none,
        .role "asset" ["Asset"] false (some "owner") none,
        .attr "since" (.prim Prim.date) false .plain] },
    { name := "PartOf", kind := .relation, parent := none, abstract := false, pfx := 11,
      fields := [
        .role "part" ["Asset"] false (some "assembly") none,
        .role "whole" ["Asset"] false (some "parts") none] },
    { name := "Agreement", kind := .relation, parent := none, abstract := true, pfx := 12, multi := true,
      fields := [
        .role "party" ["Organization"] false (some "agreements") none,
        .role "counterparty" ["Organization"] false (some "counterparty_agreements") none,
        .attr "signed" (.prim Prim.date) false .plain] },
    { name := "Supply", kind := .relation, parent := some "Agreement", abstract := false, pfx := 13,
      fields := [
        .role "supplier" ["Company"] false (some "sales") (some "party"),
        .role "buyer" ["Company"] false (some "purchases") (some "counterparty"),
        .role "item" ["Asset"] false (some "contracts") none,
        .attr "value" (.prim (Prim.decimal 18 2)) false .plain] },
    { name := "Lease", kind := .relation, parent := some "Agreement", abstract := false, pfx := 14,
      fields := [
        .role "lessor" ["Organization"] false (some "leases_out") (some "party"),
        .role "lessee" ["Organization"] false (some "leases_in") (some "counterparty"),
        .role "asset" ["Aircraft"] false (some "leases") none,
        .attr "term" dateInterval false .interval] },
    { name := "Certifies", kind := .relation, parent := none, abstract := false, pfx := 15,
      colocate := some "contract",
      fields := [
        .role "authority" ["Authority"] false (some "certifications") none,
        .role "contract" ["Supply"] false (some "certification") none,
        .attr "valid" dateInterval false .interval] }]

/-- The aviation schema's bases: `av:` for its vocabulary and instances, and the graph `production`. -/
def bases : Bases where
  vocab := "https://example.org/aviation#"
  inst := "https://example.org/aviation#"
  graph := "https://example.org/aviation#production"

/-! ## Building values -/

/-- A record of the label `l`'s slots in their order, a slot not given absent. -/
def record (l : Name) (kvs : List (Name × VTerm typeside)) : VTerm typeside :=
  .record ((catalog.slots l).map fun s => (s.1, ((kvs.find? (·.1 == s.1)).map Prod.snd).getD absent))

/-- A primitive value. -/
def prim (p : Prim) (a : p.V) : VTerm typeside := .prim (T := typeside) p a

/-- A present optional value. -/
def present (v : VTerm typeside) : VTerm typeside := .inj "some" v

/-- A reference to the element `e` of label `l` through a role admitting `ps`: tagged when the role admits several
types. -/
def ref (ps : List Name) (l : Name) (e : Nat) : VTerm typeside :=
  match catalog.participantType ps with
  | .lbl _ => .elem e
  | _ => .inj l (.elem e)

/-- A string's digits, at least four. -/
def num4 (i : Nat) : String := pad 4 i

/-! ## Instance sets -/

/-- The companies of the `n`th set. -/
def companies (n : Nat) : List Nat := (List.range (n + 2)).map (· + 1)

/-- The `n`th instance set. -/
def instances (n : Nat) : FinApg catalog.schema where
  elems :=
    let cs := companies n
    let company (j : Nat) := cs.getD (j % cs.length) 1
    let day (i : Nat) : Int := 19000 + i
    (cs.map fun c => (c, "Company", record "Company" [("name", prim .string ("Company " ++ num4 c))])) ++
    [(100, "Authority", record "Authority" [("name", prim .string "EASA"), ("jurisdiction", prim .string "EU")])] ++
    (List.range (n + 1)).flatMap fun i =>
      [(1000 + i, "Aircraft", record "Aircraft" [
          ("serial", prim .string ("AC-" ++ num4 i)), ("commissioned", prim .date (day i)),
          ("status", .inj "active" (.record [])),
          ("tags", .set [prim .string "fleet", prim .string ("wing-" ++ num4 i)]),
          ("notes", if i % 2 = 0 then present (prim .text ("overhauled " ++ toString i, some "en")) else absent),
          ("tail_number", prim .string ("D-A" ++ num4 i))]),
        (2000 + i, "Engine", record "Engine" [
          ("serial", prim .string ("EN-" ++ num4 i)), ("commissioned", prim .date (day i)),
          ("status", .inj (if i % 3 = 0 then "stored" else "active") (.record [])),
          ("tags", .set [prim .string "engine"]),
          ("thrust_kn", prim .float64 ⟨241 + 2 * i, -1⟩)]),
        (3000 + i, "Avionics", record "Avionics" [
          ("serial", prim .string ("AV-" ++ num4 i)), ("commissioned", prim .date (day i)),
          ("status", .inj "retired" (.record [])),
          ("tags", .set [prim .string "cockpit"]),
          ("firmware", if i % 2 = 1 then present (prim .string ("v" ++ toString i)) else absent)]),
        (4000 + i, "Owns", record "Owns" [
          ("owner", ref ["Organization"] "Company" (company i)),
          ("asset", ref ["Asset"] "Aircraft" (1000 + i)), ("since", prim .date (day i + 30))]),
        (5000 + i, "PartOf", record "PartOf" [
          ("part", ref ["Asset"] "Engine" (2000 + i)), ("whole", ref ["Asset"] "Aircraft" (1000 + i))]),
        (5500 + i, "PartOf", record "PartOf" [
          ("part", ref ["Asset"] "Avionics" (3000 + i)), ("whole", ref ["Asset"] "Aircraft" (1000 + i))]),
        (6000 + i, "Supply", record "Supply" [
          ("supplier", ref ["Company"] "Company" (company i)),
          ("buyer", ref ["Company"] "Company" (company (i + 1))),
          ("item", ref ["Asset"] "Engine" (2000 + i)),
          ("value", prim (.decimal 18 2) (150000000 + 25 * i)), ("signed", prim .date (day i + 1))]),
        (7000 + i, "Lease", record "Lease" [
          ("lessor", ref ["Organization"] "Company" (company i)),
          ("lessee", ref ["Organization"] "Company" (company (i + 1))),
          ("asset", ref ["Aircraft"] "Aircraft" (1000 + i)),
          ("term", .record [
            ("end", present (prim .date (daysOfCivil 2030 1 1))),
            ("end_precision", present (prim .precision .month)),
            ("start", present (prim .date (day i + 2))),
            ("start_precision", present (prim .precision .day))]),
          ("signed", prim .date (day i + 2))]),
        (8000 + i, "Certifies", record "Certifies" [
          ("authority", ref ["Authority"] "Authority" 100),
          ("contract", ref ["Supply"] "Supply" (6000 + i)),
          ("valid", .record [
            ("end", absent), ("end_precision", absent),
            ("start", present (prim .date (day i + 3))),
            ("start_precision", present (prim .precision .day))])])]

/-! ## The view's parameters -/

/-- The precision of a temporal type's own unit: a day for `Date`, a nanosecond for the others. -/
def nativePrecision : Prim → Option ((q : Prim) × q.V)
  | .date => some ⟨.precision, Precision.day⟩
  | .dateTime | .localDateTime | .time => some ⟨.precision, Precision.nanosecond⟩
  | _ => none

/-- The oracle's view: a precision shown where it is not its bound's type's own. -/
def viewParams : ViewParams typeside where
  precisionShown := fun p q x =>
    match nativePrecision p with
    | some ⟨q', x'⟩ => !(if h : q' = q then decide (h ▸ x' = x) else false)
    | none => true
  native := nativePrecision

/-- The order of a set's members: primitive values by their type's order, members by name. -/
def ord : VTerm typeside → VTerm typeside → Bool
  | .prim p a, .prim q b => if h : p = q then Prim.cmp q (h ▸ a) b == .lt else false
  | .inj m _, .inj n _ => decide (m < n)
  | _, _ => false

end ApgModel.Oracle.Aviation
