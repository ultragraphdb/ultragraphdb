/-
 SPDX-License-Identifier: Apache-2.0
 Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
-/
import ApgModel.Core.Hom

/-!
# Mapping terms and their evaluation

`SCHEMA-APG.md` definitions 8.5, 8.6 and 12.4, and proposition 8.7, with the collection operations of corollary 12.3.

A mapping term `x : τ ⊢ s : t` is a term of `Tm S [τ] t`: intrinsically typed over a context of de Bruijn variables,
the argument `x` the context's only variable, and a `case` branch binding one more, which may read `x` as well, the
distributivity of section 8.2. `map` and `filter` take a body over the member `y` alone, `Tm S [a] b`; definition
12.4 also asks the body to dereference nothing (`Tm.DerefFree`), which its laws below do not need. In binary form a
record's field is `fst` and `snd`, an injection `inl` and `inr` (proposition 3.3 carries the named forms).

Evaluation (definition 8.6) is structural recursion on the term, so it is a total function, which is proposition
8.7: `deref` returns an element's stored value and evaluates nothing further, so a cyclic APG is evaluated like any
other. Two terms are equal when they evaluate equally on every APG (decision L3); the laws of definition 12.4 are
proved so, through substitution (`Tm.subst`), which `Δ`'s composition reuses.

Of corollary 12.3's collection operations, the terms hold those built from collections alone: `nil`, `cons`,
`append`, `empty`, `insert`, `union`. `length`, `at`, `size` and `member` need a typeside with counts and decidable
equality of members, which the oracle's concrete typeside supplies.
-/

namespace ApgModel.Core

variable {T : Typeside} {L : Type}

/-- `enum false true` in binary form, `1 + (1 + 0)`: `false` the first member, `true` the second. -/
def Ty.bool {P L : Type} : Ty P L := .sum .one (.sum .one .zero)

/-- A variable of type `t` in the context `Γ`, by its de Bruijn index. -/
inductive Var {P L : Type} : List (Ty P L) → Ty P L → Type
  /-- The innermost variable. -/
  | here {Γ : List (Ty P L)} {t : Ty P L} : Var (t :: Γ) t
  /-- A variable under one more binder. -/
  | there {Γ : List (Ty P L)} {t u : Ty P L} : Var Γ t → Var (u :: Γ) t

/-- The mapping terms of `S` in the context `Γ` of type `t`, definitions 8.5 and 12.4. -/
inductive Tm (S : Schema T.P L) : List (Ty T.P L) → Ty T.P L → Type
  /-- A variable: the argument `x`, or a variable a branch binds. -/
  | var {Γ t} : Var Γ t → Tm S Γ t
  /-- A primitive value. -/
  | lit {Γ} (p : T.P) (a : T.PV p) : Tm S Γ (.prim p)
  /-- A typeside function, one argument term per position. -/
  | app {Γ} (f : T.F) (args : (i : Fin (T.dom f).length) → Tm S Γ (.prim ((T.dom f).get i))) :
      Tm S Γ (.prim (T.cod f))
  /-- The unit record. -/
  | unit {Γ} : Tm S Γ .one
  /-- A pair. -/
  | pair {Γ a b} : Tm S Γ a → Tm S Γ b → Tm S Γ (.prod a b)
  /-- The first component. -/
  | fst {Γ a b} : Tm S Γ (.prod a b) → Tm S Γ a
  /-- The second component. -/
  | snd {Γ a b} : Tm S Γ (.prod a b) → Tm S Γ b
  /-- The left injection. -/
  | inl {Γ a b} : Tm S Γ a → Tm S Γ (.sum a b)
  /-- The right injection. -/
  | inr {Γ a b} : Tm S Γ b → Tm S Γ (.sum a b)
  /-- A case on a sum, each branch binding the summand's value beside the context. -/
  | case {Γ a b c} : Tm S Γ (.sum a b) → Tm S (a :: Γ) c → Tm S (b :: Γ) c → Tm S Γ c
  /-- Out of the empty type. -/
  | absurd {Γ c} : Tm S Γ .zero → Tm S Γ c
  /-- An element's value, the generator `δ_l`. -/
  | deref {Γ} (l : L) : Tm S Γ (.lbl l) → Tm S Γ (S.σ l)
  /-- The empty list. -/
  | nil {Γ a} : Tm S Γ (.list a)
  /-- A list with one more member in front. -/
  | cons {Γ a} : Tm S Γ a → Tm S Γ (.list a) → Tm S Γ (.list a)
  /-- Two lists one after the other. -/
  | append {Γ a} : Tm S Γ (.list a) → Tm S Γ (.list a) → Tm S Γ (.list a)
  /-- `map(s, y ⇒ u)` over a list. -/
  | map {Γ a b} : Tm S Γ (.list a) → Tm S [a] b → Tm S Γ (.list b)
  /-- `filter(s, y ⇒ q)` over a list. -/
  | filter {Γ a} : Tm S Γ (.list a) → Tm S [a] Ty.bool → Tm S Γ (.list a)
  /-- The empty set. -/
  | empty {Γ a} : Tm S Γ (.set a)
  /-- A set with one more member. -/
  | insert {Γ a} : Tm S Γ a → Tm S Γ (.set a) → Tm S Γ (.set a)
  /-- The union of two sets. -/
  | union {Γ a} : Tm S Γ (.set a) → Tm S Γ (.set a) → Tm S Γ (.set a)
  /-- `map(s, y ⇒ u)` over a set: members with equal images become one. -/
  | mapSet {Γ a b} : Tm S Γ (.set a) → Tm S [a] b → Tm S Γ (.set b)
  /-- `filter(s, y ⇒ q)` over a set. -/
  | filterSet {Γ a} : Tm S Γ (.set a) → Tm S [a] Ty.bool → Tm S Γ (.set a)

variable {S : Schema T.P L}

/-- A term dereferences no element: definition 12.4's condition on a `map` or `filter` body. -/
def Tm.DerefFree : {Γ : List (Ty T.P L)} → {t : Ty T.P L} → Tm S Γ t → Prop
  | _, _, .var _ | _, _, .lit _ _ | _, _, .unit | _, _, .nil | _, _, .empty => True
  | _, _, .app _ args => ∀ i, (args i).DerefFree
  | _, _, .pair a b | _, _, .cons a b | _, _, .append a b | _, _, .insert a b | _, _, .union a b =>
    a.DerefFree ∧ b.DerefFree
  | _, _, .fst s | _, _, .snd s | _, _, .inl s | _, _, .inr s | _, _, .absurd s => s.DerefFree
  | _, _, .case s b₁ b₂ => s.DerefFree ∧ b₁.DerefFree ∧ b₂.DerefFree
  | _, _, .deref _ _ => False
  | _, _, .map s u | _, _, .filter s u | _, _, .mapSet s u | _, _, .filterSet s u => s.DerefFree ∧ u.DerefFree

/-! ## Environments and evaluation -/

/-- An environment: a value for every variable of the context. -/
def Env (T : Typeside) {L : Type} (E : L → Type) (Γ : List (Ty T.P L)) : Type := ∀ t, Var Γ t → V T E t

/-- The environment of the empty context. -/
def Env.nil {E : L → Type} : Env T E [] := fun _ v => nomatch v

/-- An environment extended by a value for a new innermost variable. -/
def Env.cons {E : L → Type} {a : Ty T.P L} {Γ : List (Ty T.P L)} (x : V T E a) (ρ : Env T E Γ) :
    Env T E (a :: Γ)
  | _, .here => x
  | _, .there v => ρ _ v

/-- The environment of a mapping term's one argument. -/
def Env.one {E : L → Type} {a : Ty T.P L} (x : V T E a) : Env T E [a] := Env.cons x Env.nil

/-- A Boolean value as a `Bool`. -/
def asBool {E : L → Type} : V T E Ty.bool → Bool
  | Sum.inl _ => false
  | Sum.inr _ => true

/-- `⟦s⟧_G`, definition 8.6: the value of `s` in the APG `G` under the environment `ρ`. -/
def Tm.eval (G : Apg S) : {Γ : List (Ty T.P L)} → {t : Ty T.P L} → Tm S Γ t → Env T G.E Γ → V T G.E t
  | _, _, .var v, ρ => ρ _ v
  | _, _, .lit _ a, _ => a
  | _, _, .app f args, ρ => T.interp f (fun i => (args i).eval G ρ)
  | _, _, .unit, _ => ()
  | _, _, .pair a b, ρ => (a.eval G ρ, b.eval G ρ)
  | _, _, .fst s, ρ => Prod.fst (s.eval G ρ)
  | _, _, .snd s, ρ => Prod.snd (s.eval G ρ)
  | _, _, .inl s, ρ => Sum.inl (s.eval G ρ)
  | _, _, .inr s, ρ => Sum.inr (s.eval G ρ)
  | _, _, .case s b₁ b₂, ρ =>
    match s.eval G ρ with
    | Sum.inl x => b₁.eval G (Env.cons x ρ)
    | Sum.inr y => b₂.eval G (Env.cons y ρ)
  | _, _, .absurd s, ρ => nomatch (s.eval G ρ)
  | _, _, .deref l s, ρ => G.υ l (s.eval G ρ)
  | _, _, .nil, _ => List.nil
  | _, _, .cons x xs, ρ => List.cons (x.eval G ρ) (xs.eval G ρ)
  | _, _, .append xs ys, ρ => List.append (xs.eval G ρ) (ys.eval G ρ)
  | _, _, .map s u, ρ => List.map (fun y => u.eval G (Env.one y)) (s.eval G ρ)
  | _, _, .filter s q, ρ => List.filter (fun y => asBool (q.eval G (Env.one y))) (s.eval G ρ)
  | _, _, .empty, _ => FinSet.empty
  | _, _, .insert x s, ρ => FinSet.insert (x.eval G ρ) (s.eval G ρ)
  | _, _, .union s s', ρ => FinSet.union (s.eval G ρ) (s'.eval G ρ)
  | _, _, .mapSet s u, ρ => FinSet.map (fun y => u.eval G (Env.one y)) (s.eval G ρ)
  | _, _, .filterSet s q, ρ => FinSet.filter (fun y => asBool (q.eval G (Env.one y))) (s.eval G ρ)

/-- A mapping term `x : τ ⊢ s : t` as the function `⟦s⟧_G : V_E(τ) → V_E(t)` of definition 8.6. -/
def Tm.denote (G : Apg S) {τ t : Ty T.P L} (s : Tm S [τ] t) : V T G.E τ → V T G.E t :=
  fun x => s.eval G (Env.one x)

/-- Proposition 8.7: evaluation terminates on every APG, cyclic ones included. `Tm.eval` is a total function, defined
by structural recursion on the term with no `partial` and no well-founded measure on the APG, so every term has a
value in every APG under every environment. -/
theorem Tm.eval_terminates (G : Apg S) {Γ : List (Ty T.P L)} {t : Ty T.P L} (s : Tm S Γ t) (ρ : Env T G.E Γ) :
    ∃ v, s.eval G ρ = v :=
  ⟨_, rfl⟩

/-- A typeside with no primitive types, for examples that need none. -/
def Typeside.empty : Typeside where
  P := Empty
  PV := fun p => nomatch p
  F := Empty
  dom := fun f => nomatch f
  cod := fun f => nomatch f
  interp := fun f => nomatch f

/-- A schema of one label whose value is a reference to an element of that label: a stream that returns. -/
def cyclicSchema : Schema Typeside.empty.P Unit := ⟨fun _ => .lbl ()⟩

/-- Two elements, each referencing the other: a cycle of length two in the element graph. -/
def cyclicApg : Apg cyclicSchema := ⟨fun _ => Bool, fun _ b => !b⟩

/-- Proposition 8.7 on a cyclic APG: dereferencing twice from `true` returns to `true`, and evaluation ends. -/
example : (Tm.deref () (Tm.deref () (Tm.var .here)) : Tm cyclicSchema [.lbl ()] (.lbl ())).eval cyclicApg
    (Env.one (show V Typeside.empty cyclicApg.E (.lbl ()) from true)) = true := rfl

/-! ## Renaming and substitution -/

/-- A renaming of the variables of `Γ` into `Δ`. -/
def Ren {P L : Type} (Γ Δ : List (Ty P L)) : Type := ∀ t, Var Γ t → Var Δ t

/-- A renaming under one more binder. -/
def Ren.lift {P L : Type} {Γ Δ : List (Ty P L)} {a : Ty P L} (r : Ren Γ Δ) : Ren (a :: Γ) (a :: Δ)
  | _, .here => .here
  | _, .there v => .there (r _ v)

/-- A term with its variables renamed. -/
def Tm.rename : {Γ Δ : List (Ty T.P L)} → {t : Ty T.P L} → Ren Γ Δ → Tm S Γ t → Tm S Δ t
  | _, _, _, r, .var v => .var (r _ v)
  | _, _, _, _, .lit p a => .lit p a
  | _, _, _, r, .app f args => .app f (fun i => (args i).rename r)
  | _, _, _, _, .unit => .unit
  | _, _, _, r, .pair a b => .pair (a.rename r) (b.rename r)
  | _, _, _, r, .fst s => .fst (s.rename r)
  | _, _, _, r, .snd s => .snd (s.rename r)
  | _, _, _, r, .inl s => .inl (s.rename r)
  | _, _, _, r, .inr s => .inr (s.rename r)
  | _, _, _, r, .case s b₁ b₂ => .case (s.rename r) (b₁.rename r.lift) (b₂.rename r.lift)
  | _, _, _, r, .absurd s => .absurd (s.rename r)
  | _, _, _, r, .deref l s => .deref l (s.rename r)
  | _, _, _, _, .nil => .nil
  | _, _, _, r, .cons x xs => .cons (x.rename r) (xs.rename r)
  | _, _, _, r, .append xs ys => .append (xs.rename r) (ys.rename r)
  | _, _, _, r, .map s u => .map (s.rename r) u
  | _, _, _, r, .filter s q => .filter (s.rename r) q
  | _, _, _, _, .empty => .empty
  | _, _, _, r, .insert x s => .insert (x.rename r) (s.rename r)
  | _, _, _, r, .union s s' => .union (s.rename r) (s'.rename r)
  | _, _, _, r, .mapSet s u => .mapSet (s.rename r) u
  | _, _, _, r, .filterSet s q => .filterSet (s.rename r) q

/-- A substitution of terms over `Δ` for the variables of `Γ`. -/
def Sub (S : Schema T.P L) (Γ Δ : List (Ty T.P L)) : Type := ∀ t, Var Γ t → Tm S Δ t

/-- A substitution under one more binder. -/
def Sub.lift {Γ Δ : List (Ty T.P L)} {a : Ty T.P L} (σ : Sub S Γ Δ) : Sub S (a :: Γ) (a :: Δ)
  | _, .here => .var .here
  | _, .there v => (σ _ v).rename (fun _ w => .there w)

/-- A term with terms substituted for its variables. The bodies of `map` and `filter` are closed over their member
and take no substitution. -/
def Tm.subst : {Γ Δ : List (Ty T.P L)} → {t : Ty T.P L} → Sub S Γ Δ → Tm S Γ t → Tm S Δ t
  | _, _, _, σ, .var v => σ _ v
  | _, _, _, _, .lit p a => .lit p a
  | _, _, _, σ, .app f args => .app f (fun i => (args i).subst σ)
  | _, _, _, _, .unit => .unit
  | _, _, _, σ, .pair a b => .pair (a.subst σ) (b.subst σ)
  | _, _, _, σ, .fst s => .fst (s.subst σ)
  | _, _, _, σ, .snd s => .snd (s.subst σ)
  | _, _, _, σ, .inl s => .inl (s.subst σ)
  | _, _, _, σ, .inr s => .inr (s.subst σ)
  | _, _, _, σ, .case s b₁ b₂ => .case (s.subst σ) (b₁.subst σ.lift) (b₂.subst σ.lift)
  | _, _, _, σ, .absurd s => .absurd (s.subst σ)
  | _, _, _, σ, .deref l s => .deref l (s.subst σ)
  | _, _, _, _, .nil => .nil
  | _, _, _, σ, .cons x xs => .cons (x.subst σ) (xs.subst σ)
  | _, _, _, σ, .append xs ys => .append (xs.subst σ) (ys.subst σ)
  | _, _, _, σ, .map s u => .map (s.subst σ) u
  | _, _, _, σ, .filter s q => .filter (s.subst σ) q
  | _, _, _, _, .empty => .empty
  | _, _, _, σ, .insert x s => .insert (x.subst σ) (s.subst σ)
  | _, _, _, σ, .union s s' => .union (s.subst σ) (s'.subst σ)
  | _, _, _, σ, .mapSet s u => .mapSet (s.subst σ) u
  | _, _, _, σ, .filterSet s q => .filterSet (s.subst σ) q

/-- The substitution of `u` for the one variable of a context `[b]`. -/
def Sub.one {a b : Ty T.P L} (u : Tm S [a] b) : Sub S [b] [a]
  | _, .here => u

/-- `u'[u/y]`: the term `u` substituted for the one variable of `u'`. -/
def Tm.subst1 {a b c : Ty T.P L} (u' : Tm S [b] c) (u : Tm S [a] b) : Tm S [a] c :=
  u'.subst (Sub.one u)

/-! ## Evaluation commutes with renaming and substitution -/

private theorem congr_two {α β γ : Type} (f : α → β → γ) {a a' : α} {b b' : β} (ha : a = a') (hb : b = b') :
    f a b = f a' b' := by
  subst ha hb
  rfl

theorem Env.cons_lift {E : L → Type} {Γ Δ : List (Ty T.P L)} {a : Ty T.P L} (r : Ren Γ Δ) (ρ : Env T E Δ)
    (x : V T E a) : (fun t (v : Var (a :: Γ) t) => Env.cons x ρ t (r.lift t v)) = Env.cons x (fun t v => ρ t (r t v)) := by
  funext t v
  cases v <;> rfl

theorem Tm.eval_rename (G : Apg S) {Γ Δ : List (Ty T.P L)} {t : Ty T.P L} (s : Tm S Γ t) (r : Ren Γ Δ)
    (ρ : Env T G.E Δ) : (s.rename r).eval G ρ = s.eval G (fun t v => ρ t (r t v)) := by
  induction s generalizing Δ with
  | var v => rfl
  | lit p a => rfl
  | app f args ih => exact congrArg (T.interp f) (funext fun i => ih i r ρ)
  | unit => rfl
  | pair a b iha ihb => exact Prod.ext (iha r ρ) (ihb r ρ)
  | fst s ih => exact congrArg Prod.fst (ih r ρ)
  | snd s ih => exact congrArg Prod.snd (ih r ρ)
  | inl s ih => exact congrArg Sum.inl (ih r ρ)
  | inr s ih => exact congrArg Sum.inr (ih r ρ)
  | case s b₁ b₂ ihs ih₁ ih₂ =>
    simp only [Tm.rename, Tm.eval]
    rw [ihs r ρ]
    split
    · rw [ih₁ r.lift, Env.cons_lift]
    · rw [ih₂ r.lift, Env.cons_lift]
  | absurd s ih =>
    simp only [Tm.rename, Tm.eval]
    exact nomatch (s.eval G (fun t v => ρ t (r t v)))
  | deref l s ih => exact congrArg (G.υ l) (ih r ρ)
  | nil => rfl
  | cons x xs ihx ihxs => exact congr_two List.cons (ihx r ρ) (ihxs r ρ)
  | append xs ys ihx ihy => exact congr_two List.append (ihx r ρ) (ihy r ρ)
  | map s u ih _ => exact congrArg (List.map _) (ih r ρ)
  | filter s q ih _ => exact congrArg (List.filter _) (ih r ρ)
  | empty => rfl
  | insert x s ihx ihs => exact congr_two FinSet.insert (ihx r ρ) (ihs r ρ)
  | union s s' ih ih' => exact congr_two FinSet.union (ih r ρ) (ih' r ρ)
  | mapSet s u ih _ => exact congrArg (FinSet.map _) (ih r ρ)
  | filterSet s q ih _ => exact congrArg (FinSet.filter _) (ih r ρ)

theorem Env.cons_subst_lift (G : Apg S) {Γ Δ : List (Ty T.P L)} {a : Ty T.P L} (σ : Sub S Γ Δ)
    (ρ : Env T G.E Δ) (x : V T G.E a) :
    (fun t (v : Var (a :: Γ) t) => (σ.lift t v).eval G (Env.cons x ρ)) =
      Env.cons x (fun t v => (σ t v).eval G ρ) := by
  funext t v
  cases v with
  | here => rfl
  | there v =>
    show ((σ _ v).rename (fun _ w => .there w)).eval G (Env.cons x ρ) = (σ _ v).eval G ρ
    rw [Tm.eval_rename]
    rfl

theorem Tm.eval_subst (G : Apg S) {Γ Δ : List (Ty T.P L)} {t : Ty T.P L} (s : Tm S Γ t) (σ : Sub S Γ Δ)
    (ρ : Env T G.E Δ) : (s.subst σ).eval G ρ = s.eval G (fun t v => (σ t v).eval G ρ) := by
  induction s generalizing Δ with
  | var v => rfl
  | lit p a => rfl
  | app f args ih => exact congrArg (T.interp f) (funext fun i => ih i σ ρ)
  | unit => rfl
  | pair a b iha ihb => exact Prod.ext (iha σ ρ) (ihb σ ρ)
  | fst s ih => exact congrArg Prod.fst (ih σ ρ)
  | snd s ih => exact congrArg Prod.snd (ih σ ρ)
  | inl s ih => exact congrArg Sum.inl (ih σ ρ)
  | inr s ih => exact congrArg Sum.inr (ih σ ρ)
  | case s b₁ b₂ ihs ih₁ ih₂ =>
    simp only [Tm.subst, Tm.eval]
    rw [ihs σ ρ]
    split
    · rw [ih₁ σ.lift, Env.cons_subst_lift]
    · rw [ih₂ σ.lift, Env.cons_subst_lift]
  | absurd s ih =>
    simp only [Tm.subst, Tm.eval]
    exact nomatch (s.eval G (fun t v => (σ t v).eval G ρ))
  | deref l s ih => exact congrArg (G.υ l) (ih σ ρ)
  | nil => rfl
  | cons x xs ihx ihxs => exact congr_two List.cons (ihx σ ρ) (ihxs σ ρ)
  | append xs ys ihx ihy => exact congr_two List.append (ihx σ ρ) (ihy σ ρ)
  | map s u ih _ => exact congrArg (List.map _) (ih σ ρ)
  | filter s q ih _ => exact congrArg (List.filter _) (ih σ ρ)
  | empty => rfl
  | insert x s ihx ihs => exact congr_two FinSet.insert (ihx σ ρ) (ihs σ ρ)
  | union s s' ih ih' => exact congr_two FinSet.union (ih σ ρ) (ih' σ ρ)
  | mapSet s u ih _ => exact congrArg (FinSet.map _) (ih σ ρ)
  | filterSet s q ih _ => exact congrArg (FinSet.filter _) (ih σ ρ)

theorem Tm.eval_subst1 (G : Apg S) {a b c : Ty T.P L} (u' : Tm S [b] c) (u : Tm S [a] b) (y : V T G.E a) :
    (u'.subst1 u).eval G (Env.one y) = u'.eval G (Env.one (u.eval G (Env.one y))) := by
  rw [Tm.subst1, Tm.eval_subst]
  congr 1
  funext t v
  cases v with
  | here => rfl
  | there v => exact nomatch v

/-! ## Definition 12.4: the laws of `map` -/

/-- Definition 12.4, over lists: `map(s, y ⇒ y) = s`. -/
theorem Tm.eval_map_id (G : Apg S) {Γ : List (Ty T.P L)} {a : Ty T.P L} (s : Tm S Γ (.list a))
    (ρ : Env T G.E Γ) : (Tm.map s (.var .here)).eval G ρ = s.eval G ρ :=
  List.map_id' (s.eval G ρ)

/-- Definition 12.4, over lists: `map(map(s, y ⇒ u), y ⇒ u') = map(s, y ⇒ u'[u/y])`. -/
theorem Tm.eval_map_map (G : Apg S) {Γ : List (Ty T.P L)} {a b c : Ty T.P L} (s : Tm S Γ (.list a))
    (u : Tm S [a] b) (u' : Tm S [b] c) (ρ : Env T G.E Γ) :
    (Tm.map (Tm.map s u) u').eval G ρ = (Tm.map s (u'.subst1 u)).eval G ρ := by
  show List.map _ (List.map _ (s.eval G ρ)) = List.map _ (s.eval G ρ)
  rw [List.map_map]
  exact List.map_congr_left (fun y _ => (Tm.eval_subst1 G u' u y).symm)

/-- Definition 12.4, over sets: `map(s, y ⇒ y) = s`. -/
theorem Tm.eval_mapSet_id (G : Apg S) {Γ : List (Ty T.P L)} {a : Ty T.P L} (s : Tm S Γ (.set a))
    (ρ : Env T G.E Γ) : (Tm.mapSet s (.var .here)).eval G ρ = s.eval G ρ :=
  FinSet.map_id' (s.eval G ρ)

/-- Definition 12.4, over sets: `map(map(s, y ⇒ u), y ⇒ u') = map(s, y ⇒ u'[u/y])`. -/
theorem Tm.eval_mapSet_mapSet (G : Apg S) {Γ : List (Ty T.P L)} {a b c : Ty T.P L} (s : Tm S Γ (.set a))
    (u : Tm S [a] b) (u' : Tm S [b] c) (ρ : Env T G.E Γ) :
    (Tm.mapSet (Tm.mapSet s u) u').eval G ρ = (Tm.mapSet s (u'.subst1 u)).eval G ρ := by
  show FinSet.map _ (FinSet.map _ (s.eval G ρ)) = FinSet.map _ (s.eval G ρ)
  rw [FinSet.map_map]
  exact FinSet.map_congr (fun y => (Tm.eval_subst1 G u' u y).symm) (s.eval G ρ)

end ApgModel.Core
