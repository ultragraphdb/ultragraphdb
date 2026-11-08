/-
 SPDX-License-Identifier: Apache-2.0
 Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
-/
import ApgModel.Oracle.Gen5
import ApgModel.Views.RdfInverse
import ApgModel.Views.Gql

/-!
# The oracle's checks

What `apg_oracle` checks of every instance set before it writes the set's file (`SCHEMA-APG.md` section 19): the set is
a valid APG; its constraints hold; `R⁻¹(R(G))` holds exactly its elements, each under its identity (proposition 17.3,
here computed); and the file's N-Quads read back as `R(G)` quad for quad (decision L5). A failed check names the set
and what failed, and no file is written. Beside them it checks the GQL view: every type's property names are
distinct, and `P(G)` projected onto MGQL's property graphs conforms to `Γ(S(K))` projected likewise under MGQL's own
`graphConformsSchema` (`SCHEMA-APG.md` section 18).
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

/-- Every primitive value a term holds lies in its type's values (`Prim.admits`): an integer in its width's range, a
float in canonical form. -/
partial def admitted : VTerm typeside → Bool
  | .prim p a => Prim.admits p a
  | .elem _ => true
  | .record fs => fs.all fun f => admitted f.2
  | .inj _ v => admitted v
  | .list vs | .set vs => vs.all admitted

/-- MGQL's sorts of the oracle's primitives: integers as `int`, strings, IRIs, units and quantity kinds as `string`,
Booleans as `bool`; the others have no MGQL sort. -/
def mgqlSorts : MgqlSorts typeside where
  sort
    | .bool => some .bool
    | .int _ => some .int
    | .string | .iri | .unit | .quantityKind => some .string
    | _ => none
  prim
    | .bool, b => .bool b
    | .int _, x => .int x
    | .string, x | .iri, x | .unit, x | .quantityKind, x | .geo, x => .string x
    | _, _ => .bool false
  coh := by
    intro p b a h
    cases p <;> simp at h <;> subst h <;> rfl

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
  (if G.elems.all fun x => admitted x.2.2 then [] else ["a primitive value outside its type"]) ++
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
    | none => ["N-Quads do not read back"]) ++
  (if C.catalog.types.all fun c => decide (((gPropTypes C.catalog c.name).map Prod.fst).Nodup) then []
    else ["property names collide"]) ++
  (if MGQL.graphConformsSchema (mgqlGraph C.catalog mgqlSorts (gql C.catalog ι G)) (mgqlType C.catalog mgqlSorts)
    then [] else ["P(G) does not conform under MGQL"])

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

/-! ## The Gen5 corpus -/

/-- An element is live while its `asserted` has no end; an element without `asserted` always is. -/
def liveIn {S : NSchema typeside.P Name} (G : FinApg S) (e : Nat) : Bool :=
  match (G.entry e).bind fun x => x.2.field? "asserted" with
  | some a => decide (a.field? "end" = some absent)
  | none => true

/-- The Gen5 subset's constraints: `Node`'s key, `teloid_id`'s `@unique`, the role label's exclusivity and `members`'
non-emptiness, `AssertedBy`'s `@set`, the cardinalities `asserted_by?` and `alternative_set?`, and `Relation`'s
`@limit(count() <= 1 per (subject, type) where rank = preferred)`, its subject the role each relation specializes it
to. -/
def gen5Constraints (G : FinApg Gen5.catalog.schema) : List (String × List Nat) :=
  let live := liveIn G
  let all : Nat → Bool := fun _ => true
  let preferred (v : VTerm typeside) : Bool := decide (v.field? "rank" = some (.inj "preferred" (.record [])))
  [("key Node", keyViolations G (Gen5.catalog.conc "Node") ["id"]),
   ("unique teloid_id", uniqueViolations G ["Teloid"] "teloid_id" false),
   ("role label Alternatives_members", roleLabelViolations G "Alternatives_members"),
   ("non-empty members", nonEmptyViolations G "Alternatives" "Alternatives_members"),
   ("set AssertedBy", setViolations G all "AssertedBy" ["statement", "asserter"] false []),
   ("asserted_by?", participationViolations G live ["AssertedBy"] "statement" (Gen5.catalog.conc "Statement") 0
      (some 1)),
   ("alternative_set?", participationViolations G live ["Alternatives_members"] "participant" ["DerivesFrom"] 0
      (some 1))] ++
  ([("MemberOf", "member"), ("ConnectsTo", "a"), ("DerivesFrom", "lower"), ("ClassifiedAs", "instance")].map
    fun (R, subject) => ("limit preferred " ++ R,
      limitGroupViolations G live [R] [subject] true (fun _ => true) preferred List.length (fun k => decide (k ≤ 1))))

/-- The Gen5 subset. -/
def gen5 : Corpus where
  name := "gen5"
  catalog := Gen5.catalog
  bases := Gen5.bases
  view := Aviation.viewParams
  ord := Aviation.ord
  instances := Gen5.instances
  constraints := gen5Constraints

/-- The corpus schemas. -/
def corpora : List Corpus := [aviation, gen5]

#eval show IO Unit from do
  for C in corpora do
    for n in [0, 1, 2, 3] do
      let fs := C.failures n
      unless fs.isEmpty do throw (IO.userError s!"{C.name} set {n}: {fs}")

end ApgModel.Oracle
