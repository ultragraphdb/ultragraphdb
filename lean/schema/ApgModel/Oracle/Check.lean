/-
 SPDX-License-Identifier: Apache-2.0
 Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
-/
import ApgModel.Oracle.Gen5
import ApgModel.Views.RdfInverse
import ApgModel.Views.Gql

/-!
# The oracle's checks

What `apg_oracle` checks of every instance set before it writes the set's files (`SCHEMA-APG.md` section 19): the set
is a valid APG; its constraints hold; `R⁻¹(R(G))` holds exactly its elements, each under its identity (proposition
17.3, here computed); and the view's N-Quads read back as `R(G)` quad for quad (decision L5). Of the set's write, what a
client writes (`exportWrite`), it checks that its N-Quads read back as its quads, that it states no quad the view
derives or the engine writes (`statesDerived`), and that its view is `R(G)`: the write with every `rdf:type` completed
by the type's supertypes and statement types and every `@symmetric` triple by its reverse, which the view derives
(`SCHEMA.md` section 11.2), and with the engine's `asserted` and `live`, holds the quads of `R(G)` and no other
(`writeView`). Of the set's retractions, which a client applies after the write (`clientRetractions`), it checks that
each is admitted and that the view after them, the engine's `@on_retract` actions applied (`afterRetractions`), is
`R(G)` but for the instants of `asserted`. A failed check names the set and what failed, and no file is written.
Beside them it checks the GQL view: every type's property names are distinct, and `P(G)` projected onto MGQL's
property graphs conforms to `Γ(S(K))` projected likewise under MGQL's own `graphConformsSchema` (`SCHEMA-APG.md`
section 18).
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

/-- Whether a navigation name is a `@symmetric` relation's, whose triple the view answers in both directions. -/
def symmetricNav (K : Catalog typeside) (n : Name) : Bool :=
  K.types.any fun c => c.symmetric && c.fields.any fun f =>
    match f with
    | .role _ _ _ (some m) _ => m == n
    | _ => false

/-- The view of a write: each quad with what the view derives from it, an `rdf:type` with every supertype and
statement type above it and a `@symmetric` triple with its reverse (`SCHEMA.md` section 11.2), and the engine's
`asserted` and `live` quads `A`. -/
def writeView (K : Catalog typeside) (W A : List (Quad typeside)) : List (Quad typeside) :=
  (W.flatMap fun q =>
    match q.p, q.o with
    | .rdfType, .cls c => (K.chain c).map fun C => { q with o := .cls C }
    | .nav n, .iri o => if symmetricNav K n then [q, { q with s := o, o := .iri q.s }] else [q]
    | _, _ => [q]) ++ A

/-- Whether a write states a quad the view derives or the engine writes: a subject's second `rdf:type` or one of an
abstract type, `asserted` or `live`, or a `@symmetric` triple beside its reverse. -/
def statesDerived (K : Catalog typeside) (W : List (Quad typeside)) : Bool :=
  W.any fun q =>
    match q.p, q.o with
    | .rdfType, .cls c =>
      !(((K.find c).map CType.concrete).getD false) ||
        decide (1 < (W.filter fun r => r.s == q.s && decide (r.p = .rdfType)).length)
    | .assertedStart, _ | .assertedEnd, _ | .live, _ => true
    | .nav n, .iri o =>
      symmetricNav K n && o != q.s && W.any fun r => decide (r.p = .nav n) && r.s == o && termEq r.o (.iri q.s)
    | _, _ => false

/-- Two lists of quads hold the same quads. -/
def sameQuads (P Q : List (Quad typeside)) : Bool :=
  P.all (fun a => Q.any (quadEq a)) && Q.all (fun b => P.any (quadEq b))

/-- Equality of quads but for the instants of `asserted`, which the engine's clock decides. -/
def quadEqUpToInstants (q r : Quad typeside) : Bool :=
  match q.p with
  | .assertedStart | .assertedEnd => q.s == r.s && decide (q.p = r.p) && q.g == r.g
  | _ => quadEq q r

/-- Two lists of quads hold the same quads but for the instants of `asserted`. -/
def sameQuadsUpToInstants (P Q : List (Quad typeside)) : Bool :=
  P.all (fun a => Q.any (quadEqUpToInstants a)) && Q.all (fun b => P.any (quadEqUpToInstants b))

/-! ## Retractions -/

/-- The instant at which the element `e` is retracted, its `asserted` end; none while it is live. -/
def retractedAt {S : NSchema typeside.P Name} (G : FinApg S) (e : Nat) : Option (VTerm typeside) :=
  (G.entry e).bind fun x => (x.2.field? "asserted").bind fun a => (a.field? "end").bind (presentValue true)

/-- Whether the element `e` is open: its valid interval, where it has one, without an end. -/
def openIn {S : NSchema typeside.P Name} (G : FinApg S) (e : Nat) : Bool :=
  match (G.entry e).bind fun x => x.2.field? "valid" with
  | some (.inj "some" iv) => decide (iv.field? "end" = some absent)
  | _ => true

/-- Whether one instant precedes another. -/
def instantLt : VTerm typeside → VTerm typeside → Bool
  | .prim .dateTime (a, _), .prim .dateTime (b, _) => decide (a < b)
  | _, _ => false

/-- The `@on_retract` action of the role `r` of the label `l`: the nearest declaring type's, `cascade` where none is
declared (`SCHEMA.md` section 8.4, rule 6). -/
def retractAction (K : Catalog typeside) (l r : Name) : RetractAction :=
  ((K.chain l).findSome? fun m => (K.find m).bind fun c => (c.onRetract.find? (·.1 == r)).map Prod.snd).getD .cascade

/-- The references a retraction follows, each `(x, p, a)`: an element `x` of a `@bitemporal` relation naming `p`
through a role whose `@on_retract` action is `a`, a set-valued role through its role label's elements. -/
def retractRefs (K : Catalog typeside) (G : FinApg K.schema) : List (Nat × Nat × RetractAction) :=
  G.elems.flatMap fun y =>
    match K.find y.2.1 with
    | some c =>
      if c.kind == .relation && K.inherits CType.bitemporal y.2.1 then
        (K.allFields y.2.1).flatMap fun f =>
          match f with
          | .role r _ _ _ _ => (fieldRefs y.2.2 r).map fun p => (y.1, p, retractAction K y.2.1 r)
          | _ => []
      else []
    | none =>
      match K.roleLabelOf y.2.1 with
      | some (R, _, r, _) =>
        if K.inherits CType.bitemporal R then
          (fieldRefs y.2.2 "instance").flatMap fun i =>
            (fieldRefs y.2.2 "participant").map fun p => (i, p, retractAction K R r)
        else []
      | none => []

/-- The elements a client retracts after the write, in the order of their retractions' instants: each `R(G)` shows
retracted, but those a participant's retraction at the same instant brings with it, by `cascade` or `restrict open`,
which the engine retracts by itself (`SCHEMA.md` section 8.4, rule 6). -/
def clientRetractions (K : Catalog typeside) (G : FinApg K.schema) : List Nat :=
  let refs := retractRefs K G
  let own := (G.elems.filterMap fun x => (retractedAt G x.1).map fun t => (x.1, t)).filter fun (x, t) =>
    !refs.any fun (y, p, a) => y == x && a != .restrict && decide (retractedAt G p = some t)
  (sortBy (fun a b => instantLt a.2 b.2) own).map Prod.fst

/-- One round of a retraction's consequences: every live element naming a retracted one is retracted with it under
`cascade`, and under `restrict open` when it is not open; none when the commit is refused, a live element naming a
retracted one under `restrict` or an open one under `restrict open` (`UGS1020`). -/
def retractRound (refs : List (Nat × Nat × RetractAction)) (isOpen live : Nat → Bool) (acc : List Nat) :
    Option (List Nat) :=
  refs.foldlM (fun acc (x, p, a) =>
    if acc.contains p && live x && !acc.contains x then
      match a with
      | .cascade => some (acc ++ [x])
      | .restrictOpen => if isOpen x then none else some (acc ++ [x])
      | .restrict => none
    else some acc) acc

/-- The rounds of a retraction's consequences until none adds an element, at most `fuel` of them. -/
def retractFix (refs : List (Nat × Nat × RetractAction)) (isOpen live : Nat → Bool) :
    Nat → List Nat → Option (List Nat)
  | 0, acc => some acc
  | fuel + 1, acc =>
    (retractRound refs isOpen live acc).bind fun acc' =>
      if acc'.length == acc.length then some acc else retractFix refs isOpen live fuel acc'

/-- What retracting the element `e` retracts after the elements `gone` are retracted: `e` and the live elements its
retraction brings with it; none when the retraction is refused, `e` not live or of a type that is not `@bitemporal`
(`UGS1019`, `UGS1026`), or a `restrict` (`UGS1020`). -/
def retractCommit (K : Catalog typeside) (G : FinApg K.schema) (gone : List Nat) (e : Nat) : Option (List Nat) :=
  let refs := retractRefs K G
  let live (x : Nat) : Bool := !gone.contains x
  if live e && ((G.entry e).map fun x => K.inherits CType.bitemporal x.1).getD false then
    retractFix refs (openIn G) live (refs.length + 1) [e]
  else none

/-- The elements retracted once the client's retractions `rs` are applied in turn to the write; none when one is
refused. -/
def afterRetractions (K : Catalog typeside) (G : FinApg K.schema) (rs : List Nat) : Option (List Nat) :=
  rs.foldlM (fun gone e => (retractCommit K G gone e).map (gone ++ ·)) []

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
  let W := G.elems.flatMap fun x =>
    (writeTemplates C.catalog C.view x.1 x.2.1 x.2.2).filterMap (Tmpl.inst ι fun _ => C.bases.graph)
  let A := G.elems.flatMap fun x =>
    (if C.catalog.inherits CType.bitemporal x.2.1 then assertedTemplates x.1 x.2.2 else []).filterMap
      (Tmpl.inst ι fun _ => C.bases.graph)
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
  (match readNQuads C.catalog C.bases (exportWrite C.catalog C.view C.bases G) with
    | some P =>
      if P.length == W.length && (P.zip W).all fun (a, b) => quadEq a b then []
      else ["the write's N-Quads read back differ"]
    | none => ["the write's N-Quads do not read back"]) ++
  (if statesDerived C.catalog W then ["the write states a quad the view derives"] else []) ++
  (if sameQuads (writeView C.catalog W A) Q then [] else ["the write's view differs from R(G)"]) ++
  (match afterRetractions C.catalog G (clientRetractions C.catalog G) with
    | some gone =>
      let A' := G.elems.flatMap fun x =>
        let start := (x.2.2.field? "asserted").bind fun a => a.field? "start"
        (if C.catalog.inherits CType.bitemporal x.2.1 then
          assertedFrom x.1 start (if gone.contains x.1 then start else none) else []).filterMap
          (Tmpl.inst ι fun _ => C.bases.graph)
      if sameQuadsUpToInstants (writeView C.catalog W A') Q then []
      else ["the view after the write and its retractions differs from R(G)"]
    | none => ["a retraction is refused"]) ++
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
