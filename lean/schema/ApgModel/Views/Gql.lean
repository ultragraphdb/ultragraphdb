/-
 SPDX-License-Identifier: Apache-2.0
 Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
-/
import ApgModel.Views.RdfRoundTrip
import MGQL.Schema

/-!
# The GQL view

`SCHEMA-APG.md` definition 18.1 and propositions 18.2 to 18.4: the GQL view `P(G)` of a finite APG on a catalog's
schema `S(K)`, its closed graph type `Γ(S(K))`, and the view's projection onto MGQL's property graphs.

As the RDF view does, each element contributes through templates whose ends may be the element itself or elements it
names, named only when the view is taken: a node template (`gNodeTmpl`) and edge templates (`gEdgeTmpls`), each
depending on the element's label and value alone. `SCHEMA.md` section 11.3 gives them:

- a **node** for an element of an entity type, of a node-encoded relation, and of an edge-encoded relation that
  allocates a reifier (`gPresented`), labelled with its type's chain, the type, every supertype and statement type
  above it, and carrying its properties (`gProps`): each present attribute, `type`, and `asserted` and `live` on a
  `@bitemporal` element;
- a **stored edge** for an element of an edge-encoded relation, from its anchor to its other participant, undirected
  when `@symmetric`, labelled and propertied as its node; a **role edge** from a node-encoded relation's node to each
  participant and from a role label's instance to its participant, labelled `{Relation}_{role}`, the relation the type
  declaring the role, as the RDF view's role predicate is, and for a specialized role the super role's too
  (`gRoleLabels`, decided 2026-11-08);
- **derived edges**: the two role edges of an edge-encoded relation's node, labelled as a node-encoded relation's are,
  and a binary node-encoded relation's navigation edges, from the participant gaining the name to the one it reaches.

The graph type (`gType`) is closed: a node type per presented concrete type, an edge type per edge-encoded relation,
role edge and navigation edge and per pair of concrete types its roles admit. A property's type is its field's named
type, checked by the APG's own check, `type` a string and `live` a Boolean; GQL writes an enumeration and a catalog
type as a `STRING`, which is their lexical presentation.

**Proposition 18.2** (`gql_conforms`): every node and edge of `P(G)` conforms to `Γ(S(K))` under PG-Schema's strict,
closed semantics, for `G` valid and written as the store writes it. **MGQL** (decided 2026-11-08): MGQL's property
graphs carry `int`, `string` and `bool` properties, every one required; the view's projection onto them, its labels,
endpoints and required properties of those sorts (`mgqlGraph`), conforms to the projected graph type under
`graphConformsSchema` (`mgql_conforms`). **Proposition 18.3** (`gql_monotone`): a homomorphism preserving names maps
`P(G)` into `P(G')`, each property's value renamed, which leaves an attribute's value as it is. **Proposition 18.4**
(`gql_of_rdf`): `P(G)` and the view of `R⁻¹(R(G))` hold the same nodes and edges.
-/

namespace ApgModel.Views

open Named Catalog

variable {T : Typeside}

/-! ## Property graphs -/

/-- A property's value: a field's value, a string or a Boolean. -/
inductive PVal (T : Typeside) where
  | val (v : VTerm T)
  | str (s : String)
  | bool (b : Bool)

/-- A property's type: a field's named type, `STRING` or `BOOLEAN`. -/
inductive PType (T : Typeside) where
  | ty (t : NTy T.P Name)
  | str
  | bool

/-- A node: its element's IRI, its labels and its properties. -/
structure GNode (T : Typeside) where
  /-- The element id. -/
  id : String
  /-- The labels. -/
  labels : List Name
  /-- The properties. -/
  props : List (Name × PVal T)

/-- An edge: its element id where it has one, its endpoints' ids, whether it is directed and derived, its labels and
its properties. -/
structure GEdge (T : Typeside) where
  /-- The element id. -/
  id : Option String
  /-- The source's id. -/
  src : String
  /-- The destination's id. -/
  dst : String
  /-- Directed, else undirected. -/
  directed : Bool
  /-- Derived and read-only. -/
  derived : Bool
  /-- The labels. -/
  labels : List Name
  /-- The properties. -/
  props : List (Name × PVal T)

/-- A property graph: its nodes and its edges. -/
structure PGraph (T : Typeside) where
  /-- The nodes. -/
  nodes : List (GNode T)
  /-- The edges. -/
  edges : List (GEdge T)

/-- An end of an edge template: the element itself, or an element it names. -/
inductive GEnd where
  | self
  | elem (p : Nat)

/-- An edge template. -/
structure ETmpl (T : Typeside) where
  /-- The source. -/
  src : GEnd
  /-- The destination. -/
  dst : GEnd
  /-- Directed, else undirected. -/
  directed : Bool
  /-- Derived and read-only. -/
  derived : Bool
  /-- The edge answers its element's IRI. -/
  named : Bool
  /-- The labels. -/
  labels : List Name
  /-- The properties. -/
  props : List (Name × PVal T)

/-! ## Templates -/

variable (K : Catalog T)

/-- A label is presented as a node: an entity type, a node-encoded relation, or an edge-encoded relation that allocates
a reifier. -/
def gPresented (l : Name) : Bool :=
  match K.find l with
  | some c => c.kind != .statement && !tripleNamed K l
  | none => false

/-- The properties of an element of `l` with value `v`: each present attribute, `type`, and `asserted` and `live` on a
`@bitemporal` element. -/
def gProps (l : Name) (v : VTerm T) : List (Name × PVal T) :=
  ((K.slots l).filterMap fun s =>
    match s with
    | (a, .attr opt _ _) => ((v.field? a).bind (presentValue opt)).map fun u => (a, .val u)
    | _ => none) ++
  [("type", .str l)] ++
  (if K.inherits CType.bitemporal l then
    optList (v.field? "asserted") fun u =>
      [("asserted", .val u), ("live", .bool ((u.field? "end").bind (presentValue true)).isNone)]
  else [])

/-- The property types of `l`: each attribute's type, optional where the attribute is, `type`, and `asserted` and
`live` on a `@bitemporal` type. -/
def gPropTypes (l : Name) : List (Name × PType T × Bool) :=
  ((K.slots l).filterMap fun s =>
    match s with
    | (a, .attr opt t _) => some (a, .ty t, opt)
    | _ => none) ++
  [("type", .str, false)] ++
  (if K.inherits CType.bitemporal l then [("asserted", .ty K.assertedType, false), ("live", .bool, false)] else [])

/-- An element's node template: its labels and properties, when its label is presented as a node. -/
def gNodeTmpl (l : Name) (v : VTerm T) : List (List Name × List (Name × PVal T)) :=
  if gPresented K l then [(K.chain l, gProps K l v)] else []

/-- The type declaring the role `r` of `l`: the nearest in `l`'s chain whose own fields hold it. -/
def gDeclarer (l r : Name) : Name :=
  ((K.chain l).find? fun m => (((K.find m).map CType.fields).getD []).any (·.name == r)).getD l

/-- A role edge's labels: `{Relation}_{role}`, the relation the type declaring the role, and for a specialized role the
super role's. -/
def gRoleLabels (l r : Name) : List Name :=
  Catalog.roleLabelName (gDeclarer K l r) r ::
    ((K.allFields l).findSome? fun f => if f.name == r then f.specializes else none).toList.map fun s =>
      Catalog.roleLabelName (gDeclarer K l s) s

/-- A relation's two roles with their participants when it is binary: both single-valued, none set-valued. -/
def gBinary (l : Name) : Option ((Name × List Name) × (Name × List Name)) :=
  match (K.allFields l).filter Field.isRole with
  | [.role r₁ ps₁ _ _ _, .role r₂ ps₂ _ _ _] => some ((r₁, ps₁), (r₂, ps₂))
  | _ => none

/-- A role edge from the element to each participant in `r`. -/
def gRoleEdges (l r : Name) (v : VTerm T) (derived : Bool) : List (ETmpl T) :=
  (fieldRefs v r).map fun p => ⟨.self, .elem p, true, derived, false, gRoleLabels K l r, []⟩

/-- A navigation edge from the participant in `r` to the participant in `r'`, labelled with `r`'s navigation name. -/
def gNavEdges (l r r' : Name) (v : VTerm T) : List (ETmpl T) :=
  optList (K.navOf l r) fun n =>
    (fieldRefs v r).flatMap fun p => (fieldRefs v r').map fun p' => ⟨.elem p, .elem p', true, true, false, [n], []⟩

/-- An edge-encoded relation's element: its edge from the anchor and, with a reifier, its node's two role edges. -/
def gEdgeEncoded (l : Name) (v : VTerm T) : List (ETmpl T) :=
  optList (K.anchorRole l) fun a => optList (K.otherRole l) fun o =>
    ((fieldRefs v a).flatMap fun pa => (fieldRefs v o).map fun po =>
      ⟨.elem pa, .elem po, !K.inherits CType.symmetric l, false, K.allocatesReifier l, K.chain l, gProps K l v⟩) ++
    (if K.allocatesReifier l then gRoleEdges K l a v true ++ gRoleEdges K l o v true else [])

/-- A node-encoded relation's element: a role edge per single-valued role, and its navigation edges when binary. -/
def gNodeEncoded (l : Name) (v : VTerm T) : List (ETmpl T) :=
  ((K.ownedFields l).flatMap fun x =>
    match x with
    | (_, .role r _ _ _ _) => gRoleEdges K l r v false
    | _ => []) ++
  optList (gBinary K l) fun rs => gNavEdges K l rs.1.1 rs.2.1 v ++ gNavEdges K l rs.2.1 rs.1.1 v

/-- A role label's element: the role edge from its instance to its participant. -/
def gRoleLabelEdges (l : Name) (v : VTerm T) : List (ETmpl T) :=
  optList (K.roleLabelOf l) fun x =>
    (fieldRefs v "instance").flatMap fun i => (fieldRefs v "participant").map fun p =>
      ⟨.elem i, .elem p, true, false, false, gRoleLabels K x.1 x.2.2.1, []⟩

/-- An element's edge templates. -/
def gEdgeTmpls (l : Name) (v : VTerm T) : List (ETmpl T) :=
  match K.find l with
  | some c =>
    if c.kind == .relation then (if K.edgeEncoded l then gEdgeEncoded K l v else gNodeEncoded K l v) else []
  | none => gRoleLabelEdges K l v

/-! ## The view -/

/-- An end named: the element's own IRI, or the IRI of an element it names. -/
def GEnd.inst (own : Option String) (ι : Nat → Option String) : GEnd → Option String
  | .self => own
  | .elem p => ι p

/-- An edge template named; one answering its element's IRI needs the element named. -/
def ETmpl.inst (own : Option String) (ι : Nat → Option String) (t : ETmpl T) : Option (GEdge T) :=
  (t.src.inst own ι).bind fun s => (t.dst.inst own ι).bind fun d =>
    (if t.named then own.map some else some none).map fun i => ⟨i, s, d, t.directed, t.derived, t.labels, t.props⟩

/-- The view of elements, each with its own IRI where it has one, its label and its value, references named by `ι`. -/
def gqlOf (ι : Nat → Option String) (xs : List (Option String × Name × VTerm T)) : PGraph T :=
  ⟨xs.flatMap fun x => optList x.1 fun s => (gNodeTmpl K x.2.1 x.2.2).map fun n => ⟨s, n.1, n.2⟩,
    xs.flatMap fun x => (gEdgeTmpls K x.2.1 x.2.2).filterMap (ETmpl.inst x.1 ι)⟩

/-- `P(G)`, definition 18.1. -/
def gql (ι : Nat → Option String) (G : FinApg K.schema) : PGraph T :=
  gqlOf K ι (G.elems.map fun x => (ι x.1, x.2.1, x.2.2))

/-! ## Proposition 18.3 -/

/-- A property's value with its references renamed. -/
def PVal.rename (f : Nat → Nat) : PVal T → PVal T
  | .val v => .val (VTerm.mapRefs f v)
  | p => p

/-- A node with its properties' references renamed. -/
def GNode.rename (f : Nat → Nat) (n : GNode T) : GNode T := { n with props := n.props.map fun p => (p.1, p.2.rename f) }

/-- An edge with its properties' references renamed. -/
def GEdge.rename (f : Nat → Nat) (e : GEdge T) : GEdge T := { e with props := e.props.map fun p => (p.1, p.2.rename f) }

/-- A template's ends and properties renamed. -/
def ETmpl.rename (f : Nat → Nat) (t : ETmpl T) : ETmpl T :=
  { t with
    src := match t.src with | .self => .self | .elem p => .elem (f p)
    dst := match t.dst with | .self => .self | .elem p => .elem (f p)
    props := t.props.map fun p => (p.1, p.2.rename f) }

theorem gProps_mapRefs (f : Nat → Nat) (l : Name) (v : VTerm T) :
    gProps K l (VTerm.mapRefs f v) = (gProps K l v).map fun p => (p.1, p.2.rename f) := by
  unfold gProps
  simp only [List.map_append]
  congr 1
  · congr 1
    · rw [List.map_filterMap]
      congr 1
      funext s
      match s with
      | (a, .attr opt _ _) =>
        simp only [field?_mapRefs]
        cases (v.field? a) with
        | none => rfl
        | some w =>
          simp only [Option.map_some', Option.some_bind, presentValue_mapRefs]
          cases presentValue opt w <;> rfl
      | (_, .role _ _ _) => rfl
      | (_, .asserted) => rfl
  · split
    · rw [field?_mapRefs]
      cases v.field? "asserted" with
      | none => rfl
      | some u =>
        simp only [Option.map_some', optList, List.map_cons, List.map_nil, PVal.rename, field?_mapRefs,
          bind_present_map]
        cases (VTerm.field? "end" u).bind (presentValue true) <;> rfl
    · rfl

theorem gNodeTmpl_mapRefs (f : Nat → Nat) (l : Name) (v : VTerm T) :
    gNodeTmpl K l (VTerm.mapRefs f v) = (gNodeTmpl K l v).map fun n => (n.1, n.2.map fun p => (p.1, p.2.rename f)) := by
  unfold gNodeTmpl
  split <;> simp [gProps_mapRefs]

theorem gRoleEdges_mapRefs (f : Nat → Nat) (l r : Name) (v : VTerm T) (d : Bool) :
    gRoleEdges K l r (VTerm.mapRefs f v) d = (gRoleEdges K l r v d).map (ETmpl.rename f) := by
  simp [gRoleEdges, ETmpl.rename, Function.comp_def]

theorem gNavEdges_mapRefs (f : Nat → Nat) (l r r' : Name) (v : VTerm T) :
    gNavEdges K l r r' (VTerm.mapRefs f v) = (gNavEdges K l r r' v).map (ETmpl.rename f) := by
  unfold gNavEdges
  cases K.navOf l r <;> simp [optList, ETmpl.rename, List.map_flatMap, List.flatMap_map, Function.comp_def]

theorem gEdgeTmpls_mapRefs (f : Nat → Nat) (l : Name) (v : VTerm T) :
    gEdgeTmpls K l (VTerm.mapRefs f v) = (gEdgeTmpls K l v).map (ETmpl.rename f) := by
  unfold gEdgeTmpls
  split
  · split
    · split
      · unfold gEdgeEncoded
        cases K.anchorRole l <;> cases K.otherRole l <;>
          simp [optList, ETmpl.rename, List.map_flatMap, List.flatMap_map, Function.comp_def, gProps_mapRefs,
            gRoleEdges_mapRefs, apply_ite (List.map (ETmpl.rename f))]
      · unfold gNodeEncoded
        simp only [List.map_append, List.map_flatMap]
        congr 1
        · congr 1
          funext x
          match x with
          | (_, .role r _ _ _ _) => exact gRoleEdges_mapRefs K f l r v false
          | (_, .attr _ _ _ _) => rfl
          | (_, .setRole _ _ _) => rfl
        · cases gBinary K l <;> simp [optList, gNavEdges_mapRefs]
    · rfl
  · unfold gRoleLabelEdges
    cases K.roleLabelOf l <;> simp [optList, ETmpl.rename, List.map_flatMap, List.flatMap_map, Function.comp_def]

/-- Naming a renamed template is naming the template under the renamed naming. -/
theorem ETmpl.inst_mono {own own' : Option String} {ι ι' : Nat → Option String} {f : Nat → Nat}
    (hown : ∀ s, own = some s → own' = some s) (hι : ∀ e s, ι e = some s → ι' (f e) = some s) {t : ETmpl T}
    {e : GEdge T} (h : t.inst own ι = some e) : (t.rename f).inst own' ι' = some (e.rename f) := by
  have hend : ∀ (g : GEnd) s, g.inst own ι = some s →
      (match g with | .self => GEnd.self | .elem p => .elem (f p)).inst own' ι' = some s := by
    intro g s hs
    cases g with
    | self => exact hown s hs
    | elem p => exact hι p s hs
  unfold ETmpl.inst at h ⊢
  obtain ⟨s, hs, h⟩ := Option.bind_eq_some.mp h
  obtain ⟨d, hd, h⟩ := Option.bind_eq_some.mp h
  obtain ⟨i, hi, rfl⟩ := Option.map_eq_some'.mp h
  simp only [ETmpl.rename, hend _ s hs, hend _ d hd, Option.some_bind]
  have hi' : (if t.named then own'.map some else some none) = some i := by
    split
    · next hn =>
      rw [if_pos hn] at hi
      obtain ⟨s', hs', rfl⟩ := Option.map_eq_some'.mp hi
      rw [hown s' hs']
      rfl
    · next hn => rw [if_neg hn] at hi; exact hi
  rw [hi']
  rfl

/-- Proposition 18.3: a homomorphism preserving names maps every node and edge of `P(G)` into `P(G')`, its
properties' references renamed. -/
theorem gql_monotone {G G' : FinApg K.schema} (h : FinHom G G') {ι ι' : Nat → Option String}
    (hι : ∀ e s, ι e = some s → ι' (h.f e) = some s) :
    (∀ n ∈ (gql K ι G).nodes, n.rename h.f ∈ (gql K ι' G').nodes) ∧
      ∀ e ∈ (gql K ι G).edges, e.rename h.f ∈ (gql K ι' G').edges := by
  constructor
  · intro n hn
    simp only [gql, gqlOf, List.flatMap_map, List.mem_flatMap] at hn ⊢
    obtain ⟨x, hx, hn⟩ := hn
    refine ⟨_, h.elems x hx, ?_⟩
    cases hs : ι x.1 with
    | none => rw [hs] at hn; cases hn
    | some s =>
      rw [hs] at hn
      simp only [optList, List.mem_map] at hn ⊢
      obtain ⟨nt, hnt, rfl⟩ := hn
      simp only [hι _ s hs, gNodeTmpl_mapRefs, List.mem_map, optList]
      exact ⟨_, ⟨nt, hnt, rfl⟩, rfl⟩
  · intro e he
    simp only [gql, gqlOf, List.flatMap_map, List.mem_flatMap, List.mem_filterMap] at he ⊢
    obtain ⟨x, hx, t, ht, hte⟩ := he
    refine ⟨_, h.elems x hx, t.rename h.f, ?_, ?_⟩
    · rw [gEdgeTmpls_mapRefs]
      exact List.mem_map_of_mem _ ht
    · exact ETmpl.inst_mono (fun s hs => hι _ s hs) hι hte

/-! ## Proposition 18.4 -/

/-- The IRI an identity names. -/
def Ident.iri? : Ident → Option String
  | .iri s => some s
  | _ => none

/-- Elements with the same members give a view with the same nodes and edges. -/
theorem gqlOf_congr (ι : Nat → Option String) {xs ys : List (Option String × Name × VTerm T)}
    (h : ∀ z, z ∈ xs ↔ z ∈ ys) :
    (∀ n, n ∈ (gqlOf K ι xs).nodes ↔ n ∈ (gqlOf K ι ys).nodes) ∧
      ∀ e, e ∈ (gqlOf K ι xs).edges ↔ e ∈ (gqlOf K ι ys).edges := by
  constructor
  · intro n
    simp only [gqlOf, List.mem_flatMap]
    exact ⟨fun ⟨z, hz, hn⟩ => ⟨z, (h z).mp hz, hn⟩, fun ⟨z, hz, hn⟩ => ⟨z, (h z).mpr hz, hn⟩⟩
  · intro e
    simp only [gqlOf, List.mem_flatMap]
    exact ⟨fun ⟨z, hz, he⟩ => ⟨z, (h z).mp hz, he⟩, fun ⟨z, hz, he⟩ => ⟨z, (h z).mpr hz, he⟩⟩

section RoundTrip

variable [DecidableEq T.P] {K} {vp : ViewParams T} {ord : VTerm T → VTerm T → Bool} {ι : Nat → Option String}
  {idOf : String → Nat} {G : FinApg K.schema}

/-- On an APG written as the store writes it, every element has an identity, which names its IRI exactly when the
element has one. -/
theorem ident_own (H : RoundTrip K vp ord ι idOf G) (graph : Nat → String) {x : Nat × Name × VTerm T}
    (hx : x ∈ G.elems) : ∃ i, ident K ι x = some i ∧ i.iri? = ι x.1 := by
  have hn := H.named x hx
  cases hc : K.find x.2.1 with
  | some c =>
    have hconc : c.concrete = true := by
      rcases H.labels x hx with ⟨c', hc', hconc⟩ | ⟨hnone, _⟩
      · rw [hc] at hc'
        cases hc'
        exact hconc
      · rw [hc] at hnone
        cases hnone
    cases ht : tripleNamed K x.2.1 with
    | false =>
      rw [hc, ht] at hn
      obtain ⟨s, hs⟩ := Option.isSome_iff_exists.mp (by simpa using hn)
      exact ⟨.iri s, by simp [ident, hc, ht, hs], hs.symm⟩
    | true =>
      rw [hc, ht] at hn
      obtain ⟨i, hi, _⟩ := triple_complete K H graph hx hc hconc ht
      obtain ⟨_, _, _, _, _, _, _, _, _, _, _, _, _, _, rfl⟩ := ident_triple K hc ht hi
      exact ⟨_, hi, by simp only [Ident.iri?]; exact (by simpa using hn : ι x.1 = none).symm⟩
  | none =>
    rw [hc] at hn
    obtain ⟨i, hi, _⟩ := pair_complete K H graph hx hc
    obtain ⟨_, _, _, _, _, _, _, _, rfl⟩ := ident_pair K hc hi
    exact ⟨_, hi, by simp only [Ident.iri?]; exact (by simpa using hn : ι x.1 = none).symm⟩

/-- Proposition 18.4: on a valid APG written as the store writes it, `P(G)` and the view of `R⁻¹(R(G))`, each element
under its identity's IRI, hold the same nodes and the same edges. -/
theorem gql_of_rdf (H : RoundTrip K vp ord ι idOf G) (graph : Nat → String) :
    let Z := (rdfInv K vp ord idOf (rdf K vp ι graph G)).map fun z => (z.1.iri?, z.2.1, z.2.2)
    (∀ n, n ∈ (gql K ι G).nodes ↔ n ∈ (gqlOf K ι Z).nodes) ∧
      ∀ e, e ∈ (gql K ι G).edges ↔ e ∈ (gqlOf K ι Z).edges := by
  intro Z
  apply gqlOf_congr
  intro z
  simp only [Z, List.mem_map]
  constructor
  · rintro ⟨x, hx, rfl⟩
    obtain ⟨i, hi, hown⟩ := ident_own H graph hx
    refine ⟨(i, x.2.1, x.2.2), (rdf_round_trip K H graph _).mpr ⟨x, hx, hi, rfl⟩, ?_⟩
    simp [hown]
  · rintro ⟨w, hw, rfl⟩
    obtain ⟨x, hx, hi, hxw⟩ := (rdf_round_trip K H graph w).mp hw
    obtain ⟨i, hi', hown⟩ := ident_own H graph hx
    rw [hi] at hi'
    cases hi'
    refine ⟨x, hx, ?_⟩
    rw [← hown, hxw]

end RoundTrip

/-! ## The graph type -/

/-- A node type: its labels and its property types, each optional or required. -/
structure GNodeType (T : Typeside) where
  /-- The labels. -/
  labels : List Name
  /-- The property types: name, type, optional. -/
  props : List (Name × PType T × Bool)

/-- An edge type: its labels, its endpoints' node types by their labels, its direction and its property types. -/
structure GEdgeType (T : Typeside) where
  /-- The labels. -/
  labels : List Name
  /-- The source's node type, by its labels. -/
  src : List Name
  /-- The destination's node type, by its labels. -/
  dst : List Name
  /-- Directed, else undirected. -/
  directed : Bool
  /-- The property types. -/
  props : List (Name × PType T × Bool)

/-- A graph type: its node types and its edge types. -/
structure GType (T : Typeside) where
  /-- The node types. -/
  nodes : List (GNodeType T)
  /-- The edge types. -/
  edges : List (GEdgeType T)

/-- The concrete types a role's participants admit. -/
def gAdmits (ps : List Name) : List Name := ps.flatMap K.conc

/-- The edge types between the concrete types two lists of participants admit. -/
def gPairTypes (labels : List Name) (directed : Bool) (props : List (Name × PType T × Bool)) (ps ps' : List Name) :
    List (GEdgeType T) :=
  (gAdmits K ps).flatMap fun s => (gAdmits K ps').map fun d => ⟨labels, K.chain s, K.chain d, directed, props⟩

/-- The role edge types of a relation node: from the relation to every concrete type each role admits. -/
def gRoleTypes (c : Name) : List (GEdgeType T) :=
  (K.allFields c).flatMap fun f =>
    if f.isRole then (gAdmits K f.participants).map fun d => ⟨gRoleLabels K c f.name, K.chain c, K.chain d, true, []⟩
    else []

/-- The edge types of a concrete relation: an edge-encoded one's edge types, a binary node-encoded one's navigation
edge types, and the role edge types of one presented as a node. -/
def gEdgeTypesOf (c : Name) : List (GEdgeType T) :=
  (if K.edgeEncoded c then
    optList (K.anchorRole c) fun a => optList (K.otherRole c) fun o =>
      (K.allFields c).flatMap fun f => if f.isRole && f.name == a then (K.allFields c).flatMap fun f' =>
        if f'.isRole && f'.name == o then
          gPairTypes K (K.chain c) (!K.inherits CType.symmetric c) (gPropTypes K c) f.participants f'.participants
        else []
      else []
  else
    optList (gBinary K c) fun rs =>
      optList (K.navOf c rs.1.1) (fun n => gPairTypes K [n] true [] rs.1.2 rs.2.2) ++
        optList (K.navOf c rs.2.1) (fun n => gPairTypes K [n] true [] rs.2.2 rs.1.2)) ++
  (if gPresented K c then gRoleTypes K c else [])

/-- `Γ(S(K))`: a node type per concrete type presented as a node, the edge types of every concrete relation. -/
def gType : GType T :=
  ⟨(K.types.filter fun c => c.concrete && gPresented K c.name).map fun c => ⟨K.chain c.name, gPropTypes K c.name⟩,
    (K.types.filter fun c => c.concrete && c.kind == .relation).flatMap fun c => gEdgeTypesOf K c.name⟩

/-! ## Conformance under PG-Schema's strict, closed semantics -/

section ConformanceDefs

variable [DecidableEq T.P]

/-- Two label sets are equal. -/
def SameLabels (a b : List Name) : Prop := ∀ x, x ∈ a ↔ x ∈ b

/-- A property's value has its type: a field's value checks against the field's type. -/
def PVal.Conforms (lam : Nat → Option Name) : PVal T → PType T → Prop
  | .val v, .ty t => (denote lam v t).isSome = true
  | .str _, .str => True
  | .bool _, .bool => True
  | _, _ => False

/-- Properties conform, closed: each has a type the list declares and conforms to it, and each required one is present.
-/
def PropsConform (lam : Nat → Option Name) (ps : List (Name × PVal T)) (pts : List (Name × PType T × Bool)) : Prop :=
  (∀ p ∈ ps, ∃ pt ∈ pts, pt.1 = p.1 ∧ p.2.Conforms lam pt.2.1) ∧ ∀ pt ∈ pts, pt.2.2 = false → ∃ p ∈ ps, p.1 = pt.1

/-- A node conforms to a node type of the graph type: its exact label set, its properties. -/
def NodeConforms (lam : Nat → Option Name) (Γ : GType T) (n : GNode T) : Prop :=
  ∃ nt ∈ Γ.nodes, SameLabels n.labels nt.labels ∧ PropsConform lam n.props nt.props

/-- An end of an edge is a node of the graph with the labels of the end's node type. -/
def EndConforms (P : PGraph T) (id : String) (ls : List Name) : Prop :=
  ∃ n ∈ P.nodes, n.id = id ∧ SameLabels n.labels ls

/-- An edge conforms to an edge type: its exact label set, its properties, its direction and its endpoints' node
types, either way round when undirected. -/
def EdgeConforms (lam : Nat → Option Name) (Γ : GType T) (P : PGraph T) (e : GEdge T) : Prop :=
  ∃ et ∈ Γ.edges, SameLabels e.labels et.labels ∧ PropsConform lam e.props et.props ∧ e.directed = et.directed ∧
    ((EndConforms P e.src et.src ∧ EndConforms P e.dst et.dst) ∨
      (e.directed = false ∧ EndConforms P e.dst et.src ∧ EndConforms P e.src et.dst))

end ConformanceDefs

section Conformance

variable [DecidableEq T.P] {K} {vp : ViewParams T} {ord : VTerm T → VTerm T → Bool} {ι : Nat → Option String}
  {idOf : String → Nat} {G : FinApg K.schema}

theorem concrete_of_find (H : RoundTrip K vp ord ι idOf G) {x : Nat × Name × VTerm T} (hx : x ∈ G.elems)
    {c : CType T} (hc : K.find x.2.1 = some c) : c.concrete = true := by
  rcases H.labels x hx with ⟨c', hc', hconc⟩ | ⟨hnone, _⟩
  · rw [hc] at hc'
    cases hc'
    exact hconc
  · rw [hc] at hnone
    cases hnone

omit [DecidableEq T.P] in
theorem asserted_slot {l : Name} (h : K.inherits CType.bitemporal l = true) :
    ("asserted", Catalog.Slot.asserted) ∈ (K.slots l : List (Name × Catalog.Slot T)) := by
  unfold Catalog.slots
  rw [mem_sortBy, List.mem_append]
  right
  simp [h]

/-- An element's properties conform to its type's property types. -/
theorem props_conform (H : RoundTrip K vp ord ι idOf G) {x : Nat × Name × VTerm T} (hx : x ∈ G.elems)
    {c : CType T} (hc : K.find x.2.1 = some c) : PropsConform G.lam (gProps K x.2.1 x.2.2) (gPropTypes K x.2.1) := by
  obtain ⟨_, _, _, hall⟩ := slot_value K H hx hc (concrete_of_find H hx hc)
  constructor
  · intro p hp
    simp only [gProps, List.mem_append, List.mem_singleton] at hp
    rcases hp with (hp | rfl) | hp
    · obtain ⟨⟨a, sl⟩, hs, hsp⟩ := List.mem_filterMap.mp hp
      cases sl with
      | attr opt t view =>
        obtain ⟨u, hu, rfl⟩ := Option.map_eq_some'.mp hsp
        refine ⟨(a, .ty t, opt), ?_, rfl, ?_⟩
        · simp only [gPropTypes, List.mem_append]
          exact Or.inl (Or.inl (List.mem_filterMap.mpr ⟨_, hs, rfl⟩))
        · obtain ⟨w, hw, hwc⟩ := hall a _ hs
          rw [hw, Option.some_bind] at hu
          show (denote G.lam u t).isSome = true
          cases opt with
          | false =>
            simp only [presentValue, Bool.false_eq_true, if_false, Option.some.injEq] at hu
            subst hu
            simpa [Catalog.slotType] using hwc
          | true =>
            simp only [Catalog.slotType, if_true] at hwc
            rcases check_optional hwc with rfl | ⟨u', rfl, hu'⟩
            · simp [presentValue, absent] at hu
            · simp only [presentValue, if_true, Option.some.injEq] at hu
              subst hu
              exact hu'
      | role _ _ _ => cases hsp
      | asserted => cases hsp
    · exact ⟨("type", .str, false), by simp [gPropTypes], rfl, trivial⟩
    · split at hp
      · next hb =>
        obtain ⟨w, hw, hwc⟩ := hall "asserted" _ (asserted_slot hb)
        rw [hw] at hp
        simp only [optList, List.mem_cons, List.mem_singleton, List.not_mem_nil, or_false] at hp
        rcases hp with rfl | rfl
        · exact ⟨("asserted", .ty K.assertedType, false), by simp [gPropTypes, hb], rfl, hwc⟩
        · exact ⟨("live", .bool, false), by simp [gPropTypes, hb], rfl, trivial⟩
      · cases hp
  · intro pt hpt hreq
    simp only [gPropTypes, List.mem_append, List.mem_singleton] at hpt
    rcases hpt with (hpt | rfl) | hpt
    · obtain ⟨⟨a, sl⟩, hs, hsp⟩ := List.mem_filterMap.mp hpt
      cases sl with
      | attr opt t view =>
        simp only [Option.some.injEq] at hsp
        subst hsp
        simp only at hreq
        subst hreq
        obtain ⟨w, hw, _⟩ := hall a _ hs
        refine ⟨(a, .val w), ?_, rfl⟩
        simp only [gProps, List.mem_append]
        refine Or.inl (Or.inl (List.mem_filterMap.mpr ⟨_, hs, ?_⟩))
        simp [hw, presentValue]
      | role _ _ _ => cases hsp
      | asserted => cases hsp
    · exact ⟨("type", .str x.2.1), by simp [gProps], rfl⟩
    · split at hpt
      · next hb =>
        obtain ⟨w, hw, _⟩ := hall "asserted" _ (asserted_slot hb)
        simp only [List.mem_cons, List.mem_singleton, List.not_mem_nil, or_false] at hpt
        rcases hpt with rfl | rfl
        · exact ⟨("asserted", .val w), by simp [gProps, hb, hw, optList], rfl⟩
        · exact ⟨("live", .bool ((w.field? "end").bind (presentValue true)).isNone), by
            simp [gProps, hb, hw, optList], rfl⟩
      · cases hpt

omit [DecidableEq T.P] in
theorem node_mem {x : Nat × Name × VTerm T} (hx : x ∈ G.elems) (hp : gPresented K x.2.1 = true) {s : String}
    (hs : ι x.1 = some s) : (⟨s, K.chain x.2.1, gProps K x.2.1 x.2.2⟩ : GNode T) ∈ (gql K ι G).nodes := by
  simp only [gql, gqlOf, List.flatMap_map, List.mem_flatMap]
  exact ⟨x, hx, by simp [hs, optList, gNodeTmpl, hp]⟩

theorem presented_of (H : RoundTrip K vp ord ι idOf G) {y : Nat × Name × VTerm T} (hy : y ∈ G.elems)
    (hf : (K.find y.2.1).isSome = true) (ht : tripleNamed K y.2.1 = false) : gPresented K y.2.1 = true := by
  obtain ⟨c, hc⟩ := Option.isSome_iff_exists.mp hf
  have hconc := concrete_of_find H hy hc
  rw [CType.concrete, Bool.and_eq_true] at hconc
  simp [gPresented, hc, ht, hconc.2]

/-- An element a value names is presented as a node with the label set of its type. -/
theorem ref_end (H : RoundTrip K vp ord ι idOf G) {x : Nat × Name × VTerm T} (hx : x ∈ G.elems) {p : Nat}
    (hp : p ∈ x.2.2.refs) : ∃ y ∈ G.elems, y.1 = p ∧ (K.find y.2.1).isSome = true ∧ gPresented K y.2.1 = true ∧
      ∃ s, ι p = some s ∧ ∃ n ∈ (gql K ι G).nodes, n.id = s ∧ n.labels = K.chain y.2.1 := by
  obtain ⟨y, hy, rfl, hf, ht⟩ := H.parts x hx p hp
  have hn := H.named y hy
  rw [hf, ht] at hn
  obtain ⟨s, hs⟩ := Option.isSome_iff_exists.mp (by simpa using hn)
  exact ⟨y, hy, rfl, hf, presented_of H hy hf ht, s, hs, _, node_mem hy (presented_of H hy hf ht) hs, rfl, rfl⟩

/-- A participant's label is a concrete type its role admits. -/
theorem admitted (H : RoundTrip K vp ord ι idOf G) {w : VTerm T} {ps : List Name}
    (hw : (denote G.lam w (K.participantType ps)).isSome = true) {p : Nat} (hp : w.refs = [p])
    {y : Nat × Name × VTerm T} (hy : y ∈ G.elems) (hyp : y.1 = p) (hf : (K.find y.2.1).isSome = true) :
    y.2.1 ∈ gAdmits K ps := by
  subst hyp
  obtain ⟨q, hq, hch⟩ := participant_admitted K hw hp (lam_of_mem H.nodup hy)
  obtain ⟨c, hc⟩ := Option.isSome_iff_exists.mp hf
  obtain ⟨hname, hmem⟩ := find_name K hc
  simp only [gAdmits, List.mem_flatMap]
  exact ⟨q, hq, (K.mem_conc _ _).mpr ⟨c, hmem, hname, concrete_of_find H hy hc, hch⟩⟩

/-- A single-valued role's participant: the value naming it, checked against the role's participants. -/
theorem role_value (H : RoundTrip K vp ord ι idOf G) {x : Nat × Name × VTerm T} (hx : x ∈ G.elems) {c : CType T}
    (hc : K.find x.2.1 = some c) {owner r : Name} {ps : List Name} {opt : Bool} {nv sp : Option Name}
    (hf : (owner, Field.role r ps opt nv sp) ∈ K.ownedFields x.2.1) {p : Nat} (hp : p ∈ fieldRefs x.2.2 r) :
    ∃ w : VTerm T, (denote G.lam w (K.participantType ps)).isSome = true ∧ w.refs = [p] ∧
      ∀ q ∈ w.refs, q ∈ x.2.2.refs := by
  obtain ⟨_, _, _, hall⟩ := slot_value K H hx hc (concrete_of_find H hx hc)
  obtain ⟨w0, hw0, hwc⟩ := hall r _ (mem_slots_of_role K hf)
  rw [fieldRefs_of_field hw0] at hp
  have hsub := refs_field hw0
  have fin : ∀ u : VTerm T, (denote G.lam u (K.participantType ps)).isSome = true →
      (∀ q ∈ u.refs, q ∈ x.2.2.refs) → p ∈ u.refs →
      ∃ w : VTerm T, (denote G.lam w (K.participantType ps)).isSome = true ∧ w.refs = [p] ∧
        ∀ q ∈ w.refs, q ∈ x.2.2.refs := by
    intro u hu hur hpu
    obtain ⟨p', _, hr, _, _⟩ := participant_round K H (fun _ => "") hu fun q hq => H.parts x hx q (hur q hq)
    rw [hr, List.mem_singleton] at hpu
    subst hpu
    exact ⟨u, hu, hr, hur⟩
  cases opt with
  | false => exact fin w0 (by simpa [Catalog.slotType] using hwc) hsub hp
  | true =>
    simp only [Catalog.slotType, if_true] at hwc
    rcases check_optional hwc with rfl | ⟨u, rfl, hu⟩
    · simp [absent, VTerm.refs, VTerm.refs.refsFields] at hp
    · exact fin u hu (fun q hq => hsub q (by simpa [VTerm.refs] using hq)) (by simpa [VTerm.refs] using hp)

omit [DecidableEq T.P] in
theorem nodeType_mem {l : Name} {c : CType T} (hc : K.find l = some c) (hconc : c.concrete = true)
    (hp : gPresented K l = true) : (⟨K.chain l, gPropTypes K l⟩ : GNodeType T) ∈ (gType K).nodes := by
  obtain ⟨hname, hmem⟩ := find_name K hc
  subst hname
  simp only [gType, List.mem_map, List.mem_filter, Bool.and_eq_true]
  exact ⟨c, ⟨hmem, hconc, hp⟩, rfl⟩

omit [DecidableEq T.P] in
theorem edgeType_mem {l : Name} {c : CType T} (hc : K.find l = some c) (hconc : c.concrete = true)
    (hrel : c.kind = .relation) {et : GEdgeType T} (h : et ∈ gEdgeTypesOf K l) : et ∈ (gType K).edges := by
  obtain ⟨hname, hmem⟩ := find_name K hc
  subst hname
  simp only [gType, List.mem_flatMap, List.mem_filter, Bool.and_eq_true]
  exact ⟨c, ⟨hmem, hconc, by simp [hrel]⟩, h⟩

omit [DecidableEq T.P] in
theorem roleType_mem {l : Name} (hp : gPresented K l = true) {f : Field T} (hf : f ∈ K.allFields l)
    (hr : f.isRole = true) {d : Name} (hd : d ∈ gAdmits K f.participants) :
    (⟨gRoleLabels K l f.name, K.chain l, K.chain d, true, []⟩ : GEdgeType T) ∈ gEdgeTypesOf K l := by
  simp only [gEdgeTypesOf, hp, if_true, List.mem_append]
  right
  simp only [gRoleTypes, List.mem_flatMap]
  exact ⟨f, hf, by simp only [hr, if_true, List.mem_map]; exact ⟨d, hd, rfl⟩⟩

omit [DecidableEq T.P] in
theorem pairType_mem {labels : List Name} {directed : Bool} {props : List (Name × PType T × Bool)}
    {ps ps' : List Name} {s d : Name} (hs : s ∈ gAdmits K ps) (hd : d ∈ gAdmits K ps') :
    (⟨labels, K.chain s, K.chain d, directed, props⟩ : GEdgeType T) ∈ gPairTypes K labels directed props ps ps' := by
  simp only [gPairTypes, List.mem_flatMap, List.mem_map]
  exact ⟨s, hs, d, hd, rfl⟩

omit [DecidableEq T.P] in
/-- A role label's set-valued role, owned by its relation. -/
theorem roleLabelOf_owned {l : Name} {y : Name × Name × Name × List Name} (h : K.roleLabelOf l = some y) :
    ∃ sp, (y.2.1, Field.setRole y.2.2.1 y.2.2.2 sp) ∈ K.ownedFields y.1 := by
  unfold Catalog.roleLabelOf at h
  obtain ⟨c, _, hc⟩ := List.exists_of_findSome?_eq_some h
  split at hc
  · obtain ⟨z, hz, hzy⟩ := List.exists_of_findSome?_eq_some hc
    match z, hz, hzy with
    | (owner, .setRole r ps sp), hz, hzy =>
      simp only at hzy
      split at hzy
      · cases hzy
        exact ⟨sp, hz⟩
      · cases hzy
    | (_, .attr _ _ _ _), _, hzy => cases hzy
    | (_, .role _ _ _ _ _), _, hzy => cases hzy
  · cases hc

/-- Properties of an edge without any conform to an edge type without any. -/
theorem props_nil (lam : Nat → Option Name) : PropsConform (T := T) lam [] [] :=
  ⟨fun _ h => (nomatch h), fun _ h => (nomatch h)⟩

omit [DecidableEq T.P] in
theorem inst_elem {own : Option String} {p p' : Nat} {dir der nm : Bool} {ls : List Name}
    {ps : List (Name × PVal T)} {e : GEdge T}
    (h : (⟨.elem p, .elem p', dir, der, nm, ls, ps⟩ : ETmpl T).inst own ι = some e) :
    ∃ s d, ι p = some s ∧ ι p' = some d ∧ e.src = s ∧ e.dst = d ∧ e.directed = dir ∧ e.labels = ls ∧ e.props = ps := by
  unfold ETmpl.inst at h
  simp only [GEnd.inst] at h
  obtain ⟨s, hs, h⟩ := Option.bind_eq_some.mp h
  obtain ⟨d, hd, h⟩ := Option.bind_eq_some.mp h
  obtain ⟨i, _, rfl⟩ := Option.map_eq_some'.mp h
  exact ⟨s, d, hs, hd, rfl, rfl, rfl, rfl, rfl⟩

omit [DecidableEq T.P] in
theorem inst_self {own : Option String} {p : Nat} {dir der nm : Bool} {ls : List Name}
    {ps : List (Name × PVal T)} {e : GEdge T}
    (h : (⟨.self, .elem p, dir, der, nm, ls, ps⟩ : ETmpl T).inst own ι = some e) :
    ∃ s d, own = some s ∧ ι p = some d ∧ e.src = s ∧ e.dst = d ∧ e.directed = dir ∧ e.labels = ls ∧ e.props = ps := by
  unfold ETmpl.inst at h
  simp only [GEnd.inst] at h
  obtain ⟨s, hs, h⟩ := Option.bind_eq_some.mp h
  obtain ⟨d, hd, h⟩ := Option.bind_eq_some.mp h
  obtain ⟨i, _, rfl⟩ := Option.map_eq_some'.mp h
  exact ⟨s, d, hs, hd, rfl, rfl, rfl, rfl, rfl⟩

/-- An edge with the edge type it has: its labels and direction the type's, a node with each endpoint's id carrying
the type's label set for that end, and its properties none, as the type's are, or its element's under its element's
type's. -/
def EdgeShape (K : Catalog T) (ι : Nat → Option String) (G : FinApg K.schema) (e : GEdge T) : Prop :=
  ∃ et ∈ (gType K).edges, e.labels = et.labels ∧ e.directed = et.directed ∧
    (∃ n ∈ (gql K ι G).nodes, n.id = e.src ∧ n.labels = et.src) ∧
    (∃ n ∈ (gql K ι G).nodes, n.id = e.dst ∧ n.labels = et.dst) ∧
    ((e.props = [] ∧ et.props = []) ∨
      ∃ x ∈ G.elems, (K.find x.2.1).isSome = true ∧ e.props = gProps K x.2.1 x.2.2 ∧ et.props = gPropTypes K x.2.1)

/-- A role edge from a relation node to a participant has its role edge type. -/
theorem role_edge_shape (H : RoundTrip K vp ord ι idOf G) {x : Nat × Name × VTerm T} (hx : x ∈ G.elems)
    {c : CType T} (hc : K.find x.2.1 = some c) (hrel : c.kind = .relation) (hp : gPresented K x.2.1 = true)
    {owner r : Name} {ps : List Name} {opt : Bool} {nv sp : Option Name}
    (hf : (owner, Field.role r ps opt nv sp) ∈ K.ownedFields x.2.1) {d : Bool} {t : ETmpl T}
    (ht : t ∈ gRoleEdges K x.2.1 r x.2.2 d) {e : GEdge T} (he : t.inst (ι x.1) ι = some e) :
    EdgeShape K ι G e := by
  simp only [gRoleEdges, List.mem_map] at ht
  obtain ⟨p, hpr, rfl⟩ := ht
  obtain ⟨s, s', hs, hs', hsrc, hdst, hdir, hlab, hprops⟩ := inst_self he
  obtain ⟨w, hw, hr, hsub⟩ := role_value H hx hc hf hpr
  obtain ⟨y, hy, hy1, hfy, _, s'', hs'', endy⟩ := ref_end H hx (hsub p (by simp [hr]))
  rw [hs'] at hs''
  cases hs''
  have had := admitted H hw hr hy hy1 hfy
  refine ⟨⟨gRoleLabels K x.2.1 r, K.chain x.2.1, K.chain y.2.1, true, []⟩,
    edgeType_mem hc (concrete_of_find H hx hc) hrel
      (roleType_mem hp (f := .role r ps opt nv sp) (List.mem_map_of_mem Prod.snd hf) rfl had), hlab, hdir, ?_, ?_,
    Or.inl ⟨hprops, rfl⟩⟩
  · rw [hsrc]; exact ⟨_, node_mem hx hp hs, rfl, rfl⟩
  · rw [hdst]; exact endy

/-- A navigation edge between a binary relation's participants has its navigation edge type. -/
theorem nav_shape (H : RoundTrip K vp ord ι idOf G) {x : Nat × Name × VTerm T} (hx : x ∈ G.elems)
    {c : CType T} (hc : K.find x.2.1 = some c) (hrel : c.kind = .relation) {r r' : Name} {ps ps' : List Name}
    (hf : ∃ o op nv sp, (o, Field.role r ps op nv sp) ∈ K.ownedFields x.2.1)
    (hf' : ∃ o op nv sp, (o, Field.role r' ps' op nv sp) ∈ K.ownedFields x.2.1)
    (hmem : ∀ n, K.navOf x.2.1 r = some n → ∀ s d, s ∈ gAdmits K ps → d ∈ gAdmits K ps' →
      (⟨[n], K.chain s, K.chain d, true, []⟩ : GEdgeType T) ∈ gEdgeTypesOf K x.2.1)
    {t : ETmpl T} (ht : t ∈ gNavEdges K x.2.1 r r' x.2.2) {e : GEdge T} (he : t.inst (ι x.1) ι = some e) :
    EdgeShape K ι G e := by
  obtain ⟨_, _, _, _, hf⟩ := hf
  obtain ⟨_, _, _, _, hf'⟩ := hf'
  unfold gNavEdges at ht
  cases hn : K.navOf x.2.1 r with
  | none => rw [hn] at ht; cases ht
  | some n =>
    rw [hn] at ht
    simp only [optList, List.mem_flatMap, List.mem_map] at ht
    obtain ⟨p, hp, p', hp', rfl⟩ := ht
    obtain ⟨s, s', hs, hs', hsrc, hdst, hdir, hlab, hprops⟩ := inst_elem he
    obtain ⟨w, hw, hr, hsub⟩ := role_value H hx hc hf hp
    obtain ⟨w', hw', hr', hsub'⟩ := role_value H hx hc hf' hp'
    obtain ⟨y, hy, hy1, hfy, _, s₁, hs₁, endy⟩ := ref_end H hx (hsub p (by simp [hr]))
    obtain ⟨y', hy', hy1', hfy', _, s₂, hs₂, endy'⟩ := ref_end H hx (hsub' p' (by simp [hr']))
    rw [hs] at hs₁
    cases hs₁
    rw [hs'] at hs₂
    cases hs₂
    refine ⟨⟨[n], K.chain y.2.1, K.chain y'.2.1, true, []⟩,
      edgeType_mem hc (concrete_of_find H hx hc) hrel
        (hmem n hn _ _ (admitted H hw hr hy hy1 hfy) (admitted H hw' hr' hy' hy1' hfy')), hlab, hdir, ?_, ?_,
      Or.inl ⟨hprops, rfl⟩⟩
    · rw [hsrc]; exact endy
    · rw [hdst]; exact endy'

omit [DecidableEq T.P] in
theorem binary_fields {l r₁ r₂ : Name} {ps₁ ps₂ : List Name} (h : gBinary K l = some ((r₁, ps₁), (r₂, ps₂))) :
    (∃ o op nv sp, (o, Field.role r₁ ps₁ op nv sp) ∈ K.ownedFields l) ∧
      ∃ o op nv sp, (o, Field.role r₂ ps₂ op nv sp) ∈ K.ownedFields l := by
  have key : ∀ f ∈ (K.allFields l).filter Field.isRole, ∃ o, (o, f) ∈ K.ownedFields l := fun f hf => by
    obtain ⟨⟨o, f'⟩, hof, rfl⟩ := List.mem_map.mp (List.mem_filter.mp hf).1
    exact ⟨o, hof⟩
  unfold gBinary at h
  split at h
  · rename_i hfl
    simp only [Option.some.injEq, Prod.mk.injEq] at h
    obtain ⟨⟨rfl, rfl⟩, rfl, rfl⟩ := h
    rw [hfl] at key
    obtain ⟨o₁, h₁⟩ := key _ (List.mem_cons_self _ _)
    obtain ⟨o₂, h₂⟩ := key _ (List.mem_cons_of_mem _ (List.mem_cons_self _ _))
    exact ⟨⟨o₁, _, _, _, h₁⟩, ⟨o₂, _, _, _, h₂⟩⟩
  · cases h

/-- An edge-encoded relation's edge and, with a reifier, its node's role edges have their edge types. -/
theorem edge_encoded_shape (H : RoundTrip K vp ord ι idOf G) {x : Nat × Name × VTerm T} (hx : x ∈ G.elems)
    {c : CType T} (hc : K.find x.2.1 = some c) (hrel : c.kind = .relation) (hedge : K.edgeEncoded x.2.1 = true)
    {t : ETmpl T} (ht : t ∈ gEdgeEncoded K x.2.1 x.2.2) {e : GEdge T} (he : t.inst (ι x.1) ι = some e) :
    EdgeShape K ι G e := by
  have hconc := concrete_of_find H hx hc
  obtain ⟨a, o, _, ha, ho, _, _, _, ⟨owa, psa, nva, spa, hfa⟩, ⟨owo, pso, nvo, spo, hfo⟩⟩ := H.edges x hx hedge
  simp only [gEdgeEncoded, ha, ho, optList, List.mem_append, List.mem_flatMap, List.mem_map] at ht
  rcases ht with ⟨pa, hpa, po, hpo, rfl⟩ | ht
  · obtain ⟨sa, so, hsa, hso, hsrc, hdst, hdir, hlab, hprops⟩ := inst_elem he
    obtain ⟨wa, hwa, hra, hsuba⟩ := role_value H hx hc hfa hpa
    obtain ⟨wo, hwo, hro, hsubo⟩ := role_value H hx hc hfo hpo
    obtain ⟨ya, hya, hya1, hfya, _, sa', hsa', enda⟩ := ref_end H hx (hsuba pa (by simp [hra]))
    obtain ⟨yo, hyo, hyo1, hfyo, _, so', hso', endo⟩ := ref_end H hx (hsubo po (by simp [hro]))
    rw [hsa] at hsa'
    cases hsa'
    rw [hso] at hso'
    cases hso'
    have hda := admitted H hwa hra hya hya1 hfya
    have hdo := admitted H hwo hro hyo hyo1 hfyo
    refine ⟨⟨K.chain x.2.1, K.chain ya.2.1, K.chain yo.2.1, !K.inherits CType.symmetric x.2.1, gPropTypes K x.2.1⟩,
      edgeType_mem hc hconc hrel ?_, hlab, hdir, ?_, ?_, Or.inr ⟨x, hx, by simp [hc], hprops, rfl⟩⟩
    · simp only [gEdgeTypesOf, hedge, if_true, ha, ho, optList, List.mem_append, List.mem_flatMap]
      left
      refine ⟨_, List.mem_map_of_mem Prod.snd hfa, ?_⟩
      simp only [Field.isRole, Field.name, beq_self_eq_true, Bool.and_self, if_true, List.mem_flatMap]
      refine ⟨_, List.mem_map_of_mem Prod.snd hfo, ?_⟩
      simp only [Field.isRole, Field.name, beq_self_eq_true, Bool.and_self, if_true]
      exact pairType_mem hda hdo
    · rw [hsrc]; exact enda
    · rw [hdst]; exact endo
  · split at ht
    · next hreif =>
      have hp : gPresented K x.2.1 = true := presented_of H hx (by simp [hc]) (by simp [tripleNamed, hreif])
      rcases List.mem_append.mp ht with ht | ht
      · exact role_edge_shape H hx hc hrel hp hfa ht he
      · exact role_edge_shape H hx hc hrel hp hfo ht he
    · cases ht

/-- A node-encoded relation's role edges and navigation edges have their edge types. -/
theorem node_encoded_shape (H : RoundTrip K vp ord ι idOf G) {x : Nat × Name × VTerm T} (hx : x ∈ G.elems)
    {c : CType T} (hc : K.find x.2.1 = some c) (hrel : c.kind = .relation) (hedge : K.edgeEncoded x.2.1 = false)
    {t : ETmpl T} (ht : t ∈ gNodeEncoded K x.2.1 x.2.2) {e : GEdge T} (he : t.inst (ι x.1) ι = some e) :
    EdgeShape K ι G e := by
  have hp : gPresented K x.2.1 = true := presented_of H hx (by simp [hc]) (by simp [tripleNamed, hedge])
  unfold gNodeEncoded at ht
  rcases List.mem_append.mp ht with ht | ht
  · obtain ⟨⟨owner, f⟩, hf, ht⟩ := List.mem_flatMap.mp ht
    cases f with
    | role r ps opt nv sp => exact role_edge_shape H hx hc hrel hp hf ht he
    | attr _ _ _ _ => cases ht
    | setRole _ _ _ => cases ht
  · cases hb : gBinary K x.2.1 with
    | none => rw [hb] at ht; cases ht
    | some rs =>
      obtain ⟨⟨r₁, ps₁⟩, ⟨r₂, ps₂⟩⟩ := rs
      rw [hb] at ht
      obtain ⟨hf₁, hf₂⟩ := binary_fields hb
      simp only [optList, List.mem_append] at ht
      rcases ht with ht | ht
      · refine nav_shape H hx hc hrel hf₁ hf₂ (fun n hn s d hs hd => ?_) ht he
        simp only [gEdgeTypesOf, hedge, Bool.false_eq_true, if_false, hb, optList, hn, List.mem_append]
        exact Or.inl (Or.inl (pairType_mem hs hd))
      · refine nav_shape H hx hc hrel hf₂ hf₁ (fun n hn s d hs hd => ?_) ht he
        simp only [gEdgeTypesOf, hedge, Bool.false_eq_true, if_false, hb, optList, hn, List.mem_append]
        exact Or.inl (Or.inr (pairType_mem hs hd))

/-- A role label's edge from its instance to its participant has its role edge type. -/
theorem role_label_shape (H : RoundTrip K vp ord ι idOf G) {x : Nat × Name × VTerm T} (hx : x ∈ G.elems)
    (hn : K.find x.2.1 = none) {t : ETmpl T} (ht : t ∈ gRoleLabelEdges K x.2.1 x.2.2) {e : GEdge T}
    (he : t.inst (ι x.1) ι = some e) : EdgeShape K ι G e := by
  unfold gRoleLabelEdges at ht
  cases hy : K.roleLabelOf x.2.1 with
  | none => rw [hy] at ht; cases ht
  | some y4 =>
    rw [hy] at ht
    simp only [optList, List.mem_flatMap, List.mem_map] at ht
    obtain ⟨i, hi, p, hp, rfl⟩ := ht
    obtain ⟨s, s', hs, hs', hsrc, hdst, hdir, hlab, hprops⟩ := inst_elem he
    obtain ⟨i0, wp, hv, hi0, hwp⟩ := roleLabel_value K H hx hn hy
    have hfi : fieldRefs x.2.2 "instance" = [i0] := by rw [hv]; rfl
    rw [hfi, List.mem_singleton] at hi
    subst hi
    have hrefs : ∀ q ∈ wp.refs, q ∈ x.2.2.refs := by
      intro q hq
      rw [hv]
      simp [VTerm.refs, VTerm.refs.refsFields, hq]
    obtain ⟨p', _, hrp, _, _⟩ := participant_round K H (fun _ => "") hwp fun q hq => H.parts x hx q (hrefs q hq)
    have hfp : fieldRefs x.2.2 "participant" = [p'] := by
      rw [hv]
      simp [fieldRefs, VTerm.field?, hrp]
    rw [hfp, List.mem_singleton] at hp
    subst hp
    obtain ⟨yi, hyi, hyi1, hfyi, hpres, si, hsi, endi⟩ :=
      ref_end H hx (p := i) (by rw [hv]; simp [VTerm.refs, VTerm.refs.refsFields])
    obtain ⟨yp, hyp, hyp1, hfyp, _, sp, hsp, endp⟩ := ref_end H hx (hrefs p (by simp [hrp]))
    rw [hs] at hsi
    cases hsi
    rw [hs'] at hsp
    cases hsp
    have had := admitted H hwp hrp hyp hyp1 hfyp
    have hlab_i : yi.2.1 = y4.1 := by
      have := lam_of_mem H.nodup hyi
      rw [hyi1, hi0] at this
      exact (Option.some.inj this).symm
    obtain ⟨spr, hown⟩ := roleLabelOf_owned hy
    obtain ⟨c', hc'⟩ := Option.isSome_iff_exists.mp hfyi
    have hfield : Field.setRole y4.2.2.1 y4.2.2.2 spr ∈ K.allFields yi.2.1 := by
      rw [hlab_i]
      exact List.mem_map_of_mem Prod.snd hown
    have hrel' : c'.kind = .relation := H.relations yi hyi c' hc' _ hfield rfl
    rw [hlab_i] at hc' hpres hfield endi
    refine ⟨⟨gRoleLabels K y4.1 y4.2.2.1, K.chain y4.1, K.chain yp.2.1, true, []⟩,
      edgeType_mem hc' (concrete_of_find H hyi (hlab_i ▸ hc')) hrel' (roleType_mem hpres hfield rfl had), hlab, hdir,
      ?_, ?_, Or.inl ⟨hprops, rfl⟩⟩
    · rw [hsrc]; exact endi
    · rw [hdst]; exact endp

/-- Every edge an element's template gives has its edge type. -/
theorem edge_shape (H : RoundTrip K vp ord ι idOf G) {x : Nat × Name × VTerm T} (hx : x ∈ G.elems)
    {t : ETmpl T} (ht : t ∈ gEdgeTmpls K x.2.1 x.2.2) {e : GEdge T} (he : t.inst (ι x.1) ι = some e) :
    EdgeShape K ι G e := by
  unfold gEdgeTmpls at ht
  cases hc : K.find x.2.1 with
  | none =>
    rw [hc] at ht
    exact role_label_shape H hx hc ht he
  | some c =>
    rw [hc] at ht
    simp only at ht
    split at ht
    · next hrel =>
      have hrel' : c.kind = .relation := by simpa using hrel
      cases hedge : K.edgeEncoded x.2.1 with
      | true =>
        rw [hedge, if_pos rfl] at ht
        exact edge_encoded_shape H hx hc hrel' hedge ht he
      | false =>
        rw [hedge] at ht
        exact node_encoded_shape H hx hc hrel' hedge ht he
    · cases ht

/-- Proposition 18.2: on a valid APG written as the store writes it, every node and every edge of `P(G)` conforms to
`Γ(S(K))` under PG-Schema's strict, closed semantics. -/
theorem gql_conforms (H : RoundTrip K vp ord ι idOf G) :
    (∀ n ∈ (gql K ι G).nodes, NodeConforms G.lam (gType K) n) ∧
      ∀ e ∈ (gql K ι G).edges, EdgeConforms G.lam (gType K) (gql K ι G) e := by
  constructor
  · intro n hn
    simp only [gql, gqlOf, List.flatMap_map, List.mem_flatMap] at hn
    obtain ⟨x, hx, hn⟩ := hn
    cases hs : ι x.1 with
    | none => rw [hs] at hn; cases hn
    | some s =>
      rw [hs] at hn
      simp only [optList, gNodeTmpl] at hn
      split at hn
      · next hp =>
        simp only [List.map_cons, List.map_nil, List.mem_singleton] at hn
        subst hn
        cases hc : K.find x.2.1 with
        | none => simp [gPresented, hc] at hp
        | some c => exact ⟨_, nodeType_mem hc (concrete_of_find H hx hc) hp, fun _ => Iff.rfl, props_conform H hx hc⟩
      · cases hn
  · intro e he
    simp only [gql, gqlOf, List.flatMap_map, List.mem_flatMap, List.mem_filterMap] at he
    obtain ⟨x, hx, t, ht, hte⟩ := he
    obtain ⟨et, het, hlab, hdir, ⟨n, hn, hni, hnl⟩, ⟨m, hm, hmi, hml⟩, hprops⟩ := edge_shape H hx ht hte
    refine ⟨et, het, by rw [hlab]; exact fun _ => Iff.rfl, ?_, hdir,
      Or.inl ⟨⟨n, hn, hni, by rw [hnl]; exact fun _ => Iff.rfl⟩, ⟨m, hm, hmi, by rw [hml]; exact fun _ => Iff.rfl⟩⟩⟩
    rcases hprops with ⟨h1, h2⟩ | ⟨y, hy, hf, h1, h2⟩
    · rw [h1, h2]; exact props_nil _
    · obtain ⟨c, hc⟩ := Option.isSome_iff_exists.mp hf
      rw [h1, h2]
      exact props_conform H hy hc

end Conformance

/-! ## MGQL's projection -/

/-- How the typeside's primitive types map onto MGQL's sorts: a primitive type's sort where it has one, and each value
of a mapped type a value of that sort. -/
structure MgqlSorts (T : Typeside) where
  /-- A primitive type's sort. -/
  sort : T.P → Option MGQL.BaseSort
  /-- A value's MGQL value. -/
  prim : (p : T.P) → T.PV p → MGQL.PrimValue
  /-- A mapped type's values have its sort. -/
  coh : ∀ p b (a : T.PV p), sort p = some b → (prim p a).hasSort b = true

section Mgql

variable (ms : MgqlSorts T)

/-- A list of property types projected onto MGQL's: the required ones of a mapped primitive type, `STRING` or
`BOOLEAN`. -/
def mSchemaOf (pts : List (Name × PType T × Bool)) : MGQL.PropSchema :=
  pts.filterMap fun pt =>
    match pt with
    | (k, .ty (.prim p), false) => (ms.sort p).map fun b => (k, b)
    | (k, .str, false) => some (k, .string)
    | (k, .bool, false) => some (k, .bool)
    | _ => none

/-- A property's MGQL value, where it has one. -/
def mValue : PVal T → Option MGQL.Value
  | .val (.prim p a) => (ms.sort p).map fun _ => .prim (ms.prim p a)
  | .str s => some (.prim (.string s))
  | .bool b => some (.prim (.bool b))
  | _ => none

/-- The type a property map's `type` property names. -/
def mTypeOf (ps : List (Name × PVal T)) : Option Name :=
  (ps.find? (·.1 == "type")).bind fun p =>
    match p.2 with
    | .str l => some l
    | _ => none

/-- A property's MGQL value by its name. -/
def mLook (ps : List (Name × PVal T)) (k : Name) : Option MGQL.Value :=
  (ps.find? (·.1 == k)).bind fun p => mValue ms p.2

/-- A property map projected onto MGQL's: the properties its type's projected property types name. -/
def mPropsOf (ps : List (Name × PVal T)) : MGQL.PropMap :=
  optList (mTypeOf ps) fun l =>
    (mSchemaOf ms (gPropTypes K l)).filterMap fun kt => (mLook ms ps kt.1).map fun v => (kt.1, v)

/-- The position of the first node with the id. -/
def findNode : (ns : List (GNode T)) → String → Option (Fin ns.length)
  | [], _ => none
  | n :: ns, id => if n.id == id then some ⟨0, Nat.zero_lt_succ _⟩ else (findNode ns id).map Fin.succ

/-- The edges whose endpoints are nodes, with their endpoints' positions. -/
def mEdges (P : PGraph T) : List (Fin P.nodes.length × Fin P.nodes.length × GEdge T) :=
  P.edges.filterMap fun e => (findNode P.nodes e.src).bind fun i => (findNode P.nodes e.dst).map fun j => (i, j, e)

/-- A property graph projected onto MGQL's: its nodes and edges with their labels, and their properties of MGQL's
sorts. -/
def mgqlGraph (P : PGraph T) : MGQL.PropertyGraph where
  numNodes := P.nodes.length
  numEdges := (mEdges P).length
  src k := ((mEdges P).get k).1
  dst k := ((mEdges P).get k).2.1
  edgeDirected k := ((mEdges P).get k).2.2.directed
  nodeLabels i := (P.nodes.get i).labels
  edgeLabels k := ((mEdges P).get k).2.2.labels
  nodeProps i := mPropsOf K ms (P.nodes.get i).props
  edgeProps k := mPropsOf K ms ((mEdges P).get k).2.2.props

/-- The node schema of a label set: its type's projected property types. -/
def mNodeOf (ls : List Name) : MGQL.NodeSchemaFull := ⟨ls, mSchemaOf ms (gPropTypes K (ls.headD ""))⟩

/-- `Γ(S(K))` projected onto MGQL's graph schemas. -/
def mgqlType : MGQL.GraphSchemaFull :=
  ⟨(gType K).nodes.map fun nt => ⟨nt.labels, mSchemaOf ms nt.props⟩,
    (gType K).edges.map fun et =>
      ⟨et.labels, mNodeOf K ms et.src, mNodeOf K ms et.dst, mSchemaOf ms et.props, et.directed⟩⟩

theorem findNode_id : ∀ {ns : List (GNode T)} {id : String} {i : Fin ns.length}, findNode ns id = some i →
    (ns.get i).id = id
  | [], _, _, h => by cases h
  | n :: ns, id, i, h => by
    unfold findNode at h
    split at h
    · next hn =>
      cases h
      simpa using hn
    · obtain ⟨j, hj, rfl⟩ := Option.map_eq_some'.mp h
      exact findNode_id (ns := ns) hj

theorem eq_of_key {α β : Type} : ∀ {l : List (α × β)}, (l.map Prod.fst).Nodup → ∀ {a b : α × β}, a ∈ l → b ∈ l →
    a.1 = b.1 → a = b
  | [], _, _, _, ha, _, _ => by cases ha
  | c :: l, hnd, a, b, ha, hb, hab => by
    rw [List.map_cons, List.nodup_cons] at hnd
    rcases List.mem_cons.mp ha with ha' | ha' <;> rcases List.mem_cons.mp hb with hb' | hb'
    · rw [ha', hb']
    · subst ha'
      exact (hnd.1 (by rw [hab]; exact List.mem_map_of_mem Prod.fst hb')).elim
    · subst hb'
      exact (hnd.1 (by rw [← hab]; exact List.mem_map_of_mem Prod.fst ha')).elim
    · exact eq_of_key hnd.2 ha' hb' hab

theorem nodup_filterMap_key {α β δ : Type} {f : α × β → Option (α × δ)} (hf : ∀ x y, f x = some y → y.1 = x.1) :
    ∀ {l : List (α × β)}, (l.map Prod.fst).Nodup → ((l.filterMap f).map Prod.fst).Nodup
  | [], _ => List.nodup_nil
  | x :: l, hnd => by
    rw [List.map_cons, List.nodup_cons] at hnd
    rw [List.filterMap_cons]
    cases hx : f x with
    | none => exact nodup_filterMap_key hf hnd.2
    | some y =>
      rw [List.map_cons, List.nodup_cons]
      refine ⟨fun hy => hnd.1 ?_, nodup_filterMap_key hf hnd.2⟩
      obtain ⟨z, hz, hzy⟩ := List.mem_map.mp hy
      obtain ⟨w, hw, hwz⟩ := List.mem_filterMap.mp hz
      rw [← hf x y hx, ← hzy, hf w z hwz]
      exact List.mem_map_of_mem Prod.fst hw

theorem mSchemaOf_key (pt : Name × PType T × Bool) (kt : Name × MGQL.BaseSort)
    (h : (match pt with
      | (k, .ty (.prim p), false) => (ms.sort p).map fun b => (k, b)
      | (k, .str, false) => some (k, .string)
      | (k, .bool, false) => some (k, .bool)
      | _ => none) = some kt) :
    kt.1 = pt.1 ∧ ((∃ p, pt.2 = (.ty (.prim p), false) ∧ ms.sort p = some kt.2) ∨
      (pt.2 = (.str, false) ∧ kt.2 = .string) ∨ (pt.2 = (.bool, false) ∧ kt.2 = .bool)) := by
  split at h
  · next k p =>
    obtain ⟨b, hb, rfl⟩ := Option.map_eq_some'.mp h
    exact ⟨rfl, Or.inl ⟨p, rfl, hb⟩⟩
  · cases h
    exact ⟨rfl, Or.inr (Or.inl ⟨rfl, rfl⟩)⟩
  · cases h
    exact ⟨rfl, Or.inr (Or.inr ⟨rfl, rfl⟩)⟩
  · cases h

theorem mSchemaOf_nodup {pts : List (Name × PType T × Bool)} (hnd : (pts.map Prod.fst).Nodup) :
    ((mSchemaOf ms pts).map Prod.fst).Nodup :=
  nodup_filterMap_key (fun x y h => (mSchemaOf_key ms x y h).1) hnd

theorem find?_schemaKey {pm : MGQL.PropMap} {Ph : MGQL.PropSchema} (look : Name → Option MGQL.Value)
    (hpm : pm = Ph.filterMap fun kt => (look kt.1).map fun v => (kt.1, v)) (hnd : (Ph.map Prod.fst).Nodup)
    (h : ∀ kt ∈ Ph, ∃ v, look kt.1 = some v ∧ v.conformsToBase kt.2 = true) :
    ∀ kt ∈ Ph, ∃ v, pm.find? (fun e => e.1 == kt.1) = some (kt.1, v) ∧ v.conformsToBase kt.2 = true := by
  subst hpm
  induction Ph with
  | nil => intro _ h; cases h
  | cons k0 rest ih =>
    rw [List.map_cons, List.nodup_cons] at hnd
    obtain ⟨v0, hv0, hc0⟩ := h k0 (List.mem_cons_self _ _)
    intro kt hkt
    rw [List.filterMap_cons, hv0]
    simp only [Option.map_some']
    rcases List.mem_cons.mp hkt with rfl | hkt
    · exact ⟨v0, by simp [List.find?_cons], hc0⟩
    · have hne : (k0.1 == kt.1) = false := by
        rw [beq_eq_false_iff_ne]
        exact fun he => hnd.1 (he ▸ List.mem_map_of_mem Prod.fst hkt)
      obtain ⟨v, hv, hc⟩ := ih hnd.2 (fun k hk => h k (List.mem_cons_of_mem _ hk)) kt hkt
      exact ⟨v, by simp only [List.find?_cons, hne]; exact hv, hc⟩

/-- A property map with every key of a schema whose keys are distinct, each value of its key's sort, conforms. -/
theorem propMap_conforms {Ph : MGQL.PropSchema} (hnd : (Ph.map Prod.fst).Nodup) (look : Name → Option MGQL.Value)
    (h : ∀ kt ∈ Ph, ∃ v, look kt.1 = some v ∧ v.conformsToBase kt.2 = true) :
    MGQL.propMapConformsSchema (Ph.filterMap fun kt => (look kt.1).map fun v => (kt.1, v)) Ph = true := by
  have hk : (Ph.filterMap fun kt => (look kt.1).map fun v => (kt.1, v)).map Prod.fst = Ph.map Prod.fst := by
    clear hnd
    induction Ph with
    | nil => rfl
    | cons kt rest ih =>
      obtain ⟨v, hv, _⟩ := h kt (List.mem_cons_self _ _)
      rw [List.filterMap_cons, hv]
      simp only [Option.map_some', List.map_cons]
      rw [ih fun k hk => h k (List.mem_cons_of_mem _ hk)]
  have hf := find?_schemaKey look rfl hnd h
  unfold MGQL.propMapConformsSchema
  simp only [MGQL.PropMap.dom, MGQL.PropSchema.dom, hk, Bool.and_eq_true, List.all_eq_true, List.any_eq_true,
    beq_iff_eq]
  refine ⟨⟨⟨trivial, fun x hx => ⟨x, hx, rfl⟩⟩, fun x hx => ⟨x, hx, rfl⟩⟩, ?_⟩
  intro kt hkt
  obtain ⟨v, hv, hc⟩ := hf kt hkt
  simp [MGQL.PropMap.lookupOpt, hv, hc]

end Mgql

section MgqlConformance

variable [DecidableEq T.P] (ms : MgqlSorts T) {K} {vp : ViewParams T} {ord : VTerm T → VTerm T → Bool}
  {ι : Nat → Option String} {idOf : String → Nat} {G : FinApg K.schema}

/-- Every type's property names are distinct: no attribute is named `type`, `asserted` or `live` (`UGS0417`,
`UGS0802`). -/
def PropNamesUnique (K : Catalog T) : Prop := ∀ c ∈ K.types, ((gPropTypes K c.name).map Prod.fst).Nodup

omit [DecidableEq T.P] in
theorem gProps_type {x : Nat × Name × VTerm T} (hnd : ((gPropTypes K x.2.1).map Prod.fst).Nodup) :
    mTypeOf (gProps K x.2.1 x.2.2) = some x.2.1 := by
  have huniq : ∀ p ∈ gProps K x.2.1 x.2.2, p.1 = "type" → p = ("type", .str x.2.1) := by
    intro p hp hpt
    simp only [gProps, List.mem_append, List.mem_singleton] at hp
    rcases hp with (hp | rfl) | hp
    · obtain ⟨⟨a, sl⟩, hs, hsp⟩ := List.mem_filterMap.mp hp
      cases sl with
      | attr opt t view =>
        obtain ⟨u, _, rfl⟩ := Option.map_eq_some'.mp hsp
        have h1 : (a, PType.ty t, opt) ∈ gPropTypes K x.2.1 := by
          simp only [gPropTypes, List.mem_append]
          exact Or.inl (Or.inl (List.mem_filterMap.mpr ⟨_, hs, rfl⟩))
        have h2 : ("type", (PType.str : PType T), false) ∈ gPropTypes K x.2.1 := by simp [gPropTypes]
        have := eq_of_key hnd h1 h2 hpt
        simp at this
      | role _ _ _ => cases hsp
      | asserted => cases hsp
    · rfl
    · split at hp
      · cases hu : x.2.2.field? "asserted" with
        | none => rw [hu] at hp; cases hp
        | some u =>
          rw [hu] at hp
          simp only [optList, List.mem_cons, List.mem_singleton, List.not_mem_nil, or_false] at hp
          rcases hp with rfl | rfl <;> simp at hpt
      · cases hp
  have hmem : ("type", PVal.str x.2.1) ∈ gProps K x.2.1 x.2.2 := by simp [gProps]
  unfold mTypeOf
  cases hf : (gProps K x.2.1 x.2.2).find? (·.1 == "type") with
  | none => exact absurd (List.find?_eq_none.mp hf _ hmem) (by simp)
  | some p0 =>
    rw [huniq p0 (List.mem_of_find?_eq_some hf) (by simpa using List.find?_some hf)]
    rfl

/-- An element's properties projected onto MGQL's conform to its type's projected property types. -/
theorem mprops_conform (H : RoundTrip K vp ord ι idOf G) {x : Nat × Name × VTerm T} (hx : x ∈ G.elems)
    {c : CType T} (hc : K.find x.2.1 = some c) (hnd : ((gPropTypes K x.2.1).map Prod.fst).Nodup) :
    MGQL.propMapConformsSchema (mPropsOf K ms (gProps K x.2.1 x.2.2)) (mSchemaOf ms (gPropTypes K x.2.1)) = true := by
  unfold mPropsOf
  rw [gProps_type hnd]
  simp only [optList]
  apply propMap_conforms (mSchemaOf_nodup ms hnd) (mLook ms (gProps K x.2.1 x.2.2))
  intro kt hkt
  obtain ⟨pt, hpt, hm⟩ := List.mem_filterMap.mp hkt
  obtain ⟨hk1, hcase⟩ := mSchemaOf_key ms pt kt hm
  have hpc := props_conform H hx hc
  have hreq : pt.2.2 = false := by
    rcases hcase with ⟨_, h, _⟩ | ⟨h, _⟩ | ⟨h, _⟩ <;> rw [h]
  obtain ⟨q, hq, hq1⟩ := hpc.2 pt hpt hreq
  unfold mLook
  cases hf : (gProps K x.2.1 x.2.2).find? (·.1 == kt.1) with
  | none => exact absurd (List.find?_eq_none.mp hf q hq) (by simp [hq1, hk1])
  | some p0 =>
    have hp0 := List.mem_of_find?_eq_some hf
    have hp01 : p0.1 = kt.1 := by simpa using List.find?_some hf
    obtain ⟨pt', hpt', hpt'1, hconf⟩ := hpc.1 p0 hp0
    have hpp : pt' = pt := eq_of_key hnd hpt' hpt (by rw [hpt'1, hp01, hk1])
    subst hpp
    simp only [Option.some_bind]
    rcases hcase with ⟨p, h, hs⟩ | ⟨h, hs⟩ | ⟨h, hs⟩
    · rw [h] at hconf
      cases hv : p0.2 with
      | val u =>
        rw [hv] at hconf
        obtain ⟨a, rfl⟩ := check_prim hconf
        exact ⟨.prim (ms.prim p a), by simp [mValue, hv, hs], ms.coh p kt.2 a hs⟩
      | str _ => rw [hv] at hconf; exact hconf.elim
      | bool _ => rw [hv] at hconf; exact hconf.elim
    · rw [h] at hconf
      cases hv : p0.2 with
      | str s => exact ⟨.prim (.string s), by simp [mValue, hv], by rw [hs]; rfl⟩
      | val _ => rw [hv] at hconf; exact hconf.elim
      | bool _ => rw [hv] at hconf; exact hconf.elim
    · rw [h] at hconf
      cases hv : p0.2 with
      | bool b => exact ⟨.prim (.bool b), by simp [mValue, hv], by rw [hs]; rfl⟩
      | val _ => rw [hv] at hconf; exact hconf.elim
      | str _ => rw [hv] at hconf; exact hconf.elim

omit [DecidableEq T.P] in
theorem node_shape {n : GNode T} (hn : n ∈ (gql K ι G).nodes) : ∃ x ∈ G.elems, ι x.1 = some n.id ∧
    gPresented K x.2.1 = true ∧ n.labels = K.chain x.2.1 ∧ n.props = gProps K x.2.1 x.2.2 := by
  simp only [gql, gqlOf, List.flatMap_map, List.mem_flatMap] at hn
  obtain ⟨x, hx, hn⟩ := hn
  cases hs : ι x.1 with
  | none => rw [hs] at hn; cases hn
  | some s =>
    rw [hs] at hn
    simp only [optList, gNodeTmpl] at hn
    split at hn
    · next hp =>
      simp only [List.map_cons, List.map_nil, List.mem_singleton] at hn
      subst hn
      exact ⟨x, hx, hs, hp, rfl, rfl⟩
    · cases hn

omit [DecidableEq T.P] in
theorem find_of_presented {l : Name} (hp : gPresented K l = true) : ∃ c, K.find l = some c := by
  cases hc : K.find l with
  | none => simp [gPresented, hc] at hp
  | some c => exact ⟨c, rfl⟩

/-- Two nodes with one id have one label set: an id is one element's IRI. -/
theorem node_labels_eq (H : RoundTrip K vp ord ι idOf G) {n n' : GNode T} (hn : n ∈ (gql K ι G).nodes)
    (hn' : n' ∈ (gql K ι G).nodes) (hid : n.id = n'.id) : n.labels = n'.labels := by
  obtain ⟨x, hx, hs, _, hl, _⟩ := node_shape hn
  obtain ⟨x', hx', hs', _, hl', _⟩ := node_shape hn'
  rw [hid] at hs
  have hxx : x = x' := eq_of_key H.nodup hx hx' (H.inj x hx x' hx' _ hs hs')
  rw [hl, hl', hxx]

/-- Every node of the projection conforms to the node schema of its label set. -/
theorem mgql_node (H : RoundTrip K vp ord ι idOf G) (hkeys : PropNamesUnique K) (i : Fin (gql K ι G).nodes.length) :
    MGQL.nodeConformsSchema (mgqlGraph K ms (gql K ι G)) i (mNodeOf K ms ((gql K ι G).nodes.get i).labels) = true := by
  obtain ⟨x, hx, _, hp, hl, hprops⟩ := node_shape (n := (gql K ι G).nodes.get i) (List.get_mem _ _ _)
  obtain ⟨c, hc⟩ := find_of_presented hp
  obtain ⟨hname, hmem⟩ := find_name K hc
  have hnd := hkeys c hmem
  rw [hname] at hnd
  unfold MGQL.nodeConformsSchema
  simp only [mgqlGraph, mNodeOf, Bool.and_eq_true, beq_self_eq_true, List.all_eq_true, List.any_eq_true, beq_iff_eq,
    true_and]
  refine ⟨⟨fun l hl => ⟨l, hl, rfl⟩, fun l hl => ⟨l, hl, rfl⟩⟩, ?_⟩
  rw [hl, hprops]
  exact mprops_conform ms H hx hc hnd

/-- Every edge of `P(G)` has its edge type. -/
theorem edge_shape_of_mem (H : RoundTrip K vp ord ι idOf G) {e : GEdge T} (he : e ∈ (gql K ι G).edges) :
    EdgeShape K ι G e := by
  simp only [gql, gqlOf, List.flatMap_map, List.mem_flatMap, List.mem_filterMap] at he
  obtain ⟨x, hx, t, ht, hte⟩ := he
  exact edge_shape H hx ht hte

/-- MGQL's conformance (decided 2026-11-08): on a valid APG written as the store writes it, with distinct property
names, the projection of `P(G)` onto MGQL's property graphs conforms to the projection of `Γ(S(K))` under
`graphConformsSchema`. -/
theorem mgql_conforms (H : RoundTrip K vp ord ι idOf G) (hkeys : PropNamesUnique K) :
    MGQL.graphConformsSchema (mgqlGraph K ms (gql K ι G)) (mgqlType K ms) = true := by
  unfold MGQL.graphConformsSchema
  simp only [Bool.and_eq_true, List.all_eq_true, List.mem_range]
  constructor
  · intro i hi
    rw [dif_pos hi, List.any_eq_true]
    obtain ⟨x, hx, _, hp, hl, _⟩ := node_shape (n := (gql K ι G).nodes.get ⟨i, hi⟩) (List.get_mem _ _ _)
    obtain ⟨c, hc⟩ := find_of_presented hp
    refine ⟨mNodeOf K ms ((gql K ι G).nodes.get ⟨i, hi⟩).labels, ?_, mgql_node ms H hkeys ⟨i, hi⟩⟩
    simp only [mgqlType, List.mem_map]
    refine ⟨⟨K.chain x.2.1, gPropTypes K x.2.1⟩, nodeType_mem hc (concrete_of_find H hx hc) hp, ?_⟩
    rw [hl]
    rfl
  · intro k hk
    rw [dif_pos hk, List.any_eq_true]
    have hqm : (mEdges (gql K ι G)).get ⟨k, hk⟩ ∈ mEdges (gql K ι G) := List.get_mem _ _ _
    obtain ⟨e, he, hfe⟩ := List.mem_filterMap.mp hqm
    obtain ⟨i, hi, hfe⟩ := Option.bind_eq_some.mp hfe
    obtain ⟨j, hj, hfe⟩ := Option.map_eq_some'.mp hfe
    obtain ⟨et, het, hlab, hdir, ⟨n, hn, hni, hnl⟩, ⟨m, hm, hmi, hml⟩, hprops⟩ := edge_shape_of_mem H he
    have hsrc : ((gql K ι G).nodes.get i).labels = et.src := by
      rw [← hnl]
      exact node_labels_eq H (List.get_mem _ _ _) hn (by rw [findNode_id hi, hni])
    have hdst : ((gql K ι G).nodes.get j).labels = et.dst := by
      rw [← hml]
      exact node_labels_eq H (List.get_mem _ _ _) hm (by rw [findNode_id hj, hmi])
    refine ⟨⟨et.labels, mNodeOf K ms et.src, mNodeOf K ms et.dst, mSchemaOf ms et.props, et.directed⟩,
      List.mem_map_of_mem _ het, ?_⟩
    have hsn := mgql_node ms H hkeys i
    have hdn := mgql_node ms H hkeys j
    rw [hsrc] at hsn
    rw [hdst] at hdn
    unfold MGQL.edgeConformsSchema
    simp only [mgqlGraph, ← hfe, hlab, hdir, Bool.and_eq_true, beq_self_eq_true, List.all_eq_true,
      List.any_eq_true, beq_iff_eq, true_and]
    refine ⟨⟨⟨⟨fun l hl => ⟨l, hl, rfl⟩, fun l hl => ⟨l, hl, rfl⟩⟩, ?_⟩, trivial⟩, ?_⟩
    · rcases hprops with ⟨h1, h2⟩ | ⟨y, hy, hf, h1, h2⟩
      · rw [h1, h2]
        rfl
      · obtain ⟨c, hc⟩ := Option.isSome_iff_exists.mp hf
        obtain ⟨hname, hmem⟩ := find_name K hc
        have hnd := hkeys c hmem
        rw [hname] at hnd
        rw [h1, h2]
        exact mprops_conform ms H hy hc hnd
    · split
      · exact (Bool.and_eq_true _ _).mpr ⟨hsn, hdn⟩
      · exact (Bool.or_eq_true _ _).mpr (Or.inl ((Bool.and_eq_true _ _).mpr ⟨hsn, hdn⟩))

end MgqlConformance

end ApgModel.Views
