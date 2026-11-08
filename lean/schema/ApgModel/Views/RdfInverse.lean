/-
 SPDX-License-Identifier: Apache-2.0
 Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
-/
import ApgModel.Views.Rdf

/-!
# The inverse of the RDF view

`SCHEMA-APG.md` proposition 17.3: on valid APGs on `S(K)`, `R` is injective up to naming, and a partial function `R⁻¹`
on its image gives back each element, identified by its IRI, its triple or its pair (section 15.1).

`rdfInv` reads a list of quads:

- an element named by an IRI is a subject of an `rdf:type` quad; its label is the most specific of its types, the one
  whose chain holds every other (`labelOf`), and its value the record of its label's slots, each read from the quads
  under its IRI (`decodeElem`): an attribute's literals, an interval's bounds with each missing precision its type's
  own, a single-valued role's participant from its role triple or, on an edge-encoded relation, from the reifier's
  triple term, `asserted` from the built-in predicates;
- an element of an edge-encoded relation without a reifier is a navigation triple, its relation the one whose anchor
  role declares that navigation name and admits the subject's type (`tripleElem`); a `@symmetric` relation's two
  triples give one element, the one in IRI order;
- an element of a role label is a role triple of a set-valued role (`pairElem`).

References are read back through `idOf`, the naming's inverse, so `R⁻¹` is relative to the naming, which is what
"up to naming" means. A set's members are read back in the order `ord` sorts them, without repetitions.
-/

namespace ApgModel.Views

open Named Catalog

variable {T : Typeside}

/-- An element's identity in the view (`SCHEMA-APG.md` section 15.1): its IRI, its triple, or its role label with the
pair of its instance and participant. -/
inductive Ident where
  | iri (i : String)
  | triple (s : String) (n : Name) (o : String)
  | pair (l : Name) (i p : String)
  deriving DecidableEq, Repr

variable (K : Catalog T)

/-- An edge-encoded relation without a reifier, whose elements are named by their triples. -/
def tripleNamed (l : Name) : Bool :=
  ((K.find l).map fun c => c.kind == .relation).getD false && K.edgeEncoded l && !K.allocatesReifier l

/-- An element's identity under the naming `ι`. -/
def ident (ι : Nat → Option String) (x : Nat × Name × VTerm T) : Option Ident :=
  match K.find x.2.1 with
  | some _ =>
    if tripleNamed K x.2.1 then
      (K.anchorRole x.2.1).bind fun a => (K.otherRole x.2.1).bind fun o => (K.navOf x.2.1 a).bind fun n =>
        match fieldRefs x.2.2 a, fieldRefs x.2.2 o with
        | [pa], [po] => (ι pa).bind fun s => (ι po).map fun t => .triple s n t
        | _, _ => none
    else (ι x.1).map .iri
  | none =>
    match fieldRefs x.2.2 "instance", fieldRefs x.2.2 "participant" with
    | [i], [p] => (ι i).bind fun s => (ι p).map fun t => .pair x.2.1 s t
    | _, _ => none

/-! ## Reading quads -/

/-- The objects of the quads with subject `s` and predicate `p`, in order. -/
def objectsOf (Q : List (Quad T)) (s : String) (p : Pred) : List (Term T) :=
  (Q.filter fun q => q.s == s && q.p == p).map Quad.o

/-- The classes of the subject `s`. -/
def classesOf (Q : List (Quad T)) (s : String) : List Name :=
  (objectsOf Q s .rdfType).filterMap fun o =>
    match o with
    | .cls c => some c
    | _ => none

/-- The label of the subject `s`: the most specific of its classes, whose chain holds every other. -/
def labelOf (Q : List (Quad T)) (s : String) : Option Name :=
  let cs := classesOf Q s
  cs.find? fun c => cs.all fun c' => (K.chain c).contains c'

/-- A value object read back as a value term. -/
def termValue : Term T → VTerm T
  | .lit p a => .prim p a
  | .member m => .inj m (.record [])
  | .json v => v
  | .measure v => v
  | .cls c => .inj c (.record [])
  | _ => .record []

/-- The one object of a single-valued field. -/
def single (os : List (Term T)) : VTerm T :=
  match os with
  | [o] => termValue o
  | _ => .record []

/-- Insertion into a list sorted by `lt`, a member already there kept once. -/
def insertOrd (lt : VTerm T → VTerm T → Bool) (x : VTerm T) : List (VTerm T) → List (VTerm T)
  | [] => [x]
  | y :: ys => if lt y x then y :: insertOrd lt x ys else if lt x y then x :: y :: ys else y :: ys

/-- A list sorted by `lt`, without repetitions. -/
def sortOrd (lt : VTerm T → VTerm T → Bool) (l : List (VTerm T)) : List (VTerm T) := l.foldr (insertOrd lt) []

/-- An optional field's absence. -/
def absent : VTerm T := .inj "none" (.record [])

/-- A field read from its objects, absent when there are none and the field is optional. -/
def optional (opt : Bool) (os : List (Term T)) (k : List (Term T) → VTerm T) : VTerm T :=
  if opt then (if os.isEmpty then absent else .inj "some" (k os)) else k os

/-- An attribute's present value of type `t` read from its objects: a set's members sorted, else the one object. -/
def plainValue (ord : VTerm T → VTerm T → Bool) (t : NTy T.P Name) (os : List (Term T)) : VTerm T :=
  match t with
  | .set _ => .set (sortOrd ord (os.map termValue))
  | _ => single os

variable (vp : ViewParams T) (ord : VTerm T → VTerm T → Bool) (idOf : String → Nat)

/-- An interval bound's precision: its precision quad, else its type's own. -/
def precisionOf (os bound : List (Term T)) : VTerm T :=
  match os with
  | [o] => termValue o
  | _ =>
    match single bound with
    | .prim p _ => match vp.native p with
      | some ⟨q, x⟩ => .prim q x
      | none => .record []
    | _ => .record []

/-- An interval attribute `a` of type `t` read from its bound quads: each bound optional, each precision present
exactly when its bound is; an optional interval is present when a bound is (`SCHEMA.md` section 5.5). -/
def intervalValue (Q : List (Quad T)) (s : String) (a : Name) (t : NTy T.P Name) : VTerm T :=
  let b (x : Bound) := objectsOf Q s (.bound a x)
  let names : List Name := match t with
    | .record fs => fs.map Prod.fst
    | _ => []
  .record (names.map fun n =>
    (n, match n with
      | "start" => optional true (b .start) single
      | "end" => optional true (b .stop) single
      | "start_precision" => optional true (b .start) fun bs => precisionOf vp (b .startPrecision) bs
      | "end_precision" => optional true (b .stop) fun bs => precisionOf vp (b .stopPrecision) bs
      | _ => .record []))

/-- A participant read back: its identifier, tagged with its label when the role admits several types. -/
def participantValue (Q : List (Quad T)) (ps : List Name) (p : String) : VTerm T :=
  match K.participantType ps with
  | .lbl _ => .elem (idOf p)
  | _ => .inj ((labelOf K Q p).getD "") (.elem (idOf p))

/-- The participants of the role `r` of the element `s` of label `l`: from its reifier's triple term on an
edge-encoded relation, else from its role triples. -/
def roleObjects (Q : List (Quad T)) (s l r owner : Name) : List String :=
  if K.edgeEncoded l then
    (objectsOf Q s .reifies).filterMap fun o =>
      match o with
      | .triple a _ b => if some r == K.anchorRole l then some a else some b
      | _ => none
  else
    (objectsOf Q s (.role owner r)).filterMap fun o =>
      match o with
      | .iri p => some p
      | _ => none

/-- A slot of the element `s` of label `l`, read from the quads. -/
def decodeSlot (Q : List (Quad T)) (s l : Name) : Name × Catalog.Slot T → VTerm T
  | (a, .attr opt t view) =>
    match view with
    | .plain => optional opt (objectsOf Q s (.attr a)) (plainValue ord t)
    | .measure | .typeValue => optional opt (objectsOf Q s (.attr a)) single
    | .interval =>
      let present := !(objectsOf Q s (.bound a .start)).isEmpty || !(objectsOf Q s (.bound a .stop)).isEmpty
      if opt then (if present then .inj "some" (intervalValue vp Q s a t) else absent) else intervalValue vp Q s a t
  | (r, .role ps opt owner) =>
    let ps' := roleObjects K Q s l r owner
    if opt then
      (match ps' with
        | [p] => .inj "some" (participantValue K idOf Q ps p)
        | _ => absent)
    else
      (match ps' with
        | [p] => participantValue K idOf Q ps p
        | _ => .record [])
  | (_, .asserted) =>
    .record [("end", optional true (objectsOf Q s .assertedEnd) single),
      ("start", single (objectsOf Q s .assertedStart))]

/-- The value of the element `s` of label `l`: the record of its slots. -/
def decodeElem (Q : List (Quad T)) (s l : Name) : VTerm T :=
  .record ((K.slots l).map fun x => (x.1, decodeSlot K vp ord idOf Q s l x))

/-- The element named by the IRI `s`, from one of its type quads. -/
def iriElem (Q : List (Quad T)) (q : Quad T) : Option (Ident × Name × VTerm T) :=
  if q.p == .rdfType then (labelOf K Q q.s).map fun l => (.iri q.s, l, decodeElem K vp ord idOf Q q.s l) else none

/-- The anchor role of `l` admits a participant of label `lp`. -/
def admitsAnchor (l lp : Name) : Bool :=
  (K.allFields l).any fun f => some f.name == K.anchorRole l && f.participants.any fun p => (K.chain lp).contains p

/-- The other role of `l` admits a participant of label `lp`. -/
def admitsOther (l lp : Name) : Bool :=
  (K.allFields l).any fun f => some f.name == K.otherRole l && f.participants.any fun p => (K.chain lp).contains p

/-- The relation a navigation triple `s {n} o` names an element of: concrete, edge-encoded without a reifier, its
anchor role declaring `n` and admitting the subject's label `ls`, its other role the object's `lo`; a `@symmetric`
relation's triple read in IRI order. -/
def tripleMatch (n : Name) (s o ls lo : String) (c : CType T) : Bool :=
  c.concrete && tripleNamed K c.name && ((K.anchorRole c.name).bind (K.navOf c.name) == some n) &&
    admitsAnchor K c.name ls && admitsOther K c.name lo &&
    (!K.inherits CType.symmetric c.name || decide (s ≤ o))

/-- A slot of an element named by a triple: a role's participant, the subject at the anchor, the object at the other
role. -/
def tripleSlot (Q : List (Quad T)) (a b s o : Name) (x : Name × Catalog.Slot T) : VTerm T :=
  match x.2 with
  | .role ps _ _ => participantValue K idOf Q ps (if x.1 == a then s else if x.1 == b then o else "")
  | _ => .record []

/-- The element named by a navigation triple, of the edge-encoded relation without a reifier `tripleMatch` finds. -/
def tripleElem (Q : List (Quad T)) (q : Quad T) : Option (Ident × Name × VTerm T) :=
  match q.p, q.o with
  | .nav n, .iri o =>
    (labelOf K Q q.s).bind fun ls => (labelOf K Q o).bind fun lo =>
      (K.types.find? (tripleMatch K n q.s o ls lo)).bind fun c =>
      (K.anchorRole c.name).bind fun a => (K.otherRole c.name).map fun b =>
        (.triple q.s n o, c.name, .record ((K.slots c.name).map fun x => (x.1, tripleSlot K idOf Q a b q.s o x)))
  | _, _ => none

/-- The participants of the set-valued role `r`, if the field is that role. -/
def setRoleOf (r : Name) : Field T → Option (List Name)
  | .setRole m ps _ => if m == r then some ps else none
  | _ => none

/-- The element of a role label named by a role triple of a set-valued role. -/
def pairElem (Q : List (Quad T)) (q : Quad T) : Option (Ident × Name × VTerm T) :=
  match q.p, q.o with
  | .role owner r, .iri p =>
    (K.find owner).bind fun c =>
      (c.fields.findSome? (setRoleOf r)).bind fun ps =>
      (labelOf K Q q.s).map fun R =>
        (.pair (Catalog.roleLabelName R r) q.s p, Catalog.roleLabelName R r,
          .record [("instance", .elem (idOf q.s)), ("participant", participantValue K idOf Q ps p)])
  | _, _ => none

/-- `R⁻¹`: every element the quads name, by IRI, triple or pair. -/
def rdfInv (Q : List (Quad T)) : List (Ident × Name × VTerm T) :=
  Q.filterMap (iriElem K vp ord idOf Q) ++ Q.filterMap (tripleElem K idOf Q) ++ Q.filterMap (pairElem K idOf Q)

end ApgModel.Views
