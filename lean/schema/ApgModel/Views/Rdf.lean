/-
 SPDX-License-Identifier: Apache-2.0
 Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
-/
import ApgModel.Catalog.Constraints

/-!
# The RDF view

`SCHEMA-APG.md` definition 17.1 and proposition 17.2: the RDF view `R(G)` of a finite APG on a catalog's schema `S(K)`,
and its monotonicity under homomorphisms that preserve names.

Each element contributes quads through **templates**: a quad whose subject and objects may be elements, named only when
the view is taken (`Tmpl.inst`), the naming `ι` partial (`SCHEMA-APG.md` section 15.1). An element's templates depend on
its identifier, its label and its value term alone (`templates`), and the view is every template named
(`rdf`). Objects are structured values, never strings (decision L5): a literal of a primitive type, an enumeration
member, a boolean, an `rdf:JSON` or `ugdb:measure` literal of a label-free value, an IRI, or a triple term; and so are
predicates and class IRIs (`Pred`, `Term.cls`), which the exporter writes as IRIs, distinct by `UGS0801`.

The templates follow definition 17.1's table and `SCHEMA.md` section 11:

- an element's types, its concrete type and every supertype and statement type above it, under `rdf:type`;
- an attribute's literals: one per member of a set, an `rdf:JSON` literal for a list or record, an interval's bounds
  under `{a}_start` and `{a}_end` where present, with their precisions where the view shows them, a measure with its
  unit on the value as one literal, a catalog type as its class IRI; an absent optional attribute has none;
- `ugdb:asserted_start`, `ugdb:asserted_end` and `ugdb:live` on a `@bitemporal` element;
- a node-encoded relation's role triples `{base}{Relation}_{role}`, the relation the type declaring the role;
- an edge-encoded relation's triple from its anchor under the anchor role's navigation name, in both directions when
  it is `@symmetric`, and, when it allocates a reifier, `rdf:reifies` the triple term;
- a role label's element as the triple from its instance to its participant.

Proposition 17.2 follows from two facts: an element's templates are natural in its references
(`templates_mapRefs`), and naming a template is monotone in the naming (`Tmpl.inst_mono`). An injective homomorphism
keeps every element with its label and its value, references renamed; proposition 17.2 needs no injectivity, and holds
for every homomorphism preserving names and graphs (`rdf_monotone`).

`Undeclared` and a role naming a catalog type are not modelled: no schema of the corpus writes them.
-/

namespace ApgModel.Views

open Named Catalog

variable {T : Typeside}

/-! ## Renaming the references of a value term -/

/-- A value term with every reference renamed by `f`. -/
def VTerm.mapRefs (f : Nat → Nat) : VTerm T → VTerm T
  | .prim p a => .prim p a
  | .elem e => .elem (f e)
  | .record fs => .record (mapFields fs)
  | .inj n v => .inj n (VTerm.mapRefs f v)
  | .list vs => .list (mapAll vs)
  | .set vs => .set (mapAll vs)
where
  /-- A record's fields renamed. -/
  mapFields : List (Name × VTerm T) → List (Name × VTerm T)
    | [] => []
    | (n, v) :: fs => (n, VTerm.mapRefs f v) :: mapFields fs
  /-- A collection's members renamed. -/
  mapAll : List (VTerm T) → List (VTerm T)
    | [] => []
    | v :: vs => VTerm.mapRefs f v :: mapAll vs

theorem mapFields_eq (f : Nat → Nat) : (fs : List (Name × VTerm T)) →
    VTerm.mapRefs.mapFields f fs = fs.map fun x => (x.1, VTerm.mapRefs f x.2)
  | [] => rfl
  | (_, _) :: fs => by simp only [VTerm.mapRefs.mapFields, List.map_cons, mapFields_eq f fs]

theorem mapAll_eq (f : Nat → Nat) : (vs : List (VTerm T)) → VTerm.mapRefs.mapAll f vs = vs.map (VTerm.mapRefs f)
  | [] => rfl
  | _ :: vs => by simp only [VTerm.mapRefs.mapAll, List.map_cons, mapAll_eq f vs]

/-- The references of a renamed term are the renamed references. -/
theorem refs_mapRefs (f : Nat → Nat) : (v : VTerm T) → (VTerm.mapRefs f v).refs = v.refs.map f
  | .prim _ _ => rfl
  | .elem _ => rfl
  | .record fs => by simp only [VTerm.mapRefs, VTerm.refs]; exact fields fs
  | .inj _ v => by simp only [VTerm.mapRefs, VTerm.refs]; exact refs_mapRefs f v
  | .list vs => by simp only [VTerm.mapRefs, VTerm.refs]; exact all vs
  | .set vs => by simp only [VTerm.mapRefs, VTerm.refs]; exact all vs
where
  fields : (fs : List (Name × VTerm T)) →
      VTerm.refs.refsFields (VTerm.mapRefs.mapFields f fs) = (VTerm.refs.refsFields fs).map f
    | [] => rfl
    | (_, v) :: fs => by
      simp only [VTerm.mapRefs.mapFields, VTerm.refs.refsFields, List.map_append]
      rw [refs_mapRefs f v, fields fs]
  all : (vs : List (VTerm T)) → VTerm.refs.refsAll (VTerm.mapRefs.mapAll f vs) = (VTerm.refs.refsAll vs).map f
    | [] => rfl
    | v :: vs => by
      simp only [VTerm.mapRefs.mapAll, VTerm.refs.refsAll, List.map_append]
      rw [refs_mapRefs f v, all vs]

/-- A term without references is its own renaming. -/
theorem mapRefs_of_refs_nil (f : Nat → Nat) : (v : VTerm T) → v.refs = [] → VTerm.mapRefs f v = v
  | .prim _ _, _ => rfl
  | .elem _, h => by simp [VTerm.refs] at h
  | .record fs, h => by
    simp only [VTerm.mapRefs]
    rw [fields fs (by simpa [VTerm.refs] using h)]
  | .inj _ v, h => by
    simp only [VTerm.mapRefs]
    rw [mapRefs_of_refs_nil f v (by simpa [VTerm.refs] using h)]
  | .list vs, h => by
    simp only [VTerm.mapRefs]
    rw [all vs (by simpa [VTerm.refs] using h)]
  | .set vs, h => by
    simp only [VTerm.mapRefs]
    rw [all vs (by simpa [VTerm.refs] using h)]
where
  fields : (fs : List (Name × VTerm T)) → VTerm.refs.refsFields fs = [] → VTerm.mapRefs.mapFields f fs = fs
    | [], _ => rfl
    | (_, v) :: fs, h => by
      simp only [VTerm.refs.refsFields, List.append_eq_nil] at h
      simp only [VTerm.mapRefs.mapFields]
      rw [mapRefs_of_refs_nil f v h.1, fields fs h.2]
  all : (vs : List (VTerm T)) → VTerm.refs.refsAll vs = [] → VTerm.mapRefs.mapAll f vs = vs
    | [], _ => rfl
    | v :: vs, h => by
      simp only [VTerm.refs.refsAll, List.append_eq_nil] at h
      simp only [VTerm.mapRefs.mapAll]
      rw [mapRefs_of_refs_nil f v h.1, all vs h.2]

@[simp] theorem field?_mapRefs (f : Nat → Nat) (n : Name) (v : VTerm T) :
    (VTerm.mapRefs f v).field? n = (v.field? n).map (VTerm.mapRefs f) := by
  cases v with
  | record fs =>
    simp only [VTerm.mapRefs, VTerm.field?, mapFields_eq, List.find?_map, Option.map_map]
    rfl
  | _ => rfl

@[simp] theorem fieldRefs_mapRefs (f : Nat → Nat) (v : VTerm T) (n : Name) :
    fieldRefs (VTerm.mapRefs f v) n = (fieldRefs v n).map f := by
  unfold fieldRefs
  rw [field?_mapRefs]
  cases v.field? n with
  | none => rfl
  | some w => exact refs_mapRefs f w

@[simp] theorem presentValue_mapRefs (f : Nat → Nat) (opt : Bool) (v : VTerm T) :
    presentValue opt (VTerm.mapRefs f v) = (presentValue opt v).map (VTerm.mapRefs f) := by
  unfold presentValue
  cases opt with
  | false => rfl
  | true =>
    simp only [if_true]
    cases v with
    | inj n w =>
      simp only [VTerm.mapRefs]
      split <;> split <;> simp_all
    | _ => rfl

/-! ## Quads and templates -/

/-- The bounds of an interval in the view, `SCHEMA.md` section 7.4. -/
inductive Bound where
  | start
  | stop
  | startPrecision
  | stopPrecision
  deriving DecidableEq, Repr

/-- A predicate of the view, structured: `rdf:type`, `rdf:reifies`, an attribute `{base}{a}`, an interval bound
`{base}{a}_start` and its kin, the built-in `ugdb:asserted_start`, `ugdb:asserted_end` and `ugdb:live`, a navigation
name `{base}{nav}`, and a role `{base}{Relation}_{role}`. The exporter writes each as its IRI. -/
inductive Pred where
  | rdfType
  | reifies
  | attr (a : Name)
  | bound (a : Name) (b : Bound)
  | assertedStart
  | assertedEnd
  | live
  | nav (n : Name)
  | role (R r : Name)
  deriving DecidableEq, Repr

/-- An object of the RDF view, a structured value (decision L5). -/
inductive Term (T : Typeside) where
  /-- An element's IRI. -/
  | iri (i : String)
  /-- A type's class IRI `{base}{C}`. -/
  | cls (c : Name)
  /-- A literal of a primitive type. -/
  | lit (p : T.P) (a : T.PV p)
  /-- An enumeration member, stored as its name. -/
  | member (m : Name)
  /-- An `xsd:boolean`. -/
  | bool (b : Bool)
  /-- An `rdf:JSON` literal of a label-free value. -/
  | json (v : VTerm T)
  /-- A `ugdb:measure` literal, a measure with its unit on the value. -/
  | measure (v : VTerm T)
  /-- An RDF 1.2 triple term `<<( s {base}{n} o )>>`. -/
  | triple (s : String) (n : Name) (o : String)

/-- A quad: subject, predicate, object, graph. -/
structure Quad (T : Typeside) where
  /-- The subject, an element's IRI. -/
  s : String
  /-- The predicate. -/
  p : Pred
  /-- The object. -/
  o : Term T
  /-- The named graph. -/
  g : String

/-- A template's object: an element, named when the view is taken, or a value. -/
inductive TObj (T : Typeside) where
  | elem (e : Nat)
  | cls (c : Name)
  | lit (p : T.P) (a : T.PV p)
  | member (m : Name)
  | bool (b : Bool)
  | json (v : VTerm T)
  | measure (v : VTerm T)
  | triple (s : Nat) (n : Name) (o : Nat)

/-- A quad template: its subject an element, in the graph of the element `g`. -/
structure Tmpl (T : Typeside) where
  /-- The subject. -/
  s : Nat
  /-- The predicate. -/
  p : Pred
  /-- The object. -/
  o : TObj T
  /-- The element whose graph holds the quad. -/
  g : Nat

/-- An object's elements renamed. -/
def TObj.rename (f : Nat → Nat) : TObj T → TObj T
  | .elem e => .elem (f e)
  | .cls c => .cls c
  | .lit p a => .lit p a
  | .member m => .member m
  | .bool b => .bool b
  | .json v => .json (VTerm.mapRefs f v)
  | .measure v => .measure (VTerm.mapRefs f v)
  | .triple s n o => .triple (f s) n (f o)

/-- A template's elements renamed. -/
def Tmpl.rename (f : Nat → Nat) (t : Tmpl T) : Tmpl T := ⟨f t.s, t.p, t.o.rename f, f t.g⟩

/-- An object named; a literal of a value with references is none, which a valid APG never holds. -/
def TObj.inst (name : Nat → Option String) : TObj T → Option (Term T)
  | .elem e => (name e).map Term.iri
  | .cls c => some (.cls c)
  | .lit p a => some (.lit p a)
  | .member m => some (.member m)
  | .bool b => some (.bool b)
  | .json v => if v.refs.isEmpty then some (.json v) else none
  | .measure v => if v.refs.isEmpty then some (.measure v) else none
  | .triple s n o => (name s).bind fun a => (name o).map fun b => .triple a n b

/-- A template named, its graph the element's. -/
def Tmpl.inst (name : Nat → Option String) (graph : Nat → String) (t : Tmpl T) : Option (Quad T) :=
  (name t.s).bind fun s => (t.o.inst name).map fun o => ⟨s, t.p, o, graph t.g⟩

theorem TObj.inst_rename (f : Nat → Nat) (name : Nat → Option String) (o : TObj T) :
    (o.rename f).inst name = o.inst (fun e => name (f e)) := by
  cases o with
  | json v | measure v =>
    simp only [TObj.rename, TObj.inst, refs_mapRefs]
    by_cases h : v.refs = []
    · simp [h, mapRefs_of_refs_nil f v h]
    · have h1 : (v.refs.map f).isEmpty = false := by
        cases hr : v.refs with
        | nil => exact absurd hr h
        | cons _ _ => rfl
      have h2 : v.refs.isEmpty = false := by
        cases hr : v.refs with
        | nil => exact absurd hr h
        | cons _ _ => rfl
      simp [h1, h2]
  | _ => rfl

theorem Tmpl.inst_rename (f : Nat → Nat) (name : Nat → Option String) (graph : Nat → String) (t : Tmpl T) :
    (t.rename f).inst name graph = t.inst (fun e => name (f e)) (fun e => graph (f e)) := by
  simp only [Tmpl.inst, Tmpl.rename, TObj.inst_rename]

/-- Naming is monotone: a naming defined wherever another is, and agreeing there, names every template alike. -/
theorem Tmpl.inst_mono {name name' : Nat → Option String} (graph : Nat → String)
    (h : ∀ e i, name e = some i → name' e = some i) {t : Tmpl T} {q : Quad T} (hq : t.inst name graph = some q) :
    t.inst name' graph = some q := by
  have ho : ∀ o : TObj T, ∀ x, o.inst name = some x → o.inst name' = some x := by
    intro o x hx
    cases o with
    | elem e =>
      simp only [TObj.inst, Option.map_eq_some'] at hx ⊢
      obtain ⟨i, hi, rfl⟩ := hx
      exact ⟨i, h e i hi, rfl⟩
    | triple s n o =>
      simp only [TObj.inst, Option.bind_eq_some, Option.map_eq_some'] at hx ⊢
      obtain ⟨a, ha, b, hb, rfl⟩ := hx
      exact ⟨a, h s a ha, b, h o b hb, rfl⟩
    | _ => exact hx
  simp only [Tmpl.inst, Option.bind_eq_some, Option.map_eq_some'] at hq ⊢
  obtain ⟨s, hs, o, ho', rfl⟩ := hq
  exact ⟨s, h _ s hs, o, ho _ o ho', rfl⟩

/-! ## Definition 17.1 -/

/-- What a view takes beside the catalog: which interval precisions it shows, given the bound's primitive type and
the precision, where coarser than the bound's type (`SCHEMA.md` section 5.1). -/
structure ViewParams (T : Typeside) where
  /-- Whether a bound of the primitive type `p` shows its precision `x` of the primitive type `q`. -/
  precisionShown : (p q : T.P) → T.PV q → Bool
  /-- The precision of the primitive type `p`'s own unit, which the view leaves out and its inverse restores. -/
  native : T.P → Option ((q : T.P) × T.PV q)

/-- The list `k a` of an optional `a`, empty for none. -/
def optList {α β : Type} (o : Option α) (k : α → List β) : List β :=
  match o with
  | none => []
  | some a => k a

@[simp] theorem optList_map {α β γ : Type} (o : Option α) (g : α → β) (k : β → List γ) :
    optList (o.map g) k = optList o fun a => k (g a) := by
  cases o <;> rfl

@[simp] theorem map_optList {α β γ : Type} (o : Option α) (k : α → List β) (h : β → γ) :
    (optList o k).map h = optList o fun a => (k a).map h := by
  cases o <;> rfl

@[simp] theorem map_ite_nil {α β : Type} (c : Prop) [Decidable c] (l : List α) (g : α → β) :
    (if c then l else []).map g = if c then l.map g else [] := by
  split <;> rfl

/-- A collection member's object: a literal, an enumeration member, or an `rdf:JSON` literal. -/
def memberObj : VTerm T → TObj T
  | .prim p a => .lit p a
  | .inj m (.record []) => .member m
  | v => .json v

/-- An attribute value's objects: one per member of a set. -/
def attrObjs : VTerm T → List (TObj T)
  | .set vs => vs.map memberObj
  | v => [memberObj v]

theorem memberObj_mapRefs (f : Nat → Nat) (v : VTerm T) :
    memberObj (VTerm.mapRefs f v) = (memberObj v).rename f := by
  cases v with
  | inj m w =>
    cases w with
    | record fs => cases fs <;> rfl
    | _ => rfl
  | _ => rfl

@[simp] theorem attrObjs_mapRefs (f : Nat → Nat) (v : VTerm T) :
    attrObjs (VTerm.mapRefs f v) = (attrObjs v).map (TObj.rename f) := by
  cases v with
  | set vs =>
    simp only [VTerm.mapRefs, attrObjs, mapAll_eq, List.map_map]
    exact List.map_congr_left fun w _ => memberObj_mapRefs f w
  | _ => simp only [attrObjs, List.map_cons, List.map_nil, ← memberObj_mapRefs]; rfl

/-- A primitive value term's type and value. -/
def asPrim : VTerm T → Option ((p : T.P) × T.PV p)
  | .prim p a => some ⟨p, a⟩
  | _ => none

@[simp] theorem asPrim_mapRefs (f : Nat → Nat) (v : VTerm T) : asPrim (VTerm.mapRefs f v) = asPrim v := by
  cases v <;> rfl

/-- An injection's tag: a catalog type as a value. -/
def asTag : VTerm T → Option Name
  | .inj n _ => some n
  | _ => none

@[simp] theorem asTag_mapRefs (f : Nat → Nat) (v : VTerm T) : asTag (VTerm.mapRefs f v) = asTag v := by
  cases v <;> rfl

variable (K : Catalog T) (vp : ViewParams T)

/-- The templates of an attribute `a` of the element `e` with the present value `u`, by its view. -/
def attrTemplates (e : Nat) (a : Name) (view : AttrView) (u : VTerm T) : List (Tmpl T) :=
  match view with
  | .plain => (attrObjs u).map fun o => ⟨e, .attr a, o, e⟩
  | .measure => [⟨e, .attr a, .measure u, e⟩]
  | .typeValue => optList (asTag u) fun c => [⟨e, .attr a, .cls c, e⟩]
  | .interval =>
    let bound (b : Bound) (w : Option (VTerm T)) : List (Tmpl T) :=
      optList w fun w => (attrObjs w).map fun o => ⟨e, .bound a b, o, e⟩
    let prec (b : Bound) (w pr : Option (VTerm T)) : List (Tmpl T) :=
      optList (w.bind asPrim) fun x => optList (pr.bind asPrim) fun y =>
        if vp.precisionShown x.1 y.1 y.2 then [⟨e, .bound a b, .lit y.1 y.2, e⟩] else []
    let start := (u.field? "start").bind (presentValue true)
    let stop := (u.field? "end").bind (presentValue true)
    bound .start start ++ bound .stop stop ++
      prec .startPrecision start ((u.field? "start_precision").bind (presentValue true)) ++
      prec .stopPrecision stop ((u.field? "end_precision").bind (presentValue true))

/-- The templates of `asserted_start`, `asserted_end` and `live` from the bounds of `asserted`. -/
def assertedFrom (e : Nat) (start stop : Option (VTerm T)) : List (Tmpl T) :=
  optList start (fun w => (attrObjs w).map fun o => ⟨e, .assertedStart, o, e⟩) ++
    optList stop (fun w => (attrObjs w).map fun o => ⟨e, .assertedEnd, o, e⟩) ++
    [⟨e, .live, .bool stop.isNone, e⟩]

/-- The templates of `asserted_start`, `asserted_end` and `live` of a `@bitemporal` element. -/
def assertedTemplates (e : Nat) (v : VTerm T) : List (Tmpl T) :=
  optList (v.field? "asserted") fun u =>
    assertedFrom e (u.field? "start") ((u.field? "end").bind (presentValue true))

/-- The templates of an edge-encoded relation's element: its triple from the anchor, both ways when `@symmetric`, and
the reifier's `rdf:reifies` when it allocates one. -/
def edgeTemplates (e : Nat) (l : Name) (v : VTerm T) : List (Tmpl T) :=
  optList (K.anchorRole l) fun a => optList (K.otherRole l) fun o => optList (K.navOf l a) fun nav =>
    (fieldRefs v a).flatMap fun pa => (fieldRefs v o).flatMap fun po =>
      [⟨pa, .nav nav, .elem po, e⟩] ++
        (if K.inherits CType.symmetric l then [⟨po, .nav nav, .elem pa, e⟩] else []) ++
        (if K.allocatesReifier l then [⟨e, .reifies, .triple pa nav po, e⟩] else [])

/-- The templates of a node-encoded relation's element: a role triple per participant. -/
def nodeTemplates (e : Nat) (l : Name) (v : VTerm T) : List (Tmpl T) :=
  (K.ownedFields l).flatMap fun x =>
    match x with
    | (owner, .role r _ _ _ _) =>
      (fieldRefs v r).map fun p => ⟨e, .role owner r, .elem p, e⟩
    | _ => []

/-- The type templates of an element: its concrete type, every supertype and statement type above it. -/
def typeTemplates (e : Nat) (l : Name) : List (Tmpl T) :=
  (K.chain l).map fun C => ⟨e, .rdfType, .cls C, e⟩

/-- The templates of an element's attributes. -/
def attrsTemplates (e : Nat) (l : Name) (v : VTerm T) : List (Tmpl T) :=
  (K.allFields l).flatMap fun f =>
    match f with
    | .attr a _ opt view => optList ((v.field? a).bind (presentValue opt)) (attrTemplates vp e a view)
    | _ => []

/-- The templates of a relation's element by its encoding. -/
def relTemplates (c : CType T) (e : Nat) (l : Name) (v : VTerm T) : List (Tmpl T) :=
  if c.kind == .relation then
    if K.edgeEncoded l then edgeTemplates K e l v else nodeTemplates K e l v
  else []

/-- The templates of a role label's element: the triple from its instance to its participant. -/
def roleLabelTemplates (e : Nat) (l : Name) (v : VTerm T) : List (Tmpl T) :=
  optList (K.roleLabelOf l) fun x =>
    (fieldRefs v "instance").flatMap fun i => (fieldRefs v "participant").map fun p =>
      ⟨i, .role x.2.1 x.2.2.1, .elem p, e⟩

/-- The templates of the element `e` of label `l` with value `v`, definition 17.1. -/
def templates (e : Nat) (l : Name) (v : VTerm T) : List (Tmpl T) :=
  match K.find l with
  | some c =>
    typeTemplates K e l ++ attrsTemplates K vp e l v ++
      (if K.inherits CType.bitemporal l then assertedTemplates e v else []) ++ relTemplates K c e l v
  | none => roleLabelTemplates K e l v

/-- `R(G)`, definition 17.1: every element's templates, named by `ι`, each quad in its element's graph. -/
def rdf (ι : Nat → Option String) (graph : Nat → String) (G : FinApg K.schema) : List (Quad T) :=
  G.elems.flatMap fun x => (templates K vp x.1 x.2.1 x.2.2).filterMap (Tmpl.inst ι graph)

/-! ## Proposition 17.2 -/

@[simp] theorem optList_bind {α β γ : Type} (o : Option α) (g : α → Option β) (k : β → List γ) :
    optList (o.bind g) k = optList o fun a => optList (g a) k := by
  cases o <;> rfl

theorem attrTemplates_mapRefs (f : Nat → Nat) (e : Nat) (a : Name) (view : AttrView) (u : VTerm T) :
    attrTemplates vp (f e) a view (VTerm.mapRefs f u) = (attrTemplates vp e a view u).map (Tmpl.rename f) := by
  cases view <;> simp [attrTemplates, Tmpl.rename, TObj.rename, List.map_map, Function.comp_def]

theorem assertedFrom_map (f : Nat → Nat) (e : Nat) (start stop : Option (VTerm T)) :
    assertedFrom (f e) (start.map (VTerm.mapRefs f)) (stop.map (VTerm.mapRefs f)) =
      (assertedFrom e start stop).map (Tmpl.rename f) := by
  cases start <;> cases stop <;>
    simp [assertedFrom, optList, Tmpl.rename, TObj.rename, List.map_map, Function.comp_def]

theorem bind_present_map (f : Nat → Nat) (o : Option (VTerm T)) :
    (o.map (VTerm.mapRefs f)).bind (presentValue true) = (o.bind (presentValue true)).map (VTerm.mapRefs f) := by
  cases o <;> simp [presentValue_mapRefs]

theorem assertedTemplates_mapRefs (f : Nat → Nat) (e : Nat) (v : VTerm T) :
    assertedTemplates (f e) (VTerm.mapRefs f v) = (assertedTemplates e v).map (Tmpl.rename f) := by
  unfold assertedTemplates
  rw [field?_mapRefs, optList_map, map_optList]
  congr 1
  funext u
  rw [field?_mapRefs, field?_mapRefs, bind_present_map]
  exact assertedFrom_map f e _ _

theorem edgeTemplates_mapRefs (f : Nat → Nat) (e : Nat) (l : Name) (v : VTerm T) :
    edgeTemplates K (f e) l (VTerm.mapRefs f v) = (edgeTemplates K e l v).map (Tmpl.rename f) := by
  simp [edgeTemplates, Tmpl.rename, TObj.rename, List.map_flatMap, List.flatMap_map,
    Function.comp_def]

theorem nodeTemplates_mapRefs (f : Nat → Nat) (e : Nat) (l : Name) (v : VTerm T) :
    nodeTemplates K (f e) l (VTerm.mapRefs f v) = (nodeTemplates K e l v).map (Tmpl.rename f) := by
  simp only [nodeTemplates, List.map_flatMap]
  congr 1
  funext x
  match x with
  | (_, .role r _ _ _ _) => simp [Tmpl.rename, TObj.rename, Function.comp_def]
  | (_, .attr _ _ _ _) => rfl
  | (_, .setRole _ _ _) => rfl

theorem typeTemplates_rename (f : Nat → Nat) (e : Nat) (l : Name) :
    typeTemplates K (f e) l = (typeTemplates K e l).map (Tmpl.rename f) := by
  simp [typeTemplates, Tmpl.rename, TObj.rename, Function.comp_def]

theorem attrsTemplates_mapRefs (f : Nat → Nat) (e : Nat) (l : Name) (v : VTerm T) :
    attrsTemplates K vp (f e) l (VTerm.mapRefs f v) = (attrsTemplates K vp e l v).map (Tmpl.rename f) := by
  simp only [attrsTemplates, List.map_flatMap]
  congr 1
  funext g
  match g with
  | .attr a _ opt view =>
    simp only [field?_mapRefs, optList_bind, optList_map, presentValue_mapRefs, map_optList,
      attrTemplates_mapRefs]
  | .role _ _ _ _ _ => rfl
  | .setRole _ _ _ => rfl

theorem relTemplates_mapRefs (c : CType T) (f : Nat → Nat) (e : Nat) (l : Name) (v : VTerm T) :
    relTemplates K c (f e) l (VTerm.mapRefs f v) = (relTemplates K c e l v).map (Tmpl.rename f) := by
  unfold relTemplates
  split
  · split
    · exact edgeTemplates_mapRefs K f e l v
    · exact nodeTemplates_mapRefs K f e l v
  · rfl

theorem roleLabelTemplates_mapRefs (f : Nat → Nat) (e : Nat) (l : Name) (v : VTerm T) :
    roleLabelTemplates K (f e) l (VTerm.mapRefs f v) = (roleLabelTemplates K e l v).map (Tmpl.rename f) := by
  simp [roleLabelTemplates, List.map_flatMap, List.flatMap_map, Tmpl.rename, TObj.rename,
    Function.comp_def]

/-- An element's templates are natural in its references: renaming the element and every reference renames its
templates. -/
theorem templates_mapRefs (f : Nat → Nat) (e : Nat) (l : Name) (v : VTerm T) :
    templates K vp (f e) l (VTerm.mapRefs f v) = (templates K vp e l v).map (Tmpl.rename f) := by
  unfold templates
  split
  · simp only [List.map_append, typeTemplates_rename, attrsTemplates_mapRefs, relTemplates_mapRefs, map_ite_nil,
      assertedTemplates_mapRefs]
  · exact roleLabelTemplates_mapRefs K f e l v

/-- A homomorphism of finite APGs: each element sent to an element with its label and its value, every reference
renamed (definition 7.1 in single-set form). -/
structure FinHom {S : NSchema T.P Name} (G G' : FinApg S) where
  /-- The function on identifiers. -/
  f : Nat → Nat
  /-- Each element's image, with its label and its renamed value. -/
  elems : ∀ x ∈ G.elems, (f x.1, x.2.1, VTerm.mapRefs f x.2.2) ∈ G'.elems

/-- Proposition 17.2: a homomorphism preserving names and graphs includes `R(G)` in `R(G')`. -/
theorem rdf_monotone {G G' : FinApg K.schema} (h : FinHom G G') {ι ι' : Nat → Option String}
    {graph graph' : Nat → String} (hι : ∀ e i, ι e = some i → ι' (h.f e) = some i)
    (hg : ∀ e, graph' (h.f e) = graph e) {q : Quad T} (hq : q ∈ rdf K vp ι graph G) :
    q ∈ rdf K vp ι' graph' G' := by
  simp only [rdf, List.mem_flatMap, List.mem_filterMap] at hq ⊢
  obtain ⟨x, hx, t, ht, htq⟩ := hq
  refine ⟨(h.f x.1, x.2.1, VTerm.mapRefs h.f x.2.2), h.elems x hx, t.rename h.f, ?_, ?_⟩
  · rw [templates_mapRefs]
    exact List.mem_map_of_mem _ ht
  · rw [Tmpl.inst_rename]
    have : (fun e => graph' (h.f e)) = graph := funext hg
    rw [this]
    exact Tmpl.inst_mono graph hι htq

end ApgModel.Views
