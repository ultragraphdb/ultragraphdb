/-
 SPDX-License-Identifier: Apache-2.0
 Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
-/
import ApgModel.Core.Delta
import ApgModel.Catalog.Constraints

/-!
# Selections, key lookups, extended terms and coproducts

`SCHEMA-APG.md` section 16: definitions 16.1, 16.2, 16.3 and 16.5, and propositions 16.4 and 16.6.

- **Selection** (definition 16.1, `select`). A selection keeps the elements a predicate holds of; it is defined when no
  kept element references a dropped one (`Closed`), and then each kept element's value is its value in `G` with every
  reference restricted to the kept elements (`V.restrict`). Its inclusion into `G` is an injective homomorphism
  (`select_incl`, `select_incl_injective`) that preserves names (`select_incl_preserves_names`). Where the selection is
  undefined, a kept element references a dropped one, and the failure names both (`select_undefined`).
- **Key lookup** (definition 16.2, `lookup`), over finite APGs: on an APG satisfying the key's exclusivity it finds the
  one element with the key (`lookup_unique`), nothing when no element has it (`lookup_none`), and the exclusivity holds
  in every sub-APG (`keyViolations_sub`).
- **Extended mappings** (definition 16.3, `ExtMapping`): a label's term may read its source APG, which a pure mapping's
  cannot, and is defined on the admitted sources `P` alone, the valid ones. A key lookup, an inverse navigation, a
  derivation, a fold or a function is such a term. A pure mapping is one (`Mapping.toExt`), with the same `Δ`
  (`Mapping.toExt_delta`); `Δ` along an extended mapping is no functor, so it acts on APGs and not on homomorphisms.
- **The fragment** (proposition 16.4, `fragment`): `(Δ_Φ G⁺)|p`, defined exactly when its selection is. `G⁺` is the
  source with the unnests its maps name, an APG on the source schema with their derived labels (definition 12.7,
  `ApgModel.Named.Collections`). Each target element of a label or variant-of-labels image has one origin
  (`fragment_label_origin`, `fragment_one_origin`).
- **Coproducts** (definition 16.5, `Apg.coprod`): the APG on `S + S'` holding the elements of both; `Δ` along each
  injection gives back its summand (`coprod_inl`, `coprod_inr`).
- **History through a migration** (proposition 16.6, `history_through_migration`): the slice believed at `t`, an APG
  (proposition 15.2) included into the stored present by an injective homomorphism preserving names, maps through the
  fragment, and each primary image's name, its origin's, is the name of its origin's image in the present.
-/

namespace ApgModel.Migration

open Core

variable {T : Typeside} {L L' : Type}

/-! ## Restricting a value to kept elements -/

/-- The kept elements of a family: those `keep` holds of. -/
def Kept (E : L → Type) (keep : ∀ l, E l → Bool) (l : L) : Type := { e : E l // keep l e = true }

/-- A list's image under a partial function, when it is defined on every member. -/
def optMapList {α β : Type} (f : α → Option β) : List α → Option (List β)
  | [] => some []
  | x :: xs => (f x).bind fun y => (optMapList f xs).map (y :: ·)

/-- A finite set's image under a partial function, when it is defined on every member. -/
def optMapSet {α β : Type} (f : α → Option β) : FinSet α → Option (FinSet β) :=
  Quot.lift (fun l => if l.all (fun x => (f x).isSome) then some (FinSet.ofList (l.filterMap f)) else none) (by
    intro l l' h
    have hall : l.all (fun x => (f x).isSome) = l'.all (fun x => (f x).isSome) := by
      apply Bool.eq_iff_iff.mpr
      simp only [List.all_eq_true]
      exact ⟨fun H x hx => H x ((h x).mpr hx), fun H x hx => H x ((h x).mp hx)⟩
    simp only [hall]
    split
    · congr 1
      apply Quot.sound
      intro y
      simp only [List.mem_filterMap]
      exact ⟨fun ⟨x, hx, e⟩ => ⟨x, (h x).mp hx, e⟩, fun ⟨x, hx, e⟩ => ⟨x, (h x).mpr hx, e⟩⟩
    · rfl)

/-- A value with every reference restricted to the kept elements, when every one is kept. -/
def V.restrict {E : L → Type} (keep : ∀ l, E l → Bool) : (t : Ty T.P L) → V T E t → Option (V T (Kept E keep) t)
  | .zero, x => nomatch x
  | .one, x => some x
  | .sum a _, .inl x => (V.restrict keep a x).map .inl
  | .sum _ b, .inr y => (V.restrict keep b y).map .inr
  | .prod a b, (x, y) => (V.restrict keep a x).bind fun x' => (V.restrict keep b y).map fun y' => (x', y')
  | .prim _, x => some x
  | .lbl l, e => if h : keep l e = true then some ⟨e, h⟩ else none
  | .list t, xs => optMapList (V.restrict keep t) xs
  | .set t, s => optMapSet (V.restrict keep t) s

theorem optMapList_spec {α β : Type} {f : α → Option β} {g : β → α} :
    (xs : List α) → (∀ x ∈ xs, ∃ y, f x = some y ∧ g y = x) → ∃ ys, optMapList f xs = some ys ∧ ys.map g = xs
  | [], _ => ⟨[], rfl, rfl⟩
  | x :: xs, H => by
    obtain ⟨y, hy, hg⟩ := H x (List.mem_cons_self _ _)
    obtain ⟨ys, hys, hgs⟩ := optMapList_spec xs fun z hz => H z (List.mem_cons_of_mem _ hz)
    exact ⟨y :: ys, by simp [optMapList, hy, hys], by simp [hg, hgs]⟩

theorem optMapSet_spec {α β : Type} {f : α → Option β} {g : β → α} (s : FinSet α)
    (H : ∀ x, FinSet.Mem x s → ∃ y, f x = some y ∧ g y = x) :
    ∃ s', optMapSet f s = some s' ∧ FinSet.map g s' = s := by
  induction s using Quot.ind with
  | mk l =>
    have hall : l.all (fun x => (f x).isSome) = true := by
      simp only [List.all_eq_true]
      intro x hx
      obtain ⟨y, hy, _⟩ := H x hx
      simp [hy]
    refine ⟨FinSet.ofList (l.filterMap f), by simp [optMapSet, hall, FinSet.ofList], ?_⟩
    apply Quot.sound
    intro z
    simp only [List.mem_map, List.mem_filterMap]
    constructor
    · rintro ⟨y, ⟨x, hx, hxy⟩, rfl⟩
      obtain ⟨y', hy', hg⟩ := H x hx
      rw [hxy] at hy'
      cases hy'
      rw [hg]
      exact hx
    · intro hz
      obtain ⟨y, hy, hg⟩ := H z hz
      exact ⟨y, ⟨z, hz, hy⟩, hg⟩

/-- A value whose references are all kept restricts, and the restriction read back in `E` is the value. -/
theorem restrict_spec {E : L → Type} (keep : ∀ l, E l → Bool) (t : Ty T.P L) (x : V T E t)
    (H : ∀ r, V.Ref t x r → keep r.1 r.2 = true) :
    ∃ y, V.restrict keep t x = some y ∧ V.map (fun _ e => e.1) t y = x := by
  induction t with
  | zero => exact nomatch x
  | one => exact ⟨x, rfl, rfl⟩
  | prim _ => exact ⟨x, rfl, rfl⟩
  | lbl l =>
    have hk : keep l x = true := H ⟨l, x⟩ rfl
    exact ⟨⟨x, hk⟩, by simp [V.restrict, hk], rfl⟩
  | sum a b iha ihb =>
    cases x with
    | inl x =>
      obtain ⟨y, hy, hm⟩ := iha x H
      exact ⟨.inl y, by simp [V.restrict, hy], by simp [V.map, hm]⟩
    | inr x =>
      obtain ⟨y, hy, hm⟩ := ihb x H
      exact ⟨.inr y, by simp [V.restrict, hy], by simp [V.map, hm]⟩
  | prod a b iha ihb =>
    obtain ⟨x, z⟩ := x
    obtain ⟨y, hy, hm⟩ := iha x fun r hr => H r (Or.inl hr)
    obtain ⟨w, hw, hn⟩ := ihb z fun r hr => H r (Or.inr hr)
    exact ⟨(y, w), by simp [V.restrict, hy, hw], by simp [V.map, hm, hn]⟩
  | list t ih =>
    obtain ⟨ys, hys, hm⟩ := optMapList_spec (f := V.restrict keep t) (g := V.map (fun _ e => e.1) t) x
      fun z hz => ih z fun r hr => H r ⟨z, hz, hr⟩
    exact ⟨ys, hys, hm⟩
  | set t ih =>
    obtain ⟨s', hs', hm⟩ := optMapSet_spec (f := V.restrict keep t) (g := V.map (fun _ e => e.1) t) x
      fun z hz => ih z fun r hr => H r (by
        revert hz hr
        induction x using Quot.ind with
        | mk l => exact fun hz hr => ⟨z, hz, hr⟩)
    exact ⟨s', hs', hm⟩

/-! ## Definition 16.1: selection -/

variable {S : Schema T.P L}

/-- No kept element references a dropped one: the selection by `keep` is defined. -/
def Closed (G : Apg S) (keep : ∀ l, G.E l → Bool) : Prop :=
  ∀ l e, keep l e = true → ∀ r, V.Ref (S.σ l) (G.υ l e) r → keep r.1 r.2 = true

theorem restrict_isSome (G : Apg S) (keep : ∀ l, G.E l → Bool) (h : Closed G keep) (l : L) (e : Kept G.E keep l) :
    (V.restrict keep (S.σ l) (G.υ l e.1)).isSome = true := by
  obtain ⟨y, hy, _⟩ := restrict_spec keep (S.σ l) (G.υ l e.1) (h l e.1 e.2)
  simp [hy]

/-- Definition 16.1: the selection `G|p` of the elements `keep` holds of, every kept element's value its value in
`G`. -/
def select (G : Apg S) (keep : ∀ l, G.E l → Bool) (h : Closed G keep) : Apg S where
  E := Kept G.E keep
  υ l e := (V.restrict keep (S.σ l) (G.υ l e.1)).get (restrict_isSome G keep h l e)

/-- The inclusion of a selection into its APG. -/
def select_incl (G : Apg S) (keep : ∀ l, G.E l → Bool) (h : Closed G keep) : Hom (select G keep h) G where
  h _ e := e.1
  comm l e := by
    obtain ⟨y, hy, hm⟩ := restrict_spec keep (S.σ l) (G.υ l e.1) (h l e.1 e.2)
    have hget : (V.restrict keep (S.σ l) (G.υ l e.1)).get (restrict_isSome G keep h l e) = y :=
      Option.some.inj ((Option.some_get _).trans hy)
    show G.υ l e.1 = V.map (fun _ e => e.1) (S.σ l) ((V.restrict keep (S.σ l) (G.υ l e.1)).get _)
    rw [hget, hm]

/-- The inclusion is injective. -/
theorem select_incl_injective (G : Apg S) (keep : ∀ l, G.E l → Bool) (h : Closed G keep) (l : L)
    (e e' : (select G keep h).E l) (heq : (select_incl G keep h).h l e = (select_incl G keep h).h l e') : e = e' :=
  Subtype.ext heq

/-- A naming of an APG's elements: each element's IRI where it has one. -/
def Naming (G : Apg S) : Type := ∀ l, G.E l → Option String

/-- A homomorphism preserves names when each named element's image has its name. -/
def PreservesNames {G G' : Apg S} (f : Hom G G') (ι : Naming G) (ι' : Naming G') : Prop :=
  ∀ l e s, ι l e = some s → ι' l (f.h l e) = some s

/-- The inclusion preserves names: a kept element's name is its name in `G`. -/
theorem select_incl_preserves_names (G : Apg S) (keep : ∀ l, G.E l → Bool) (h : Closed G keep) (ι : Naming G) :
    PreservesNames (select_incl G keep h) (fun l e => ι l e.1) ι :=
  fun _ _ _ hs => hs

/-- Where the selection is undefined, a kept element references a dropped one, which the failure names. -/
theorem select_undefined (G : Apg S) (keep : ∀ l, G.E l → Bool) (h : ¬ Closed G keep) :
    ∃ l e r, keep l e = true ∧ V.Ref (S.σ l) (G.υ l e) r ∧ keep r.1 r.2 = false :=
  Classical.byContradiction fun hn => h fun l e hk r hr =>
    Classical.byContradiction fun hr' => hn ⟨l, e, r, hk, hr, by simpa using hr'⟩

/-! ## Definition 16.2: key lookup -/

section Lookup

open Named Catalog

variable [DecidableEq L] [DecidableEq T.P] [∀ p, DecidableEq (T.PV p)] {N : NSchema T.P L}

/-- Definition 16.2: the element of the labels `ls` whose key `ks` is `v`, none when no element has it. -/
def lookup (G : FinApg N) (ls : List L) (ks : List Name) (v : List (Option (VTerm T))) : Option Nat :=
  ((ofLabels G ls).find? fun x => decide (keyOf ks x.2.2 = v)).map Prod.fst

/-- On an APG satisfying the key's exclusivity, the lookup of an element's key finds that element. -/
theorem lookup_unique {G : FinApg N} {ls : List L} {ks : List Name} (hk : keyViolations G ls ks = [])
    {x : Nat × L × VTerm T} (hx : x ∈ ofLabels G ls) : lookup G ls ks (keyOf ks x.2.2) = some x.1 := by
  unfold lookup
  cases hf : (ofLabels G ls).find? fun y => decide (keyOf ks y.2.2 = keyOf ks x.2.2) with
  | none =>
    have := List.find?_eq_none.mp hf x hx
    simp at this
  | some y =>
    have hy := List.mem_of_find?_eq_some hf
    have hyk := List.find?_some hf
    simp only [decide_eq_true_eq] at hyk
    simp only [Option.map_some', Option.some.injEq]
    exact (keyViolations_nil_iff G ls ks).mp hk y hy x hx hyk

/-- The lookup finds nothing exactly when no element has the key. -/
theorem lookup_none {G : FinApg N} {ls : List L} {ks : List Name} {v : List (Option (VTerm T))} :
    lookup G ls ks v = none ↔ ∀ x ∈ ofLabels G ls, keyOf ks x.2.2 ≠ v := by
  unfold lookup
  rw [Option.map_eq_none', List.find?_eq_none]
  simp

/-- A key's exclusivity holds in every sub-APG: a key names one element for all time (`SCHEMA.md` section 7.2), so a
slice's lookups find at most one element. -/
theorem keyViolations_sub {G B : FinApg N} (hsub : ∀ x ∈ B.elems, x ∈ G.elems) {ls : List L} {ks : List Name}
    (hk : keyViolations G ls ks = []) : keyViolations B ls ks = [] := by
  rw [keyViolations_nil_iff] at hk ⊢
  intro x hx y hy hxy
  exact hk x (mem_ofLabels.mpr ⟨hsub x (mem_ofLabels.mp hx).1, (mem_ofLabels.mp hx).2⟩) y
    (mem_ofLabels.mpr ⟨hsub y (mem_ofLabels.mp hy).1, (mem_ofLabels.mp hy).2⟩) hxy

end Lookup

/-! ## Definition 16.3: extended mappings -/

section Ext

variable {S' : Schema T.P L'}

/-- Definition 16.3: an extended mapping from `S` to `S'`, defined on the sources `P` admits: each label's image `τ_l`
and its term, which reads the element's value of `τ_l` and the source itself. -/
structure ExtMapping (S : Schema T.P L) (S' : Schema T.P L') (P : Apg S' → Prop) where
  /-- The label's image, `τ_l`. -/
  τ : L → Ty T.P L'
  /-- The label's term on an admitted source. -/
  t : (G' : Apg S') → P G' → (l : L) → V T G'.E (τ l) → V T G'.E ((S.σ l).subst τ)

variable {P : Apg S' → Prop}

/-- `Δ` along an extended mapping, definition 9.2 with extended terms, on an admitted source. -/
def ExtMapping.delta (Φ : ExtMapping S S' P) (G' : Apg S') (hG : P G') : Apg S where
  E l := V T G'.E (Φ.τ l)
  υ l v := iota Φ.τ G'.E (S.σ l) (Φ.t G' hG l v)

/-- A pure mapping as an extended one, its terms evaluated on the source. -/
def toExt (Φ : Mapping S S') (P : Apg S' → Prop) : ExtMapping S S' P where
  τ := Φ.τ
  t G' _ l v := (Φ.t l).eval G' (Env.one v)

/-- `Δ` along a pure mapping is `Δ` along it as an extended mapping. -/
theorem toExt_delta (Φ : Mapping S S') (G' : Apg S') (hG : P G') : (toExt Φ P).delta G' hG = Φ.delta G' := rfl

/-- Proposition 9.5 for an extended mapping: under `τ_l = var { n₁ ↦ lbl l'₁, … }`, each element of `Δ_Φ(G')(l)` is
one element of one `G'(l'ᵢ)`. -/
def ExtMapping.delta_one_origin (Φ : ExtMapping S S' P) (G' : Apg S') (hG : P G') {l : L} {ls : List L'}
    (h : Φ.τ l = labelSum ls) : Bij ((Φ.delta G' hG).E l) ((i : Fin ls.length) × G'.E (ls.get i)) := by
  show Bij (V T G'.E (Φ.τ l)) _
  rw [h]
  exact labelSumBij G'.E ls

/-! ## Proposition 16.4: the fragment -/

/-- Proposition 16.4: the fragment's denotation `(Δ_Φ G⁺)|p` of an admitted source `G⁺`, defined when its selection
is. -/
def fragment (Φ : ExtMapping S S' P) (G' : Apg S') (hG : P G') (keep : ∀ l, (Φ.delta G' hG).E l → Bool)
    (h : Closed (Φ.delta G' hG) keep) : Apg S :=
  select (Φ.delta G' hG) keep h

/-- Proposition 16.4, a label image: under `τ_l = lbl l'`, each target element of `l` is one element of `G'(l')`. -/
theorem fragment_label_origin (Φ : ExtMapping S S' P) (G' : Apg S') (hG : P G')
    (keep : ∀ l, (Φ.delta G' hG).E l → Bool) (h : Closed (Φ.delta G' hG) keep) {l : L} {l' : L'}
    (hl : Φ.τ l = .lbl l') :
    ∃ f : (fragment Φ G' hG keep h).E l → G'.E l', ∀ e e', f e = f e' → e = e' := by
  have hE : V T G'.E (Φ.τ l) = G'.E l' := by rw [hl]; rfl
  refine ⟨fun e => cast hE e.1, fun e e' he => Subtype.ext ?_⟩
  have := congrArg (cast hE.symm) he
  simpa using this

/-- Proposition 16.4, a variant of labels: each target element of `l` is one element of one `G'(l'ᵢ)`, its origin. -/
theorem fragment_one_origin (Φ : ExtMapping S S' P) (G' : Apg S') (hG : P G')
    (keep : ∀ l, (Φ.delta G' hG).E l → Bool) (h : Closed (Φ.delta G' hG) keep) {l : L} {ls : List L'}
    (hl : Φ.τ l = labelSum ls) :
    ∃ f : (fragment Φ G' hG keep h).E l → (i : Fin ls.length) × G'.E (ls.get i), ∀ e e', f e = f e' → e = e' :=
  let b := Φ.delta_one_origin G' hG hl
  ⟨fun e => b.toFun e.1, fun e e' he =>
    Subtype.ext ((b.left e.1).symm.trans ((congrArg b.invFun he).trans (b.left e'.1)))⟩

/-- Where the fragment's selection is undefined, a kept target element references a dropped one. -/
theorem fragment_undefined (Φ : ExtMapping S S' P) (G' : Apg S') (hG : P G')
    (keep : ∀ l, (Φ.delta G' hG).E l → Bool) (h : ¬ Closed (Φ.delta G' hG) keep) :
    ∃ l e r, keep l e = true ∧ V.Ref (S.σ l) ((Φ.delta G' hG).υ l e) r ∧ keep r.1 r.2 = false :=
  select_undefined _ keep h

/-! ## Proposition 16.6: history through a migration -/

theorem finSet_map_injective {α β : Type} {f : α → β} (hf : ∀ a a', f a = f a' → a = a') (s s' : FinSet α)
    (h : FinSet.map f s = FinSet.map f s') : s = s' := by
  induction s using Quot.ind with
  | mk l =>
    induction s' using Quot.ind with
    | mk l' =>
      apply FinSet.ext
      intro x
      have hm : ∀ y, FinSet.Mem y (FinSet.map f (Quot.mk _ l)) ↔ FinSet.Mem y (FinSet.map f (Quot.mk _ l')) :=
        fun y => by rw [h]
      have := hm (f x)
      simp only [FinSet.map, FinSet.Mem, FinSet.ofList, Quot.liftOn, Quot.lift, List.mem_map] at this
      show x ∈ l ↔ x ∈ l'
      constructor
      · intro hx
        obtain ⟨y, hy, hyx⟩ := this.mp ⟨x, hx, rfl⟩
        exact hf y x hyx ▸ hy
      · intro hx
        obtain ⟨y, hy, hyx⟩ := this.mpr ⟨x, hx, rfl⟩
        exact hf y x hyx ▸ hy

theorem list_map_injective {α β : Type} {f : α → β} (hf : ∀ a a', f a = f a' → a = a') :
    ∀ xs ys : List α, xs.map f = ys.map f → xs = ys
  | [], [], _ => rfl
  | [], _ :: _, h => by simp at h
  | _ :: _, [], h => by simp at h
  | x :: xs, y :: ys, h => by
    simp only [List.map_cons, List.cons.injEq] at h
    rw [hf x y h.1, list_map_injective hf xs ys h.2]

/-- A family of injections acts injectively on values. -/
theorem map_injective {E E' : L' → Type} {h : ∀ l, E l → E' l} (hinj : ∀ l a a', h l a = h l a' → a = a') :
    (t : Ty T.P L') → ∀ x y, V.map h t x = V.map h t y → x = y
  | .zero, x, _, _ => nomatch x
  | .one, _, _, hxy => hxy
  | .prim _, _, _, hxy => hxy
  | .lbl l, _, _, hxy => hinj l _ _ hxy
  | .sum a _, .inl x, .inl y, hxy => by
    injection hxy with hxy
    rw [map_injective hinj a x y hxy]
  | .sum _ _, .inl _, .inr _, hxy => by simp [V.map] at hxy
  | .sum _ _, .inr _, .inl _, hxy => by simp [V.map] at hxy
  | .sum _ b, .inr x, .inr y, hxy => by
    injection hxy with hxy
    rw [map_injective hinj b x y hxy]
  | .prod a b, (x, x'), (y, y'), hxy => by
    injection hxy with h1 h2
    rw [map_injective hinj a x y h1, map_injective hinj b x' y' h2]
  | .list t, xs, ys, hxy => list_map_injective (map_injective hinj t) xs ys hxy
  | .set t, s, s', hxy => finSet_map_injective (map_injective hinj t) s s' hxy

/-- A reference of a value is, through a family of functions, a reference of its image. -/
theorem ref_map {E E' : L' → Type} (h : ∀ l, E l → E' l) :
    (t : Ty T.P L') → ∀ (x : V T E t) (r : Σ l, E l), V.Ref t x r → V.Ref t (V.map h t x) ⟨r.1, h r.1 r.2⟩
  | .zero, x, _, _ => nomatch x
  | .one, _, _, hr => hr.elim
  | .prim _, _, _, hr => hr.elim
  | .lbl _, _, _, hr => by subst hr; rfl
  | .sum a _, .inl x, r, hr => ref_map h a x r hr
  | .sum _ b, .inr y, r, hr => ref_map h b y r hr
  | .prod a b, (x, y), r, hr => hr.elim (fun hr => Or.inl (ref_map h a x r hr)) (fun hr => Or.inr (ref_map h b y r hr))
  | .list t, xs, r, ⟨y, hy, hr⟩ => ⟨V.map h t y, List.mem_map_of_mem _ hy, ref_map h t y r hr⟩
  | .set t, s, r, hr => by
    revert hr
    induction s using Quot.ind with
    | mk l =>
      rintro ⟨y, hy, hr⟩
      exact ⟨V.map h t y, List.mem_map_of_mem _ hy, ref_map h t y r hr⟩

/-- Proposition 16.6: a migration of the fragment read on the slice `B` believed at an instant before the cutover, an
APG (proposition 15.2) included into the stored present `G` by an injective homomorphism `i` preserving names. The
fragment of `B` is an APG on the target's schema wherever its selection is defined; each of its elements is, through
`i`, one element of the target's `Δ` of the present; and each origin it names is the origin its image names, with its
name. -/
theorem history_through_migration (Φ : ExtMapping S S' P) {B G : Apg S'} (i : Hom B G)
    (hinj : ∀ l a a', i.h l a = i.h l a' → a = a') (ιB : Naming B) (ιG : Naming G) (hnames : PreservesNames i ιB ιG)
    (hB : P B) (hG : P G) (keep : ∀ l, (Φ.delta B hB).E l → Bool) (h : Closed (Φ.delta B hB) keep) (l : L) :
    (∀ e e' : (fragment Φ B hB keep h).E l,
        (V.map i.h (Φ.τ l) e.1 : (Φ.delta G hG).E l) = V.map i.h (Φ.τ l) e'.1 → e = e') ∧
      ∀ e : (fragment Φ B hB keep h).E l, ∀ r, V.Ref (Φ.τ l) e.1 r → ∀ s, ιB r.1 r.2 = some s →
        V.Ref (Φ.τ l) (V.map i.h (Φ.τ l) e.1) ⟨r.1, i.h r.1 r.2⟩ ∧ ιG r.1 (i.h r.1 r.2) = some s :=
  ⟨fun e e' he => Subtype.ext (map_injective hinj (Φ.τ l) e.1 e'.1 he),
    fun e r hr s hs => ⟨ref_map i.h (Φ.τ l) e.1 r hr, hnames r.1 r.2 s hs⟩⟩

end Ext

/-! ## Definition 16.5: coproducts -/

section Coprod

variable {S' : Schema T.P L'}

/-- The coproduct of schemas with disjoint labels, `S + S' = (L ⊔ L', σ ⊔ σ')`. -/
def coprodSchema (S : Schema T.P L) (S' : Schema T.P L') : Schema T.P (L ⊕ L') where
  σ
    | .inl l => (S.σ l).subst fun l => Ty.lbl (Sum.inl l)
    | .inr l => (S'.σ l).subst fun l => Ty.lbl (Sum.inr l)

/-- The elements of the coproduct: those of each summand. -/
def coprodE (G : Apg S) (G' : Apg S') : L ⊕ L' → Type
  | .inl l => G.E l
  | .inr l => G'.E l

/-- Definition 16.5: the coproduct of APGs, the APG on `S + S'` holding the elements of both. -/
def coprod (G : Apg S) (G' : Apg S') : Apg (coprodSchema S S') where
  E := coprodE G G'
  υ
    | .inl l, e => iotaInv (fun l => Ty.lbl (Sum.inl l) : L → Ty T.P (L ⊕ L')) (coprodE G G') (S.σ l) (G.υ l e)
    | .inr l, e => iotaInv (fun l => Ty.lbl (Sum.inr l) : L' → Ty T.P (L ⊕ L')) (coprodE G G') (S'.σ l) (G'.υ l e)

/-- The left injection of schemas, each label its copy in the coproduct. -/
def inlMapping (S : Schema T.P L) (S' : Schema T.P L') : Mapping S (coprodSchema S S') where
  τ l := Ty.lbl (Sum.inl l)
  t l := Tm.deref (Sum.inl l) (Tm.var Var.here)

/-- The right injection of schemas. -/
def inrMapping (S : Schema T.P L) (S' : Schema T.P L') : Mapping S' (coprodSchema S S') where
  τ l := Ty.lbl (Sum.inr l)
  t l := Tm.deref (Sum.inr l) (Tm.var Var.here)

/-- `Δ` along the left injection gives back the left summand. -/
def coprod_inl (G : Apg S) (G' : Apg S') : Iso ((inlMapping S S').delta (coprod G G')) G where
  hom := ⟨fun _ e => e, fun l e => by
    show G.υ l e = V.map (fun _ e => e) (S.σ l)
      (iota (fun l => Ty.lbl (Sum.inl l) : L → Ty T.P (L ⊕ L')) (coprodE G G') (S.σ l)
        (iotaInv (fun l => Ty.lbl (Sum.inl l) : L → Ty T.P (L ⊕ L')) (coprodE G G') (S.σ l) (G.υ l e)))
    rw [V.map_id, iota_iotaInv]⟩
  inv := ⟨fun _ e => e, fun l e => by
    show iota (fun l => Ty.lbl (Sum.inl l) : L → Ty T.P (L ⊕ L')) (coprodE G G') (S.σ l)
        (iotaInv (fun l => Ty.lbl (Sum.inl l) : L → Ty T.P (L ⊕ L')) (coprodE G G') (S.σ l) (G.υ l e)) =
      V.map (fun _ e => e) (S.σ l) (G.υ l e)
    rw [V.map_id, iota_iotaInv]⟩
  inv_hom _ _ := rfl
  hom_inv _ _ := rfl

/-- `Δ` along the right injection gives back the right summand. -/
def coprod_inr (G : Apg S) (G' : Apg S') : Iso ((inrMapping S S').delta (coprod G G')) G' where
  hom := ⟨fun _ e => e, fun l e => by
    show G'.υ l e = V.map (fun _ e => e) (S'.σ l)
      (iota (fun l => Ty.lbl (Sum.inr l) : L' → Ty T.P (L ⊕ L')) (coprodE G G') (S'.σ l)
        (iotaInv (fun l => Ty.lbl (Sum.inr l) : L' → Ty T.P (L ⊕ L')) (coprodE G G') (S'.σ l) (G'.υ l e)))
    rw [V.map_id, iota_iotaInv]⟩
  inv := ⟨fun _ e => e, fun l e => by
    show iota (fun l => Ty.lbl (Sum.inr l) : L' → Ty T.P (L ⊕ L')) (coprodE G G') (S'.σ l)
        (iotaInv (fun l => Ty.lbl (Sum.inr l) : L' → Ty T.P (L ⊕ L')) (coprodE G G') (S'.σ l) (G'.υ l e)) =
      V.map (fun _ e => e) (S'.σ l) (G'.υ l e)
    rw [V.map_id, iota_iotaInv]⟩
  inv_hom _ _ := rfl
  hom_inv _ _ := rfl

end Coprod

end ApgModel.Migration
