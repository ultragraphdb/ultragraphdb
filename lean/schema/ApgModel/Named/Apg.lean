/-
 SPDX-License-Identifier: Apache-2.0
 Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
-/
import ApgModel.Named.TermTranslate

/-!
# Value terms and finite APGs

`SCHEMA-APG.md` definitions 5.4, 5.5, 6.2 and 6.4 in executable form, the layer the oracle generates and the views
read: elements numbered by `Nat`, a value term for each, and a decidable check.

A value term is checked against a named type by denoting it (`denote`): the check (`check`) succeeds exactly when the
term denotes a member of `V_E(t)`, with `E(l) = λ⁻¹(l)` (`Elems`), as definition 5.4 states. Every member of a
well-formed type is denoted by some term (`denote_complete`), so the check accepts the values and nothing else. A
record term lists its fields in the type's order; a set term's duplicate members collapse, as `SCHEMA.md` section 5.4
accepts a write holding a duplicate as the set of its distinct members.

A finite APG (`FinApg`) is a list of elements, each an identifier, a label and a value term (definition 6.2, finite by
6.4). It is valid when its identifiers are distinct and every value checks against its label's type (`FinApg.valid`);
a valid finite APG is an APG on the named schema (`FinApg.toNApg`), and so, through proposition 3.3, on the core's
binary schema, which carries the core's results to what the oracle generates.
-/

namespace ApgModel.Named

open Core

variable {T : Typeside} {L : Type}

/-- The value terms of definition 5.4, with the collections of definition 12.1. -/
inductive VTerm (T : Typeside) where
  /-- A primitive value, `⟨a⟩_p`. -/
  | prim (p : T.P) (a : T.PV p)
  /-- A reference to the element `e`, `elem e`. -/
  | elem (e : Nat)
  /-- A record, `{ n₁ = v₁, …, nₖ = vₖ }`, its fields in the type's order. -/
  | record (fs : List (Name × VTerm T))
  /-- The variant field `n` with the value `v`, `n · v`. -/
  | inj (n : Name) (v : VTerm T)
  /-- A list. -/
  | list (vs : List (VTerm T))
  /-- A set, written as its members. -/
  | set (vs : List (VTerm T))

/-- `refs(v)`, the elements a value term references, definition 5.5. -/
def VTerm.refs {T : Typeside} : VTerm T → List Nat
  | .prim _ _ => []
  | .elem e => [e]
  | .record fs => refsFields fs
  | .inj _ v => v.refs
  | .list vs | .set vs => refsAll vs
where
  /-- The references of a record's fields. -/
  refsFields : List (Name × VTerm T) → List Nat
    | [] => []
    | (_, v) :: fs => v.refs ++ refsFields fs
  /-- The references of a collection's members. -/
  refsAll : List (VTerm T) → List Nat
    | [] => []
    | v :: vs => v.refs ++ refsAll vs

/-- `λ⁻¹(l)`, the elements the labelling `lam` gives the label `l`. -/
def Elems (lam : Nat → Option L) (l : L) : Type := { e : Nat // lam e = some l }

/-- A record's value from its first field's value and the rest. -/
def consField {E : L → Type} {n : Name} {t : NTy T.P L} {fs : List (Name × NTy T.P L)} (x : NTy.V T E t)
    (f : (i : Fin fs.length) → NTy.V.VAt T E fs i) : (i : Fin ((n, t) :: fs).length) → NTy.V.VAt T E ((n, t) :: fs) i
  | ⟨0, _⟩ => x
  | ⟨i + 1, h⟩ => f ⟨i, Nat.lt_of_succ_lt_succ h⟩

/-! ## Field values and well-formedness -/

theorem NTy.V.VAt.into_out {E : L → Type} : (fs : List (Name × NTy T.P L)) → (i : Fin fs.length) →
    (w : NTy.V.VAt T E fs i) → NTy.V.VAt.into fs i (NTy.V.VAt.out fs i w) = w
  | [], i, _ => i.elim0
  | (_, _) :: _, ⟨0, _⟩, _ => rfl
  | _ :: fs, ⟨i + 1, h⟩, w => into_out fs ⟨i, Nat.lt_of_succ_lt_succ h⟩ w

theorem NTy.names_nodup {fs : List (Name × NTy T.P L)} (h : (NTy.names fs).Pairwise (· < ·)) :
    (NTy.names fs).Nodup := by
  unfold List.Nodup
  exact h.imp fun {a b} (hlt : a < b) (e : a = b) => by subst e; exact String.lt_irrefl a hlt

variable [DecidableEq T.P] [DecidableEq L]

/-- The value a term denotes at a named type, under the labelling `lam`, or none when the term does not check:
definition 5.4. -/
def denote (lam : Nat → Option L) : VTerm T → (t : NTy T.P L) → Option (NTy.V T (Elems lam) t)
  | .prim p a, .prim q => if h : p = q then some (h ▸ a) else none
  | .elem e, .lbl l => if h : lam e = some l then some ⟨e, h⟩ else none
  | .record vs, .record fs => denoteRec vs fs
  | .inj n v, .variant fs =>
    match NTy.fieldIndex fs n with
    | some i => (denote lam v (NTy.fieldAt fs i)).map fun x => ⟨i, NTy.V.VAt.into fs i x⟩
    | none => none
  | .list vs, .list t => denoteAll vs t
  | .set vs, .set t => (denoteAll vs t).map Core.FinSet.ofList
  | _, _ => none
where
  /-- A record term's fields against the type's, in order. -/
  denoteRec : List (Name × VTerm T) → (fs : List (Name × NTy T.P L)) →
      Option ((i : Fin fs.length) → NTy.V.VAt T (Elems lam) fs i)
    | [], [] => some fun i => i.elim0
    | (n, v) :: vs, (m, t) :: fs =>
      if n = m then
        match denote lam v t, denoteRec vs fs with
        | some x, some f => some (consField x f)
        | _, _ => none
      else none
    | _, _ => none
  /-- A collection's members against the member type. -/
  denoteAll : List (VTerm T) → (t : NTy T.P L) → Option (List (NTy.V T (Elems lam) t))
    | [], _ => some []
    | v :: vs, t =>
      match denote lam v t, denoteAll vs t with
      | some x, some xs => some (x :: xs)
      | _, _ => none

/-- `v : t`, definition 5.4's check: the term denotes a member of `t`'s values. -/
def check (lam : Nat → Option L) (v : VTerm T) (t : NTy T.P L) : Bool := (denote lam v t).isSome

/-- Definition 5.4: `v : t` holds exactly when `v` denotes a member of `V_E(t)`. -/
theorem check_iff (lam : Nat → Option L) (v : VTerm T) (t : NTy T.P L) :
    check lam v t = true ↔ ∃ x, denote lam v t = some x :=
  Option.isSome_iff_exists

/-! ## Every value of a well-formed type is denoted -/

/-- A collection's members are denoted when each member of the member type is. -/
theorem denote_all_complete (lam : Nat → Option L) (t : NTy T.P L)
    (ih : ∀ x : NTy.V T (Elems lam) t, ∃ v, denote lam v t = some x) :
    (xs : List (NTy.V T (Elems lam) t)) → ∃ vs, denote.denoteAll lam vs t = some xs
  | [] => ⟨[], rfl⟩
  | x :: xs => by
    obtain ⟨v, hv⟩ := ih x
    obtain ⟨vs, hvs⟩ := denote_all_complete lam t ih xs
    exact ⟨v :: vs, by simp only [denote.denoteAll, hv, hvs]⟩

/-- Definition 5.4, the converse: every member of a well-formed type's values is denoted by a term, so the check
accepts the values and nothing else. -/
theorem denote_complete (lam : Nat → Option L) : (t : NTy T.P L) → t.WF → (x : NTy.V T (Elems lam) t) →
    ∃ v, denote lam v t = some x
  | .prim p, _, x => ⟨.prim p x, by simp [denote]⟩
  | .lbl l, _, ⟨e, h⟩ => ⟨.elem e, by simp [denote, h]⟩
  | .record fs, ⟨_, hw⟩, f => by
    obtain ⟨vs, hvs⟩ := fields fs hw f
    exact ⟨.record vs, hvs⟩
  | .variant fs, ⟨hp, hw⟩, ⟨i, w⟩ => by
    obtain ⟨v, hv⟩ := alts fs hw i w
    refine ⟨.inj (NTy.nameAt fs i) v, ?_⟩
    simp only [denote, NTy.fieldIndex_nameAt fs (NTy.names_nodup hp) i, hv, Option.map_some', NTy.V.VAt.into_out]
  | .list t, hw, xs => by
    obtain ⟨vs, hvs⟩ := denote_all_complete lam t (denote_complete lam t hw) xs
    exact ⟨.list vs, hvs⟩
  | .set t, hw, s => by
    obtain ⟨xs, rfl⟩ := Quot.exists_rep s
    obtain ⟨vs, hvs⟩ := denote_all_complete lam t (denote_complete lam t hw) xs
    exact ⟨.set vs, by simp only [denote, hvs, Option.map_some']; rfl⟩
where
  fields : (fs : List (Name × NTy T.P L)) → NTy.WF.WFFields fs →
      (f : (i : Fin fs.length) → NTy.V.VAt T (Elems lam) fs i) → ∃ vs, denote.denoteRec lam vs fs = some f
    | [], _, f => ⟨[], by
        simp only [denote.denoteRec]
        exact congrArg some (funext fun i => i.elim0)⟩
    | (m, t) :: fs, ⟨ht, hw⟩, f => by
      obtain ⟨v, hv⟩ := denote_complete lam t ht (f ⟨0, Nat.zero_lt_succ _⟩)
      obtain ⟨vs, hvs⟩ := fields fs hw (fun i => f i.succ)
      refine ⟨(m, v) :: vs, ?_⟩
      simp only [denote.denoteRec, if_pos rfl, hv, hvs]
      exact congrArg some (funext fun i => match i with | ⟨0, _⟩ => rfl | ⟨_ + 1, _⟩ => rfl)
  alts : (fs : List (Name × NTy T.P L)) → NTy.WF.WFFields fs → (i : Fin fs.length) →
      (w : NTy.V.VAt T (Elems lam) fs i) → ∃ v, denote lam v (NTy.fieldAt fs i) = some (NTy.V.VAt.out fs i w)
    | [], _, i, _ => i.elim0
    | (_, t) :: _, ⟨ht, _⟩, ⟨0, _⟩, w => denote_complete lam t ht w
    | _ :: fs, ⟨_, hw⟩, ⟨i + 1, h⟩, w => alts fs hw ⟨i, Nat.lt_of_succ_lt_succ h⟩ w

/-- Proposition 6.6 for value terms: a term that checks references only elements the labelling labels. -/
theorem denote_refs (lam : Nat → Option L) : (v : VTerm T) → (t : NTy T.P L) → (denote lam v t).isSome = true →
    ∀ r ∈ v.refs, (lam r).isSome = true
  | .prim _ _, _, _, _, hr => by simp [VTerm.refs] at hr
  | .elem e, t, h, r, hr => by
    simp only [VTerm.refs, List.mem_singleton] at hr
    subst hr
    cases t with
    | lbl l =>
      simp only [denote] at h
      split at h
      · next he => simp [he]
      · simp at h
    | _ => simp [denote] at h
  | .record vs, t, h, r, hr => by
    cases t with
    | record fs => exact fields vs fs h r hr
    | _ => simp [denote] at h
  | .inj n v, t, h, r, hr => by
    cases t with
    | variant fs =>
      simp only [denote] at h
      split at h
      · next i _ => exact denote_refs lam v (NTy.fieldAt fs i) (by simpa using h) r hr
      · simp at h
    | _ => simp [denote] at h
  | .list vs, t, h, r, hr => by
    cases t with
    | list t => exact all vs t h r hr
    | _ => simp [denote] at h
  | .set vs, t, h, r, hr => by
    cases t with
    | set t => exact all vs t (by simpa [denote] using h) r hr
    | _ => simp [denote] at h
where
  fields : (vs : List (Name × VTerm T)) → (fs : List (Name × NTy T.P L)) →
      (denote.denoteRec lam vs fs).isSome = true → ∀ r ∈ VTerm.refs.refsFields vs, (lam r).isSome = true
    | [], _, _, r, hr => by simp [VTerm.refs.refsFields] at hr
    | (n, v) :: vs, [], h, _, _ => by simp [denote.denoteRec] at h
    | (n, v) :: vs, (m, t) :: fs, h, r, hr => by
      simp only [denote.denoteRec] at h
      split at h
      · split at h
        · next x f hx hf =>
          simp only [VTerm.refs.refsFields, List.mem_append] at hr
          rcases hr with hr | hr
          · exact denote_refs lam v t (by simp [hx]) r hr
          · exact fields vs fs (by simp [hf]) r hr
        · simp at h
      · simp at h
  all : (vs : List (VTerm T)) → (t : NTy T.P L) → (denote.denoteAll lam vs t).isSome = true →
      ∀ r ∈ VTerm.refs.refsAll vs, (lam r).isSome = true
    | [], _, _, r, hr => by simp [VTerm.refs.refsAll] at hr
    | v :: vs, t, h, r, hr => by
      simp only [denote.denoteAll] at h
      split at h
      · next x xs hx hxs =>
        simp only [VTerm.refs.refsAll, List.mem_append] at hr
        rcases hr with hr | hr
        · exact denote_refs lam v t (by simp [hx]) r hr
        · exact all vs t (by simp [hxs]) r hr
      · simp at h

/-! ## Finite APGs -/

/-- A finite APG in single-set form, definitions 6.2 and 6.4: each element an identifier, a label and a value term. -/
structure FinApg (S : NSchema T.P L) where
  /-- The elements: identifier, label, value term. -/
  elems : List (Nat × L × VTerm T)

namespace FinApg

variable {S : NSchema T.P L} (G : FinApg S)

/-- The label and value term of the element `e`. -/
def entry (e : Nat) : Option (L × VTerm T) := (G.elems.find? fun x => x.1 == e).map Prod.snd

/-- The labelling `λ`. -/
def lam (e : Nat) : Option L := (G.entry e).map Prod.fst

/-- The identifiers of the elements. -/
def ids : List Nat := G.elems.map Prod.fst

/-- A finite APG is valid when its identifiers are distinct and every value checks against its label's type. -/
def valid : Bool := decide G.ids.Nodup && G.elems.all fun x => check G.lam x.2.2 (S.σ x.2.1)

variable {G}

omit [DecidableEq T.P] [DecidableEq L] in
theorem entry_lam {e : Nat} {l : L} {v : VTerm T} (h : G.entry e = some (l, v)) : G.lam e = some l := by
  simp [lam, h]

theorem entry_check (hv : G.valid = true) {e : Nat} {l : L} {v : VTerm T} (h : G.entry e = some (l, v)) :
    check G.lam v (S.σ l) = true := by
  simp only [entry, Option.map_eq_some'] at h
  obtain ⟨x, hx, hxv⟩ := h
  have hmem := List.mem_of_find?_eq_some hx
  simp only [valid, Bool.and_eq_true, List.all_eq_true] at hv
  have := hv.2 x hmem
  rw [hxv] at this
  exact this

/-- An element's value: its term's denotation at its label's type. -/
def value (hv : G.valid = true) (l : L) (e : Elems G.lam l) : NTy.V T (Elems G.lam) (S.σ l) :=
  match he : G.entry e.1 with
  | some (l', v) =>
    have hl : l' = l := Option.some.inj ((entry_lam he).symm.trans e.2)
    (denote G.lam v (S.σ l)).get (by rw [← hl]; exact entry_check hv he)
  | none => absurd e.2 (by simp [lam, he])

/-- A valid finite APG as an APG on the named schema: the elements of `l` are `λ⁻¹(l)`, each with its value. -/
def toNApg (hv : G.valid = true) : NApg S := ⟨Elems G.lam, value hv⟩

end FinApg

end ApgModel.Named
