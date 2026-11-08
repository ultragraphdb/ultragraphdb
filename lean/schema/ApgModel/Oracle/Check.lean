/-
 SPDX-License-Identifier: Apache-2.0
 Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
-/
import ApgModel.Oracle.Aviation
import ApgModel.Views.RdfInverse

/-!
# The oracle's checks

What `apg_oracle` checks of every instance set before it writes the set's file (`SCHEMA-APG.md` section 19): the set is
a valid APG; its constraints hold; `R⁻¹(R(G))` holds exactly its elements, each under its identity (proposition 17.3,
here computed); and the file's N-Quads read back as `R(G)` quad for quad (decision L5). A failed check names the set
and what failed, and no file is written.
-/

namespace ApgModel.Oracle

open Named Catalog Views

/-- Equality of the oracle typeside's view objects. -/
def termEq : Term typeside → Term typeside → Bool
  | .iri a, .iri b | .cls a, .cls b | .member a, .member b => a == b
  | .lit p a, .lit q b => if h : p = q then decide (h ▸ a = b) else false
  | .bool a, .bool b => a == b
  | .json a, .json b | .measure a, .measure b => decide (a = b)
  | .triple a n b, .triple c m d => a == c && n == m && b == d
  | _, _ => false

/-- Equality of quads. -/
def quadEq (q r : Quad typeside) : Bool := q.s == r.s && decide (q.p = r.p) && termEq q.o r.o && q.g == r.g

/-- A schema of the corpus: its catalog, bases, view, set order and instance sets, and the constraint queries its sets
must pass, each named. -/
structure Corpus where
  /-- The schema's name, its folder in the output. -/
  name : String
  /-- The catalog. -/
  catalog : Catalog typeside
  /-- The bases. -/
  bases : Bases
  /-- The view's parameters. -/
  view : ViewParams typeside
  /-- The order of a set's members. -/
  ord : VTerm typeside → VTerm typeside → Bool
  /-- The `n`th instance set. -/
  instances : Nat → FinApg catalog.schema
  /-- The constraint queries, each by name, returning the violating elements. -/
  constraints : FinApg catalog.schema → List (String × List Nat)

/-- What fails of the `n`th set, none when every check passes. -/
def Corpus.failures (C : Corpus) (n : Nat) : List String :=
  let G := C.instances n
  let ι := naming C.catalog C.bases G
  let idOf (s : String) : Nat := ((G.elems.find? fun x => ι x.1 == some s).map Prod.fst).getD 0
  let Q := rdf C.catalog C.view ι (fun _ => C.bases.graph) G
  let inv := rdfInv C.catalog C.view C.ord idOf Q
  (if G.valid then [] else ["invalid"]) ++
  ((C.constraints G).filterMap fun (k, vs) => if vs.isEmpty then none else some (k ++ " " ++ toString vs)) ++
  (if G.elems.all fun x =>
      match ident C.catalog ι x with
      | some i => inv.any fun y => decide (y = (i, x.2.1, x.2.2))
      | none => false
    then [] else ["R⁻¹(R(G)) misses an element"]) ++
  (if inv.all fun y => G.elems.any fun x => decide (ident C.catalog ι x = some y.1) && decide (x.2 = y.2)
    then [] else ["R⁻¹(R(G)) holds an element G lacks"]) ++
  (match readNQuads C.catalog C.bases (exportView C.catalog C.view C.bases G) with
    | some P =>
      if P.length == Q.length && (P.zip Q).all fun (a, b) => quadEq a b then [] else ["N-Quads read back differ"]
    | none => ["N-Quads do not read back"])

/-! ## The aviation corpus -/

/-- A decimal value's scaled integer. -/
def decimalOf : VTerm typeside → Option Int
  | .prim (.decimal _ _) x => some (show Int from x)
  | _ => none

/-- The aviation schema's constraints (`SCHEMA.md` section 16): the keys of `Asset` and `Organization`, `tail_number`'s
`@unique`, `PartOf`'s `@acyclic(max_depth: 16)`, the cardinalities `owner?`, `assembly?` and `certification?`,
`thrust_kn`'s `@range(0, 2000)`, `jurisdiction`'s `@values`, `value > 0`, and `sum(value) <= 50_000_000 per buyer`. -/
def aviationConstraints (G : FinApg Aviation.catalog.schema) : List (String × List Nat) :=
  let all : Nat → Bool := fun _ => true
  let assets := ["Aircraft", "Engine", "Avionics"]
  [("key Asset", keyViolations G assets ["serial"]),
   ("key Organization", keyViolations G ["Company", "Authority"] ["name"]),
   ("unique tail_number", uniqueViolations G ["Aircraft"] "tail_number" false),
   ("acyclic PartOf", acyclicViolations G all "PartOf" "part" "whole"),
   ("max_depth PartOf", maxDepthViolations G all "PartOf" "part" "whole" 16),
   ("owner?", participationViolations G all ["Owns"] "asset" assets 0 (some 1)),
   ("assembly?", participationViolations G all ["PartOf"] "part" assets 0 (some 1)),
   ("certification?", participationViolations G all ["Certifies"] "contract" ["Supply"] 0 (some 1)),
   ("range thrust_kn", checkViolations G ["Engine"] fun v =>
      match v.field? "thrust_kn" with
      | some (.prim .float64 x) => decide (0 ≤ x.mantissa) && decide (x.mantissa * 2 ^ x.exponent.toNat ≤ 2000)
      | _ => false),
   ("values jurisdiction", checkViolations G ["Authority"] fun v =>
      match v.field? "jurisdiction" with
      | some (.prim .string j) => ["EU", "US", "UK"].contains j
      | _ => false),
   ("check value", checkViolations G ["Supply"] fun v =>
      match (v.field? "value").bind decimalOf with
      | some x => decide (0 < x)
      | none => false),
   ("limit value per buyer", limitViolations G all ["Supply"] "buyer" ["Company"] (fun _ => true) (fun _ => true)
      (fun xs => xs.foldl (fun s x =>
        s + (((x.2.2.field? "value").bind decimalOf).getD 0)) (0 : Int))
      (fun s => decide (s ≤ 5000000000)))]

/-- The aviation schema of `SCHEMA.md` section 16. -/
def aviation : Corpus where
  name := "aviation"
  catalog := Aviation.catalog
  bases := Aviation.bases
  view := Aviation.viewParams
  ord := Aviation.ord
  instances := Aviation.instances
  constraints := aviationConstraints

#eval show IO Unit from do
  for n in [0, 1, 2, 3] do
    let fs := aviation.failures n
    unless fs.isEmpty do throw (IO.userError s!"aviation set {n}: {fs}")

end ApgModel.Oracle
