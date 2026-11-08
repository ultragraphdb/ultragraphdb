/-
 SPDX-License-Identifier: Apache-2.0
 Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
-/
import ApgModel.Core.Term

/-!
# Models of the APG theory

`SCHEMA-APG.md` definitions 8.2 and 8.3 and proposition 8.4, in the generator form decision L4 of
`openspec/notes/prepare-apg-lean.md` takes: the theory `C_S` is not built as a category object; its arrows are the
mapping terms of definition 8.5, and a model is an interpretation of every term that preserves what the theory's
structure fixes.

A `Model` interprets each label as a set and the generator `δ_l` as a function into its type's values, and every
term as a function of its environment, preserving products, coproducts, primitives, the typeside's functions, the
generators and the collection operations: one equation per term constructor. Proposition 8.4 is then:

- an APG gives the model whose interpretation is evaluation (`Model.ofApg`), and a model gives the APG of its
  generators (`Model.toApg`);
- every model's interpretation is evaluation in its APG (`Model.interp_eq_eval`): evaluation is the unique extension of
  the generators, which is the freeness of `C_S` on them;
- so the two constructions are inverse (`Model.toApg_ofApg`, `Model.ofApg_toApg`);
- a homomorphism of APGs commutes with the interpretation of every term (`Tm.eval_hom`), so it is a morphism of
  models, and a morphism of models, commuting with the generators, is a homomorphism.

The paper's Coq development builds the free bicartesian category as a quotient (`APG.v`'s `freeBCC`) and constructs
both directions without a theorem that they form an equivalence; it has no distributivity. Here the `case` of a term
reads its context, which is the distributivity of section 8.2.
-/

namespace ApgModel.Core

variable {T : Typeside} {L : Type} {S : Schema T.P L}

/-! ## Homomorphisms commute with evaluation -/

/-- An environment mapped by a morphism of families. -/
def Env.map {E E' : L → Type} (h : ∀ l, E l → E' l) {Γ : List (Ty T.P L)} (ρ : Env T E Γ) : Env T E' Γ :=
  fun t v => V.map h t (ρ t v)

theorem Env.map_cons {E E' : L → Type} (h : ∀ l, E l → E' l) {Γ : List (Ty T.P L)} {a : Ty T.P L}
    (x : V T E a) (ρ : Env T E Γ) : Env.map h (Env.cons x ρ) = Env.cons (V.map h a x) (Env.map h ρ) := by
  funext t v
  cases v <;> rfl

theorem asBool_map {E E' : L → Type} (h : ∀ l, E l → E' l) (b : V T E Ty.bool) :
    asBool (V.map h Ty.bool b) = asBool b := by
  rcases b with _ | _ | b
  · rfl
  · rfl
  · exact nomatch b

/-- Proposition 8.4, morphisms: a homomorphism commutes with the evaluation of every term, `V_h ∘ ⟦s⟧_G = ⟦s⟧_G' ∘ V_h`:
the natural transformation of models it induces. -/
theorem Tm.eval_hom {G G' : Apg S} (h : Hom G G') {Γ : List (Ty T.P L)} {t : Ty T.P L} (s : Tm S Γ t)
    (ρ : Env T G.E Γ) : V.map h.h t (s.eval G ρ) = s.eval G' (Env.map h.h ρ) := by
  induction s with
  | var v => rfl
  | lit p a => rfl
  | app f args ih => exact congrArg (T.interp f) (funext fun i => ih i ρ)
  | unit => rfl
  | pair a b iha ihb => exact Prod.ext (iha ρ) (ihb ρ)
  | fst s ih =>
    show V.map h.h _ (Prod.fst (s.eval G ρ)) = Prod.fst (s.eval G' (Env.map h.h ρ))
    rw [← ih ρ]
    obtain ⟨x, y⟩ := s.eval G ρ
    rfl
  | snd s ih =>
    show V.map h.h _ (Prod.snd (s.eval G ρ)) = Prod.snd (s.eval G' (Env.map h.h ρ))
    rw [← ih ρ]
    obtain ⟨x, y⟩ := s.eval G ρ
    rfl
  | inl s ih => exact congrArg Sum.inl (ih ρ)
  | inr s ih => exact congrArg Sum.inr (ih ρ)
  | case s b₁ b₂ ihs ih₁ ih₂ =>
    simp only [Tm.eval]
    rw [← ihs ρ]
    generalize s.eval G ρ = sv
    cases sv with
    | inl x =>
      show V.map h.h _ (b₁.eval G (Env.cons x ρ)) = b₁.eval G' (Env.cons (V.map h.h _ x) (Env.map h.h ρ))
      rw [ih₁, Env.map_cons]
    | inr y =>
      show V.map h.h _ (b₂.eval G (Env.cons y ρ)) = b₂.eval G' (Env.cons (V.map h.h _ y) (Env.map h.h ρ))
      rw [ih₂, Env.map_cons]
  | absurd s ih => exact nomatch (s.eval G ρ)
  | deref l s ih =>
    show V.map h.h _ (G.υ l (s.eval G ρ)) = G'.υ l (s.eval G' (Env.map h.h ρ))
    rw [← ih ρ]
    exact (h.comm l _).symm
  | nil => rfl
  | cons x xs ihx ihxs => exact congr_two List.cons (ihx ρ) (ihxs ρ)
  | append xs ys ihx ihy => exact (List.map_append _ _ _).trans (congr_two (· ++ ·) (ihx ρ) (ihy ρ))
  | @map Γ a b s u ihs ihu =>
    exact (list_map_comm (h := V.map h.h b) (f := fun y => u.eval G (Env.one y)) (k := V.map h.h a)
        (g := fun y => u.eval G' (Env.one y))
        (fun y => (ihu (Env.one y)).trans (congrArg (u.eval G') (Env.map_cons h.h y Env.nil ▸ by
          funext t v; cases v with | here => rfl | there v => exact nomatch v))) _).trans
      (congrArg (List.map fun y => u.eval G' (Env.one y)) (ihs ρ))
  | @filter Γ a s q ihs ihq =>
    have w : ∀ y, asBool (q.eval G (Env.one y)) = asBool (q.eval G' (Env.one (V.map h.h a y))) := by
      intro y
      rw [← asBool_map h.h, ihq (Env.one y)]
      congr 2
      funext t v
      cases v with
      | here => rfl
      | there v => exact nomatch v
    exact (list_map_filter_comm (V.map h.h a) w _).trans
      (congrArg (List.filter fun y => asBool (q.eval G' (Env.one y))) (ihs ρ))
  | empty => rfl
  | insert x s ihx ihs => exact (FinSet.map_insert _ _ _).trans (congr_two FinSet.insert (ihx ρ) (ihs ρ))
  | union s s' ih ih' => exact (FinSet.map_union _ _ _).trans (congr_two FinSet.union (ih ρ) (ih' ρ))
  | @mapSet Γ a b s u ihs ihu =>
    exact (FinSet.map_comm (h := V.map h.h b) (f := fun y => u.eval G (Env.one y)) (k := V.map h.h a)
        (g := fun y => u.eval G' (Env.one y))
        (fun y => (ihu (Env.one y)).trans (congrArg (u.eval G') (by
          funext t v; cases v with | here => rfl | there v => exact nomatch v))) _).trans
      (congrArg (FinSet.map fun y => u.eval G' (Env.one y)) (ihs ρ))
  | @filterSet Γ a s q ihs ihq =>
    have w : ∀ y, asBool (q.eval G (Env.one y)) = asBool (q.eval G' (Env.one (V.map h.h a y))) := by
      intro y
      rw [← asBool_map h.h, ihq (Env.one y)]
      congr 2
      funext t v
      cases v with
      | here => rfl
      | there v => exact nomatch v
    exact (FinSet.map_filter_comm (V.map h.h a) w _).trans
      (congrArg (FinSet.filter fun y => asBool (q.eval G' (Env.one y))) (ihs ρ))

/-! ## Models -/

/-- A model of `C_S`, definitions 8.2 and 8.3 in generator form: a set per label, the generators `δ_l`, and an
interpretation of every term preserving the theory's structure, one equation per term constructor. -/
structure Model (S : Schema T.P L) where
  /-- The set of each label, `M(lbl l)`. -/
  M : L → Type
  /-- The generators, `M(δ_l)`. -/
  δ : (l : L) → M l → V T M (S.σ l)
  /-- The interpretation of every term. -/
  interp : {Γ : List (Ty T.P L)} → {t : Ty T.P L} → Tm S Γ t → Env T M Γ → V T M t
  var : ∀ {Γ t} (v : Var Γ t) ρ, interp (.var v) ρ = ρ _ v
  lit : ∀ {Γ} p a (ρ : Env T M Γ), interp (.lit p a) ρ = a
  app : ∀ {Γ} f args (ρ : Env T M Γ), interp (.app f args) ρ = T.interp f (fun i => interp (args i) ρ)
  unit : ∀ {Γ} (ρ : Env T M Γ), interp (Γ := Γ) .unit ρ = ()
  pair : ∀ {Γ a b} (x : Tm S Γ a) (y : Tm S Γ b) ρ, interp (.pair x y) ρ = (interp x ρ, interp y ρ)
  fst : ∀ {Γ a b} (s : Tm S Γ (.prod a b)) ρ, interp (.fst s) ρ = Prod.fst (interp s ρ)
  snd : ∀ {Γ a b} (s : Tm S Γ (.prod a b)) ρ, interp (.snd s) ρ = Prod.snd (interp s ρ)
  inl : ∀ {Γ a b} (s : Tm S Γ a) ρ, interp (Tm.inl (b := b) s) ρ = Sum.inl (interp s ρ)
  inr : ∀ {Γ a b} (s : Tm S Γ b) ρ, interp (Tm.inr (a := a) s) ρ = Sum.inr (interp s ρ)
  case : ∀ {Γ a b c} (s : Tm S Γ (.sum a b)) (b₁ : Tm S (a :: Γ) c) (b₂ : Tm S (b :: Γ) c) ρ,
    interp (.case s b₁ b₂) ρ =
      match interp s ρ with
      | Sum.inl x => interp b₁ (Env.cons x ρ)
      | Sum.inr y => interp b₂ (Env.cons y ρ)
  deref : ∀ {Γ} l (s : Tm S Γ (.lbl l)) ρ, interp (.deref l s) ρ = δ l (interp s ρ)
  nil : ∀ {Γ a} (ρ : Env T M Γ), interp (Tm.nil (a := a)) ρ = List.nil
  cons : ∀ {Γ a} (x : Tm S Γ a) xs ρ, interp (.cons x xs) ρ = List.cons (interp x ρ) (interp xs ρ)
  append : ∀ {Γ a} (xs ys : Tm S Γ (.list a)) ρ, interp (.append xs ys) ρ = List.append (interp xs ρ) (interp ys ρ)
  map : ∀ {Γ a b} (s : Tm S Γ (.list a)) (u : Tm S [a] b) ρ,
    interp (.map s u) ρ = List.map (fun y => interp u (Env.one y)) (interp s ρ)
  filter : ∀ {Γ a} (s : Tm S Γ (.list a)) q ρ,
    interp (.filter s q) ρ = List.filter (fun y => asBool (interp q (Env.one y))) (interp s ρ)
  empty : ∀ {Γ a} (ρ : Env T M Γ), interp (Tm.empty (a := a)) ρ = FinSet.empty
  insert : ∀ {Γ a} (x : Tm S Γ a) s ρ, interp (.insert x s) ρ = FinSet.insert (interp x ρ) (interp s ρ)
  union : ∀ {Γ a} (s s' : Tm S Γ (.set a)) ρ, interp (.union s s') ρ = FinSet.union (interp s ρ) (interp s' ρ)
  mapSet : ∀ {Γ a b} (s : Tm S Γ (.set a)) (u : Tm S [a] b) ρ,
    interp (.mapSet s u) ρ = FinSet.map (fun y => interp u (Env.one y)) (interp s ρ)
  filterSet : ∀ {Γ a} (s : Tm S Γ (.set a)) q ρ,
    interp (.filterSet s q) ρ = FinSet.filter (fun y => asBool (interp q (Env.one y))) (interp s ρ)

namespace Model

/-- Proposition 8.4, from an APG to a model: `M(t) = V_E(t)`, `M(δ_l) = υ_l`, every term interpreted by evaluation. -/
def ofApg (G : Apg S) : Model S where
  M := G.E
  δ := G.υ
  interp s ρ := s.eval G ρ
  var _ _ := rfl
  lit _ _ _ := rfl
  app _ _ _ := rfl
  unit _ := rfl
  pair _ _ _ := rfl
  fst _ _ := rfl
  snd _ _ := rfl
  inl _ _ := rfl
  inr _ _ := rfl
  case s _ _ ρ := by
    simp only [Tm.eval]
    generalize s.eval G ρ = sv
    cases sv <;> rfl
  deref _ _ _ := rfl
  nil _ := rfl
  cons _ _ _ := rfl
  append _ _ _ := rfl
  map _ _ _ := rfl
  filter _ _ _ := rfl
  empty _ := rfl
  insert _ _ _ := rfl
  union _ _ _ := rfl
  mapSet _ _ _ := rfl
  filterSet _ _ _ := rfl

/-- Proposition 8.4, from a model to an APG: `E(l) = M(lbl l)`, `υ_l = M(δ_l)`. -/
def toApg (𝓜 : Model S) : Apg S := ⟨𝓜.M, 𝓜.δ⟩

/-- Proposition 8.4, freeness: a model's interpretation of every term is evaluation in the APG of its generators, so
evaluation is the unique structure-preserving extension of the generators. -/
theorem interp_eq_eval (𝓜 : Model S) {Γ : List (Ty T.P L)} {t : Ty T.P L} (s : Tm S Γ t) (ρ : Env T 𝓜.M Γ) :
    𝓜.interp s ρ = s.eval 𝓜.toApg ρ := by
  induction s with
  | var v => exact 𝓜.var v ρ
  | lit p a => exact 𝓜.lit p a ρ
  | app f args ih => rw [𝓜.app]; exact congrArg (T.interp f) (funext fun i => ih i ρ)
  | unit => exact 𝓜.unit ρ
  | pair a b iha ihb => rw [𝓜.pair, iha, ihb]; rfl
  | fst s ih => rw [𝓜.fst, ih]; rfl
  | snd s ih => rw [𝓜.snd, ih]; rfl
  | inl s ih => rw [𝓜.inl, ih]; rfl
  | inr s ih => rw [𝓜.inr, ih]; rfl
  | case s b₁ b₂ ihs ih₁ ih₂ =>
    rw [𝓜.case, ihs]
    simp only [Tm.eval]
    generalize s.eval 𝓜.toApg ρ = sv
    cases sv with
    | inl x => exact ih₁ (Env.cons x ρ)
    | inr y => exact ih₂ (Env.cons y ρ)
  | absurd s ih => exact nomatch (s.eval 𝓜.toApg ρ)
  | deref l s ih => rw [𝓜.deref, ih]; rfl
  | nil => exact 𝓜.nil ρ
  | cons x xs ihx ihxs => rw [𝓜.cons, ihx, ihxs]; rfl
  | append xs ys ihx ihy => rw [𝓜.append, ihx, ihy]; rfl
  | map s u ihs ihu =>
    rw [𝓜.map, ihs]
    exact congrArg (List.map · _) (funext fun y => ihu (Env.one y))
  | filter s q ihs ihq =>
    rw [𝓜.filter, ihs]
    exact congrArg (List.filter · _) (funext fun y => congrArg asBool (ihq (Env.one y)))
  | empty => exact 𝓜.empty ρ
  | insert x s ihx ihs => rw [𝓜.insert, ihx, ihs]; rfl
  | union s s' ih ih' => rw [𝓜.union, ih, ih']; rfl
  | mapSet s u ihs ihu =>
    rw [𝓜.mapSet, ihs]
    exact congrArg (FinSet.map · _) (funext fun y => ihu (Env.one y))
  | filterSet s q ihs ihq =>
    rw [𝓜.filterSet, ihs]
    exact congrArg (FinSet.filter · _) (funext fun y => congrArg asBool (ihq (Env.one y)))

/-- Proposition 8.4: an APG through its model and back is itself. -/
theorem toApg_ofApg (G : Apg S) : (ofApg G).toApg = G := rfl

/-- Proposition 8.4: a model through its APG and back is itself, its interpretation being evaluation. -/
theorem ofApg_toApg (𝓜 : Model S) : ofApg 𝓜.toApg = 𝓜 := by
  have h : @interp _ _ _ (ofApg 𝓜.toApg) = @interp _ _ _ 𝓜 := by
    funext Γ t s ρ
    exact (interp_eq_eval 𝓜 s ρ).symm
  cases 𝓜
  simp only [ofApg, toApg, mk.injEq, heq_eq_eq, true_and] at h ⊢
  exact h

/-- A morphism of models: a function per label commuting with the generators. -/
structure Hom (𝓜 𝓝 : Model S) where
  /-- The component at each label. -/
  η : ∀ l, 𝓜.M l → 𝓝.M l
  /-- Commuting with the generators. -/
  comm : ∀ l x, 𝓝.δ l (η l x) = V.map η (S.σ l) (𝓜.δ l x)

/-- Proposition 8.4, morphisms: a morphism of models commutes with the interpretation of every term, so it is the
natural transformation `η` with `η_{t × u} = η_t × η_u` and `η_{t + u} = η_t + η_u` of definition 8.3, and it is a
homomorphism of the models' APGs. -/
theorem Hom.natural {𝓜 𝓝 : Model S} (f : Hom 𝓜 𝓝) {Γ : List (Ty T.P L)} {t : Ty T.P L} (s : Tm S Γ t)
    (ρ : Env T 𝓜.M Γ) : V.map f.η t (𝓜.interp s ρ) = 𝓝.interp s (Env.map f.η ρ) := by
  rw [interp_eq_eval, interp_eq_eval]
  exact Tm.eval_hom (G := 𝓜.toApg) (G' := 𝓝.toApg) ⟨f.η, f.comm⟩ s ρ

end Model

end ApgModel.Core
