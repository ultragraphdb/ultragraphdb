/-
 SPDX-License-Identifier: Apache-2.0
 Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
-/
import ApgModel.Views.RdfInverse

/-!
# Proposition 17.3: the RDF view's round trip

`SCHEMA-APG.md` proposition 17.3: on valid APGs on `S(K)`, `R⁻¹(R(G)) = G`, each element identified by its IRI, its
triple or its pair.

The proof reads `R(G)` back element by element, under the conditions `RoundTrip` collects, each a rule of
`SCHEMA.md`:

- **Attribution.** A quad's predicate tells whose quad it is: `rdf:type`, an attribute, an interval bound, the built-in
  fields, `rdf:reifies` and a single-valued role's predicate belong to their subject (`ownPred`), and every template
  with such a predicate has its element as its subject (`templates_subject`). Under an injective naming, the quads with
  subject `ι(x)` and such a predicate are `x`'s own (`objectsOf_rdf`).
- **Labels.** An element's type objects are its chain, whose head is its label (`labelOf_rdf`).
- **Slots.** Each slot reads back its value from its own quads, inverting the check against its type: an attribute's
  literals (`decode_attr`), an interval's bounds and precisions, a hidden precision its type's own (`decode_interval`),
  `asserted` (`decode_asserted`), and a role's participant from its role triple or the reifier's triple term
  (`decode_role`); the record of the slots is the element's value (`decodeElem_round`).
- **Identities.** An element named by an IRI is read back from its type quads (`iri_complete`, `iri_sound`), one of a
  role label from its role triple (`pair_complete`, `pair_sound`), and one named by a triple from its navigation quad,
  its relation the one its navigation name and its participants' types determine, a `@symmetric` relation's reverse
  triple the same element in IRI order (`triple_complete`, `triple_sound`).

`rdf_round_trip` is proposition 17.3: `R⁻¹(R(G))` holds exactly `G`'s elements, each under its identity.
-/

namespace ApgModel.Views

open Named Catalog

variable {T : Typeside} (K : Catalog T)

/-! ## Attribution -/

/-- The type `owner` declares a set-valued role `r`. -/
def declaresSet (owner r : Name) : Bool :=
  match K.find owner with
  | some c => c.fields.any fun f =>
    match f with
    | .setRole m _ _ => m == r
    | _ => false
  | none => false

/-- A predicate whose quads belong to their subject: every predicate but a navigation name and a set-valued role's. -/
def ownPred : Pred → Bool
  | .nav _ => false
  | .role owner r => !declaresSet K owner r
  | _ => true

/-- A foldl that appends keeps an invariant every appended piece holds. -/
theorem foldl_append_inv {α β : Type} (P : α → Prop) (F : List α → β → List α) (hF : ∀ acc m, ∀ y ∈ F acc m, P y) :
    (ms : List β) → (acc : List α) → (∀ y ∈ acc, P y) → ∀ y ∈ ms.foldl (fun acc m => acc ++ F acc m) acc, P y
  | [], _, h => h
  | m :: ms, acc, h => foldl_append_inv P F hF ms (acc ++ F acc m) fun y hy => by
    rcases List.mem_append.mp hy with hy | hy
    · exact h y hy
    · exact hF acc m y hy

/-- A field `n` holds with its declaring type was declared by that type. -/
theorem mem_ownedFields {n m : Name} {f : Field T} (h : (m, f) ∈ K.ownedFields n) :
    ∃ c, K.find m = some c ∧ f ∈ c.fields := by
  have := foldl_append_inv (fun y : Name × Field T => ∃ c, K.find y.1 = some c ∧ y.2 ∈ c.fields)
    (fun acc m => ((((K.find m).map CType.fields).getD []).filter fun f =>
      !(acc.filterMap fun f => f.2.specializes).contains f.name).map fun f => (m, f))
    (by
      intro acc m y hy
      simp only [List.mem_map, List.mem_filter] at hy
      obtain ⟨f, ⟨hf, _⟩, rfl⟩ := hy
      cases hc : K.find m with
      | none => simp [hc] at hf
      | some c => exact ⟨c, rfl, by simpa [hc] using hf⟩)
    (K.chain n) [] (fun _ h => nomatch h) (m, f) h
  exact this

/-- A role label's role is a set-valued role its declaring type declares. -/
theorem roleLabelOf_declares {l : Name} {x : Name × Name × Name × List Name} (h : K.roleLabelOf l = some x) :
    declaresSet K x.2.1 x.2.2.1 = true := by
  unfold Catalog.roleLabelOf at h
  obtain ⟨c, _, hc⟩ := List.exists_of_findSome?_eq_some h
  split at hc
  · obtain ⟨y, hy, hyx⟩ := List.exists_of_findSome?_eq_some hc
    match y, hy, hyx with
    | (owner, .setRole r ps sp), hy, hyx =>
      simp only at hyx
      split at hyx
      · cases hyx
        obtain ⟨d, hd, hf⟩ := mem_ownedFields K hy
        simp only [declaresSet, hd, List.any_eq_true]
        exact ⟨_, hf, by simp⟩
      · cases hyx
    | (_, .attr _ _ _ _), _, hyx => cases hyx
    | (_, .role _ _ _ _ _), _, hyx => cases hyx
  · cases hc

/-! ## The shapes of the templates -/

@[simp] theorem all_optList {α β : Type} (o : Option α) (k : α → List β) (p : β → Bool) :
    (optList o k).all p = o.all fun a => (k a).all p := by
  cases o <;> rfl

@[simp] theorem option_all_true {α : Type} (o : Option α) : (o.all fun _ => true) = true := by
  cases o <;> rfl

@[simp] theorem all_ite_nil {α : Type} (c : Prop) [Decidable c] (l : List α) (p : α → Bool) :
    (if c then l else []).all p = if c then l.all p else true := by
  split <;> rfl

@[simp] theorem list_all_true {α : Type} (l : List α) : (l.all fun _ => true) = true := by
  induction l <;> simp_all

@[simp] theorem all_map' {α β : Type} (l : List α) (f : α → β) (p : β → Bool) :
    (l.map f).all p = l.all fun x => p (f x) := by
  induction l <;> simp_all

theorem mem_optList {α β : Type} {o : Option α} {k : α → List β} {b : β} :
    b ∈ optList o k ↔ ∃ a, o = some a ∧ b ∈ k a := by
  cases o <;> simp [optList]

/-- An attribute's predicates: the attribute's own and its interval bounds'. -/
def attrPred (a : Name) : Pred → Bool
  | .attr b => b == a
  | .bound b _ => b == a
  | _ => false

/-- The built-in fields' predicates. -/
def assertedPred : Pred → Bool
  | .assertedStart | .assertedEnd | .live => true
  | _ => false

/-- A navigation name's predicate. -/
def isNav : Pred → Bool
  | .nav _ => true
  | _ => false

theorem typeTemplates_shape {e : Nat} {l : Name} {t : Tmpl T} (h : t ∈ typeTemplates K e l) :
    t.s = e ∧ t.p = .rdfType := by
  simp only [typeTemplates, List.mem_map] at h
  obtain ⟨_, _, rfl⟩ := h
  exact ⟨rfl, rfl⟩

theorem attrTemplates_shape (vp : ViewParams T) {e : Nat} {a : Name} {view : AttrView} {u : VTerm T} {t : Tmpl T}
    (h : t ∈ attrTemplates vp e a view u) : t.s = e ∧ attrPred a t.p = true := by
  have hall : (attrTemplates vp e a view u).all (fun t => t.s == e && attrPred a t.p) = true := by
    cases view <;> simp [attrTemplates, attrPred, List.all_map, List.all_append, Function.comp_def]
  have := List.all_eq_true.mp hall t h
  simpa using this

theorem assertedTemplates_shape {e : Nat} {v : VTerm T} {t : Tmpl T} (h : t ∈ assertedTemplates e v) :
    t.s = e ∧ assertedPred t.p = true := by
  have hall : (assertedTemplates e v).all (fun t => t.s == e && assertedPred t.p) = true := by
    simp [assertedTemplates, assertedFrom, assertedPred, List.all_map, List.all_append, Function.comp_def]
  have := List.all_eq_true.mp hall t h
  simpa using this

theorem edgeTemplates_shape {e : Nat} {l : Name} {v : VTerm T} {t : Tmpl T} (h : t ∈ edgeTemplates K e l v) :
    isNav t.p = true ∨ (t.p = .reifies ∧ t.s = e) := by
  have hall : (edgeTemplates K e l v).all (fun t => isNav t.p || (t.p == .reifies && t.s == e)) = true := by
    simp [edgeTemplates, isNav, List.all_flatMap, List.all_append]
  have := List.all_eq_true.mp hall t h
  simpa using this

theorem nodeTemplates_shape {e : Nat} {l : Name} {v : VTerm T} {t : Tmpl T} (h : t ∈ nodeTemplates K e l v) :
    t.s = e ∧ ∃ owner r ps o nv sp, t.p = .role owner r ∧ (owner, .role r ps o nv sp) ∈ K.ownedFields l := by
  simp only [nodeTemplates, List.mem_flatMap] at h
  obtain ⟨x, hx, ht⟩ := h
  match x, hx, ht with
  | (owner, .role r ps o nv sp), hx, ht =>
    simp only [List.mem_map] at ht
    obtain ⟨_, _, rfl⟩ := ht
    exact ⟨rfl, owner, r, ps, o, nv, sp, rfl, hx⟩
  | (_, .attr _ _ _ _), _, ht => simp at ht
  | (_, .setRole _ _ _), _, ht => simp at ht

theorem roleLabelTemplates_shape {e : Nat} {l : Name} {v : VTerm T} {t : Tmpl T}
    (h : t ∈ roleLabelTemplates K e l v) : ∃ x, K.roleLabelOf l = some x ∧ t.p = .role x.2.1 x.2.2.1 := by
  simp only [roleLabelTemplates, mem_optList, List.mem_flatMap, List.mem_map] at h
  obtain ⟨x, hx, _, _, _, _, rfl⟩ := h
  exact ⟨x, hx, rfl⟩

/-- Field names are unique within each declared type (`UGS0222`, `UGS0404`, `UGS0417`). -/
def FieldsUnique : Prop := ∀ c ∈ K.types, (c.fields.map Field.name).Nodup

theorem eq_of_nodup_name {l : List (Field T)} (hnd : (l.map Field.name).Nodup) {f g : Field T} (hf : f ∈ l)
    (hg : g ∈ l) (hn : f.name = g.name) : f = g := by
  induction l with
  | nil => cases hf
  | cons x xs ih =>
    rw [List.map_cons, List.nodup_cons] at hnd
    cases hf with
    | head =>
      cases hg with
      | head => rfl
      | tail _ hg => exact absurd (by rw [hn]; exact List.mem_map_of_mem Field.name hg) hnd.1
    | tail _ hf =>
      cases hg with
      | head => exact absurd (by rw [← hn]; exact List.mem_map_of_mem Field.name hf) hnd.1
      | tail _ hg => exact ih hnd.2 hf hg

/-- A single-valued role's predicate is no set-valued role's. -/
theorem declaresSet_false (hK : FieldsUnique K) {l owner r : Name} {ps : List Name} {o : Bool} {nv sp : Option Name}
    (h : (owner, .role r ps o nv sp) ∈ K.ownedFields l) : declaresSet K owner r = false := by
  obtain ⟨c, hc, hf⟩ := mem_ownedFields K h
  have hnd := hK c (List.mem_of_find?_eq_some hc)
  simp only [declaresSet, hc, List.any_eq_false]
  intro g hg
  match g, hg with
  | .setRole m ps' sp', hg =>
    simp only [beq_iff_eq]
    intro hm
    have := eq_of_nodup_name hnd hf hg (by simp [Field.name, hm])
    cases this
  | .attr _ _ _ _, _ => simp
  | .role _ _ _ _ _, _ => simp

/-- Every template with a predicate that belongs to its subject has its element as its subject. -/
theorem templates_subject (vp : ViewParams T) {e : Nat} {l : Name} {v : VTerm T} {t : Tmpl T}
    (ht : t ∈ templates K vp e l v) (hp : ownPred K t.p = true) : t.s = e := by
  unfold templates at ht
  split at ht
  · next c _ =>
    simp only [List.mem_append] at ht
    rcases ht with ((ht | ht) | ht) | ht
    · exact (typeTemplates_shape K ht).1
    · simp only [attrsTemplates, List.mem_flatMap] at ht
      obtain ⟨f, _, ht⟩ := ht
      match f, ht with
      | .attr a _ opt view, ht =>
        obtain ⟨u, _, ht⟩ := mem_optList.mp ht
        exact (attrTemplates_shape vp ht).1
      | .role _ _ _ _ _, ht => cases ht
      | .setRole _ _ _, ht => cases ht
    · split at ht
      · exact (assertedTemplates_shape ht).1
      · cases ht
    · unfold relTemplates at ht
      split at ht
      · split at ht
        · rcases edgeTemplates_shape K ht with h | h
          · cases hq : t.p <;> simp_all [isNav, ownPred]
          · exact h.2
        · exact (nodeTemplates_shape K ht).1
      · cases ht
  · obtain ⟨x, hx, hpx⟩ := roleLabelTemplates_shape K ht
    have := roleLabelOf_declares K hx
    simp [hpx, ownPred, this] at hp

/-! ## The quads of one element -/

/-- The objects of a template list's templates with the predicate `p`, named. -/
def ownObjs (ι : Nat → Option String) (p : Pred) (ts : List (Tmpl T)) : List (Term T) :=
  (ts.filter fun t => t.p == p).filterMap fun t => t.o.inst ι

theorem objectsOf_flatMap {α : Type} (L : List α) (f : α → List (Quad T)) (s : String) (p : Pred) :
    objectsOf (L.flatMap f) s p = L.flatMap fun y => objectsOf (f y) s p := by
  induction L with
  | nil => rfl
  | cons y L ih =>
    simp only [objectsOf] at ih ⊢
    simp only [List.flatMap_cons, List.filter_append, List.map_append, ih]

theorem objectsOf_cons (q : Quad T) (Q : List (Quad T)) (s : String) (p : Pred) :
    objectsOf (q :: Q) s p = (if q.s = s ∧ q.p = p then [q.o] else []) ++ objectsOf Q s p := by
  simp only [objectsOf, List.filter_cons, Bool.and_eq_true, beq_iff_eq]
  split <;> rfl

/-- The named templates of one element, read at the subject `s` under one of its own predicates: its own objects
when it is named `s`, else none. -/
theorem objectsOf_inst (ι : Nat → Option String) (graph : Nat → String) (p : Pred) (e : Nat) (s : String) :
    (ts : List (Tmpl T)) → (∀ t ∈ ts, t.p = p → t.s = e) →
      objectsOf (ts.filterMap (Tmpl.inst ι graph)) s p = if ι e = some s then ownObjs ι p ts else []
  | [], _ => by simp [objectsOf, ownObjs]
  | t :: ts, h => by
    have ih := objectsOf_inst ι graph p e s ts fun t' ht' => h t' (List.mem_cons_of_mem _ ht')
    have hown : ownObjs ι p (t :: ts) = (if t.p = p then (t.o.inst ι).toList else []) ++ ownObjs ι p ts := by
      by_cases htp : t.p = p <;> cases ho : t.o.inst ι <;> simp [ownObjs, htp, ho, List.filter_cons]
    rw [List.filterMap_cons]
    cases hq : Tmpl.inst ι graph t with
    | none =>
      simp only []
      rw [ih, hown]
      by_cases htp : t.p = p
      · by_cases hes : ι e = some s
        · have hts := h t (List.mem_cons_self _ _) htp
          have : t.o.inst ι = none := by
            cases ho : t.o.inst ι with
            | none => rfl
            | some o => simp [Tmpl.inst, hts, hes, ho] at hq
          simp [hes, htp, this]
        · simp [hes]
      · simp [htp]
    | some q =>
      simp only []
      rw [objectsOf_cons, ih, hown]
      simp only [Tmpl.inst, Option.bind_eq_some, Option.map_eq_some'] at hq
      obtain ⟨s0, hs0, o, ho, rfl⟩ := hq
      by_cases htp : t.p = p
      · have hts := h t (List.mem_cons_self _ _) htp
        rw [hts] at hs0
        by_cases h0 : s0 = s
        · subst h0
          simp [hs0, htp, ho]
        · have : ι e ≠ some s := by rw [hs0]; simpa using h0
          simp [h0, this]
      · simp [htp]

/-- A flatMap over a list with distinct first components, nonempty only at `x`, is its value at `x`. -/
theorem flatMap_single {α β : Type} {L : List (Nat × α)} (hnd : (L.map Prod.fst).Nodup) {x : Nat × α} (hx : x ∈ L)
    {g : Nat × α → List β} (hg : ∀ y ∈ L, g y ≠ [] → y.1 = x.1) : L.flatMap g = g x := by
  induction L with
  | nil => cases hx
  | cons y L ih =>
    rw [List.map_cons, List.nodup_cons] at hnd
    simp only [List.flatMap_cons]
    rcases List.mem_cons.mp hx with rfl | hx
    · have : L.flatMap g = [] := by
        rw [List.flatMap_eq_nil_iff]
        intro z hz
        cases hgz : g z with
        | nil => rfl
        | cons _ _ =>
          have := hg z (List.mem_cons_of_mem _ hz) (by simp [hgz])
          exact absurd (by rw [← this]; exact List.mem_map_of_mem _ hz) hnd.1
      rw [this, List.append_nil]
    · have hy : g y = [] := by
        cases hgy : g y with
        | nil => rfl
        | cons _ _ =>
          have := hg y (List.mem_cons_self _ _) (by simp [hgy])
          exact absurd (by rw [this]; exact List.mem_map_of_mem _ hx) hnd.1
      rw [hy, List.nil_append]
      exact ih hnd.2 hx fun z hz => hg z (List.mem_cons_of_mem _ hz)

variable (vp : ViewParams T)

/-- Attribution: under a naming injective on the elements, the quads with subject `ι(x)` and one of `x`'s own
predicates are `x`'s templates' objects, named. -/
theorem objectsOf_rdf {ι : Nat → Option String} (graph : Nat → String) {G : FinApg K.schema}
    (hnd : (G.elems.map Prod.fst).Nodup)
    (hinj : ∀ x ∈ G.elems, ∀ y ∈ G.elems, ∀ s, ι x.1 = some s → ι y.1 = some s → x.1 = y.1)
    {x : Nat × Name × VTerm T} (hx : x ∈ G.elems) {s : String} (hs : ι x.1 = some s) {p : Pred}
    (hp : ownPred K p = true) :
    objectsOf (rdf K vp ι graph G) s p = ownObjs ι p (templates K vp x.1 x.2.1 x.2.2) := by
  unfold rdf
  rw [objectsOf_flatMap]
  have each : ∀ y ∈ G.elems, objectsOf ((templates K vp y.1 y.2.1 y.2.2).filterMap (Tmpl.inst ι graph)) s p =
      if ι y.1 = some s then ownObjs ι p (templates K vp y.1 y.2.1 y.2.2) else [] := fun y _ =>
    objectsOf_inst ι graph p y.1 s _ fun t ht htp => templates_subject K vp ht (htp ▸ hp)
  rw [flatMap_single hnd hx (g := fun y => objectsOf ((templates K vp y.1 y.2.1 y.2.2).filterMap
      (Tmpl.inst ι graph)) s p)]
  · rw [each x hx, if_pos hs]
  · intro y hy hne
    rw [each y hy] at hne
    split at hne
    · next hys => exact hinj y hy x hx s hys hs
    · exact absurd rfl hne

/-! ## An element's label -/

theorem ownObjs_append (ι : Nat → Option String) (p : Pred) (ts ts' : List (Tmpl T)) :
    ownObjs ι p (ts ++ ts') = ownObjs ι p ts ++ ownObjs ι p ts' := by
  simp [ownObjs, List.filter_append, List.filterMap_append]

theorem ownObjs_nil (ι : Nat → Option String) (p : Pred) (ts : List (Tmpl T)) (h : ∀ t ∈ ts, t.p ≠ p) :
    ownObjs ι p ts = [] := by
  simp only [ownObjs]
  rw [List.filter_eq_nil_iff.mpr (fun t ht => by simpa using h t ht)]
  rfl

theorem ownObjs_all (ι : Nat → Option String) (p : Pred) (ts : List (Tmpl T)) (h : ∀ t ∈ ts, t.p = p) :
    ownObjs ι p ts = ts.filterMap fun t => t.o.inst ι := by
  simp only [ownObjs]
  rw [List.filter_eq_self.mpr (fun t ht => by simpa using h t ht)]

/-- An element's templates other than its types, attributes and built-in fields: its relation's. -/
theorem ownObjs_rel_nil (ι : Nat → Option String) {c : CType T} {e : Nat} {l : Name} {v : VTerm T} {p : Pred}
    (hp : isNav p = false) (hr : p ≠ .reifies) (hrole : ∀ owner r, p ≠ .role owner r) :
    ownObjs ι p (relTemplates K c e l v) = [] := by
  apply ownObjs_nil
  intro t ht htp
  unfold relTemplates at ht
  split at ht
  · split at ht
    · rcases edgeTemplates_shape K ht with h | h
      · rw [htp] at h
        rw [h] at hp
        cases hp
      · exact hr (htp ▸ h.1)
    · obtain ⟨_, owner, r, _, _, _, _, h, _⟩ := nodeTemplates_shape K ht
      exact hrole owner r (htp ▸ h)
  · cases ht

theorem chain_head (l : Name) : ∃ rest, K.chain l = l :: rest := ⟨_, rfl⟩

/-- An element's own type objects are its chain's classes. -/
theorem ownObjs_rdfType (ι : Nat → Option String) {c : CType T} {e : Nat} {l : Name} {v : VTerm T}
    (hc : K.find l = some c) :
    ownObjs ι .rdfType (templates K vp e l v) = (K.chain l).map Term.cls := by
  unfold templates
  rw [hc]
  simp only []
  rw [ownObjs_append, ownObjs_append, ownObjs_append,
    ownObjs_all ι _ _ (fun t ht => (typeTemplates_shape K ht).2)]
  have h1 : ownObjs ι .rdfType (attrsTemplates K vp e l v) = [] := by
    apply ownObjs_nil
    intro t ht htp
    simp only [attrsTemplates, List.mem_flatMap] at ht
    obtain ⟨f, _, ht⟩ := ht
    match f, ht with
    | .attr a _ opt view, ht =>
      obtain ⟨u, _, ht⟩ := mem_optList.mp ht
      have := (attrTemplates_shape vp ht).2
      rw [htp] at this
      cases this
    | .role _ _ _ _ _, ht => cases ht
    | .setRole _ _ _, ht => cases ht
  have h2 : ownObjs ι .rdfType (if K.inherits CType.bitemporal l = true then assertedTemplates e v else []) = [] := by
    apply ownObjs_nil
    intro t ht htp
    split at ht
    · have := (assertedTemplates_shape ht).2
      rw [htp] at this
      cases this
    · cases ht
  rw [h1, h2, ownObjs_rel_nil K ι (p := .rdfType) rfl (by simp) (by simp)]
  simp only [typeTemplates, List.filterMap_map, Function.comp_def, TObj.inst, List.append_nil]
  generalize K.chain l = cs
  induction cs <;> simp_all [List.filterMap_cons]

/-- The classes read at an element's IRI are its chain: its type and every supertype and statement type above it. -/
theorem classesOf_rdf {ι : Nat → Option String} (graph : Nat → String) {G : FinApg K.schema}
    (hnd : (G.elems.map Prod.fst).Nodup)
    (hinj : ∀ x ∈ G.elems, ∀ y ∈ G.elems, ∀ s, ι x.1 = some s → ι y.1 = some s → x.1 = y.1)
    {x : Nat × Name × VTerm T} (hx : x ∈ G.elems) {s : String} (hs : ι x.1 = some s) {c : CType T}
    (hc : K.find x.2.1 = some c) : classesOf (rdf K vp ι graph G) s = K.chain x.2.1 := by
  unfold classesOf
  rw [objectsOf_rdf K vp graph hnd hinj hx hs (p := .rdfType) rfl, ownObjs_rdfType K vp ι hc, List.filterMap_map]
  simp [Function.comp_def]

/-- The classes read at an element's IRI are its chain, so its label is read back. -/
theorem labelOf_rdf {ι : Nat → Option String} (graph : Nat → String) {G : FinApg K.schema}
    (hnd : (G.elems.map Prod.fst).Nodup)
    (hinj : ∀ x ∈ G.elems, ∀ y ∈ G.elems, ∀ s, ι x.1 = some s → ι y.1 = some s → x.1 = y.1)
    {x : Nat × Name × VTerm T} (hx : x ∈ G.elems) {s : String} (hs : ι x.1 = some s) {c : CType T}
    (hc : K.find x.2.1 = some c) : labelOf K (rdf K vp ι graph G) s = some x.2.1 := by
  have hcls := classesOf_rdf K vp graph hnd hinj hx hs hc
  obtain ⟨rest, hrest⟩ := chain_head K x.2.1
  unfold labelOf
  simp only [hcls]
  rw [hrest]
  apply List.find?_cons_of_pos
  show ((x.2.1 :: rest).all fun c' => (K.chain x.2.1).contains c') = true
  rw [hrest]
  exact List.all_eq_true.mpr fun y hy => by simpa using hy

/-! ## Inverting the check -/

/-- A type with no label inside, whose values hold no reference. -/
def refFree : NTy T.P Name → Bool
  | .prim _ => true
  | .lbl _ => false
  | .record fs => all fs
  | .variant fs => all fs
  | .list t => refFree t
  | .set t => refFree t
where
  /-- Every field's type is reference-free. -/
  all : List (Name × NTy T.P Name) → Bool
    | [] => true
    | (_, t) :: fs => refFree t && all fs

theorem refFree_fieldAt : (fs : List (Name × NTy T.P Name)) → refFree.all fs = true → (i : Fin fs.length) →
    refFree (NTy.fieldAt fs i) = true
  | [], _, i => i.elim0
  | (_, _) :: _, h, ⟨0, _⟩ => by simp [refFree.all] at h; exact h.1
  | _ :: fs, h, ⟨i + 1, hi⟩ => by
    simp only [refFree.all, Bool.and_eq_true] at h
    exact refFree_fieldAt fs h.2 ⟨i, Nat.lt_of_succ_lt_succ hi⟩

theorem mem_of_fieldIndex : (fs : List (Name × NTy T.P Name)) → (n : Name) → (i : Fin fs.length) →
    NTy.fieldIndex fs n = some i → (n, NTy.fieldAt fs i) ∈ fs
  | [], _, i, _ => i.elim0
  | (m, t) :: fs, n, i, h => by
    simp only [NTy.fieldIndex] at h
    split at h
    · next hm =>
      cases h
      subst hm
      exact List.mem_cons_self _ _
    · obtain ⟨j, hj, rfl⟩ := Option.map_eq_some'.mp h
      exact List.mem_cons_of_mem _ (mem_of_fieldIndex fs n j hj)

variable [DecidableEq T.P]

/-- A term that checks against a reference-free type holds no reference. -/
theorem refs_nil (lam : Nat → Option Name) : (w : VTerm T) → (t : NTy T.P Name) → (denote lam w t).isSome = true →
    refFree t = true → w.refs = []
  | .prim _ _, _, _, _ => rfl
  | .elem _, t, h, hr => by
    cases t with
    | lbl _ => simp [refFree] at hr
    | _ => simp [denote] at h
  | .record vs, t, h, hr => by
    cases t with
    | record fs => exact fields vs fs h hr
    | _ => simp [denote] at h
  | .inj n u, t, h, hr => by
    cases t with
    | variant fs =>
      simp only [denote] at h
      split at h
      · next i hi =>
        simp only [Option.isSome_map'] at h
        exact refs_nil lam u _ h (refFree_fieldAt fs hr i)
      · simp at h
    | _ => simp [denote] at h
  | .list vs, t, h, hr => by
    cases t with
    | list t => exact all vs t h hr
    | _ => simp [denote] at h
  | .set vs, t, h, hr => by
    cases t with
    | set t =>
      simp only [denote, Option.isSome_map'] at h
      exact all vs t h hr
    | _ => simp [denote] at h
where
  fields : (vs : List (Name × VTerm T)) → (fs : List (Name × NTy T.P Name)) →
      (denote.denoteRec lam vs fs).isSome = true → refFree.all fs = true → VTerm.refs.refsFields vs = []
    | [], _, _, _ => rfl
    | (_, _) :: _, [], h, _ => by simp [denote.denoteRec] at h
    | (n, v) :: vs, (m, t) :: fs, h, hr => by
      simp only [denote.denoteRec] at h
      split at h
      · split at h
        · next x f hx hf =>
          simp only [refFree.all, Bool.and_eq_true] at hr
          simp only [VTerm.refs.refsFields, List.append_eq_nil]
          exact ⟨refs_nil lam v t (by simp [hx]) hr.1, fields vs fs (by simp [hf]) hr.2⟩
        · simp at h
      · simp at h
  all : (vs : List (VTerm T)) → (t : NTy T.P Name) → (denote.denoteAll lam vs t).isSome = true →
      refFree t = true → VTerm.refs.refsAll vs = []
    | [], _, _, _ => rfl
    | v :: vs, t, h, hr => by
      simp only [denote.denoteAll] at h
      split at h
      · next x xs hx hxs =>
        simp only [VTerm.refs.refsAll, List.append_eq_nil]
        exact ⟨refs_nil lam v t (by simp [hx]) hr, all vs t (by simp [hxs]) hr⟩
      · simp at h

theorem check_prim {lam : Nat → Option Name} {w : VTerm T} {p : T.P} (h : (denote lam w (.prim p)).isSome = true) :
    ∃ a, w = .prim p a := by
  cases w with
  | prim q a =>
    simp only [denote] at h
    split at h
    · next hq => subst hq; exact ⟨a, rfl⟩
    · simp at h
  | _ => simp [denote] at h

theorem check_record {lam : Nat → Option Name} {w : VTerm T} {fs : List (Name × NTy T.P Name)}
    (h : (denote lam w (.record fs)).isSome = true) :
    ∃ ws, w = .record ws ∧ (denote.denoteRec lam ws fs).isSome = true := by
  cases w with
  | record ws => exact ⟨ws, rfl, h⟩
  | _ => simp [denote] at h

theorem check_variant {lam : Nat → Option Name} {w : VTerm T} {fs : List (Name × NTy T.P Name)}
    (h : (denote lam w (.variant fs)).isSome = true) :
    ∃ n u t, w = .inj n u ∧ (n, t) ∈ fs ∧ (denote lam u t).isSome = true := by
  cases w with
  | inj n u =>
    simp only [denote] at h
    split at h
    · next i hi =>
      simp only [Option.isSome_map'] at h
      exact ⟨n, u, _, rfl, mem_of_fieldIndex fs n i hi, h⟩
    · simp at h
  | _ => simp [denote] at h

theorem check_set {lam : Nat → Option Name} {w : VTerm T} {t : NTy T.P Name}
    (h : (denote lam w (.set t)).isSome = true) : ∃ ms, w = .set ms ∧ ∀ m ∈ ms, (denote lam m t).isSome = true := by
  cases w with
  | set ms =>
    refine ⟨ms, rfl, ?_⟩
    simp only [denote, Option.isSome_map'] at h
    exact all ms h
  | _ => simp [denote] at h
where
  all : (ms : List (VTerm T)) → (denote.denoteAll lam ms t).isSome = true → ∀ m ∈ ms, (denote lam m t).isSome = true
    | [], _, _, hm => by cases hm
    | v :: vs, h, m, hm => by
      simp only [denote.denoteAll] at h
      split at h
      · next x xs hx hxs =>
        rcases List.mem_cons.mp hm with rfl | hm
        · simp [hx]
        · exact all vs (by simp [hxs]) m hm
      · simp at h

theorem check_unit {lam : Nat → Option Name} {w : VTerm T} (h : (denote lam w (.record [])).isSome = true) :
    w = .record [] := by
  obtain ⟨ws, rfl, hws⟩ := check_record h
  cases ws with
  | nil => rfl
  | cons _ _ => simp [denote.denoteRec] at hws

/-- A term that checks against an optional type is absent or present with a value of the type. -/
theorem check_optional {lam : Nat → Option Name} {w : VTerm T} {t : NTy T.P Name}
    (h : (denote lam w (NTy.optional t)).isSome = true) :
    w = absent ∨ ∃ u, w = .inj "some" u ∧ (denote lam u t).isSome = true := by
  obtain ⟨n, u, t', rfl, hm, hu⟩ := check_variant h
  simp only [NTy.optional, NTy.unit, List.mem_cons, Prod.mk.injEq, List.mem_singleton, List.not_mem_nil,
    or_false] at hm
  rcases hm with ⟨rfl, rfl⟩ | ⟨rfl, rfl⟩
  · left
    rw [check_unit hu]
    rfl
  · exact Or.inr ⟨u, rfl, hu⟩

/-! ## Reading attribute values -/

omit [DecidableEq T.P] in
/-- A member's object, named, reads back as the member, which holds no reference. -/
theorem inst_memberObj (ι : Nat → Option String) {w : VTerm T} (hw : w.refs = []) :
    ((memberObj w).inst ι).map termValue = some w := by
  have hj : ∀ v : VTerm T, v.refs = [] → (TObj.json v).inst ι = some (.json v) := fun v hv => by
    simp [TObj.inst, hv]
  cases w with
  | prim p a => rfl
  | elem e => simp [VTerm.refs] at hw
  | record fs => simp only [memberObj, hj _ hw, Option.map_some', termValue]
  | inj m u =>
    cases u with
    | record fs =>
      cases fs with
      | nil => rfl
      | cons f fs => simp only [memberObj, hj _ hw, Option.map_some', termValue]
    | _ => simp only [memberObj, hj _ hw, Option.map_some', termValue]
  | list vs => simp only [memberObj, hj _ hw, Option.map_some', termValue]
  | set vs => simp only [memberObj, hj _ hw, Option.map_some', termValue]

omit [DecidableEq T.P] in
theorem members_round (ι : Nat → Option String) : (ms : List (VTerm T)) → (∀ m ∈ ms, m.refs = []) →
    ((ms.map memberObj).filterMap fun o => o.inst ι).map termValue = ms
  | [], _ => rfl
  | m :: ms, h => by
    have hm := inst_memberObj ι (h m (List.mem_cons_self _ _))
    obtain ⟨o, ho, hto⟩ := Option.map_eq_some'.mp hm
    simp only [List.map_cons, List.filterMap_cons, ho, List.map_cons, hto]
    rw [members_round ι ms fun m' hm' => h m' (List.mem_cons_of_mem _ hm')]

/-- An attribute's present value read back from its objects: a set's members in the canonical order, any other value
from its one object. -/
theorem plainValue_round (lam : Nat → Option Name) (ι : Nat → Option String) (ord : VTerm T → VTerm T → Bool)
    {t : NTy T.P Name} {w : VTerm T} (hw : (denote lam w t).isSome = true) (hr : refFree t = true)
    (hcanon : ∀ ms, w = .set ms → sortOrd ord ms = ms) :
    plainValue ord t ((attrObjs w).filterMap fun o => o.inst ι) = w := by
  cases t with
  | set t =>
    obtain ⟨ms, rfl, hms⟩ := check_set hw
    simp only [plainValue, attrObjs]
    rw [members_round ι ms fun m hm => refs_nil lam m t (hms m hm) hr, hcanon ms rfl]
  | lbl _ => simp [refFree] at hr
  | _ =>
    have hwr := refs_nil lam w _ hw hr
    have hm := inst_memberObj ι hwr
    obtain ⟨o, ho, hto⟩ := Option.map_eq_some'.mp hm
    cases w with
    | set _ => simp [denote] at hw
    | _ => simp only [attrObjs, List.filterMap_cons, ho, List.filterMap_nil, plainValue, single, hto]

/-! ## One field of an element -/

theorem flatMap_key {α β γ : Type} (key : α → γ) {L : List α} (hnd : (L.map key).Nodup) {x : α} (hx : x ∈ L)
    {g : α → List β} (hg : ∀ y ∈ L, g y ≠ [] → key y = key x) : L.flatMap g = g x := by
  induction L with
  | nil => cases hx
  | cons y L ih =>
    rw [List.map_cons, List.nodup_cons] at hnd
    simp only [List.flatMap_cons]
    rcases List.mem_cons.mp hx with rfl | hx
    · have : L.flatMap g = [] := by
        rw [List.flatMap_eq_nil_iff]
        intro z hz
        cases hgz : g z with
        | nil => rfl
        | cons _ _ =>
          have := hg z (List.mem_cons_of_mem _ hz) (by simp [hgz])
          exact absurd (by rw [← this]; exact List.mem_map_of_mem _ hz) hnd.1
      rw [this, List.append_nil]
    · have hy : g y = [] := by
        cases hgy : g y with
        | nil => rfl
        | cons _ _ =>
          have := hg y (List.mem_cons_self _ _) (by simp [hgy])
          exact absurd (by rw [this]; exact List.mem_map_of_mem _ hx) hnd.1
      rw [hy, List.nil_append]
      exact ih hnd.2 hx fun z hz => hg z (List.mem_cons_of_mem _ hz)

omit [DecidableEq T.P] in
theorem ownObjs_flatMap {α : Type} (ι : Nat → Option String) (p : Pred) (L : List α) (h : α → List (Tmpl T)) :
    ownObjs ι p (L.flatMap h) = L.flatMap fun y => ownObjs ι p (h y) := by
  induction L with
  | nil => rfl
  | cons y L ih => simp only [List.flatMap_cons, ownObjs_append, ih]

omit [DecidableEq T.P] in
theorem ownObjs_optList {α : Type} (ι : Nat → Option String) (p : Pred) (o : Option α) (k : α → List (Tmpl T)) :
    ownObjs ι p (optList o k) = optList o fun a => ownObjs ι p (k a) := by
  cases o <;> rfl

omit [DecidableEq T.P] in
theorem exists_of_ownObjs (ι : Nat → Option String) {p : Pred} {ts : List (Tmpl T)} (h : ownObjs ι p ts ≠ []) :
    ∃ t ∈ ts, t.p = p :=
  Decidable.byContradiction fun hn => h (ownObjs_nil ι p ts fun t ht htp => hn ⟨t, ht, htp⟩)

theorem attrPred_eq {a b : Name} {p : Pred} (ha : attrPred a p = true) (hb : attrPred b p = true) : a = b := by
  cases p <;> simp_all [attrPred]

omit [DecidableEq T.P] in
/-- An attribute's predicates read at an element: only that attribute's templates bear them. -/
theorem ownObjs_attr (ι : Nat → Option String) {c : CType T} {e : Nat} {l : Name} {v : VTerm T}
    (hc : K.find l = some c) (hnd : ((K.allFields l).map Field.name).Nodup) {a : Name} {t : NTy T.P Name}
    {opt : Bool} {view : AttrView} (hf : Field.attr a t opt view ∈ K.allFields l) {p : Pred}
    (hp : attrPred a p = true) :
    ownObjs ι p (templates K vp e l v) =
      optList ((v.field? a).bind (presentValue opt)) fun u => ownObjs ι p (attrTemplates vp e a view u) := by
  unfold templates
  rw [hc]
  simp only []
  rw [ownObjs_append, ownObjs_append, ownObjs_append]
  have h0 : ownObjs ι p (typeTemplates K e l) = [] := ownObjs_nil ι p _ fun t ht htp => by
    have := (typeTemplates_shape K ht).2
    rw [this] at htp
    subst htp
    simp [attrPred] at hp
  have h2 : ownObjs ι p (if K.inherits CType.bitemporal l = true then assertedTemplates e v else []) = [] := by
    apply ownObjs_nil
    intro t ht htp
    split at ht
    · have := (assertedTemplates_shape ht).2
      rw [htp] at this
      cases p <;> simp_all [attrPred, assertedPred]
    · cases ht
  have h3 : ownObjs ι p (relTemplates K c e l v) = [] :=
    ownObjs_rel_nil K ι (by cases p <;> simp_all [attrPred, isNav]) (by intro h; subst h; simp [attrPred] at hp)
      (by intro owner r h; subst h; simp [attrPred] at hp)
  rw [h0, h2, h3, List.nil_append, List.append_nil, List.append_nil]
  unfold attrsTemplates
  rw [ownObjs_flatMap]
  rw [flatMap_key Field.name hnd hf]
  · exact ownObjs_optList ι p _ _
  · intro f _ hne
    match f, hne with
    | .attr b _ _ view', hne =>
      obtain ⟨t', ht', htp⟩ := exists_of_ownObjs ι hne
      obtain ⟨u, _, ht'⟩ := mem_optList.mp ht'
      have := (attrTemplates_shape vp ht').2
      rw [htp] at this
      exact attrPred_eq this hp
    | .role _ _ _ _ _, hne => exact absurd rfl hne
    | .setRole _ _ _, hne => exact absurd rfl hne

/-! ## The conditions of proposition 17.3 -/

/-- A temporal interval's record over the bound type `p` and the precision type `pr` (`SCHEMA-APG.md` section 13.2). -/
def intervalTemporal (p pr : T.P) : NTy T.P Name :=
  .record [("end", NTy.optional (.prim p)), ("end_precision", NTy.optional (.prim pr)),
    ("start", NTy.optional (.prim p)), ("start_precision", NTy.optional (.prim pr))]

/-- An integer or decimal interval's record over the bound type `p`. -/
def intervalNumeric (p : T.P) : NTy T.P Name :=
  .record [("end", NTy.optional (.prim p)), ("start", NTy.optional (.prim p))]

/-- What an attribute's view asks of its type: the literals of a reference-free value, an enumeration of class names,
or an interval's record. -/
def AttrOk (t : NTy T.P Name) : AttrView → Prop
  | .plain | .measure => refFree t = true
  | .typeValue => ∃ cs : List Name, t = NTy.enum cs
  | .interval => ∃ p pr, t = intervalTemporal p pr ∨ t = intervalNumeric p

/-- An interval's present value as the store writes it: a bound present, and each precision present exactly when its
bound is (`SCHEMA.md` section 5.5). -/
def IntervalOk (u : VTerm T) : Prop :=
  let b (n : Name) := (u.field? n).bind (presentValue true)
  ((b "start").isSome = true ∨ (b "end").isSome = true) ∧
    ((u.field? "start_precision").isSome = true → (b "start_precision").isSome = (b "start").isSome) ∧
    ((u.field? "end_precision").isSome = true → (b "end_precision").isSome = (b "end").isSome)

/-- The conditions under which `R⁻¹(R(G)) = G`. Each mirrors a rule of `SCHEMA.md`:
- the catalog's field names unique (`UGS0222`, `UGS0404`, `UGS0417`), an attribute's view fitting its type (section
  13.2), a type with roles a relation, an edge-encoded relation's anchor declaring a navigation name (`UGS0418`) and its
  two required roles its anchor and its other role (section 11.1), navigation names unique per participant type and a
  `@symmetric` relation's one participant type (`UGS0404`, `UGS0408`);
- `G` valid on `S(K)`, every label one of its labels;
- the naming injective, inverted by `idOf`, defined on the elements named by IRIs (section 15.1) and on every
  participant, which section 11.1's reifiers name;
- `G` written as the store writes it: a set's members in the order `ord` sorts them and no empty set (section 5.2), an
  interval with a bound and each precision with its bound (section 5.5), a hidden precision its type's own (section
  5.1), and a `@symmetric` triple in IRI order (section 6.4). -/
structure RoundTrip (vp : ViewParams T) (ord : VTerm T → VTerm T → Bool) (ι : Nat → Option String)
    (idOf : String → Nat) (G : FinApg K.schema) : Prop where
  fieldsUnique : FieldsUnique K
  namesUnique : ∀ x ∈ G.elems, ((K.allFields x.2.1).map Field.name).Nodup
  slotsUnique : ∀ x ∈ G.elems, ((K.slots x.2.1).map Prod.fst).Nodup
  labels : ∀ x ∈ G.elems, (∃ c, K.find x.2.1 = some c ∧ c.concrete = true) ∨
    (K.find x.2.1 = none ∧ (K.roleLabelOf x.2.1).isSome = true)
  attrs : ∀ x ∈ G.elems, ∀ owner a t opt view, (owner, .attr a t opt view) ∈ K.ownedFields x.2.1 → AttrOk t view
  relations : ∀ x ∈ G.elems, ∀ c, K.find x.2.1 = some c → ∀ f ∈ K.allFields x.2.1, f.isRole = true →
    c.kind = .relation
  edges : ∀ x ∈ G.elems, K.edgeEncoded x.2.1 = true → ∃ a o n, K.anchorRole x.2.1 = some a ∧
    K.otherRole x.2.1 = some o ∧ K.navOf x.2.1 a = some n ∧ a ≠ o ∧
    (∀ f ∈ K.allFields x.2.1, f.isRole = true → f.name = a ∨ f.name = o) ∧
    (∃ owner ps nv sp, (owner, .role a ps false nv sp) ∈ K.ownedFields x.2.1) ∧
    (∃ owner ps nv sp, (owner, .role o ps false nv sp) ∈ K.ownedFields x.2.1)
  valid : G.valid = true
  inj : ∀ x ∈ G.elems, ∀ y ∈ G.elems, ∀ s, ι x.1 = some s → ι y.1 = some s → x.1 = y.1
  inv : ∀ x ∈ G.elems, ∀ s, ι x.1 = some s → idOf s = x.1
  named : ∀ x ∈ G.elems, (ι x.1).isSome = ((K.find x.2.1).isSome && !tripleNamed K x.2.1)
  parts : ∀ x ∈ G.elems, ∀ p ∈ x.2.2.refs, ∃ y ∈ G.elems, y.1 = p ∧ (K.find y.2.1).isSome = true ∧
    tripleNamed K y.2.1 = false
  sets : ∀ x ∈ G.elems, ∀ a w ms, x.2.2.field? a = some w → (w = .set ms ∨ w = .inj "some" (.set ms)) →
    sortOrd ord ms = ms ∧ ms ≠ []
  intervals : ∀ x ∈ G.elems, ∀ owner a t opt, (owner, .attr a t opt .interval) ∈ K.ownedFields x.2.1 →
    ∀ w u, x.2.2.field? a = some w → presentValue opt w = some u → IntervalOk u
  native : ∀ p q y, vp.precisionShown p q y = false ↔ vp.native p = some ⟨q, y⟩
  navUnique : ∀ c ∈ K.types, ∀ c' ∈ K.types, c.concrete = true → c'.concrete = true → tripleNamed K c.name = true →
    K.edgeEncoded c'.name = true → ∀ n lp, (K.anchorRole c.name).bind (K.navOf c.name) = some n →
    (K.anchorRole c'.name).bind (K.navOf c'.name) = some n → admitsAnchor K c.name lp = true →
    (admitsAnchor K c'.name lp = true ∨ (K.inherits CType.symmetric c'.name = true ∧ admitsOther K c'.name lp = true)) →
    c.name = c'.name
  symm : ∀ x ∈ G.elems, tripleNamed K x.2.1 = true → K.inherits CType.symmetric x.2.1 = true →
    ∀ a o pa po sa so, K.anchorRole x.2.1 = some a → K.otherRole x.2.1 = some o → fieldRefs x.2.2 a = [pa] →
    fieldRefs x.2.2 o = [po] → ι pa = some sa → ι po = some so → sa ≤ so

variable {K}

theorem RoundTrip.nodup {vp : ViewParams T} {ord : VTerm T → VTerm T → Bool} {ι : Nat → Option String}
    {idOf : String → Nat} {G : FinApg K.schema} (H : RoundTrip K vp ord ι idOf G) : (G.elems.map Prod.fst).Nodup := by
  have := H.valid
  simp only [FinApg.valid, Bool.and_eq_true, decide_eq_true_eq] at this
  exact this.1

theorem RoundTrip.check {vp : ViewParams T} {ord : VTerm T → VTerm T → Bool} {ι : Nat → Option String}
    {idOf : String → Nat} {G : FinApg K.schema} (H : RoundTrip K vp ord ι idOf G) {x : Nat × Name × VTerm T}
    (hx : x ∈ G.elems) : (denote G.lam x.2.2 (K.schema.σ x.2.1)).isSome = true := by
  have := H.valid
  simp only [FinApg.valid, Bool.and_eq_true, List.all_eq_true] at this
  exact this.2 x hx

variable (K)

omit [DecidableEq T.P] in
/-- An attribute's own objects under one of its views but the interval's. -/
theorem ownObjs_attrTemplates (ι : Nat → Option String) (e : Nat) (a : Name) (view : AttrView) (u : VTerm T) :
    ownObjs ι (.attr a) (attrTemplates vp e a view u) =
      match view with
      | .plain => (attrObjs u).filterMap fun o => o.inst ι
      | .measure => ((TObj.measure u).inst ι).toList
      | .typeValue => optList (asTag u) fun c => [Term.cls c]
      | .interval => [] := by
  cases view with
  | plain =>
    rw [ownObjs_all ι _ _ (by intro t ht; simp only [attrTemplates, List.mem_map] at ht; obtain ⟨_, _, rfl⟩ := ht; rfl)]
    simp only [attrTemplates, List.filterMap_map, Function.comp_def]
  | measure =>
    rw [ownObjs_all ι _ _ (by intro t ht; simp only [attrTemplates, List.mem_singleton] at ht; subst ht; rfl)]
    simp only [attrTemplates, List.filterMap_cons, List.filterMap_nil]
    cases (TObj.measure u).inst ι <;> rfl
  | typeValue =>
    simp only [attrTemplates]
    cases asTag u <;> simp [optList, ownObjs, TObj.inst]
  | interval =>
    apply ownObjs_nil
    intro t ht htp
    have := (attrTemplates_shape vp ht).2
    rw [htp] at this
    have hall : (attrTemplates vp e a .interval u).all (fun t => match t.p with | .bound _ _ => true | _ => false) =
        true := by
      simp [attrTemplates]
    have := List.all_eq_true.mp hall t ht
    rw [htp] at this
    cases this

omit [DecidableEq T.P] in
/-- A slot names the field of the catalog it comes from. -/
theorem mem_slots_attr {l a : Name} {opt : Bool} {t : NTy T.P Name} {view : AttrView}
    (h : (a, Catalog.Slot.attr opt t view) ∈ K.slots l) : ∃ owner, (owner, .attr a t opt view) ∈ K.ownedFields l := by
  unfold Catalog.slots at h
  rw [mem_sortBy, List.mem_append] at h
  rcases h with h | h
  · obtain ⟨y, hy, hyx⟩ := List.mem_filterMap.mp h
    match y, hy, hyx with
    | (owner, .attr b t' opt' view'), hy, hyx =>
      simp only [Option.some.injEq, Prod.mk.injEq, Catalog.Slot.attr.injEq] at hyx
      obtain ⟨rfl, rfl, rfl, rfl⟩ := hyx
      exact ⟨owner, hy⟩
    | (_, .role _ _ _ _ _), _, hyx => simp at hyx
    | (_, .setRole _ _ _), _, hyx => simp at hyx
  · split at h <;> simp at h

omit [DecidableEq T.P] in
theorem mem_slots_role {l r owner : Name} {ps : List Name} {opt : Bool}
    (h : (r, Catalog.Slot.role ps opt owner) ∈ K.slots l) :
    ∃ nv sp, (owner, .role r ps opt nv sp) ∈ K.ownedFields l := by
  unfold Catalog.slots at h
  rw [mem_sortBy, List.mem_append] at h
  rcases h with h | h
  · obtain ⟨y, hy, hyx⟩ := List.mem_filterMap.mp h
    match y, hy, hyx with
    | (owner', .role r' ps' opt' nv sp), hy, hyx =>
      simp only [Option.some.injEq, Prod.mk.injEq, Catalog.Slot.role.injEq] at hyx
      obtain ⟨rfl, rfl, rfl, rfl⟩ := hyx
      exact ⟨nv, sp, hy⟩
    | (_, .attr _ _ _ _), _, hyx => simp at hyx
    | (_, .setRole _ _ _), _, hyx => simp at hyx
  · split at h <;> simp at h

omit [DecidableEq T.P] in
theorem mem_slots_asserted {l n : Name} (h : (n, Catalog.Slot.asserted) ∈ K.slots l) :
    n = "asserted" ∧ K.inherits CType.bitemporal l = true := by
  unfold Catalog.slots at h
  rw [mem_sortBy, List.mem_append] at h
  rcases h with h | h
  · obtain ⟨y, _, hyx⟩ := List.mem_filterMap.mp h
    match y, hyx with
    | (_, .attr _ _ _ _), hyx => simp at hyx
    | (_, .role _ _ _ _ _), hyx => simp at hyx
    | (_, .setRole _ _ _), hyx => simp at hyx
  · split at h
    · next hb => simp only [List.mem_singleton, Prod.mk.injEq] at h; exact ⟨h.1, hb⟩
    · simp at h

omit [DecidableEq T.P] in
theorem refsAll_nil : (ms : List (VTerm T)) → VTerm.refs.refsAll ms = [] → ∀ m ∈ ms, m.refs = []
  | [], _, _, hm => by cases hm
  | y :: ys, h, m, hm => by
    simp only [VTerm.refs.refsAll, List.append_eq_nil] at h
    rcases List.mem_cons.mp hm with rfl | hm
    · exact h.1
    · exact refsAll_nil ys h.2 m hm

omit [DecidableEq T.P] in
theorem attrObjs_inst_ne_nil (ι : Nat → Option String) {u : VTerm T} (hu : u.refs = [])
    (hne : ∀ ms, u = .set ms → ms ≠ []) : ((attrObjs u).filterMap fun o => o.inst ι) ≠ [] := by
  cases u with
  | set ms =>
    intro h
    have hms : ∀ m ∈ ms, m.refs = [] := refsAll_nil ms (by simpa [VTerm.refs] using hu)
    have := members_round ι ms hms
    simp only [attrObjs] at h
    rw [h] at this
    exact hne ms rfl this.symm
  | _ =>
    have hm := inst_memberObj ι hu
    obtain ⟨o, ho, _⟩ := Option.map_eq_some'.mp hm
    simp [attrObjs, ho]

/-! ## Records -/

theorem denoteRec_nil {lam : Nat → Option Name} {ws : List (Name × VTerm T)}
    (h : (denote.denoteRec lam ws []).isSome = true) : ws = [] := by
  cases ws with
  | nil => rfl
  | cons _ _ => simp [denote.denoteRec] at h

theorem denoteRec_cons {lam : Nat → Option Name} {ws : List (Name × VTerm T)} {m : Name} {t : NTy T.P Name}
    {fs : List (Name × NTy T.P Name)} (h : (denote.denoteRec lam ws ((m, t) :: fs)).isSome = true) :
    ∃ w ws', ws = (m, w) :: ws' ∧ (denote lam w t).isSome = true ∧ (denote.denoteRec lam ws' fs).isSome = true := by
  match ws, h with
  | [], h => simp [denote.denoteRec] at h
  | (n, w) :: ws', h =>
    simp only [denote.denoteRec] at h
    split at h
    · next hn =>
      subst hn
      split at h
      · next x f hx hf => exact ⟨w, ws', rfl, by simp [hx], by simp [hf]⟩
      · simp at h
    · simp at h

omit [DecidableEq T.P] in
theorem field?_cons_self (n : Name) (w : VTerm T) (ws : List (Name × VTerm T)) :
    (VTerm.record ((n, w) :: ws)).field? n = some w := by
  simp [VTerm.field?, List.find?_cons]

omit [DecidableEq T.P] in
theorem field?_cons_ne {n m : Name} (w : VTerm T) (ws : List (Name × VTerm T)) (h : n ≠ m) :
    (VTerm.record ((n, w) :: ws)).field? m = (VTerm.record ws).field? m := by
  simp [VTerm.field?, List.find?_cons, h]

/-! ## An element's attribute slots -/

section Decode

variable {vp : ViewParams T} {ord : VTerm T → VTerm T → Bool} {ι : Nat → Option String} {idOf : String → Nat}
  {G : FinApg K.schema}

/-- How a view other than the interval's reads a present value from its objects. -/
def viewValue (ord : VTerm T → VTerm T → Bool) (t : NTy T.P Name) : AttrView → List (Term T) → VTerm T
  | .plain => plainValue ord t
  | _ => single

omit [DecidableEq T.P] in
theorem presentValue_absent : presentValue true (absent : VTerm T) = none := rfl

omit [DecidableEq T.P] in
theorem presentValue_some (u : VTerm T) : presentValue true (.inj "some" u) = some u := rfl

/-- A present attribute value read back from its own objects under a view other than the interval's. -/
theorem attr_round {t : NTy T.P Name} {view : AttrView} (lam : Nat → Option Name) (hok : AttrOk t view)
    (hview : view ≠ .interval) (e : Nat) (a : Name)
    {u : VTerm T} (hu : (denote lam u t).isSome = true) (hset : ∀ ms, u = .set ms → sortOrd ord ms = ms ∧ ms ≠ []) :
    ownObjs ι (.attr a) (attrTemplates vp e a view u) ≠ [] ∧
      viewValue ord t view (ownObjs ι (.attr a) (attrTemplates vp e a view u)) = u := by
  rw [ownObjs_attrTemplates vp]
  cases view with
  | plain =>
    have hr : refFree t = true := hok
    exact ⟨attrObjs_inst_ne_nil ι (refs_nil lam u t hu hr) fun ms h => (hset ms h).2,
      plainValue_round lam ι ord hu hr fun ms h => (hset ms h).1⟩
  | measure =>
    have hr : refFree t = true := hok
    have : (TObj.measure u).inst ι = some (.measure u) := by simp [TObj.inst, refs_nil lam u t hu hr]
    simp only [this, Option.toList_some]
    exact ⟨by simp, rfl⟩
  | typeValue =>
    obtain ⟨cs, rfl⟩ := hok
    obtain ⟨n, u', t', rfl, hm, hu'⟩ := check_variant hu
    simp only [List.mem_map, Prod.mk.injEq] at hm
    obtain ⟨_, _, rfl, rfl⟩ := hm
    rw [check_unit hu']
    simp [asTag, optList, single, termValue, viewValue]
  | interval => exact absurd rfl hview

/-- Proposition 17.3 for an attribute slot under a view other than the interval's: read back from the element's own
quads, it is the element's value. -/
theorem decode_attr (H : RoundTrip K vp ord ι idOf G) (graph : Nat → String) {x : Nat × Name × VTerm T}
    (hx : x ∈ G.elems) {s : String} (hs : ι x.1 = some s) {c : CType T} (hc : K.find x.2.1 = some c)
    {a : Name} {opt : Bool} {t : NTy T.P Name} {view : AttrView} (hview : view ≠ .interval)
    (hslot : (a, Catalog.Slot.attr opt t view) ∈ K.slots x.2.1) {w : VTerm T} (hw : x.2.2.field? a = some w)
    (hwc : (denote G.lam w (K.slotType (.attr opt t view))).isSome = true) :
    decodeSlot K vp ord idOf (rdf K vp ι graph G) s x.2.1 (a, .attr opt t view) = w := by
  obtain ⟨owner, hown⟩ := mem_slots_attr K hslot
  have hf : Field.attr a t opt view ∈ K.allFields x.2.1 := List.mem_map_of_mem Prod.snd hown
  have hok := H.attrs x hx owner a t opt view hown
  have hobj : objectsOf (rdf K vp ι graph G) s (.attr a) =
      optList ((x.2.2.field? a).bind (presentValue opt)) fun u =>
        ownObjs ι (.attr a) (attrTemplates vp x.1 a view u) := by
    rw [objectsOf_rdf K vp graph H.nodup H.inj hx hs (p := .attr a) rfl,
      ownObjs_attr K vp ι hc (H.namesUnique x hx) hf (by simp [attrPred])]
  simp only [hw] at hobj
  have hset := fun ms h => H.sets x hx a w ms hw h
  have hdec : decodeSlot K vp ord idOf (rdf K vp ι graph G) s x.2.1 (a, .attr opt t view) =
      optional opt (objectsOf (rdf K vp ι graph G) s (.attr a)) (viewValue ord t view) := by
    cases view <;> first | rfl | exact absurd rfl hview
  rw [hdec, hobj]
  cases opt with
  | false =>
    simp only [Catalog.Catalog.slotType, Bool.false_eq_true, if_false] at hwc
    simp only [Option.bind_some, presentValue, Bool.false_eq_true, if_false, optList, optional]
    exact (attr_round G.lam hok hview x.1 a hwc fun ms h => hset ms (Or.inl h)).2
  | true =>
    simp only [Catalog.Catalog.slotType, if_true] at hwc
    rcases check_optional hwc with rfl | ⟨u, rfl, hu⟩
    · simp [Option.bind_some, presentValue_absent, optList, optional]
    · have hb : ((some (VTerm.inj "some" u) : Option (VTerm T)).bind (presentValue true)) = some u := rfl
      simp only [hb, optList, optional, if_true]
      have := attr_round (vp := vp) (ι := ι) G.lam hok hview x.1 a hu fun ms h => hset ms (Or.inr (h ▸ rfl))
      rw [if_neg (by simpa using this.1), this.2]

end Decode

section Asserted

variable {vp : ViewParams T} {ord : VTerm T → VTerm T → Bool} {ι : Nat → Option String} {idOf : String → Nat}
  {G : FinApg K.schema}

theorem assertedPred_attrPred {a : Name} {p : Pred} (h : assertedPred p = true) : attrPred a p = false := by
  cases p <;> simp_all [assertedPred, attrPred]

omit [DecidableEq T.P] in
/-- The built-in fields' predicates read at a `@bitemporal` element: only its `asserted` templates bear them. -/
theorem ownObjs_asserted (vp : ViewParams T) (ι : Nat → Option String) {c : CType T} {e : Nat} {l : Name}
    {v : VTerm T} (hc : K.find l = some c) (hb : K.inherits CType.bitemporal l = true) {p : Pred}
    (hp : assertedPred p = true) : ownObjs ι p (templates K vp e l v) = ownObjs ι p (assertedTemplates e v) := by
  unfold templates
  rw [hc]
  simp only [hb, if_true]
  rw [ownObjs_append, ownObjs_append, ownObjs_append]
  have h0 : ownObjs ι p (typeTemplates K e l) = [] := ownObjs_nil ι p _ fun t ht htp => by
    have := (typeTemplates_shape K ht).2
    rw [this] at htp
    subst htp
    simp [assertedPred] at hp
  have h1 : ownObjs ι p (attrsTemplates K vp e l v) = [] := by
    apply ownObjs_nil
    intro t ht htp
    simp only [attrsTemplates, List.mem_flatMap] at ht
    obtain ⟨f, _, ht⟩ := ht
    match f, ht with
    | .attr a _ opt view, ht =>
      obtain ⟨u, _, ht⟩ := mem_optList.mp ht
      have := (attrTemplates_shape vp ht).2
      rw [htp, assertedPred_attrPred hp] at this
      cases this
    | .role _ _ _ _ _, ht => cases ht
    | .setRole _ _ _, ht => cases ht
  have h3 : ownObjs ι p (relTemplates K c e l v) = [] :=
    ownObjs_rel_nil K ι (by cases p <;> simp_all [assertedPred, isNav]) (by intro h; subst h; simp [assertedPred] at hp)
      (by intro owner r h; subst h; simp [assertedPred] at hp)
  rw [h0, h1, h3, List.nil_append, List.nil_append, List.append_nil]

omit [DecidableEq T.P] in
theorem ownObjs_assertedFrom (ι : Nat → Option String) (e : Nat) (start stop : Option (VTerm T)) :
    ownObjs ι .assertedStart (assertedFrom e start stop) =
        optList start (fun w => (attrObjs w).filterMap fun o => o.inst ι) ∧
      ownObjs ι .assertedEnd (assertedFrom e start stop) =
        optList stop (fun w => (attrObjs w).filterMap fun o => o.inst ι) := by
  unfold assertedFrom
  refine ⟨?_, ?_⟩
  · rw [ownObjs_append, ownObjs_append, ownObjs_optList, ownObjs_optList,
      ownObjs_nil ι _ [_] (by simp), List.append_nil]
    have h2 : (optList stop fun a => ownObjs ι .assertedStart
        ((attrObjs a).map fun o => (⟨e, .assertedEnd, o, e⟩ : Tmpl T))) = [] := by
      cases stop <;> simp [optList, ownObjs_nil]
    rw [h2, List.append_nil]
    congr 1
    funext w
    rw [ownObjs_all ι _ _ (by simp), List.filterMap_map]
    rfl
  · rw [ownObjs_append, ownObjs_append, ownObjs_optList, ownObjs_optList,
      ownObjs_nil ι _ [_] (by simp), List.append_nil]
    have h1 : (optList start fun a => ownObjs ι .assertedEnd
        ((attrObjs a).map fun o => (⟨e, .assertedStart, o, e⟩ : Tmpl T))) = [] := by
      cases start <;> simp [optList, ownObjs_nil]
    rw [h1, List.nil_append]
    congr 1
    funext w
    rw [ownObjs_all ι _ _ (by simp), List.filterMap_map]
    rfl

/-- Proposition 17.3 for `asserted`: its bounds read back from the built-in predicates. -/
theorem decode_asserted (H : RoundTrip K vp ord ι idOf G) (graph : Nat → String) {x : Nat × Name × VTerm T}
    (hx : x ∈ G.elems) {s : String} (hs : ι x.1 = some s) {c : CType T} (hc : K.find x.2.1 = some c) {n : Name}
    (hslot : (n, Catalog.Slot.asserted) ∈ K.slots x.2.1) {w : VTerm T} (hw : x.2.2.field? n = some w)
    (hwc : (denote G.lam w (K.slotType .asserted)).isSome = true) :
    decodeSlot K vp ord idOf (rdf K vp ι graph G) s x.2.1 (n, .asserted) = w := by
  obtain ⟨rfl, hb⟩ := mem_slots_asserted K hslot
  have hobj : ∀ p, assertedPred p = true → objectsOf (rdf K vp ι graph G) s p =
      ownObjs ι p (assertedTemplates x.1 x.2.2) := fun p hp => by
    rw [objectsOf_rdf K vp graph H.nodup H.inj hx hs (p := p) (by cases p <;> simp_all [assertedPred, ownPred]),
      ownObjs_asserted K vp ι hc hb hp]
  simp only [Catalog.Catalog.slotType, Catalog.Catalog.assertedType] at hwc
  obtain ⟨ws, rfl, hws⟩ := check_record hwc
  obtain ⟨ew, ws1, rfl, hew, hws1⟩ := denoteRec_cons hws
  obtain ⟨sw, ws2, rfl, hsw, hws2⟩ := denoteRec_cons hws1
  obtain rfl := denoteRec_nil hws2
  obtain ⟨a, rfl⟩ := check_prim hsw
  have hst : objectsOf (rdf K vp ι graph G) s .assertedStart = [.lit K.dateTime a] := by
    rw [hobj .assertedStart rfl]
    simp only [assertedTemplates, hw, optList, field?_cons_ne _ _ (by decide : ("end" : Name) ≠ "start"),
      field?_cons_self]
    rw [(ownObjs_assertedFrom ι x.1 _ _).1]
    rfl
  have hend : objectsOf (rdf K vp ι graph G) s .assertedEnd =
      optList (presentValue true ew) (fun w => (attrObjs w).filterMap fun o => o.inst ι) := by
    rw [hobj .assertedEnd rfl]
    simp only [assertedTemplates, hw, optList]
    rw [(ownObjs_assertedFrom ι x.1 _ _).2, field?_cons_self]
    rfl
  simp only [decodeSlot, hst, single, termValue, hend]
  rcases check_optional hew with rfl | ⟨u, rfl, hu⟩
  · rfl
  · obtain ⟨b, rfl⟩ := check_prim hu
    rfl

end Asserted

/-! ## Participants -/

section Participants

variable {vp : ViewParams T} {ord : VTerm T → VTerm T → Bool} {ι : Nat → Option String} {idOf : String → Nat}
  {G : FinApg K.schema}

theorem find?_key {α β : Type} [BEq α] [LawfulBEq α] {L : List (α × β)} (hnd : (L.map Prod.fst).Nodup)
    {y : α × β} (hy : y ∈ L) : L.find? (fun z => z.1 == y.1) = some y := by
  induction L with
  | nil => cases hy
  | cons z zs ih =>
    rw [List.map_cons, List.nodup_cons] at hnd
    rcases List.mem_cons.mp hy with rfl | hy
    · simp
    · have hz : (z.1 == y.1) = false := by
        simpa using fun h : z.1 = y.1 => hnd.1 (h ▸ List.mem_map_of_mem _ hy)
      simp only [List.find?_cons, hz]
      exact ih hnd.2 hy

omit [DecidableEq T.P] in
theorem lam_of_mem {S : NSchema T.P Name} {G : FinApg S} (hnd : (G.elems.map Prod.fst).Nodup)
    {y : Nat × Name × VTerm T} (hy : y ∈ G.elems) : G.lam y.1 = some y.2.1 := by
  simp [FinApg.lam, FinApg.entry, find?_key hnd hy]

theorem check_lbl {lam : Nat → Option Name} {w : VTerm T} {c : Name} (h : (denote lam w (.lbl c)).isSome = true) :
    ∃ p, w = .elem p ∧ lam p = some c := by
  cases w with
  | elem p =>
    simp only [denote] at h
    split at h
    · next hp => exact ⟨p, rfl, hp⟩
    · simp at h
  | _ => simp [denote] at h

omit [DecidableEq T.P] in
theorem mem_participantType {ps : List Name} {fs : List (Name × NTy T.P Name)}
    (h : K.participantType ps = .variant fs) {n : Name} {t : NTy T.P Name} (hm : (n, t) ∈ fs) : t = .lbl n := by
  unfold Catalog.participantType at h
  split at h
  · cases h
  · cases h
    rw [mem_sortBy, List.mem_map] at hm
    obtain ⟨_, _, he⟩ := hm
    simp only [Prod.mk.injEq] at he
    rw [← he.1, ← he.2]

/-- A participant's value read back: its identifier, tagged with its label when the role admits several types. -/
theorem participant_round (H : RoundTrip K vp ord ι idOf G) (graph : Nat → String) {ps : List Name} {w : VTerm T}
    (hw : (denote G.lam w (K.participantType ps)).isSome = true)
    (hpart : ∀ p ∈ w.refs, ∃ y ∈ G.elems, y.1 = p ∧ (K.find y.2.1).isSome = true ∧ tripleNamed K y.2.1 = false) :
    ∃ p sp, w.refs = [p] ∧ ι p = some sp ∧
      participantValue K idOf (rdf K vp ι graph G) ps sp = w := by
  have hgen : ∀ p, (∃ y ∈ G.elems, y.1 = p ∧ (K.find y.2.1).isSome = true ∧ tripleNamed K y.2.1 = false) →
      ∃ sp, ι p = some sp ∧ idOf sp = p ∧ labelOf K (rdf K vp ι graph G) sp = G.lam p := by
    rintro p ⟨y, hy, rfl, hf, ht⟩
    have hn := H.named y hy
    rw [hf, ht] at hn
    obtain ⟨sp, hsp⟩ := Option.isSome_iff_exists.mp (by simpa using hn)
    obtain ⟨c, hc⟩ := Option.isSome_iff_exists.mp hf
    exact ⟨sp, hsp, H.inv y hy sp hsp, by
      rw [labelOf_rdf K vp graph H.nodup H.inj hy hsp hc, lam_of_mem H.nodup hy]⟩
  cases hpt : K.participantType ps with
  | lbl c =>
    rw [hpt] at hw
    obtain ⟨p, rfl, _⟩ := check_lbl hw
    obtain ⟨sp, hsp, hid, _⟩ := hgen p (hpart p (by simp [VTerm.refs]))
    exact ⟨p, sp, rfl, hsp, by simp [participantValue, hpt, hid]⟩
  | variant fs =>
    rw [hpt] at hw
    obtain ⟨n, u, t, rfl, hm, hu⟩ := check_variant hw
    rw [mem_participantType K hpt hm] at hu
    obtain ⟨p, rfl, hlp⟩ := check_lbl hu
    obtain ⟨sp, hsp, hid, hlab⟩ := hgen p (hpart p (by simp [VTerm.refs]))
    exact ⟨p, sp, rfl, hsp, by simp [participantValue, hpt, hid, hlab, hlp]⟩
  | _ =>
    rw [hpt] at hw
    first
    | simp [denote] at hw
    | (unfold Catalog.participantType at hpt; split at hpt <;> cases hpt)

end Participants

/-! ## Every slot has its value -/

omit [DecidableEq T.P] in
theorem find?_field_of_nodup {ws : List (Name × VTerm T)} (hnd : (ws.map Prod.fst).Nodup) {n : Name} {w : VTerm T}
    (hm : (n, w) ∈ ws) : (VTerm.record ws).field? n = some w := by
  simp only [VTerm.field?]
  rw [show (fun f : Name × VTerm T => f.1 == n) = fun f => f.1 == (n, w).1 from rfl, find?_key hnd hm]
  rfl

/-- A record checked against a record type holds, for each of the type's fields, a value of the field's type under the
same name, in the same order. -/
theorem denoteRec_aligned {lam : Nat → Option Name} : (ws : List (Name × VTerm T)) →
    (fs : List (Name × NTy T.P Name)) → (denote.denoteRec lam ws fs).isSome = true →
    ws.map Prod.fst = fs.map Prod.fst ∧ ∀ n t, (n, t) ∈ fs → ∃ w, (n, w) ∈ ws ∧ (denote lam w t).isSome = true
  | ws, [], h => by rw [denoteRec_nil h]; exact ⟨rfl, fun _ _ h => by cases h⟩
  | ws, (m, t) :: fs, h => by
    obtain ⟨w, ws', rfl, hw, hws'⟩ := denoteRec_cons h
    obtain ⟨hn, hall⟩ := denoteRec_aligned ws' fs hws'
    refine ⟨by simp [hn], fun n t' hm => ?_⟩
    rcases List.mem_cons.mp hm with he | hm
    · simp only [Prod.mk.injEq] at he
      obtain ⟨rfl, rfl⟩ := he
      exact ⟨w, List.mem_cons_self _ _, hw⟩
    · obtain ⟨w', hw', hc'⟩ := hall n t' hm
      exact ⟨w', List.mem_cons_of_mem _ hw', hc'⟩

/-! ## Role slots -/

section Roles

variable {vp : ViewParams T} {ord : VTerm T → VTerm T → Bool} {ι : Nat → Option String} {idOf : String → Nat}
  {G : FinApg K.schema}

omit [DecidableEq T.P] in
theorem schema_concrete {l : Name} {c : CType T} (hc : K.find l = some c) (hconc : c.concrete = true) :
    K.schema.σ l = .record ((K.slots l).map fun y => (y.1, K.slotType y.2)) := by
  show (match K.find l with
    | some c => if c.concrete then _ else NTy.unit
    | none => _) = _
  rw [hc]
  simp [hconc]

/-- Every slot of a concrete element holds a value of its type, under its name. -/
theorem slot_value (H : RoundTrip K vp ord ι idOf G) {x : Nat × Name × VTerm T} (hx : x ∈ G.elems) {c : CType T}
    (hc : K.find x.2.1 = some c) (hconc : c.concrete = true) :
    ∃ ws, x.2.2 = .record ws ∧
      (denote.denoteRec (T := T) G.lam ws ((K.slots x.2.1).map fun y => (y.1, K.slotType y.2))).isSome = true ∧
      ∀ n sl, (n, sl) ∈ K.slots x.2.1 → ∃ w, x.2.2.field? n = some w ∧
        (denote G.lam w (K.slotType sl)).isSome = true := by
  have hchk : (denote G.lam x.2.2 (.record ((K.slots x.2.1).map fun y => (y.1, K.slotType y.2)))).isSome = true := by
    have := H.check hx
    rwa [schema_concrete K hc hconc] at this
  obtain ⟨ws, hv, hws⟩ := check_record hchk
  obtain ⟨hnames, hall⟩ := denoteRec_aligned ws _ hws
  refine ⟨ws, hv, hws, fun n sl hm => ?_⟩
  obtain ⟨w, hw, hwc⟩ := hall n (K.slotType sl) (List.mem_map.mpr ⟨(n, sl), hm, rfl⟩)
  have hnd : (ws.map Prod.fst).Nodup := by
    rw [hnames, List.map_map]
    exact H.slotsUnique x hx
  exact ⟨w, by rw [hv]; exact find?_field_of_nodup hnd hw, hwc⟩

omit [DecidableEq T.P] in
theorem ownObjs_rel (vp : ViewParams T) (ι : Nat → Option String) {c : CType T} {e : Nat} {l : Name} {v : VTerm T}
    (hc : K.find l = some c) {p : Pred} (hp : (∀ a, attrPred a p = false) ∧ assertedPred p = false ∧
      p ≠ .rdfType) :
    ownObjs ι p (templates K vp e l v) = ownObjs ι p (relTemplates K c e l v) := by
  unfold templates
  rw [hc]
  simp only []
  rw [ownObjs_append, ownObjs_append, ownObjs_append]
  have h0 : ownObjs ι p (typeTemplates K e l) = [] := ownObjs_nil ι p _ fun t ht htp =>
    hp.2.2 (htp ▸ (typeTemplates_shape K ht).2)
  have h1 : ownObjs ι p (attrsTemplates K vp e l v) = [] := by
    apply ownObjs_nil
    intro t ht htp
    simp only [attrsTemplates, List.mem_flatMap] at ht
    obtain ⟨f, _, ht⟩ := ht
    match f, ht with
    | .attr a _ opt view, ht =>
      obtain ⟨u, _, ht⟩ := mem_optList.mp ht
      have := (attrTemplates_shape vp ht).2
      rw [htp, hp.1 a] at this
      cases this
    | .role _ _ _ _ _, ht => cases ht
    | .setRole _ _ _, ht => cases ht
  have h2 : ownObjs ι p (if K.inherits CType.bitemporal l = true then assertedTemplates e v else []) = [] := by
    apply ownObjs_nil
    intro t ht htp
    split at ht
    · have := (assertedTemplates_shape ht).2
      rw [htp, hp.2.1] at this
      cases this
    · cases ht
  rw [h0, h1, h2, List.nil_append, List.nil_append, List.nil_append]

omit [DecidableEq T.P] in
/-- A node-encoded relation's role predicate read at its element: its role triples' participants. -/
theorem ownObjs_role_node (vp : ViewParams T) (ι : Nat → Option String) {c : CType T} {e : Nat} {l : Name}
    {v : VTerm T} (hc : K.find l = some c) (hrel : c.kind = .relation) (hedge : K.edgeEncoded l = false)
    (hnd : ((K.ownedFields l).map fun y => y.2.name).Nodup) {owner r : Name} {ps : List Name} {opt : Bool}
    {nv sp : Option Name} (hf : (owner, .role r ps opt nv sp) ∈ K.ownedFields l) :
    ownObjs ι (.role owner r) (templates K vp e l v) = (fieldRefs v r).filterMap fun p => (ι p).map Term.iri := by
  rw [ownObjs_rel K vp ι hc (p := .role owner r) ⟨fun _ => rfl, rfl, by simp⟩]
  unfold relTemplates
  simp only [hrel, hedge, beq_self_eq_true, if_true, Bool.false_eq_true, if_false]
  unfold nodeTemplates
  rw [ownObjs_flatMap, flatMap_key (fun y : Name × Field T => y.2.name) hnd hf]
  · simp only []
    rw [ownObjs_all ι _ _ (by intro t ht; simp only [List.mem_map] at ht; obtain ⟨_, _, rfl⟩ := ht; rfl),
      List.filterMap_map]
    rfl
  · intro y _ hne
    match y, hne with
    | (owner', .role r' _ _ _ _), hne =>
      obtain ⟨t, ht, htp⟩ := exists_of_ownObjs ι hne
      simp only [List.mem_map] at ht
      obtain ⟨_, _, rfl⟩ := ht
      simp only [Pred.role.injEq] at htp
      simp [Field.name, htp.2]
    | (_, .attr _ _ _ _), hne => exact absurd rfl hne
    | (_, .setRole _ _ _), hne => exact absurd rfl hne

omit [DecidableEq T.P] in
/-- An edge-encoded relation's reifier read at its element: the triple term of its anchor and other participant. -/
theorem ownObjs_reifies_edge (vp : ViewParams T) (ι : Nat → Option String) {c : CType T} {e : Nat} {l : Name}
    {v : VTerm T} (hc : K.find l = some c) (hrel : c.kind = .relation) (hedge : K.edgeEncoded l = true)
    (halloc : K.allocatesReifier l = true) {a o n : Name} (ha : K.anchorRole l = some a)
    (ho : K.otherRole l = some o) (hn : K.navOf l a = some n) :
    ownObjs ι .reifies (templates K vp e l v) =
      (fieldRefs v a).flatMap fun pa => (fieldRefs v o).flatMap fun po => ((TObj.triple pa n po).inst ι).toList := by
  rw [ownObjs_rel K vp ι hc (p := .reifies) ⟨fun _ => rfl, rfl, by simp⟩]
  unfold relTemplates
  simp only [hrel, hedge, beq_self_eq_true, if_true]
  unfold edgeTemplates
  simp only [ha, ho, hn, optList]
  rw [ownObjs_flatMap]
  congr 1
  funext pa
  rw [ownObjs_flatMap]
  congr 1
  funext po
  rw [ownObjs_append, ownObjs_append, ownObjs_nil ι _ [_] (by simp), halloc, if_pos rfl,
    ownObjs_all ι _ [_] (by simp)]
  have : ownObjs ι .reifies
      (if K.inherits CType.symmetric l = true then [(⟨po, .nav n, .elem pa, e⟩ : Tmpl T)] else []) = [] := by
    split <;> simp [ownObjs_nil]
  rw [this]
  cases h : (TObj.triple pa n po).inst ι <;> simp [h]

omit [DecidableEq T.P] in
theorem mem_slots_of_role {l owner r : Name} {ps : List Name} {opt : Bool} {nv sp : Option Name}
    (h : (owner, .role r ps opt nv sp) ∈ K.ownedFields l) : (r, Catalog.Slot.role ps opt owner) ∈ K.slots l := by
  unfold Catalog.slots
  rw [mem_sortBy, List.mem_append]
  exact Or.inl (List.mem_filterMap.mpr ⟨_, h, rfl⟩)

omit [DecidableEq T.P] in
theorem refs_field {v w : VTerm T} {n : Name} (h : v.field? n = some w) : ∀ p ∈ w.refs, p ∈ v.refs := by
  cases v with
  | record ws =>
    simp only [VTerm.field?, Option.map_eq_some'] at h
    obtain ⟨y, hy, rfl⟩ := h
    have hm := List.mem_of_find?_eq_some hy
    intro p hp
    simp only [VTerm.refs]
    clear hy
    induction ws with
    | nil => cases hm
    | cons z zs ih =>
      simp only [VTerm.refs.refsFields, List.mem_append]
      rcases List.mem_cons.mp hm with rfl | hm
      · exact Or.inl hp
      · exact Or.inr (ih hm)
  | _ => simp [VTerm.field?] at h

theorem eq_of_nodup_key {α β : Type} {key : α → β} {L : List α} (hnd : (L.map key).Nodup) {x y : α} (hx : x ∈ L)
    (hy : y ∈ L) (h : key x = key y) : x = y := by
  induction L with
  | nil => cases hx
  | cons z zs ih =>
    rw [List.map_cons, List.nodup_cons] at hnd
    cases hx with
    | head =>
      cases hy with
      | head => rfl
      | tail _ hy => exact absurd (by rw [h]; exact List.mem_map_of_mem key hy) hnd.1
    | tail _ hx =>
      cases hy with
      | head => exact absurd (by rw [← h]; exact List.mem_map_of_mem key hx) hnd.1
      | tail _ hy => exact ih hnd.2 hx hy

omit [DecidableEq T.P] in
theorem fieldRefs_of_field {v w : VTerm T} {n : Name} (h : v.field? n = some w) : fieldRefs v n = w.refs := by
  simp [fieldRefs, h]

/-- Proposition 17.3 for a single-valued role: its participant read back from its role triple, or from the reifier's
triple term on an edge-encoded relation. -/
theorem decode_role (H : RoundTrip K vp ord ι idOf G) (graph : Nat → String) {x : Nat × Name × VTerm T}
    (hx : x ∈ G.elems) {s : String} (hs : ι x.1 = some s) {c : CType T} (hc : K.find x.2.1 = some c)
    (hconc : c.concrete = true) {r owner : Name} {ps : List Name} {opt : Bool}
    (hslot : (r, Catalog.Slot.role ps opt owner) ∈ K.slots x.2.1) {w : VTerm T} (hw : x.2.2.field? r = some w)
    (hwc : (denote G.lam w (K.slotType (.role ps opt owner))).isSome = true) :
    decodeSlot K vp ord idOf (rdf K vp ι graph G) s x.2.1 (r, .role ps opt owner) = w := by
  obtain ⟨nv, sp, hown⟩ := mem_slots_role K hslot
  have hrel := H.relations x hx c hc _ (List.mem_map_of_mem Prod.snd hown) rfl
  have hnd : ((K.ownedFields x.2.1).map fun y => y.2.name).Nodup := by
    have := H.namesUnique x hx
    simpa [Catalog.Catalog.allFields, List.map_map, Function.comp_def] using this
  have hpart : ∀ u : VTerm T, (∀ p ∈ u.refs, p ∈ w.refs) → ∀ p ∈ u.refs,
      ∃ y ∈ G.elems, y.1 = p ∧ (K.find y.2.1).isSome = true ∧ tripleNamed K y.2.1 = false :=
    fun u hu p hp => H.parts x hx p (refs_field hw p (hu p hp))
  simp only [decodeSlot, roleObjects]
  cases hedge : K.edgeEncoded x.2.1 with
  | false =>
    simp only [Bool.false_eq_true, if_false]
    rw [objectsOf_rdf K vp graph H.nodup H.inj hx hs (p := .role owner r)
        (by simp [ownPred, declaresSet_false K H.fieldsUnique hown]),
      ownObjs_role_node K vp ι hc hrel hedge hnd hown, fieldRefs_of_field hw]
    cases opt with
    | false =>
      simp only [Catalog.Catalog.slotType, Bool.false_eq_true, if_false] at hwc ⊢
      obtain ⟨p, sp', hr, hsp, hpv⟩ := participant_round K H graph hwc (hpart w fun _ h => h)
      simp [hr, hsp, hpv]
    | true =>
      simp only [Catalog.Catalog.slotType, if_true] at hwc ⊢
      rcases check_optional hwc with rfl | ⟨u, rfl, hu⟩
      · rfl
      · obtain ⟨p, sp', hr, hsp, hpv⟩ := participant_round K H graph hu
          (hpart u fun p h => by simpa [VTerm.refs] using h)
        simp [VTerm.refs, hr, hsp, hpv]
  | true =>
    simp only [if_true]
    obtain ⟨a, o, n, ha, ho, hn, hao, hroles, ⟨owa, psa, nva, spa, hfa⟩, ⟨owo, pso, nvo, spo, hfo⟩⟩ :=
      H.edges x hx hedge
    have hnamed := H.named x hx
    rw [hs, hc] at hnamed
    have halloc : K.allocatesReifier x.2.1 = true := by
      simp only [Option.isSome_some, Option.isSome, Bool.true_and, tripleNamed, hc, Option.map_some', hrel,
        beq_self_eq_true, Option.getD_some, hedge] at hnamed
      simpa using hnamed
    rw [objectsOf_rdf K vp graph H.nodup H.inj hx hs (p := .reifies) rfl,
      ownObjs_reifies_edge K vp ι hc hrel hedge halloc ha ho hn]
    obtain ⟨_, _, _, hall⟩ := slot_value K H hx hc hconc
    obtain ⟨wa, hwa, hwac⟩ := hall a _ (mem_slots_of_role K hfa)
    obtain ⟨wo, hwo, hwoc⟩ := hall o _ (mem_slots_of_role K hfo)
    simp only [Catalog.Catalog.slotType, Bool.false_eq_true, if_false] at hwac hwoc
    obtain ⟨pa, sa, hra, hsa, hpva⟩ := participant_round K H graph hwac
      fun p hp => H.parts x hx p (refs_field hwa p hp)
    obtain ⟨po, so, hro, hso, hpvo⟩ := participant_round K H graph hwoc
      fun p hp => H.parts x hx p (refs_field hwo p hp)
    rw [fieldRefs_of_field hwa, fieldRefs_of_field hwo, hra, hro]
    simp only [List.flatMap_cons, List.flatMap_nil, List.append_nil, TObj.inst, hsa, hso, Option.bind_some,
      Option.map_some', Option.toList_some, List.filterMap_cons, List.filterMap_nil]
    have hr : r = a ∨ r = o := hroles _ (List.mem_map_of_mem Prod.snd hown) rfl
    rcases hr with rfl | rfl
    · have hsame := eq_of_nodup_key hnd hown hfa rfl
      simp only [Prod.mk.injEq, Field.role.injEq] at hsame
      obtain ⟨-, -, hps, hopt, -, -⟩ := hsame
      subst hps hopt
      rw [hw] at hwa
      cases hwa
      simp [ha, hpva]
    · have hsame := eq_of_nodup_key hnd hown hfo rfl
      simp only [Prod.mk.injEq, Field.role.injEq] at hsame
      obtain ⟨-, -, hps, hopt, -, -⟩ := hsame
      subst hps hopt
      rw [hw] at hwo
      cases hwo
      have : (some r == some a) = false := by simpa using fun h : r = a => hao h.symm
      simp [ha, this, hpvo]

end Roles

/-! ## Interval slots -/

theorem some_bind' {α β : Type} (a : α) (f : α → Option β) : (some a).bind f = f a := rfl

section Intervals

variable {vp : ViewParams T} {ord : VTerm T → VTerm T → Bool} {ι : Nat → Option String} {idOf : String → Nat}
  {G : FinApg K.schema}

omit [DecidableEq T.P] in
theorem ownObjs_bnd (ι : Nat → Option String) (e : Nat) (a : Name) (b : Bound) (P : Pred) (w : Option (VTerm T)) :
    ownObjs ι P (optList w fun w => (attrObjs w).map fun o => (⟨e, .bound a b, o, e⟩ : Tmpl T)) =
      if Pred.bound a b = P then optList w (fun w => (attrObjs w).filterMap fun o => o.inst ι) else [] := by
  cases w with
  | none => split <;> rfl
  | some w =>
    simp only [optList]
    split
    · next h =>
      subst h
      rw [ownObjs_all ι _ _ (by simp), List.filterMap_map]
      rfl
    · next h => exact ownObjs_nil ι _ _ (by simpa using fun _ _ => h)

omit [DecidableEq T.P] in
theorem ownObjs_prec (vp : ViewParams T) (ι : Nat → Option String) (e : Nat) (a : Name) (b : Bound) (P : Pred)
    (w pr : Option (VTerm T)) :
    ownObjs ι P (optList (w.bind asPrim) fun x => optList (pr.bind asPrim) fun y =>
        if vp.precisionShown x.1 y.1 y.2 then [(⟨e, .bound a b, .lit y.1 y.2, e⟩ : Tmpl T)] else []) =
      if Pred.bound a b = P then
        (optList (w.bind asPrim) fun x => optList (pr.bind asPrim) fun y =>
          if vp.precisionShown x.1 y.1 y.2 then [Term.lit y.1 y.2] else [])
      else [] := by
  by_cases h : Pred.bound a b = P
  · subst h
    simp only [if_true]
    rw [ownObjs_optList]
    congr 1
    funext x
    rw [ownObjs_optList]
    congr 1
    funext y
    split <;> simp [ownObjs, TObj.inst]
  · rw [if_neg h]
    apply ownObjs_nil
    intro t ht htp
    obtain ⟨x, _, ht⟩ := mem_optList.mp ht
    obtain ⟨y, _, ht⟩ := mem_optList.mp ht
    split at ht
    · rw [List.mem_singleton] at ht
      subst ht
      exact h htp
    · cases ht

omit [DecidableEq T.P] in
/-- An interval attribute's bound predicates read from its templates: each bound's literal, and each shown
precision's. -/
theorem ownObjs_interval (vp : ViewParams T) (ι : Nat → Option String) (e : Nat) (a : Name) (u : VTerm T) :
    let st := (u.field? "start").bind (presentValue true)
    let sp := (u.field? "end").bind (presentValue true)
    let prec (w pr : Option (VTerm T)) : List (Term T) :=
      optList (w.bind asPrim) fun x => optList (pr.bind asPrim) fun y =>
        if vp.precisionShown x.1 y.1 y.2 then [Term.lit y.1 y.2] else []
    ownObjs ι (.bound a .start) (attrTemplates vp e a .interval u) =
        optList st (fun w => (attrObjs w).filterMap fun o => o.inst ι) ∧
      ownObjs ι (.bound a .stop) (attrTemplates vp e a .interval u) =
        optList sp (fun w => (attrObjs w).filterMap fun o => o.inst ι) ∧
      ownObjs ι (.bound a .startPrecision) (attrTemplates vp e a .interval u) =
        prec st ((u.field? "start_precision").bind (presentValue true)) ∧
      ownObjs ι (.bound a .stopPrecision) (attrTemplates vp e a .interval u) =
        prec sp ((u.field? "end_precision").bind (presentValue true)) := by
  simp only [attrTemplates, ownObjs_append, ownObjs_bnd, ownObjs_prec]
  simp

/-- A bound read back from its literal: absent without one. -/
theorem bound_round (ι : Nat → Option String) {lam : Nat → Option Name} {p : T.P} {w : VTerm T}
    (hw : (denote lam w (NTy.optional (.prim p))).isSome = true) :
    let os := optList (presentValue true w) fun w => (attrObjs w).filterMap fun o => o.inst ι
    optional true os single = w ∧ (os.isEmpty = true ↔ presentValue true w = none) := by
  rcases check_optional hw with rfl | ⟨u, rfl, hu⟩
  · simp [presentValue_absent, optList, optional]
  · obtain ⟨v, rfl⟩ := check_prim hu
    simp [presentValue_some, optList, optional, attrObjs, memberObj, TObj.inst, single, termValue]

/-- A precision read back: its literal where shown, else its bound's type's own. -/
theorem precision_round (vp : ViewParams T) (ι : Nat → Option String) {lam : Nat → Option Name} {p pr : T.P}
    (hnative : ∀ p q y, vp.precisionShown p q y = false ↔ vp.native p = some ⟨q, y⟩) {w ep : VTerm T}
    (hw : (denote lam w (NTy.optional (.prim p))).isSome = true)
    (hep : (denote lam ep (NTy.optional (.prim pr))).isSome = true)
    (hlink : (presentValue true ep).isSome = (presentValue true w).isSome) :
    optional true (optList (presentValue true w) fun w => (attrObjs w).filterMap fun o => o.inst ι)
      (fun bs => precisionOf vp
        (optList ((presentValue true w).bind asPrim) fun x => optList ((presentValue true ep).bind asPrim) fun y =>
          if vp.precisionShown x.1 y.1 y.2 then [Term.lit y.1 y.2] else []) bs) = ep := by
  rcases check_optional hw with rfl | ⟨u, rfl, hu⟩
  · rcases check_optional hep with rfl | ⟨u', rfl, _⟩
    · rfl
    · simp [presentValue_absent, presentValue_some] at hlink
  · obtain ⟨v, rfl⟩ := check_prim hu
    rcases check_optional hep with rfl | ⟨u', rfl, hu'⟩
    · simp [presentValue_absent, presentValue_some] at hlink
    · obtain ⟨y, rfl⟩ := check_prim hu'
      simp only [presentValue_some, Option.bind_some, asPrim, optList, optional, attrObjs, memberObj,
        List.filterMap_cons, TObj.inst, List.filterMap_nil, List.isEmpty_cons, Bool.false_eq_true, if_false]
      cases hs : vp.precisionShown p pr y with
      | true => simp [precisionOf, termValue, asPrim, optList, hs]
      | false =>
        have := (hnative p pr y).mp hs
        simp [precisionOf, single, termValue, this, asPrim, optList, hs]

/-- An interval read back from its bound predicates: each bound and precision, and present when a bound is. -/
theorem intervalValue_round (vp : ViewParams T) (ι : Nat → Option String) {lam : Nat → Option Name}
    (hnative : ∀ p q y, vp.precisionShown p q y = false ↔ vp.native p = some ⟨q, y⟩) {Q : List (Quad T)}
    {s : String} {e : Nat} {a : Name} {t : NTy T.P Name}
    (hok : ∃ p pr, t = intervalTemporal p pr ∨ t = intervalNumeric p)
    {u : VTerm T} (hu : (denote lam u t).isSome = true) (hiok : IntervalOk u)
    (hB : ∀ b, objectsOf Q s (.bound a b) = ownObjs ι (.bound a b) (attrTemplates vp e a .interval u)) :
    intervalValue vp Q s a t = u ∧
      ((objectsOf Q s (.bound a .start)).isEmpty = false ∨ (objectsOf Q s (.bound a .stop)).isEmpty = false) := by
  obtain ⟨h1, h2, h3, h4⟩ := ownObjs_interval vp ι e a u
  obtain ⟨p, pr, rfl | rfl⟩ := hok
  · obtain ⟨ws, rfl, hws⟩ := check_record hu
    obtain ⟨we, ws, rfl, hwe, hws⟩ := denoteRec_cons hws
    obtain ⟨wep, ws, rfl, hwep, hws⟩ := denoteRec_cons hws
    obtain ⟨wst, ws, rfl, hwst, hws⟩ := denoteRec_cons hws
    obtain ⟨wsp, ws, rfl, hwsp, hws⟩ := denoteRec_cons hws
    obtain rfl := denoteRec_nil hws
    have fe := field?_cons_self "end" we [("end_precision", wep), ("start", wst), ("start_precision", wsp)]
    have fep : (VTerm.record [("end", we), ("end_precision", wep), ("start", wst), ("start_precision", wsp)]).field?
        "end_precision" = some wep := by rw [field?_cons_ne _ _ (by decide), field?_cons_self]
    have fst : (VTerm.record [("end", we), ("end_precision", wep), ("start", wst), ("start_precision", wsp)]).field?
        "start" = some wst := by rw [field?_cons_ne _ _ (by decide), field?_cons_ne _ _ (by decide), field?_cons_self]
    have fsp : (VTerm.record [("end", we), ("end_precision", wep), ("start", wst), ("start_precision", wsp)]).field?
        "start_precision" = some wsp := by
      rw [field?_cons_ne _ _ (by decide), field?_cons_ne _ _ (by decide), field?_cons_ne _ _ (by decide),
        field?_cons_self]
    simp only [fe, fep, fst, fsp, Option.bind_some] at h1 h2 h3 h4
    simp only [IntervalOk, fe, fep, fst, fsp, Option.bind_some, Option.isSome_some, forall_const] at hiok
    obtain ⟨hone, hls, hle⟩ := hiok
    have bs := bound_round ι hwst
    have be := bound_round ι hwe
    simp only [] at bs be
    refine ⟨?_, ?_⟩
    · simp only [intervalValue, intervalTemporal, List.map_cons, List.map_nil, hB, h1, h2, h3, h4, some_bind']
      rw [bs.1, be.1, precision_round vp ι hnative hwst hwsp (by simpa using hls),
        precision_round vp ι hnative hwe hwep (by simpa using hle)]
    · rw [hB, hB, h1, h2]
      simp only [some_bind'] at hone ⊢
      rcases hone with h | h
      · left
        exact Bool.eq_false_iff.mpr fun hc => by have := bs.2.mp hc; simp [this] at h
      · right
        exact Bool.eq_false_iff.mpr fun hc => by have := be.2.mp hc; simp [this] at h
  · obtain ⟨ws, rfl, hws⟩ := check_record hu
    obtain ⟨we, ws, rfl, hwe, hws⟩ := denoteRec_cons hws
    obtain ⟨wst, ws, rfl, hwst, hws⟩ := denoteRec_cons hws
    obtain rfl := denoteRec_nil hws
    have fe := field?_cons_self "end" we [("start", wst)]
    have fst : (VTerm.record [("end", we), ("start", wst)]).field? "start" = some wst := by
      rw [field?_cons_ne _ _ (by decide), field?_cons_self]
    simp only [fe, fst, Option.bind_some] at h1 h2
    simp only [IntervalOk, fe, fst, Option.bind_some] at hiok
    obtain ⟨hone, -, -⟩ := hiok
    have bs := bound_round ι hwst
    have be := bound_round ι hwe
    simp only [] at bs be
    refine ⟨?_, ?_⟩
    · simp only [intervalValue, intervalNumeric, List.map_cons, List.map_nil, hB, h1, h2, some_bind']
      rw [bs.1, be.1]
    · rw [hB, hB, h1, h2]
      simp only [some_bind'] at hone ⊢
      rcases hone with h | h
      · left
        exact Bool.eq_false_iff.mpr fun hc => by have := bs.2.mp hc; simp [this] at h
      · right
        exact Bool.eq_false_iff.mpr fun hc => by have := be.2.mp hc; simp [this] at h

/-- Proposition 17.3 for an interval slot: its bounds and precisions read back from the bound predicates. -/
theorem decode_interval (H : RoundTrip K vp ord ι idOf G) (graph : Nat → String) {x : Nat × Name × VTerm T}
    (hx : x ∈ G.elems) {s : String} (hs : ι x.1 = some s) {c : CType T} (hc : K.find x.2.1 = some c) {a : Name}
    {opt : Bool} {t : NTy T.P Name} (hslot : (a, Catalog.Slot.attr opt t .interval) ∈ K.slots x.2.1) {w : VTerm T}
    (hw : x.2.2.field? a = some w) (hwc : (denote G.lam w (K.slotType (.attr opt t .interval))).isSome = true) :
    decodeSlot K vp ord idOf (rdf K vp ι graph G) s x.2.1 (a, .attr opt t .interval) = w := by
  obtain ⟨owner, hown⟩ := mem_slots_attr K hslot
  have hf : Field.attr a t opt .interval ∈ K.allFields x.2.1 := List.mem_map_of_mem Prod.snd hown
  have hok : ∃ p pr, t = intervalTemporal p pr ∨ t = intervalNumeric p := H.attrs x hx owner a t opt _ hown
  have hB0 : ∀ b, objectsOf (rdf K vp ι graph G) s (.bound a b) =
      optList (presentValue opt w) fun u => ownObjs ι (.bound a b) (attrTemplates vp x.1 a .interval u) := by
    intro b
    rw [objectsOf_rdf K vp graph H.nodup H.inj hx hs (p := .bound a b) rfl,
      ownObjs_attr K vp ι hc (H.namesUnique x hx) hf (by simp [attrPred]), hw, some_bind']
  have hiok := H.intervals x hx owner a t opt hown w
  simp only [decodeSlot]
  cases opt with
  | false =>
    simp only [Catalog.Catalog.slotType, Bool.false_eq_true, if_false] at hwc ⊢
    have hB : ∀ b, objectsOf (rdf K vp ι graph G) s (.bound a b) =
        ownObjs ι (.bound a b) (attrTemplates vp x.1 a .interval w) := fun b => by
      rw [hB0 b]
      rfl
    exact (intervalValue_round vp ι H.native hok hwc (hiok w hw rfl) hB).1
  | true =>
    simp only [Catalog.Catalog.slotType, if_true] at hwc ⊢
    rcases check_optional hwc with rfl | ⟨u, rfl, hu⟩
    · simp only [hB0, presentValue_absent, optList, List.isEmpty_nil, Bool.not_true, Bool.false_or, if_false]
      rfl
    · have hB : ∀ b, objectsOf (rdf K vp ι graph G) s (.bound a b) =
          ownObjs ι (.bound a b) (attrTemplates vp x.1 a .interval u) := fun b => by
        rw [hB0 b]
        rfl
      obtain ⟨hval, hne⟩ := intervalValue_round vp ι H.native hok hu (hiok u hw rfl) hB
      have : (!(objectsOf (rdf K vp ι graph G) s (.bound a .start)).isEmpty ||
          !(objectsOf (rdf K vp ι graph G) s (.bound a .stop)).isEmpty) = true := by
        rcases hne with h | h <;> simp [h]
      simp only [this, if_true, hval]

end Intervals

/-! ## An element's value -/

section Elements

variable {vp : ViewParams T} {ord : VTerm T → VTerm T → Bool} {ι : Nat → Option String} {idOf : String → Nat}
  {G : FinApg K.schema}

/-- A record read slot by slot is the record checked against the slots' types. -/
theorem assemble {lam : Nat → Option Name} {D : Name × Catalog.Slot T → VTerm T} :
    (sl : List (Name × Catalog.Slot T)) → (ws : List (Name × VTerm T)) →
    (denote.denoteRec lam ws (sl.map fun y => (y.1, K.slotType y.2))).isSome = true →
    (∀ y ∈ sl, ∀ w, (y.1, w) ∈ ws → (denote lam w (K.slotType y.2)).isSome = true → D y = w) →
    sl.map (fun y => (y.1, D y)) = ws
  | [], ws, h, _ => by rw [denoteRec_nil h]; rfl
  | y :: sl, ws, h, hD => by
    rw [List.map_cons] at h
    obtain ⟨w, ws', rfl, hw, hws'⟩ := denoteRec_cons h
    rw [List.map_cons, hD y (List.mem_cons_self _ _) w (List.mem_cons_self _ _) hw,
      assemble sl ws' hws' fun y' hy' w' hw' hc' =>
        hD y' (List.mem_cons_of_mem _ hy') w' (List.mem_cons_of_mem _ hw') hc']

/-- Proposition 17.3 for one slot. -/
theorem decode_slot (H : RoundTrip K vp ord ι idOf G) (graph : Nat → String) {x : Nat × Name × VTerm T}
    (hx : x ∈ G.elems) {s : String} (hs : ι x.1 = some s) {c : CType T} (hc : K.find x.2.1 = some c)
    (hconc : c.concrete = true) {y : Name × Catalog.Slot T} (hy : y ∈ K.slots x.2.1) {w : VTerm T}
    (hw : x.2.2.field? y.1 = some w) (hwc : (denote G.lam w (K.slotType y.2)).isSome = true) :
    decodeSlot K vp ord idOf (rdf K vp ι graph G) s x.2.1 y = w := by
  obtain ⟨n, sl⟩ := y
  cases sl with
  | attr opt t view =>
    cases view with
    | interval => exact decode_interval K H graph hx hs hc hy hw hwc
    | _ => exact decode_attr K H graph hx hs hc (by simp) hy hw hwc
  | role ps opt owner => exact decode_role K H graph hx hs hc hconc hy hw hwc
  | asserted => exact decode_asserted K H graph hx hs hc hy hw hwc

/-- Proposition 17.3 for an element's value: read back from its quads, it is the element's value. -/
theorem decodeElem_round (H : RoundTrip K vp ord ι idOf G) (graph : Nat → String) {x : Nat × Name × VTerm T}
    (hx : x ∈ G.elems) {s : String} (hs : ι x.1 = some s) {c : CType T} (hc : K.find x.2.1 = some c)
    (hconc : c.concrete = true) :
    decodeElem K vp ord idOf (rdf K vp ι graph G) s x.2.1 = x.2.2 := by
  obtain ⟨ws, hv, hws, _⟩ := slot_value K H hx hc hconc
  have hnd : (ws.map Prod.fst).Nodup := by
    rw [(denoteRec_aligned ws _ hws).1, List.map_map]
    exact H.slotsUnique x hx
  rw [hv]
  unfold decodeElem
  congr 1
  apply assemble K _ ws hws
  intro y hy w hw hwc
  exact decode_slot K H graph hx hs hc hconc hy (by rw [hv]; exact find?_field_of_nodup hnd hw) hwc

end Elements

/-! ## Elements named by IRIs -/

section IriElements

variable {vp : ViewParams T} {ord : VTerm T → VTerm T → Bool} {ι : Nat → Option String} {idOf : String → Nat}
  {G : FinApg K.schema}

omit [DecidableEq T.P] in
theorem inst_eq {name : Nat → Option String} {graph : Nat → String} {t : Tmpl T} {q : Quad T}
    (h : t.inst name graph = some q) : name t.s = some q.s ∧ q.p = t.p ∧ t.o.inst name = some q.o := by
  simp only [Tmpl.inst, Option.bind_eq_some, Option.map_eq_some'] at h
  obtain ⟨s, hs, o, ho, rfl⟩ := h
  exact ⟨hs, rfl, ho⟩

omit [DecidableEq T.P] in
theorem mem_rdf {name : Nat → Option String} {graph : Nat → String} {G : FinApg K.schema} {q : Quad T} :
    q ∈ rdf K vp name graph G ↔ ∃ x ∈ G.elems, ∃ t ∈ templates K vp x.1 x.2.1 x.2.2, t.inst name graph = some q := by
  simp [rdf, List.mem_flatMap, List.mem_filterMap]

omit [DecidableEq T.P] in
/-- A template with `rdf:type` belongs to an element of a declared type. -/
theorem find_of_rdfType {e : Nat} {l : Name} {v : VTerm T} {t : Tmpl T} (ht : t ∈ templates K vp e l v)
    (hp : t.p = .rdfType) : ∃ c, K.find l = some c := by
  unfold templates at ht
  split at ht
  · next c hc => exact ⟨c, hc⟩
  · obtain ⟨x, _, hpx⟩ := roleLabelTemplates_shape K ht
    rw [hp] at hpx
    cases hpx

/-- An element named by an IRI is read back from its first type quad. -/
theorem iri_complete (H : RoundTrip K vp ord ι idOf G) (graph : Nat → String) {x : Nat × Name × VTerm T}
    (hx : x ∈ G.elems) {s : String} (hs : ι x.1 = some s) :
    (Ident.iri s, x.2.1, x.2.2) ∈ (rdf K vp ι graph G).filterMap (iriElem K vp ord idOf (rdf K vp ι graph G)) := by
  have hn := H.named x hx
  rw [hs] at hn
  obtain ⟨c, hc⟩ : ∃ c, K.find x.2.1 = some c := by
    cases h : K.find x.2.1 with
    | none => simp [h] at hn
    | some c => exact ⟨c, rfl⟩
  have hconc : c.concrete = true := by
    rcases H.labels x hx with ⟨c', hc', hconc⟩ | ⟨hnone, _⟩
    · rw [hc] at hc'
      cases hc'
      exact hconc
    · rw [hc] at hnone
      cases hnone
  refine List.mem_filterMap.mpr ⟨⟨s, .rdfType, .cls x.2.1, graph x.1⟩, ?_, ?_⟩
  · rw [mem_rdf]
    refine ⟨x, hx, ⟨x.1, .rdfType, .cls x.2.1, x.1⟩, ?_, by simp [Tmpl.inst, hs, TObj.inst]⟩
    unfold templates
    rw [hc]
    simp only [List.mem_append]
    left; left; left
    obtain ⟨rest, hrest⟩ := chain_head K x.2.1
    simp [typeTemplates, hrest]
  · simp only [iriElem, beq_self_eq_true, if_true]
    rw [labelOf_rdf K vp graph H.nodup H.inj hx hs hc]
    simp only [Option.map_some']
    rw [decodeElem_round K H graph hx hs hc hconc]

/-- Every element read back from a type quad is an element named by an IRI, with its label and value. -/
theorem iri_sound (H : RoundTrip K vp ord ι idOf G) (graph : Nat → String) {y : Ident × Name × VTerm T}
    (hy : y ∈ (rdf K vp ι graph G).filterMap (iriElem K vp ord idOf (rdf K vp ι graph G))) :
    ∃ x ∈ G.elems, ident K ι x = some y.1 ∧ x.2 = y.2 := by
  obtain ⟨q, hq, hqy⟩ := List.mem_filterMap.mp hy
  simp only [iriElem] at hqy
  split at hqy
  · next hp =>
    obtain ⟨x, hx, t, ht, htq⟩ := (mem_rdf K).mp hq
    obtain ⟨hts, htp, _⟩ := inst_eq htq
    have hp' : q.p = .rdfType := by simpa using hp
    have htr : t.p = .rdfType := htp ▸ hp'
    obtain ⟨c, hc⟩ := find_of_rdfType K ht htr
    have hsub := templates_subject K vp ht (by rw [htr]; rfl)
    rw [hsub] at hts
    have hconc : c.concrete = true := by
      rcases H.labels x hx with ⟨c', hc', hconc⟩ | ⟨hnone, _⟩
      · rw [hc] at hc'
        cases hc'
        exact hconc
      · rw [hc] at hnone
        cases hnone
    rw [labelOf_rdf K vp graph H.nodup H.inj hx hts hc] at hqy
    simp only [Option.map_some', Option.some.injEq] at hqy
    subst hqy
    have hn := H.named x hx
    rw [hts, hc] at hn
    have htn : tripleNamed K x.2.1 = false := by simpa using hn
    refine ⟨x, hx, ?_, ?_⟩
    · simp [ident, hc, htn, hts]
    · simp only [decodeElem_round K H graph hx hts hc hconc]
  · cases hqy

end IriElements

/-! ## Elements of role labels -/

section PairElements

variable {vp : ViewParams T} {ord : VTerm T → VTerm T → Bool} {ι : Nat → Option String} {idOf : String → Nat}
  {G : FinApg K.schema}

omit [DecidableEq T.P] in
theorem schema_roleLabel {l : Name} {y : Name × Name × Name × List Name} (hn : K.find l = none)
    (hy : K.roleLabelOf l = some y) : K.schema.σ l = roleLabelType y.1 (K.participantType y.2.2.2) := by
  show (match K.find l with
    | some c => _
    | none => match K.roleLabelOf l with
      | some (R, _, _, ps) => roleLabelType R (K.participantType ps)
      | none => NTy.unit) = _
  rw [hn, hy]

omit [DecidableEq T.P] in
/-- A role label is named after its relation and role, and its role is a set-valued role the declaring type declares
with the role label's participants. -/
theorem roleLabelOf_spec {l : Name} {y : Name × Name × Name × List Name} (h : K.roleLabelOf l = some y) :
    Catalog.roleLabelName y.1 y.2.2.1 = l ∧ ∃ c sp, K.find y.2.1 = some c ∧ .setRole y.2.2.1 y.2.2.2 sp ∈ c.fields := by
  unfold Catalog.roleLabelOf at h
  obtain ⟨c, _, hc⟩ := List.exists_of_findSome?_eq_some h
  split at hc
  · obtain ⟨z, hz, hzy⟩ := List.exists_of_findSome?_eq_some hc
    match z, hz, hzy with
    | (owner, .setRole r ps sp), hz, hzy =>
      simp only at hzy
      split at hzy
      · next hname =>
        cases hzy
        obtain ⟨d, hd, hf⟩ := mem_ownedFields K hz
        exact ⟨hname, d, sp, hd, hf⟩
      · cases hzy
    | (_, .attr _ _ _ _), _, hzy => cases hzy
    | (_, .role _ _ _ _ _), _, hzy => cases hzy
  · cases hc

omit [DecidableEq T.P] in
/-- The set-valued role a type declares, found by its name. -/
theorem findSome_setRole {c : CType T} (hnd : (c.fields.map Field.name).Nodup) {r : Name} {ps : List Name}
    {sp : Option Name} (hf : .setRole r ps sp ∈ c.fields) :
    c.fields.findSome? (setRoleOf r) = some ps := by
  cases h : c.fields.findSome? (setRoleOf r) with
  | none =>
    rw [List.findSome?_eq_none_iff] at h
    have := h _ hf
    simp [setRoleOf] at this
  | some ps' =>
    obtain ⟨f, hfm, hfe⟩ := List.exists_of_findSome?_eq_some h
    match f, hfm, hfe with
    | .setRole m ps'' sp', hfm, hfe =>
      simp only [setRoleOf, beq_iff_eq] at hfe
      split at hfe
      · next hm =>
        cases hfe
        have := eq_of_nodup_name hnd hfm hf (by simp [Field.name, hm])
        cases this
        rfl
      · cases hfe
    | .attr _ _ _ _, _, hfe => cases hfe
    | .role _ _ _ _ _, _, hfe => cases hfe

/-- A role label's element: its instance and its participant. -/
theorem roleLabel_value (H : RoundTrip K vp ord ι idOf G) {x : Nat × Name × VTerm T} (hx : x ∈ G.elems)
    (hn : K.find x.2.1 = none) {y : Name × Name × Name × List Name} (hy : K.roleLabelOf x.2.1 = some y) :
    ∃ i wp, x.2.2 = .record [("instance", .elem i), ("participant", wp)] ∧ G.lam i = some y.1 ∧
      (denote G.lam wp (K.participantType y.2.2.2)).isSome = true := by
  have hchk := H.check hx
  rw [schema_roleLabel K hn hy] at hchk
  obtain ⟨ws, hv, hws⟩ := check_record hchk
  obtain ⟨wi, ws, rfl, hwi, hws⟩ := denoteRec_cons hws
  obtain ⟨wp, ws, rfl, hwp, hws⟩ := denoteRec_cons hws
  obtain rfl := denoteRec_nil hws
  obtain ⟨i, rfl, hi⟩ := check_lbl hwi
  exact ⟨i, wp, hv, hi, hwp⟩

/-- A role label's element read back from one of its role triples: its pair, its label and its value. -/
theorem pairElem_round (H : RoundTrip K vp ord ι idOf G) (graph : Nat → String) {x : Nat × Name × VTerm T}
    (hx : x ∈ G.elems) (hn : K.find x.2.1 = none) {y : Name × Name × Name × List Name}
    (hy : K.roleLabelOf x.2.1 = some y) :
    ∃ q ∈ rdf K vp ι graph G, ∃ i, pairElem K idOf (rdf K vp ι graph G) q = some (i, x.2.1, x.2.2) ∧
      ident K ι x = some i ∧
      ∀ q' ∈ rdf K vp ι graph G, q'.p = .role y.2.1 y.2.2.1 →
        (∃ t ∈ templates K vp x.1 x.2.1 x.2.2, t.inst ι graph = some q') → q' = q := by
  obtain ⟨i, wp, hv, hi, hwp⟩ := roleLabel_value K H hx hn hy
  have hrefs : ∀ p ∈ wp.refs, p ∈ x.2.2.refs := by
    intro p hp
    rw [hv]
    simp [VTerm.refs, VTerm.refs.refsFields, hp]
  obtain ⟨p, sp, hrp, hsp, hpv⟩ := participant_round K H graph hwp fun p hp => H.parts x hx p (hrefs p hp)
  obtain ⟨yi, hyi, rfl, hfi, hti⟩ := H.parts x hx i (by rw [hv]; simp [VTerm.refs, VTerm.refs.refsFields])
  have hni := H.named yi hyi
  rw [hfi, hti] at hni
  obtain ⟨si, hsi⟩ := Option.isSome_iff_exists.mp (by simpa using hni)
  obtain ⟨ci, hci⟩ := Option.isSome_iff_exists.mp hfi
  have hlabi : labelOf K (rdf K vp ι graph G) si = some y.1 := by
    rw [labelOf_rdf K vp graph H.nodup H.inj hyi hsi hci, ← lam_of_mem H.nodup hyi, hi]
  obtain ⟨hname, c, spr, hc, hf⟩ := roleLabelOf_spec K hy
  have hcnd := H.fieldsUnique c (List.mem_of_find?_eq_some hc)
  have hfi' : fieldRefs x.2.2 "instance" = [yi.1] := by rw [hv]; rfl
  have hfp : fieldRefs x.2.2 "participant" = [p] := by
    rw [hv]
    simp [fieldRefs, VTerm.field?, hrp]
  let q : Quad T := ⟨si, .role y.2.1 y.2.2.1, .iri sp, graph x.1⟩
  have hmem : (⟨yi.1, .role y.2.1 y.2.2.1, .elem p, x.1⟩ : Tmpl T) ∈ templates K vp x.1 x.2.1 x.2.2 := by
    unfold templates
    rw [hn]
    simp [roleLabelTemplates, hy, optList, hfi', hfp]
  refine ⟨q, (mem_rdf K).mpr ⟨x, hx, _, hmem, by simp [Tmpl.inst, hsi, hsp, TObj.inst, q]⟩,
    .pair x.2.1 si sp, ?_, ?_, ?_⟩
  · show pairElem K idOf (rdf K vp ι graph G) ⟨si, .role y.2.1 y.2.2.1, .iri sp, graph x.1⟩ = _
    unfold pairElem
    simp only [hc, some_bind', findSome_setRole hcnd hf, hlabi, Option.map_some', hname]
    rw [H.inv yi hyi si hsi, hpv, hv]
  · simp [ident, hn, hfi', hfp, hsi, hsp]
  · intro q' _ _ ⟨t, ht, htq'⟩
    unfold templates at ht
    rw [hn] at ht
    simp only [roleLabelTemplates, hy, optList, hfi', hfp, List.flatMap_cons, List.flatMap_nil, List.map_cons,
      List.map_nil, List.append_nil, List.mem_singleton] at ht
    subst ht
    simp only [Tmpl.inst, hsi, hsp, TObj.inst, some_bind', Option.map_some', Option.some.injEq] at htq'
    exact htq'.symm

omit [DecidableEq T.P] in
/-- A role predicate on an element of a declared type is a single-valued role's. -/
theorem role_of_declared {c : CType T} {e : Nat} {l : Name} {v : VTerm T} {t : Tmpl T}
    (ht : t ∈ templates K vp e l v) (hc : K.find l = some c) {owner r : Name} (hp : t.p = .role owner r) :
    ∃ ps o nv sp, (owner, .role r ps o nv sp) ∈ K.ownedFields l := by
  unfold templates at ht
  rw [hc] at ht
  simp only [List.mem_append] at ht
  rcases ht with ((ht | ht) | ht) | ht
  · have := (typeTemplates_shape K ht).2
    rw [hp] at this
    cases this
  · simp only [attrsTemplates, List.mem_flatMap] at ht
    obtain ⟨f, _, ht⟩ := ht
    match f, ht with
    | .attr a _ opt view, ht =>
      obtain ⟨u, _, ht⟩ := mem_optList.mp ht
      have := (attrTemplates_shape vp ht).2
      rw [hp] at this
      simp [attrPred] at this
    | .role _ _ _ _ _, ht => cases ht
    | .setRole _ _ _, ht => cases ht
  · split at ht
    · have := (assertedTemplates_shape ht).2
      rw [hp] at this
      simp [assertedPred] at this
    · cases ht
  · unfold relTemplates at ht
    split at ht
    · split at ht
      · rcases edgeTemplates_shape K ht with h | h
        · rw [hp] at h
          simp [isNav] at h
        · rw [hp] at h
          cases h.1
      · obtain ⟨_, owner', r', ps, o, nv, sp, h, hm⟩ := nodeTemplates_shape K ht
        rw [hp] at h
        simp only [Pred.role.injEq] at h
        obtain ⟨rfl, rfl⟩ := h
        exact ⟨ps, o, nv, sp, hm⟩
    · cases ht

theorem pair_complete (H : RoundTrip K vp ord ι idOf G) (graph : Nat → String) {x : Nat × Name × VTerm T}
    (hx : x ∈ G.elems) (hn : K.find x.2.1 = none) :
    ∃ i, ident K ι x = some i ∧
      (i, x.2.1, x.2.2) ∈ (rdf K vp ι graph G).filterMap (pairElem K idOf (rdf K vp ι graph G)) := by
  obtain ⟨y, hy⟩ : ∃ y, K.roleLabelOf x.2.1 = some y := by
    rcases H.labels x hx with ⟨c, hc, _⟩ | ⟨_, hr⟩
    · rw [hc] at hn
      cases hn
    · exact Option.isSome_iff_exists.mp hr
  obtain ⟨q, hq, i, hpe, hid, _⟩ := pairElem_round K H graph hx hn hy
  exact ⟨i, hid, List.mem_filterMap.mpr ⟨q, hq, hpe⟩⟩

theorem pair_sound (H : RoundTrip K vp ord ι idOf G) (graph : Nat → String) {z : Ident × Name × VTerm T}
    (hz : z ∈ (rdf K vp ι graph G).filterMap (pairElem K idOf (rdf K vp ι graph G))) :
    ∃ x ∈ G.elems, ident K ι x = some z.1 ∧ x.2 = z.2 := by
  obtain ⟨q, hq, hqz⟩ := List.mem_filterMap.mp hz
  obtain ⟨x, hx, t, ht, htq⟩ := (mem_rdf K).mp hq
  obtain ⟨_, hqp, _⟩ := inst_eq htq
  unfold pairElem at hqz
  split at hqz
  · next owner r sp hpq hoq =>
    obtain ⟨c', hc', hqz⟩ := Option.bind_eq_some.mp hqz
    obtain ⟨ps, hps, _⟩ := Option.bind_eq_some.mp hqz
    have hdecl : declaresSet K owner r = true := by
      obtain ⟨f, hf, hfe⟩ := List.exists_of_findSome?_eq_some hps
      simp only [declaresSet, hc', List.any_eq_true]
      match f, hf, hfe with
      | .setRole m _ _, hf, hfe =>
        simp only [setRoleOf, beq_iff_eq] at hfe
        split at hfe
        · next hm => exact ⟨_, hf, by simp [hm]⟩
        · cases hfe
      | .attr _ _ _ _, _, hfe => cases hfe
      | .role _ _ _ _ _, _, hfe => cases hfe
    have htp : t.p = .role owner r := by rw [← hqp, hpq]
    cases hfind : K.find x.2.1 with
    | some c =>
      obtain ⟨ps', o, nv, sp', hm⟩ := role_of_declared K ht hfind htp
      have := declaresSet_false K H.fieldsUnique hm
      rw [hdecl] at this
      cases this
    | none =>
      obtain ⟨y, hy⟩ : ∃ y, K.roleLabelOf x.2.1 = some y := by
        rcases H.labels x hx with ⟨c, hc, _⟩ | ⟨_, hr⟩
        · rw [hc] at hfind
          cases hfind
        · exact Option.isSome_iff_exists.mp hr
      obtain ⟨q0, hq0, i, hpe, hid, huniq⟩ := pairElem_round K H graph hx hfind hy
      have hyp : t.p = .role y.2.1 y.2.2.1 := by
        unfold templates at ht
        rw [hfind] at ht
        obtain ⟨y', hy', hp'⟩ := roleLabelTemplates_shape K ht
        rw [hy] at hy'
        cases hy'
        exact hp'
      have hqq : q = q0 := huniq q hq (hqp.trans hyp) ⟨t, ht, htq⟩
      subst hqq
      have : pairElem K idOf (rdf K vp ι graph G) q = some z := by
        unfold pairElem
        rw [hpq, hoq]
        exact Option.bind_eq_some.mpr ⟨c', hc', hqz⟩
      rw [hpe] at this
      cases this
      exact ⟨x, hx, hid, rfl⟩
  · cases hqz

end PairElements

/-! ## Elements named by triples: supporting facts -/

section TripleFacts

variable {vp : ViewParams T} {ord : VTerm T → VTerm T → Bool} {ι : Nat → Option String} {idOf : String → Nat}
  {G : FinApg K.schema}

theorem char_antisymm (x y : Char) (h1 : ¬ x < y) (h2 : ¬ y < x) : x = y := by
  apply Char.eq_of_val_eq
  have a : ¬ x.val.toNat < y.val.toNat := h1
  have b : ¬ y.val.toNat < x.val.toNat := h2
  have : x.val.toNat = y.val.toNat := by omega
  exact UInt32.toNat.inj this

/-- The order of strings is antisymmetric. -/
theorem string_antisymm (a b : String) (h1 : a ≤ b) (h2 : b ≤ a) : a = b := by
  haveI : Std.Antisymm (fun x y : Char => ¬ x < y) := ⟨fun {x y} h1 h2 => char_antisymm x y h1 h2⟩
  apply String.ext
  exact List.le_antisymm (as := a.data) (bs := b.data) (by simpa [String.lt_iff] using h1)
    (by simpa [String.lt_iff] using h2)

theorem mem_dedup {n : Name} : (l : List Name) → n ∈ Catalog.dedup l → n ∈ l
  | [], h => h
  | m :: ms, h => by
    simp only [Catalog.dedup] at h
    split at h
    · exact List.mem_cons_of_mem _ (mem_dedup ms h)
    · rcases List.mem_cons.mp h with rfl | h
      · exact List.mem_cons_self _ _
      · exact List.mem_cons_of_mem _ (mem_dedup ms h)

omit [DecidableEq T.P] in
theorem mem_participants {ps : List Name} {lp : Name}
    (h : lp ∈ Catalog.dedup (ps.foldr (fun p acc => K.conc p ++ acc) [])) :
    ∃ q ∈ ps, (K.chain lp).contains q = true := by
  have h' := mem_dedup _ h
  clear h
  induction ps with
  | nil => cases h'
  | cons q qs ih =>
    rw [List.foldr_cons, List.mem_append] at h'
    rcases h' with h' | h'
    · obtain ⟨c, _, rfl, _, hsub⟩ := (K.mem_conc _ _).mp h'
      exact ⟨q, List.mem_cons_self _ _, hsub⟩
    · obtain ⟨q', hq', hc⟩ := ih h'
      exact ⟨q', List.mem_cons_of_mem _ hq', hc⟩

omit [DecidableEq T.P] in
theorem participantType_lbl {ps : List Name} {c : Name} (h : K.participantType ps = .lbl c) :
    c ∈ Catalog.dedup (ps.foldr (fun p acc => K.conc p ++ acc) []) := by
  unfold Catalog.participantType at h
  split at h
  · next c' hc =>
    cases h
    rw [hc]
    exact List.mem_singleton_self _
  · cases h

omit [DecidableEq T.P] in
theorem participantType_variant {ps : List Name} {fs : List (Name × NTy T.P Name)}
    (h : K.participantType ps = .variant fs) {n : Name} {t : NTy T.P Name} (hm : (n, t) ∈ fs) :
    n ∈ Catalog.dedup (ps.foldr (fun p acc => K.conc p ++ acc) []) ∧ t = .lbl n := by
  unfold Catalog.participantType at h
  split at h
  · cases h
  · cases h
    rw [mem_sortBy, List.mem_map] at hm
    obtain ⟨c, hc, he⟩ := hm
    simp only [Prod.mk.injEq] at he
    obtain ⟨rfl, rfl⟩ := he
    exact ⟨hc, rfl⟩

/-- A participant's label lies at or below a participant type its role admits. -/
theorem participant_admitted {lam : Nat → Option Name} {ps : List Name} {w : VTerm T}
    (hw : (denote lam w (K.participantType ps)).isSome = true) {p : Nat} (hp : w.refs = [p]) {lp : Name}
    (hlp : lam p = some lp) : ∃ q ∈ ps, (K.chain lp).contains q = true := by
  cases hpt : K.participantType ps with
  | lbl c =>
    rw [hpt] at hw
    obtain ⟨p', rfl, hl⟩ := check_lbl hw
    simp only [VTerm.refs, List.cons.injEq, and_true] at hp
    subst hp
    rw [hlp] at hl
    cases hl
    exact mem_participants K (participantType_lbl K hpt)
  | variant fs =>
    rw [hpt] at hw
    obtain ⟨n, u, t, rfl, hm, hu⟩ := check_variant hw
    obtain ⟨hn, rfl⟩ := participantType_variant K hpt hm
    obtain ⟨p', rfl, hl⟩ := check_lbl hu
    simp only [VTerm.refs, List.cons.injEq, and_true] at hp
    subst hp
    rw [hlp] at hl
    cases hl
    exact mem_participants K hn
  | _ =>
    rw [hpt] at hw
    first
    | simp [denote] at hw
    | (unfold Catalog.participantType at hpt; split at hpt <;> cases hpt)

omit [DecidableEq T.P] in
/-- A navigation predicate comes from an edge-encoded relation's template: its triple from the anchor, or the reverse
on a `@symmetric` relation. -/
theorem nav_template {e : Nat} {l : Name} {v : VTerm T} {t : Tmpl T} (ht : t ∈ templates K vp e l v) {n : Name}
    (hp : t.p = .nav n) :
    ∃ c a o, K.find l = some c ∧ c.kind = .relation ∧ K.edgeEncoded l = true ∧ K.anchorRole l = some a ∧
      K.otherRole l = some o ∧ K.navOf l a = some n ∧ ∃ pa ∈ fieldRefs v a, ∃ po ∈ fieldRefs v o, t.g = e ∧
      ((t.s = pa ∧ t.o = .elem po) ∨ (K.inherits CType.symmetric l = true ∧ t.s = po ∧ t.o = .elem pa)) := by
  unfold templates at ht
  split at ht
  · next c hc =>
    simp only [List.mem_append] at ht
    rcases ht with ((ht | ht) | ht) | ht
    · have := (typeTemplates_shape K ht).2
      rw [hp] at this
      cases this
    · simp only [attrsTemplates, List.mem_flatMap] at ht
      obtain ⟨f, _, ht⟩ := ht
      match f, ht with
      | .attr a _ opt view, ht =>
        obtain ⟨u, _, ht⟩ := mem_optList.mp ht
        have := (attrTemplates_shape vp ht).2
        rw [hp] at this
        simp [attrPred] at this
      | .role _ _ _ _ _, ht => cases ht
      | .setRole _ _ _, ht => cases ht
    · split at ht
      · have := (assertedTemplates_shape ht).2
        rw [hp] at this
        simp [assertedPred] at this
      · cases ht
    · unfold relTemplates at ht
      split at ht
      · next hrel =>
        split at ht
        · next hedge =>
          unfold edgeTemplates at ht
          obtain ⟨a, ha, ht⟩ := mem_optList.mp ht
          obtain ⟨o, ho, ht⟩ := mem_optList.mp ht
          obtain ⟨n', hn', ht⟩ := mem_optList.mp ht
          simp only [List.mem_flatMap, List.mem_append, List.mem_singleton] at ht
          obtain ⟨pa, hpa, po, hpo, ht⟩ := ht
          refine ⟨c, a, o, hc, by simpa using hrel, hedge, ha, ho, ?_⟩
          rcases ht with (rfl | ht) | ht
          · simp only [Pred.nav.injEq] at hp
            subst hp
            exact ⟨hn', pa, hpa, po, hpo, rfl, Or.inl ⟨rfl, rfl⟩⟩
          · split at ht
            · next hs =>
              rw [List.mem_singleton] at ht
              subst ht
              simp only [Pred.nav.injEq] at hp
              subst hp
              exact ⟨hn', pa, hpa, po, hpo, rfl, Or.inr ⟨hs, rfl, rfl⟩⟩
            · cases ht
          · split at ht
            · rw [List.mem_singleton] at ht
              subst ht
              cases hp
            · cases ht
        · obtain ⟨_, _, _, _, _, _, _, h, _⟩ := nodeTemplates_shape K ht
          rw [hp] at h
          cases h
      · cases ht
  · obtain ⟨_, _, h⟩ := roleLabelTemplates_shape K ht
    rw [hp] at h
    cases h

/-- What an edge-encoded element holds: its anchor and other role, each with its participant, named, labelled and
admitted. -/
theorem edge_roles (H : RoundTrip K vp ord ι idOf G) (graph : Nat → String) {x : Nat × Name × VTerm T}
    (hx : x ∈ G.elems) {c : CType T} (hc : K.find x.2.1 = some c) (hconc : c.concrete = true)
    (hedge : K.edgeEncoded x.2.1 = true) :
    ∃ a o n, K.anchorRole x.2.1 = some a ∧ K.otherRole x.2.1 = some o ∧ K.navOf x.2.1 a = some n ∧ a ≠ o ∧
      (∀ f ∈ K.allFields x.2.1, f.isRole = true → f.name = a ∨ f.name = o) ∧
      ∃ owa psa nva spa owo pso nvo spo,
        (owa, .role a psa false nva spa) ∈ K.ownedFields x.2.1 ∧
        (owo, .role o pso false nvo spo) ∈ K.ownedFields x.2.1 ∧
      ∃ wa wo pa po sa so lpa lpo,
        x.2.2.field? a = some wa ∧ x.2.2.field? o = some wo ∧ wa.refs = [pa] ∧ wo.refs = [po] ∧
        ι pa = some sa ∧ ι po = some so ∧
        participantValue K idOf (rdf K vp ι graph G) psa sa = wa ∧
        participantValue K idOf (rdf K vp ι graph G) pso so = wo ∧
        labelOf K (rdf K vp ι graph G) sa = some lpa ∧ labelOf K (rdf K vp ι graph G) so = some lpo ∧
        admitsAnchor K x.2.1 lpa = true ∧ admitsOther K x.2.1 lpo = true ∧
        (∃ y ∈ G.elems, y.1 = pa) ∧ (∃ y ∈ G.elems, y.1 = po) := by
  obtain ⟨a, o, n, ha, ho, hn, hao, hroles, ⟨owa, psa, nva, spa, hfa⟩, ⟨owo, pso, nvo, spo, hfo⟩⟩ :=
    H.edges x hx hedge
  obtain ⟨_, _, _, hall⟩ := slot_value K H hx hc hconc
  obtain ⟨wa, hwa, hwac⟩ := hall a _ (mem_slots_of_role K hfa)
  obtain ⟨wo, hwo, hwoc⟩ := hall o _ (mem_slots_of_role K hfo)
  simp only [Catalog.Catalog.slotType, Bool.false_eq_true, if_false] at hwac hwoc
  obtain ⟨pa, sa, hra, hsa, hpva⟩ := participant_round K H graph hwac
    fun p hp => H.parts x hx p (refs_field hwa p hp)
  obtain ⟨po, so, hro, hso, hpvo⟩ := participant_round K H graph hwoc
    fun p hp => H.parts x hx p (refs_field hwo p hp)
  obtain ⟨ya, hya, hya1, hfa', _⟩ := H.parts x hx pa (refs_field hwa pa (by simp [hra]))
  obtain ⟨yo, hyo, hyo1, hfo', _⟩ := H.parts x hx po (refs_field hwo po (by simp [hro]))
  obtain ⟨ca, hca⟩ := Option.isSome_iff_exists.mp hfa'
  obtain ⟨co, hco⟩ := Option.isSome_iff_exists.mp hfo'
  subst hya1 hyo1
  have hla := labelOf_rdf K vp graph H.nodup H.inj hya hsa hca
  have hlo := labelOf_rdf K vp graph H.nodup H.inj hyo hso hco
  obtain ⟨qa, hqa, hqac⟩ := participant_admitted K hwac hra (lam_of_mem H.nodup hya)
  obtain ⟨qo, hqo, hqoc⟩ := participant_admitted K hwoc hro (lam_of_mem H.nodup hyo)
  refine ⟨a, o, n, ha, ho, hn, hao, hroles, owa, psa, nva, spa, owo, pso, nvo, spo, hfa, hfo,
    wa, wo, ya.1, yo.1, sa, so, ya.2.1, yo.2.1, hwa, hwo, hra, hro, hsa, hso, hpva, hpvo, hla, hlo, ?_, ?_,
    ⟨ya, hya, rfl⟩, ⟨yo, hyo, rfl⟩⟩
  · simp only [admitsAnchor, List.any_eq_true, Bool.and_eq_true, beq_iff_eq]
    exact ⟨_, List.mem_map_of_mem Prod.snd hfa, by simp [Field.name, ha], ⟨qa, hqa, hqac⟩⟩
  · simp only [admitsOther, List.any_eq_true, Bool.and_eq_true, beq_iff_eq]
    exact ⟨_, List.mem_map_of_mem Prod.snd hfo, by simp [Field.name, ho], ⟨qo, hqo, hqoc⟩⟩

omit [DecidableEq T.P] in
theorem find_name {l : Name} {c : CType T} (hc : K.find l = some c) : c.name = l ∧ c ∈ K.types := by
  have h1 := List.find?_some hc
  simp only [beq_iff_eq] at h1
  exact ⟨h1, List.mem_of_find?_eq_some hc⟩

omit [DecidableEq T.P] in
theorem allocatesReifier_of_attr {l : Name} {f : Field T} (hf : f ∈ K.allFields l) (hr : f.isRole = false) :
    K.allocatesReifier l = true := by
  simp only [Catalog.Catalog.allocatesReifier, Bool.or_eq_true, List.any_eq_true]
  exact Or.inl (Or.inl (Or.inl (Or.inl ⟨f, hf, by simp [hr]⟩)))

/-- An element named by a triple is read back from its navigation quad: its triple, its label and its value. -/
theorem tripleElem_round (H : RoundTrip K vp ord ι idOf G) (graph : Nat → String) {x : Nat × Name × VTerm T}
    (hx : x ∈ G.elems) {c : CType T} (hc : K.find x.2.1 = some c) (hconc : c.concrete = true)
    (htn : tripleNamed K x.2.1 = true) :
    ∃ a o n pa po sa so, K.anchorRole x.2.1 = some a ∧ K.otherRole x.2.1 = some o ∧ K.navOf x.2.1 a = some n ∧
      fieldRefs x.2.2 a = [pa] ∧ fieldRefs x.2.2 o = [po] ∧ ι pa = some sa ∧ ι po = some so ∧
      (∀ g, tripleElem K idOf (rdf K vp ι graph G) ⟨sa, .nav n, .iri so, g⟩ =
        some (.triple sa n so, x.2.1, x.2.2)) ∧
      ident K ι x = some (.triple sa n so) := by
  have htn' := htn
  simp only [tripleNamed, hc, Option.map_some', Option.getD_some, Bool.and_eq_true, beq_iff_eq,
    Bool.not_eq_true'] at htn'
  obtain ⟨⟨hrel, hedge⟩, halloc⟩ := htn'
  obtain ⟨a, o, n, ha, ho, hn, hao, hroles, owa, psa, nva, spa, owo, pso, nvo, spo, hfa, hfo,
    wa, wo, pa, po, sa, so, lpa, lpo, hwa, hwo, hra, hro, hsa, hso, hpva, hpvo, hla, hlo, hadA, hadO, _, _⟩ :=
    edge_roles K H graph hx hc hconc hedge
  have hfra : fieldRefs x.2.2 a = [pa] := by rw [fieldRefs_of_field hwa, hra]
  have hfro : fieldRefs x.2.2 o = [po] := by rw [fieldRefs_of_field hwo, hro]
  obtain ⟨hname, hcmem⟩ := find_name K hc
  have hP : tripleMatch K n sa so lpa lpo c = true := by
    simp only [tripleMatch, hconc, hname, htn, ha, hn, Option.bind_some, beq_self_eq_true, hadA, hadO,
      Bool.true_and, Bool.and_true]
    cases hsym : K.inherits CType.symmetric x.2.1 with
    | false => simp [some_bind', hn]
    | true => simpa [some_bind', hn] using H.symm x hx htn hsym a o pa po sa so ha ho hfra hfro hsa hso
  obtain ⟨c'', hfind⟩ : ∃ c'', K.types.find? (tripleMatch K n sa so lpa lpo) = some c'' :=
    Option.isSome_iff_exists.mp (List.find?_isSome.mpr ⟨c, hcmem, hP⟩)
  have hP'' := List.find?_some hfind
  have hmem'' := List.mem_of_find?_eq_some hfind
  simp only [tripleMatch, Bool.and_eq_true, beq_iff_eq] at hP''
  obtain ⟨⟨⟨⟨⟨hc''conc, hc''tn⟩, hc''nav⟩, hc''adA⟩, _⟩, _⟩ := hP''
  have hsame : c''.name = c.name := H.navUnique c'' hmem'' c hcmem hc''conc hconc hc''tn (hname ▸ hedge) n lpa
    hc''nav (by rw [hname, ha]; simpa using hn) hc''adA (Or.inl (hname ▸ hadA))
  rw [hname] at hsame
  obtain ⟨_, hv, hws, _⟩ := slot_value K H hx hc hconc
  obtain ⟨ws, _⟩ : ∃ ws, x.2.2 = .record ws := ⟨_, hv⟩
  have hnd : ((K.ownedFields x.2.1).map fun y => y.2.name).Nodup := by
    have := H.namesUnique x hx
    simpa [Catalog.Catalog.allFields, List.map_map, Function.comp_def] using this
  refine ⟨a, o, n, pa, po, sa, so, ha, ho, hn, hfra, hfro, hsa, hso, fun g => ?_, ?_⟩
  · simp only [tripleElem, hla, hlo, some_bind', hfind, hsame, ha, ho, Option.map_some', Option.some.injEq,
      Prod.mk.injEq, true_and]
    obtain ⟨ws', hv', hws', _⟩ := slot_value K H hx hc hconc
    have hnd' : (ws'.map Prod.fst).Nodup := by
      rw [(denoteRec_aligned ws' _ hws').1, List.map_map]
      exact H.slotsUnique x hx
    rw [hv']
    congr 1
    apply assemble K _ ws' hws'
    intro y hy w hw hwc
    have hfw : x.2.2.field? y.1 = some w := by rw [hv']; exact find?_field_of_nodup hnd' hw
    obtain ⟨r, sl⟩ := y
    cases sl with
    | attr opt t view =>
      obtain ⟨_, hown⟩ := mem_slots_attr K hy
      have := allocatesReifier_of_attr K (List.mem_map_of_mem Prod.snd hown) rfl
      rw [halloc] at this
      cases this
    | asserted =>
      obtain ⟨_, hb⟩ := mem_slots_asserted K hy
      have : K.allocatesReifier x.2.1 = true := by
        simp [Catalog.Catalog.allocatesReifier, hb]
      rw [halloc] at this
      cases this
    | role ps opt owner =>
      obtain ⟨nv, sp, hown⟩ := mem_slots_role K hy
      rcases hroles _ (List.mem_map_of_mem Prod.snd hown) rfl with hr | hr
      · simp only [Field.name] at hr
        subst hr
        have hsame' := eq_of_nodup_key hnd hown hfa rfl
        simp only [Prod.mk.injEq, Field.role.injEq] at hsame'
        obtain ⟨-, -, rfl, -, -, -⟩ := hsame'
        rw [hwa] at hfw
        cases hfw
        simp [tripleSlot, hpva]
      · simp only [Field.name] at hr
        subst hr
        have hsame' := eq_of_nodup_key hnd hown hfo rfl
        simp only [Prod.mk.injEq, Field.role.injEq] at hsame'
        obtain ⟨-, -, rfl, -, -, -⟩ := hsame'
        rw [hwo] at hfw
        cases hfw
        have : (r == a) = false := by simpa using fun h : r = a => hao h.symm
        simp [tripleSlot, this, hpvo]
  · simp [ident, hc, htn, ha, ho, hn, hfra, hfro, hsa, hso]

theorem triple_complete (H : RoundTrip K vp ord ι idOf G) (graph : Nat → String) {x : Nat × Name × VTerm T}
    (hx : x ∈ G.elems) {c : CType T} (hc : K.find x.2.1 = some c) (hconc : c.concrete = true)
    (htn : tripleNamed K x.2.1 = true) :
    ∃ i, ident K ι x = some i ∧
      (i, x.2.1, x.2.2) ∈ (rdf K vp ι graph G).filterMap (tripleElem K idOf (rdf K vp ι graph G)) := by
  obtain ⟨a, o, n, pa, po, sa, so, ha, ho, hn, hfra, hfro, hsa, hso, hround, hid⟩ :=
    tripleElem_round K H graph hx hc hconc htn
  have htn' := htn
  simp only [tripleNamed, hc, Option.map_some', Option.getD_some, Bool.and_eq_true, beq_iff_eq,
    Bool.not_eq_true'] at htn'
  obtain ⟨⟨hrel, hedge⟩, _⟩ := htn'
  refine ⟨_, hid, List.mem_filterMap.mpr ⟨⟨sa, .nav n, .iri so, graph x.1⟩, ?_, hround _⟩⟩
  rw [mem_rdf]
  refine ⟨x, hx, ⟨pa, .nav n, .elem po, x.1⟩, ?_, by simp [Tmpl.inst, hsa, hso, TObj.inst]⟩
  unfold templates
  rw [hc]
  simp only [List.mem_append]
  right
  unfold relTemplates
  simp only [hrel, hedge, beq_self_eq_true, if_true]
  unfold edgeTemplates
  simp [ha, ho, hn, optList, hfra, hfro]

/-- A quad read back as an element named by a triple, from an element named by a triple, is that element. -/
theorem triple_sound_core (H : RoundTrip K vp ord ι idOf G) (graph : Nat → String) {y : Nat × Name × VTerm T}
    (hy : y ∈ G.elems) {cy : CType T} (hcy : K.find y.2.1 = some cy) (hconc : cy.concrete = true)
    (htny : tripleNamed K y.2.1 = true) {q : Quad T} {z : Ident × Name × VTerm T}
    (hz0 : tripleElem K idOf (rdf K vp ι graph G) q = some z) {a o n : Name} {pa po : Nat} {sa so : String}
    (ha : K.anchorRole y.2.1 = some a) (ho : K.otherRole y.2.1 = some o) (hn : K.navOf y.2.1 a = some n)
    (hfra : fieldRefs y.2.2 a = [pa]) (hfro : fieldRefs y.2.2 o = [po]) (hsa : ι pa = some sa)
    (hso : ι po = some so) (hqs : q.s = sa) (hqp : q.p = .nav n) (hqo : q.o = .iri so) :
    ident K ι y = some z.1 ∧ y.2 = z.2 := by
  obtain ⟨a', o', n', pa', po', sa', so', ha', ho', hn', hfra', hfro', hsa', hso', hround, hid⟩ :=
    tripleElem_round K H graph hy hcy hconc htny
  have haa : a' = a := Option.some.inj (ha'.symm.trans ha)
  have hoo : o' = o := Option.some.inj (ho'.symm.trans ho)
  rw [haa] at hn' hfra'
  rw [hoo] at hfro'
  have hnn : n' = n := Option.some.inj (hn'.symm.trans hn)
  have hpp : pa' = pa := by
    rw [hfra] at hfra'
    exact (List.cons.inj hfra'.symm).1
  have hpo : po' = po := by
    rw [hfro] at hfro'
    exact (List.cons.inj hfro'.symm).1
  rw [hpp] at hsa'
  rw [hpo] at hso'
  have hss : sa' = sa := Option.some.inj (hsa'.symm.trans hsa)
  have hso2 : so' = so := Option.some.inj (hso'.symm.trans hso)
  rw [hss, hso2, hnn] at hround hid
  have hq : q = ⟨sa, .nav n, .iri so, q.g⟩ := by
    cases q
    simp only at hqs hqp hqo
    rw [hqs, hqp, hqo]
  rw [hq, hround] at hz0
  cases hz0
  exact ⟨hid, rfl⟩

theorem triple_sound (H : RoundTrip K vp ord ι idOf G) (graph : Nat → String) {z : Ident × Name × VTerm T}
    (hz : z ∈ (rdf K vp ι graph G).filterMap (tripleElem K idOf (rdf K vp ι graph G))) :
    ∃ x ∈ G.elems, ident K ι x = some z.1 ∧ x.2 = z.2 := by
  obtain ⟨q, hq, hz0⟩ := List.mem_filterMap.mp hz
  obtain ⟨y, hy, t, ht, htq⟩ := (mem_rdf K).mp hq
  obtain ⟨hts, hqp, hto⟩ := inst_eq htq
  have hqz := hz0
  unfold tripleElem at hqz
  split at hqz
  · next n o hpq hoq =>
    have htp : t.p = .nav n := by rw [← hqp, hpq]
    obtain ⟨cy, a, o', hcy, _, hedge, ha, ho', hn, pa, hpa, po, hpo, _, hdir⟩ := nav_template K ht htp
    have hconc : cy.concrete = true := by
      rcases H.labels y hy with ⟨c', hc', hconc⟩ | ⟨hnone, _⟩
      · rw [hcy] at hc'
        cases hc'
        exact hconc
      · rw [hcy] at hnone
        cases hnone
    obtain ⟨a₂, o₂, n₂, ha₂, ho₂, hn₂, _, _, _, _, _, _, _, _, _, _, _, _,
      wa, wo, pa₂, po₂, sa, so, lpa, lpo, hwa, hwo, hra, hro, hsa, hso, _, _, hla, hlo, hadA, hadO, _, _⟩ :=
      edge_roles K H graph hy hcy hconc hedge
    have haa : a₂ = a := Option.some.inj (ha₂.symm.trans ha)
    have hoo : o₂ = o' := Option.some.inj (ho₂.symm.trans ho')
    rw [haa] at hn₂ hwa
    rw [hoo] at hwo
    have hnn : n₂ = n := Option.some.inj (hn₂.symm.trans hn)
    have hfra : fieldRefs y.2.2 a = [pa₂] := by rw [fieldRefs_of_field hwa, hra]
    have hfro : fieldRefs y.2.2 o' = [po₂] := by rw [fieldRefs_of_field hwo, hro]
    rw [hfra, List.mem_singleton] at hpa
    rw [hfro, List.mem_singleton] at hpo
    rw [← hpa] at hsa hfra
    rw [← hpo] at hso hfro
    obtain ⟨ls, hls, hqz⟩ := Option.bind_eq_some.mp hqz
    obtain ⟨lo, _, hqz⟩ := Option.bind_eq_some.mp hqz
    obtain ⟨c, hfind, _⟩ := Option.bind_eq_some.mp hqz
    have hP := List.find?_some hfind
    have hcmem := List.mem_of_find?_eq_some hfind
    simp only [tripleMatch, Bool.and_eq_true, beq_iff_eq] at hP
    obtain ⟨⟨⟨⟨⟨hcconc, hctn⟩, hcnav⟩, hcadA⟩, _⟩, hcord⟩ := hP
    obtain ⟨hname, hcymem⟩ := find_name K hcy
    have hnav' : (K.anchorRole cy.name).bind (K.navOf cy.name) = some n := by
      rw [hname, ha]
      simpa using hn
    rcases hdir with ⟨hs, ho⟩ | ⟨hsym, hs, ho⟩
    · have hqs : q.s = sa := by
        rw [hs, hsa] at hts
        exact (Option.some.inj hts).symm
      have hqo : q.o = .iri so := by
        rw [ho] at hto
        simp only [TObj.inst, hso, Option.map_some', Option.some.injEq] at hto
        exact hto.symm
      rw [hqs, hla] at hls
      have hls' : ls = lpa := (Option.some.inj hls).symm
      rw [hls'] at hcadA
      have hsame := H.navUnique c hcmem cy hcymem hcconc hconc hctn (hname ▸ hedge) n lpa hcnav hnav' hcadA
        (Or.inl (hname ▸ hadA))
      have htny : tripleNamed K y.2.1 = true := by rw [← hname, ← hsame]; exact hctn
      obtain ⟨hid, hv⟩ := triple_sound_core K H graph hy hcy hconc htny hz0 ha ho' hn hfra hfro hsa hso hqs
        (hqp.trans htp) hqo
      exact ⟨y, hy, hid, hv⟩
    · have hqs : q.s = so := by
        rw [hs, hso] at hts
        exact (Option.some.inj hts).symm
      have hqo : q.o = .iri sa := by
        rw [ho] at hto
        simp only [TObj.inst, hsa, Option.map_some', Option.some.injEq] at hto
        exact hto.symm
      rw [hqs, hlo] at hls
      have hls' : ls = lpo := (Option.some.inj hls).symm
      rw [hls'] at hcadA
      have hsame := H.navUnique c hcmem cy hcymem hcconc hconc hctn (hname ▸ hedge) n lpo hcnav hnav' hcadA
        (Or.inr ⟨hname ▸ hsym, hname ▸ hadO⟩)
      have htny : tripleNamed K y.2.1 = true := by rw [← hname, ← hsame]; exact hctn
      have hcsym : K.inherits CType.symmetric c.name = true := by rw [hsame, hname]; exact hsym
      have hoq' : o = sa := by
        rw [hoq] at hqo
        exact Term.iri.inj hqo
      rw [hcsym, hqs, hoq'] at hcord
      simp only [Bool.not_true, Bool.false_or, decide_eq_true_eq] at hcord
      have hle := H.symm y hy htny hsym a o' _ _ sa so ha ho' hfra hfro hsa hso
      have heq : sa = so := string_antisymm sa so hle hcord
      obtain ⟨hid, hv⟩ := triple_sound_core K H graph hy hcy hconc htny hz0 ha ho' hn hfra hfro hsa hso
        (hqs.trans heq.symm) (hqp.trans htp) (hqo.trans (by rw [heq]))
      exact ⟨y, hy, hid, hv⟩
  · cases hqz

end TripleFacts

/-! ## Proposition 17.3 -/

/-- Proposition 17.3: on a valid APG on `S(K)` written as the store writes it, `R⁻¹(R(G))` holds exactly `G`'s
elements, each under its identity, its IRI, its triple or its pair, with its label and its value. -/
theorem rdf_round_trip {vp : ViewParams T} {ord : VTerm T → VTerm T → Bool} {ι : Nat → Option String}
    {idOf : String → Nat} {G : FinApg K.schema} (H : RoundTrip K vp ord ι idOf G) (graph : Nat → String)
    (z : Ident × Name × VTerm T) :
    z ∈ rdfInv K vp ord idOf (rdf K vp ι graph G) ↔ ∃ x ∈ G.elems, ident K ι x = some z.1 ∧ x.2 = z.2 := by
  constructor
  · intro hz
    simp only [rdfInv, List.mem_append] at hz
    rcases hz with (hz | hz) | hz
    · exact iri_sound K H graph hz
    · exact triple_sound K H graph hz
    · exact pair_sound K H graph hz
  · rintro ⟨x, hx, hid, hxz⟩
    obtain ⟨i, l, v⟩ := z
    simp only at hid hxz
    have hxz' : x.2.1 = l ∧ x.2.2 = v := by rw [hxz]; exact ⟨rfl, rfl⟩
    obtain ⟨rfl, rfl⟩ := hxz'
    simp only [rdfInv, List.mem_append]
    cases hfind : K.find x.2.1 with
    | some c =>
      have hconc : c.concrete = true := by
        rcases H.labels x hx with ⟨c', hc', hconc⟩ | ⟨hnone, _⟩
        · rw [hfind] at hc'
          cases hc'
          exact hconc
        · rw [hfind] at hnone
          cases hnone
      cases htn : tripleNamed K x.2.1 with
      | false =>
        simp only [ident, hfind, htn, Bool.false_eq_true, if_false, Option.map_eq_some'] at hid
        obtain ⟨s, hs, rfl⟩ := hid
        exact Or.inl (Or.inl (iri_complete K H graph hx hs))
      | true =>
        obtain ⟨i', hid', hmem⟩ := triple_complete K H graph hx hfind hconc htn
        rw [hid] at hid'
        cases hid'
        exact Or.inl (Or.inr hmem)
    | none =>
      obtain ⟨i', hid', hmem⟩ := pair_complete K H graph hx hfind
      rw [hid] at hid'
      cases hid'
      exact Or.inr hmem

end ApgModel.Views
