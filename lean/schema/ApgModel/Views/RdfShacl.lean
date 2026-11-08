/-
 SPDX-License-Identifier: Apache-2.0
 Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
-/
import ApgModel.Views.RdfRoundTrip

/-!
# Corollary 17.4: the SHACL shapes hold on the RDF view

`SCHEMA-APG.md` corollary 17.4 and decision L10 of `openspec/notes/archive/prepare-apg-lean.md`: one lemma per shape
condition `SCHEMA.md` section 11.2 publishes, stated over the quads of `R(G)` at an element's IRI; no SHACL engine is
modelled.

- `sh:minCount 1`: a required attribute has a quad (`shacl_min_count`);
- `sh:maxCount 1`: a single-valued attribute has at most one (`shacl_max_count`);
- `sh:datatype`: a primitive attribute's objects, a set's members among them, are literals of its type
  (`shacl_datatype`);
- `sh:in`: an enumeration's objects are its members (`shacl_in`);
- `sh:class`: a role triple's participant, and an edge-encoded relation's navigation triple's object, carries a class
  its role admits (`shacl_class`, `shacl_class_edge`).

Each follows from the shapes of an element's own quads that proposition 17.3's proof established, under the same
conditions (`RoundTrip`).
-/

namespace ApgModel.Views

open Named Catalog

variable {T : Typeside} (K : Catalog T) [DecidableEq T.P]

variable {vp : ViewParams T} {ord : VTerm T → VTerm T → Bool} {ι : Nat → Option String} {idOf : String → Nat}
  {G : FinApg K.schema}

/-- An attribute's objects at an element's IRI: its present value's own objects. -/
theorem attr_objects (H : RoundTrip K vp ord ι idOf G) (graph : Nat → String) {x : Nat × Name × VTerm T}
    (hx : x ∈ G.elems) {s : String} (hs : ι x.1 = some s) {c : CType T} (hc : K.find x.2.1 = some c) {a : Name}
    {opt : Bool} {t : NTy T.P Name} {view : AttrView} (hslot : (a, Catalog.Slot.attr opt t view) ∈ K.slots x.2.1) :
    objectsOf (rdf K vp ι graph G) s (.attr a) =
      optList ((x.2.2.field? a).bind (presentValue opt)) fun u =>
        ownObjs ι (.attr a) (attrTemplates vp x.1 a view u) := by
  obtain ⟨owner, hown⟩ := mem_slots_attr K hslot
  rw [objectsOf_rdf K vp graph H.nodup H.inj hx hs (p := .attr a) rfl,
    ownObjs_attr K vp ι hc (H.namesUnique x hx) (List.mem_map_of_mem Prod.snd hown) (by simp [attrPred])]

/-- A present value checks against the attribute's type. -/
theorem present_check {lam : Nat → Option Name} {opt : Bool} {t : NTy T.P Name} {w u : VTerm T}
    (hw : (denote lam w (if opt then NTy.optional t else t)).isSome = true) (hu : presentValue opt w = some u) :
    (denote lam u t).isSome = true := by
  cases opt with
  | false =>
    have hwu : w = u := Option.some.inj hu
    subst hwu
    simpa using hw
  | true =>
    replace hw : (denote lam w (NTy.optional t)).isSome = true := by simpa using hw
    rcases check_optional hw with rfl | ⟨u', rfl, hu'⟩
    · simp [presentValue_absent] at hu
    · rw [presentValue_some] at hu
      cases hu
      exact hu'

/-- The value of an attribute slot, from the element's record. -/
theorem attr_value (H : RoundTrip K vp ord ι idOf G) {x : Nat × Name × VTerm T} (hx : x ∈ G.elems) {c : CType T}
    (hc : K.find x.2.1 = some c) (hconc : c.concrete = true) {a : Name} {opt : Bool} {t : NTy T.P Name}
    {view : AttrView} (hslot : (a, Catalog.Slot.attr opt t view) ∈ K.slots x.2.1) :
    ∃ w, x.2.2.field? a = some w ∧ (denote G.lam w (if opt then NTy.optional t else t)).isSome = true := by
  obtain ⟨_, _, _, hall⟩ := slot_value K H hx hc hconc
  exact hall a _ hslot

/-- `sh:minCount 1`: a required attribute has a quad at its element's IRI. -/
theorem shacl_min_count (H : RoundTrip K vp ord ι idOf G) (graph : Nat → String) {x : Nat × Name × VTerm T}
    (hx : x ∈ G.elems) {s : String} (hs : ι x.1 = some s) {c : CType T} (hc : K.find x.2.1 = some c)
    (hconc : c.concrete = true) {a : Name} {t : NTy T.P Name} {view : AttrView} (hview : view ≠ .interval)
    (hslot : (a, Catalog.Slot.attr false t view) ∈ K.slots x.2.1) :
    objectsOf (rdf K vp ι graph G) s (.attr a) ≠ [] := by
  obtain ⟨w, hw, hwc⟩ := attr_value K H hx hc hconc hslot
  obtain ⟨owner, hown⟩ := mem_slots_attr K hslot
  rw [attr_objects K H graph hx hs hc hslot, hw]
  replace hwc : (denote G.lam w t).isSome = true := by simpa using hwc
  exact (attr_round (vp := vp) (ι := ι) (ord := ord) G.lam (H.attrs x hx owner a t false view hown) hview x.1 a hwc
    fun ms h => H.sets x hx a w ms hw (Or.inl h)).1

/-- `sh:maxCount 1`: a single-valued attribute has at most one quad at its element's IRI. -/
theorem shacl_max_count (H : RoundTrip K vp ord ι idOf G) (graph : Nat → String) {x : Nat × Name × VTerm T}
    (hx : x ∈ G.elems) {s : String} (hs : ι x.1 = some s) {c : CType T} (hc : K.find x.2.1 = some c)
    (hconc : c.concrete = true) {a : Name} {opt : Bool} {t : NTy T.P Name} {view : AttrView}
    (hns : ∀ t', t ≠ .set t') (hslot : (a, Catalog.Slot.attr opt t view) ∈ K.slots x.2.1) :
    (objectsOf (rdf K vp ι graph G) s (.attr a)).length ≤ 1 := by
  obtain ⟨w, hw, hwc⟩ := attr_value K H hx hc hconc hslot
  rw [attr_objects K H graph hx hs hc hslot, hw]
  simp only [some_bind']
  cases hu : presentValue opt w with
  | none => simp [optList]
  | some u =>
    have huc := present_check hwc hu
    simp only [optList]
    rw [ownObjs_attrTemplates vp]
    cases view with
    | plain =>
      have : attrObjs u = [memberObj u] := by
        cases u with
        | set ms =>
          cases t with
          | set t' => exact absurd rfl (hns t')
          | _ => simp [denote] at huc
        | _ => rfl
      rw [this]
      simp only [List.filterMap_cons, List.filterMap_nil]
      split <;> simp
    | measure => cases (TObj.measure u).inst ι <;> simp
    | typeValue => cases asTag u <;> simp [optList]
    | interval => simp

/-- `sh:datatype`: a primitive attribute's objects, a set's members among them, are literals of its type. -/
theorem shacl_datatype (H : RoundTrip K vp ord ι idOf G) (graph : Nat → String) {x : Nat × Name × VTerm T}
    (hx : x ∈ G.elems) {s : String} (hs : ι x.1 = some s) {c : CType T} (hc : K.find x.2.1 = some c)
    (hconc : c.concrete = true) {a : Name} {opt : Bool} {p : T.P} {t : NTy T.P Name}
    (ht : t = .prim p ∨ t = .set (.prim p)) (hslot : (a, Catalog.Slot.attr opt t .plain) ∈ K.slots x.2.1) :
    ∀ o ∈ objectsOf (rdf K vp ι graph G) s (.attr a), ∃ v, o = .lit p v := by
  obtain ⟨w, hw, hwc⟩ := attr_value K H hx hc hconc hslot
  rw [attr_objects K H graph hx hs hc hslot, hw]
  simp only [some_bind']
  intro o ho
  obtain ⟨u, hu, ho⟩ := mem_optList.mp ho
  have huc := present_check hwc hu
  rw [ownObjs_attrTemplates vp] at ho
  rcases ht with rfl | rfl
  · obtain ⟨v, rfl⟩ := check_prim huc
    simp [attrObjs, memberObj, TObj.inst] at ho
    exact ⟨v, ho⟩
  · obtain ⟨ms, rfl, hms⟩ := check_set huc
    simp only [attrObjs, List.filterMap_map, List.mem_filterMap, Function.comp_def] at ho
    obtain ⟨m, hm, hmo⟩ := ho
    obtain ⟨v, rfl⟩ := check_prim (hms m hm)
    simp [memberObj, TObj.inst] at hmo
    exact ⟨v, hmo.symm⟩

/-- `sh:in`: an enumeration's objects, a set's members among them, are its members. -/
theorem shacl_in (H : RoundTrip K vp ord ι idOf G) (graph : Nat → String) {x : Nat × Name × VTerm T}
    (hx : x ∈ G.elems) {s : String} (hs : ι x.1 = some s) {c : CType T} (hc : K.find x.2.1 = some c)
    (hconc : c.concrete = true) {a : Name} {opt : Bool} {cs : List Name} {t : NTy T.P Name}
    (ht : t = NTy.enum cs ∨ t = .set (NTy.enum cs)) (hslot : (a, Catalog.Slot.attr opt t .plain) ∈ K.slots x.2.1) :
    ∀ o ∈ objectsOf (rdf K vp ι graph G) s (.attr a), ∃ m ∈ cs, o = .member m := by
  obtain ⟨w, hw, hwc⟩ := attr_value K H hx hc hconc hslot
  rw [attr_objects K H graph hx hs hc hslot, hw]
  simp only [some_bind']
  intro o ho
  obtain ⟨u, hu, ho⟩ := mem_optList.mp ho
  have huc := present_check hwc hu
  rw [ownObjs_attrTemplates vp] at ho
  have member : ∀ m : VTerm T, (denote G.lam m (NTy.enum cs)).isSome = true →
      ∃ n ∈ cs, m = .inj n (.record []) := by
    intro m hm
    obtain ⟨n, u', t', rfl, hmem, hu'⟩ := check_variant hm
    simp only [NTy.enum, List.mem_map, Prod.mk.injEq] at hmem
    obtain ⟨n', hn', rfl, rfl⟩ := hmem
    exact ⟨n', hn', by rw [check_unit hu']⟩
  rcases ht with rfl | rfl
  · obtain ⟨n, hn, rfl⟩ := member u huc
    simp [attrObjs, memberObj, TObj.inst] at ho
    exact ⟨n, hn, ho⟩
  · obtain ⟨ms, rfl, hms⟩ := check_set huc
    simp only [attrObjs, List.filterMap_map, List.mem_filterMap, Function.comp_def] at ho
    obtain ⟨m, hm, hmo⟩ := ho
    obtain ⟨n, hn, rfl⟩ := member m (hms m hm)
    simp [memberObj, TObj.inst] at hmo
    exact ⟨n, hn, hmo.symm⟩

/-- A participant's IRI carries a class its role admits. -/
theorem participant_class (H : RoundTrip K vp ord ι idOf G) (graph : Nat → String) {x : Nat × Name × VTerm T}
    (hx : x ∈ G.elems) {ps : List Name} {u : VTerm T} (hu : (denote G.lam u (K.participantType ps)).isSome = true)
    (hrefs : ∀ p ∈ u.refs, p ∈ x.2.2.refs) :
    ∃ p sp, u.refs = [p] ∧ ι p = some sp ∧ ∃ C ∈ classesOf (rdf K vp ι graph G) sp, C ∈ ps := by
  obtain ⟨p, sp, hr, hsp, _⟩ := participant_round K H graph hu fun q hq => H.parts x hx q (hrefs q hq)
  obtain ⟨y, hy, rfl, hfy, _⟩ := H.parts x hx p (hrefs p (by simp [hr]))
  obtain ⟨cy, hcy⟩ := Option.isSome_iff_exists.mp hfy
  obtain ⟨C, hC, hcontains⟩ := participant_admitted K hu hr (lam_of_mem H.nodup hy)
  refine ⟨y.1, sp, hr, hsp, C, ?_, hC⟩
  rw [classesOf_rdf K vp graph H.nodup H.inj hy hsp hcy]
  simpa using hcontains

/-- `sh:class` on a node-encoded relation's role: each role triple's participant carries a class the role admits. -/
theorem shacl_class (H : RoundTrip K vp ord ι idOf G) (graph : Nat → String) {x : Nat × Name × VTerm T}
    (hx : x ∈ G.elems) {s : String} (hs : ι x.1 = some s) {c : CType T} (hc : K.find x.2.1 = some c)
    (hconc : c.concrete = true) (hedge : K.edgeEncoded x.2.1 = false) {r owner : Name} {ps : List Name}
    {opt : Bool} (hslot : (r, Catalog.Slot.role ps opt owner) ∈ K.slots x.2.1) :
    ∀ o ∈ objectsOf (rdf K vp ι graph G) s (.role owner r),
      ∃ sp, o = .iri sp ∧ ∃ C ∈ classesOf (rdf K vp ι graph G) sp, C ∈ ps := by
  obtain ⟨nv, sp, hown⟩ := mem_slots_role K hslot
  have hrel := H.relations x hx c hc _ (List.mem_map_of_mem Prod.snd hown) rfl
  have hnd : ((K.ownedFields x.2.1).map fun y => y.2.name).Nodup := by
    have := H.namesUnique x hx
    simpa [Catalog.Catalog.allFields, List.map_map, Function.comp_def] using this
  obtain ⟨_, _, _, hall⟩ := slot_value K H hx hc hconc
  obtain ⟨w, hw, hwc⟩ := hall r _ hslot
  rw [objectsOf_rdf K vp graph H.nodup H.inj hx hs (p := .role owner r)
      (by simp [ownPred, declaresSet_false K H.fieldsUnique hown]),
    ownObjs_role_node K vp ι hc hrel hedge hnd hown, fieldRefs_of_field hw]
  intro o ho
  obtain ⟨p, hp, hpo⟩ := List.mem_filterMap.mp ho
  have hrefs : ∀ q ∈ w.refs, q ∈ x.2.2.refs := refs_field hw
  have key : ∀ u : VTerm T, (denote G.lam u (K.participantType ps)).isSome = true → u.refs = w.refs →
      ∃ sp, o = .iri sp ∧ ∃ C ∈ classesOf (rdf K vp ι graph G) sp, C ∈ ps := by
    intro u hu hur
    obtain ⟨p', sp', hr', hsp', hcls⟩ := participant_class K H graph hx hu (hur ▸ hrefs)
    rw [← hur, hr', List.mem_singleton] at hp
    subst hp
    rw [hsp'] at hpo
    simp only [Option.map_some', Option.some.injEq] at hpo
    exact ⟨sp', hpo.symm, hcls⟩
  cases opt with
  | false =>
    simp only [Catalog.Catalog.slotType, Bool.false_eq_true, if_false] at hwc
    exact key w hwc rfl
  | true =>
    simp only [Catalog.Catalog.slotType, if_true] at hwc
    rcases check_optional hwc with rfl | ⟨u, rfl, hu⟩
    · simp [absent, VTerm.refs, VTerm.refs.refsFields] at hp
    · exact key u hu (by simp [VTerm.refs])

/-- `sh:class` on a navigation property: every navigation triple's object carries a class a role of its relation
admits. -/
theorem shacl_class_edge (H : RoundTrip K vp ord ι idOf G) (graph : Nat → String) {q : Quad T}
    (hq : q ∈ rdf K vp ι graph G) {n : Name} (hp : q.p = .nav n) :
    ∃ y ∈ G.elems, ∃ so, q.o = .iri so ∧ ∃ f ∈ K.allFields y.2.1, f.isRole = true ∧
      ∃ C ∈ classesOf (rdf K vp ι graph G) so, C ∈ f.participants := by
  obtain ⟨y, hy, t, ht, htq⟩ := (mem_rdf K).mp hq
  obtain ⟨_, hqp, hto⟩ := inst_eq htq
  obtain ⟨cy, a, o, hcy, _, hedge, ha, ho, _, pa, hpa, po, hpo, _, hdir⟩ := nav_template K ht (hqp ▸ hp)
  have hconc : cy.concrete = true := by
    rcases H.labels y hy with ⟨c', hc', hconc⟩ | ⟨hnone, _⟩
    · rw [hcy] at hc'
      cases hc'
      exact hconc
    · rw [hcy] at hnone
      cases hnone
  obtain ⟨a₂, o₂, _, ha₂, ho₂, _, _, _, owa, psa, nva, spa, owo, pso, nvo, spo, hfa, hfo,
    wa, wo, _, _, _, _, _, _, hwa, hwo, _, _, _, _, _, _, _, _, _, _, _, _⟩ :=
    edge_roles K H graph hy hcy hconc hedge
  have haa : a₂ = a := Option.some.inj (ha₂.symm.trans ha)
  have hoo : o₂ = o := Option.some.inj (ho₂.symm.trans ho)
  rw [haa] at hfa hwa
  rw [hoo] at hfo hwo
  obtain ⟨_, _, _, hall⟩ := slot_value K H hy hcy hconc
  obtain ⟨wa', hwa', hwac⟩ := hall a _ (mem_slots_of_role K hfa)
  obtain ⟨wo', hwo', hwoc⟩ := hall o _ (mem_slots_of_role K hfo)
  rw [hwa] at hwa'
  cases hwa'
  rw [hwo] at hwo'
  cases hwo'
  simp only [Catalog.Catalog.slotType, Bool.false_eq_true, if_false] at hwac hwoc
  rw [fieldRefs_of_field hwa] at hpa
  rw [fieldRefs_of_field hwo] at hpo
  have hroleA : Field.role a psa false nva spa ∈ K.allFields y.2.1 := List.mem_map_of_mem Prod.snd hfa
  have hroleO : Field.role o pso false nvo spo ∈ K.allFields y.2.1 := List.mem_map_of_mem Prod.snd hfo
  rcases hdir with ⟨_, hto'⟩ | ⟨_, _, hto'⟩
  · obtain ⟨p, sp, hr, hsp, hcls⟩ := participant_class K H graph hy hwoc (refs_field hwo)
    rw [hr, List.mem_singleton] at hpo
    subst hpo
    rw [hto'] at hto
    simp only [TObj.inst, hsp, Option.map_some', Option.some.injEq] at hto
    exact ⟨y, hy, sp, hto.symm, _, hroleO, rfl, hcls⟩
  · obtain ⟨p, sp, hr, hsp, hcls⟩ := participant_class K H graph hy hwac (refs_field hwa)
    rw [hr, List.mem_singleton] at hpa
    subst hpa
    rw [hto'] at hto
    simp only [TObj.inst, hsp, Option.map_some', Option.some.injEq] at hto
    exact ⟨y, hy, sp, hto.symm, _, hroleA, rfl, hcls⟩

end ApgModel.Views
