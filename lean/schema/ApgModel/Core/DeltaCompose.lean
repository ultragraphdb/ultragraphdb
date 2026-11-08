/-
 SPDX-License-Identifier: Apache-2.0
 Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
-/
import ApgModel.Core.Delta

/-!
# Mappings compose as functors of the APG theories

`SCHEMA-APG.md` proposition 9.3, the second half: the identity mapping and the composite of two mappings, with `Δ`
along them isomorphic to the identity and to the composite of the two `Δ`s, naturally. With the first half
(`Mapping.deltaHom_id`, `Mapping.deltaHom_comp`) this is `S ↦ S-APG`, `Φ ↦ Δ_Φ` a functor `Schema^op → Cat`, up to
the isomorphisms the type substitutions' equations give.

- The identity mapping (`Mapping.id`) has `τ_l = lbl l` and `t_l = deref x`; `Δ` along it is isomorphic to the
  identity (`Mapping.delta_id`).
- A mapping term over `S'` translates along `Ψ : S' → S''` to one over `S''` (`Tm.along`): types by `Ψ̂`, and each
  `deref` of a label `l'` replaced by `Ψ`'s term `t_{l'}` applied to the translated argument. Evaluating the
  translation on `G''` is evaluating the term on `Δ_Ψ(G'')`, through `ι` (`Tm.eval_along`): the functor of theories
  `Ψ` defines, acting on terms.
- The composite `Ψ ∘ Φ` (`Mapping.comp`) has images `Ψ̂(τ_l)` and terms `t_l` translated along `Ψ`, and
  `Δ_{Ψ∘Φ}(G'') ≅ Δ_Φ(Δ_Ψ(G''))` (`Mapping.delta_comp`).

A translated term's type is `Ψ̂(Φ̂(σ(l)))`, which equals the composite's `(Ψ∘Φ)^(σ(l))` by `Ty.subst_subst`, a
proved equation rather than a definitional one; the composite's term is cast along it once (`Tm.castTy`), and the
proofs compare values across it by heterogeneous equality (`iota_lbl`, `iota_comp`).
-/

namespace ApgModel.Core

variable {T : Typeside} {L L' L'' : Type}

/-! ## Casts along equations of types -/

/-- A term moved along an equation of its type. -/
def Tm.castTy {S : Schema T.P L} {Γ : List (Ty T.P L)} {a b : Ty T.P L} (h : a = b) (s : Tm S Γ a) : Tm S Γ b :=
  h ▸ s

theorem Tm.eval_castTy {S : Schema T.P L} (G : Apg S) {Γ : List (Ty T.P L)} {a b : Ty T.P L} (h : a = b)
    (s : Tm S Γ a) (ρ : Env T G.E Γ) : HEq ((s.castTy h).eval G ρ) (s.eval G ρ) := by
  cases h
  rfl

private theorem heq_sum {A B A' B' : Type} (ea : A = A') (eb : B = B') {y : A ⊕ B} {x : A' ⊕ B'} (h : HEq y x) :
    (∃ y₁ x₁, y = Sum.inl y₁ ∧ x = Sum.inl x₁ ∧ HEq y₁ x₁) ∨
      (∃ y₂ x₂, y = Sum.inr y₂ ∧ x = Sum.inr x₂ ∧ HEq y₂ x₂) := by
  subst ea eb
  cases eq_of_heq h
  cases y with
  | inl a => exact Or.inl ⟨a, a, rfl, rfl, HEq.rfl⟩
  | inr b => exact Or.inr ⟨b, b, rfl, rfl, HEq.rfl⟩

private theorem heq_prod {A B A' B' : Type} (ea : A = A') (eb : B = B') {y : A × B} {x : A' × B'} (h : HEq y x) :
    HEq y.1 x.1 ∧ HEq y.2 x.2 := by
  subst ea eb
  cases eq_of_heq h
  exact ⟨HEq.rfl, HEq.rfl⟩

private theorem list_map_heq {A A' B : Type} (e : A = A') {ys : List A} {xs : List A'} (h : HEq ys xs)
    (f : A → B) (g : A' → B) (w : ∀ y x, HEq y x → f y = g x) : List.map f ys = List.map g xs := by
  subst e
  cases eq_of_heq h
  exact List.map_congr_left fun y _ => w y y HEq.rfl

private theorem finset_map_heq {A A' B : Type} (e : A = A') {ys : FinSet A} {xs : FinSet A'} (h : HEq ys xs)
    (f : A → B) (g : A' → B) (w : ∀ y x, HEq y x → f y = g x) : FinSet.map f ys = FinSet.map g xs := by
  subst e
  cases eq_of_heq h
  exact FinSet.map_congr (fun y => w y y HEq.rfl) ys

/-! ## The identity mapping -/

/-- `ι` along the identity images is the identity, across the equation `Ty.subst_lbl`. -/
theorem iota_lbl (E : L → Type) (t : Ty T.P L) (y : V T E (t.subst .lbl)) (x : V T E t) (h : HEq y x) :
    iota (T := T) .lbl E t y = x := by
  induction t with
  | zero => exact nomatch x
  | one | prim _ => exact eq_of_heq h
  | lbl _ => exact eq_of_heq h
  | sum a b iha ihb =>
    rcases heq_sum (congrArg (V T E) (Ty.subst_lbl a)) (congrArg (V T E) (Ty.subst_lbl b)) h with
      ⟨y₁, x₁, rfl, rfl, h₁⟩ | ⟨y₂, x₂, rfl, rfl, h₂⟩
    · exact congrArg Sum.inl (iha y₁ x₁ h₁)
    · exact congrArg Sum.inr (ihb y₂ x₂ h₂)
  | prod a b iha ihb =>
    obtain ⟨h₁, h₂⟩ := heq_prod (congrArg (V T E) (Ty.subst_lbl a)) (congrArg (V T E) (Ty.subst_lbl b)) h
    obtain ⟨x₁, x₂⟩ := x
    exact Prod.ext (iha y.1 x₁ h₁) (ihb y.2 x₂ h₂)
  | list t ih =>
    show List.map (iota .lbl E t) y = x
    exact (list_map_heq (congrArg (V T E) (Ty.subst_lbl t)) h _ (fun x => x) ih).trans (List.map_id' x)
  | set t ih =>
    show FinSet.map (iota .lbl E t) y = x
    exact (finset_map_heq (congrArg (V T E) (Ty.subst_lbl t)) h _ (fun x => x) ih).trans (FinSet.map_id' x)

variable {S : Schema T.P L} {S' : Schema T.P L'} {S'' : Schema T.P L''}

/-- The identity mapping: each label its own image, its term `deref x`. -/
def Mapping.id (S : Schema T.P L) : Mapping S S where
  τ := .lbl
  t l := (Tm.deref l (.var .here)).castTy (Ty.subst_lbl (S.σ l)).symm

/-- Proposition 9.3: `Δ` along the identity mapping is isomorphic to the identity. -/
def Mapping.delta_id (G : Apg S) : Iso ((Mapping.id S).delta G) G :=
  Iso.ofHom
    { h := fun _ e => e
      comm := by
        intro l e
        show G.υ l e = V.map (fun _ e => e) (S.σ l)
          (iota .lbl G.E (S.σ l) (((Tm.deref l (.var .here)).castTy (Ty.subst_lbl (S.σ l)).symm).eval G (Env.one e)))
        rw [V.map_id]
        exact (iota_lbl G.E (S.σ l) _ (G.υ l e)
          (Tm.eval_castTy G (Ty.subst_lbl (S.σ l)).symm (Tm.deref l (.var .here)) (Env.one e))).symm }
    (fun _ e => e) (fun _ _ => rfl) (fun _ _ => rfl)

/-! ## Terms along a mapping -/

/-- A variable along `Φ̂`. -/
def Var.along {P L L' : Type} (τ : L → Ty P L') : {Γ : List (Ty P L)} → {t : Ty P L} → Var Γ t →
    Var (Γ.map (Ty.subst τ)) (t.subst τ)
  | _, _, .here => .here
  | _, _, .there v => .there (v.along τ)

/-- The substitution of one term for the one variable of a context. -/
def Sub.single {S : Schema T.P L} {Δ : List (Ty T.P L)} {a : Ty T.P L} (s : Tm S Δ a) : Sub S [a] Δ
  | _, .here => s

/-- A mapping term over `S'` translated along `Ψ : S' → S''`: types by `Ψ̂`, each `deref` of `l'` replaced by
`Ψ`'s term `t_{l'}` applied to the translated argument. -/
def Tm.along (Ψ : Mapping S' S'') : {Γ : List (Ty T.P L')} → {t : Ty T.P L'} → Tm S' Γ t →
    Tm S'' (Γ.map (Ty.subst Ψ.τ)) (t.subst Ψ.τ)
  | _, _, .var v => .var (v.along Ψ.τ)
  | _, _, .lit p a => .lit p a
  | _, _, .app f args => .app f (fun i => (args i).along Ψ)
  | _, _, .unit => .unit
  | _, _, .pair a b => .pair (a.along Ψ) (b.along Ψ)
  | _, _, .fst s => .fst (s.along Ψ)
  | _, _, .snd s => .snd (s.along Ψ)
  | _, _, .inl s => .inl (s.along Ψ)
  | _, _, .inr s => .inr (s.along Ψ)
  | _, _, .case s b₁ b₂ => .case (s.along Ψ) (b₁.along Ψ) (b₂.along Ψ)
  | _, _, .absurd s => .absurd (s.along Ψ)
  | _, _, .deref l' s => (Ψ.t l').subst (Sub.single (s.along Ψ))
  | _, _, .nil => .nil
  | _, _, .cons x xs => .cons (x.along Ψ) (xs.along Ψ)
  | _, _, .append xs ys => .append (xs.along Ψ) (ys.along Ψ)
  | _, _, .map s u => .map (s.along Ψ) (u.along Ψ)
  | _, _, .filter s q => .filter (s.along Ψ) (q.along Ψ)
  | _, _, .empty => .empty
  | _, _, .insert x s => .insert (x.along Ψ) (s.along Ψ)
  | _, _, .union s s' => .union (s.along Ψ) (s'.along Ψ)
  | _, _, .mapSet s u => .mapSet (s.along Ψ) (u.along Ψ)
  | _, _, .filterSet s q => .filterSet (s.along Ψ) (q.along Ψ)

/-- An environment of the translated context read as one of `Δ_Ψ`, through `ι`. -/
def envIota (Ψ : Mapping S' S'') {E'' : L'' → Type} {Γ : List (Ty T.P L')} (ρ : Env T E'' (Γ.map (Ty.subst Ψ.τ))) :
    Env T (fun l => V T E'' (Ψ.τ l)) Γ :=
  fun u v => iota Ψ.τ E'' u (ρ _ (v.along Ψ.τ))

theorem envIota_cons (Ψ : Mapping S' S'') {E'' : L'' → Type} {Γ : List (Ty T.P L')} {a : Ty T.P L'}
    (x : V T E'' (a.subst Ψ.τ)) (ρ : Env T E'' (Γ.map (Ty.subst Ψ.τ))) :
    envIota Ψ (Γ := a :: Γ) (Env.cons x ρ) = Env.cons (iota Ψ.τ E'' a x) (envIota Ψ ρ) := by
  funext u v
  cases v <;> rfl

theorem envIota_one (Ψ : Mapping S' S'') {E'' : L'' → Type} {a : Ty T.P L'} (x : V T E'' (a.subst Ψ.τ)) :
    envIota Ψ (Γ := [a]) (Env.one x) = Env.one (iota Ψ.τ E'' a x) := by
  funext u v
  cases v with
  | here => rfl
  | there v => exact nomatch v

theorem asBool_iota (τ : L' → Ty T.P L'') (E'' : L'' → Type) (b : V T E'' (Ty.bool.subst τ)) :
    asBool (iota τ E'' Ty.bool b) = asBool (E := E'') b := by
  rcases b with _ | _ | b
  · rfl
  · rfl
  · exact nomatch b

/-- Proposition 9.3, the functor of theories acting on terms: evaluating a term translated along `Ψ` on `G''` is
evaluating the term on `Δ_Ψ(G'')`, through `ι`. -/
theorem Tm.eval_along (Ψ : Mapping S' S'') (G'' : Apg S'') {Γ : List (Ty T.P L')} {t : Ty T.P L'} (s : Tm S' Γ t)
    (ρ : Env T G''.E (Γ.map (Ty.subst Ψ.τ))) :
    iota Ψ.τ G''.E t ((s.along Ψ).eval G'' ρ) = s.eval (Ψ.delta G'') (envIota Ψ ρ) := by
  induction s with
  | var v => rfl
  | lit p a => rfl
  | app f args ih => exact congrArg (T.interp f) (funext fun i => ih i ρ)
  | unit => rfl
  | pair a b iha ihb => exact Prod.ext (iha ρ) (ihb ρ)
  | fst s ih =>
    show iota Ψ.τ G''.E _ (Prod.fst ((s.along Ψ).eval G'' ρ)) = Prod.fst (s.eval (Ψ.delta G'') (envIota Ψ ρ))
    rw [← ih ρ]
    obtain ⟨x, y⟩ := (s.along Ψ).eval G'' ρ
    rfl
  | snd s ih =>
    show iota Ψ.τ G''.E _ (Prod.snd ((s.along Ψ).eval G'' ρ)) = Prod.snd (s.eval (Ψ.delta G'') (envIota Ψ ρ))
    rw [← ih ρ]
    obtain ⟨x, y⟩ := (s.along Ψ).eval G'' ρ
    rfl
  | inl s ih => exact congrArg Sum.inl (ih ρ)
  | inr s ih => exact congrArg Sum.inr (ih ρ)
  | case s b₁ b₂ ihs ih₁ ih₂ =>
    simp only [Tm.along, Tm.eval]
    rw [← ihs ρ]
    generalize (s.along Ψ).eval G'' ρ = sv
    cases sv with
    | inl x =>
      show iota Ψ.τ G''.E _ ((b₁.along Ψ).eval G'' (Env.cons x ρ)) =
        b₁.eval (Ψ.delta G'') (Env.cons (iota Ψ.τ G''.E _ x) (envIota Ψ ρ))
      rw [ih₁, envIota_cons]
    | inr y =>
      show iota Ψ.τ G''.E _ ((b₂.along Ψ).eval G'' (Env.cons y ρ)) =
        b₂.eval (Ψ.delta G'') (Env.cons (iota Ψ.τ G''.E _ y) (envIota Ψ ρ))
      rw [ih₂, envIota_cons]
  | absurd s ih => exact nomatch ((s.along Ψ).eval G'' ρ)
  | deref l' s ih =>
    show iota Ψ.τ G''.E _ (((Ψ.t l').subst (Sub.single (s.along Ψ))).eval G'' ρ) =
      iota Ψ.τ G''.E _ ((Ψ.t l').eval G'' (Env.one (s.eval (Ψ.delta G'') (envIota Ψ ρ))))
    rw [Tm.eval_subst, ← ih ρ]
    congr 2
    funext u v
    cases v with
    | here => rfl
    | there v => exact nomatch v
  | nil => rfl
  | cons x xs ihx ihxs => exact congr_two List.cons (ihx ρ) (ihxs ρ)
  | append xs ys ihx ihy => exact (List.map_append _ _ _).trans (congr_two (· ++ ·) (ihx ρ) (ihy ρ))
  | @map Γ a b s u ihs ihu =>
    exact (list_map_comm (h := iota Ψ.τ G''.E b) (f := fun y => (u.along Ψ).eval G'' (Env.one y))
        (k := iota Ψ.τ G''.E a) (g := fun y => u.eval (Ψ.delta G'') (Env.one y))
        (fun y => (ihu (Env.one y)).trans (by rw [envIota_one])) _).trans
      (congrArg (List.map fun y => u.eval (Ψ.delta G'') (Env.one y)) (ihs ρ))
  | @filter Γ a s q ihs ihq =>
    have w : ∀ y, asBool ((q.along Ψ).eval G'' (Env.one y)) =
        asBool (q.eval (Ψ.delta G'') (Env.one (iota Ψ.τ G''.E a y))) := by
      intro y
      rw [← asBool_iota Ψ.τ G''.E, ihq (Env.one y), envIota_one]
    exact (list_map_filter_comm (iota Ψ.τ G''.E a) w _).trans
      (congrArg (List.filter fun y => asBool (q.eval (Ψ.delta G'') (Env.one y))) (ihs ρ))
  | empty => rfl
  | insert x s ihx ihs => exact (FinSet.map_insert _ _ _).trans (congr_two FinSet.insert (ihx ρ) (ihs ρ))
  | union s s' ih ih' => exact (FinSet.map_union _ _ _).trans (congr_two FinSet.union (ih ρ) (ih' ρ))
  | @mapSet Γ a b s u ihs ihu =>
    exact (FinSet.map_comm (h := iota Ψ.τ G''.E b) (f := fun y => (u.along Ψ).eval G'' (Env.one y))
        (k := iota Ψ.τ G''.E a) (g := fun y => u.eval (Ψ.delta G'') (Env.one y))
        (fun y => (ihu (Env.one y)).trans (by rw [envIota_one])) _).trans
      (congrArg (FinSet.map fun y => u.eval (Ψ.delta G'') (Env.one y)) (ihs ρ))
  | @filterSet Γ a s q ihs ihq =>
    have w : ∀ y, asBool ((q.along Ψ).eval G'' (Env.one y)) =
        asBool (q.eval (Ψ.delta G'') (Env.one (iota Ψ.τ G''.E a y))) := by
      intro y
      rw [← asBool_iota Ψ.τ G''.E, ihq (Env.one y), envIota_one]
    exact (FinSet.map_filter_comm (iota Ψ.τ G''.E a) w _).trans
      (congrArg (FinSet.filter fun y => asBool (q.eval (Ψ.delta G'') (Env.one y))) (ihs ρ))

/-! ## The composite mapping -/

/-- The composite `Ψ ∘ Φ : S → S''`: images `Ψ̂(τ_l)`, terms `t_l` translated along `Ψ`, cast along
`Ty.subst_subst`. -/
def Mapping.comp (Φ : Mapping S S') (Ψ : Mapping S' S'') : Mapping S S'' where
  τ l := (Φ.τ l).subst Ψ.τ
  t l := ((Φ.t l).along Ψ).castTy (Ty.subst_subst Φ.τ Ψ.τ (S.σ l))

/-- `ι` along the composite, mapped into `Δ_Φ(Δ_Ψ)`, is `ι` along `Ψ` then along `Φ`, across `Ty.subst_subst`. -/
theorem iota_comp (τΦ : L → Ty T.P L') (τΨ : L' → Ty T.P L'') (E'' : L'' → Type) (t : Ty T.P L)
    (y'' : V T E'' (t.subst fun l => (τΦ l).subst τΨ)) (y : V T E'' ((t.subst τΦ).subst τΨ)) (h : HEq y'' y) :
    V.map (fun l => iota τΨ E'' (τΦ l)) t (iota (fun l => (τΦ l).subst τΨ) E'' t y'') =
      iota τΦ (fun l => V T E'' (τΨ l)) t (iota τΨ E'' (t.subst τΦ) y) := by
  have e : ∀ u : Ty T.P L, V T E'' (u.subst fun l => (τΦ l).subst τΨ) = V T E'' ((u.subst τΦ).subst τΨ) :=
    fun u => congrArg (V T E'') (Ty.subst_subst τΦ τΨ u).symm
  induction t with
  | zero => exact nomatch y
  | one | prim _ => cases eq_of_heq h; rfl
  | lbl _ => cases eq_of_heq h; rfl
  | sum a b iha ihb =>
    rcases heq_sum (e a) (e b) h with ⟨y₁, x₁, rfl, rfl, h₁⟩ | ⟨y₂, x₂, rfl, rfl, h₂⟩
    · exact congrArg Sum.inl (iha y₁ x₁ h₁)
    · exact congrArg Sum.inr (ihb y₂ x₂ h₂)
  | prod a b iha ihb =>
    obtain ⟨h₁, h₂⟩ := heq_prod (e a) (e b) h
    exact Prod.ext (iha y''.1 y.1 h₁) (ihb y''.2 y.2 h₂)
  | list t ih =>
    show List.map _ (List.map _ y'') = List.map _ (List.map _ y)
    rw [List.map_map, List.map_map]
    exact list_map_heq (e t) h _ _ ih
  | set t ih =>
    show FinSet.map _ (FinSet.map _ y'') = FinSet.map _ (FinSet.map _ y)
    rw [FinSet.map_map, FinSet.map_map]
    exact finset_map_heq (e t) h _ _ ih

/-- Proposition 9.3: `Δ` along the composite is isomorphic to the composite of the `Δ`s,
`Δ_{Ψ∘Φ}(G'') ≅ Δ_Φ(Δ_Ψ(G''))`, each element's component `ι` along `Ψ`. -/
def Mapping.delta_comp (Φ : Mapping S S') (Ψ : Mapping S' S'') (G'' : Apg S'') :
    Iso ((Φ.comp Ψ).delta G'') (Φ.delta (Ψ.delta G'')) :=
  Iso.ofHom
    { h := fun l y => iota Ψ.τ G''.E (Φ.τ l) y
      comm := by
        intro l y
        show iota Φ.τ (fun l => V T G''.E (Ψ.τ l)) (S.σ l)
            ((Φ.t l).eval (Ψ.delta G'') (Env.one (iota Ψ.τ G''.E (Φ.τ l) y))) =
          V.map (fun l => iota Ψ.τ G''.E (Φ.τ l)) (S.σ l)
            (iota (Φ.comp Ψ).τ G''.E (S.σ l) (((Φ.comp Ψ).t l).eval G'' (Env.one y)))
        rw [← envIota_one, ← Tm.eval_along]
        exact (iota_comp Φ.τ Ψ.τ G''.E (S.σ l) _ _ (Tm.eval_castTy G'' _ _ _)).symm }
    (fun l y => iotaInv Ψ.τ G''.E (Φ.τ l) y)
    (fun l y => iotaInv_iota Ψ.τ G''.E (Φ.τ l) y)
    (fun l y => iota_iotaInv Ψ.τ G''.E (Φ.τ l) y)

end ApgModel.Core
