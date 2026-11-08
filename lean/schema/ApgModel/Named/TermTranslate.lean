/-
 SPDX-License-Identifier: Apache-2.0
 Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
-/
import ApgModel.Named.Term

/-!
# Proposition 3.3, part 2: mapping terms

`SCHEMA-APG.md` proposition 3.3, part 2: each named mapping term translates to the binary grammar, and evaluation
commutes with the translation and the isomorphisms of part 1 (`NTm.eval_bin`).

- A record `{ n₁ = s₁, …, nₖ = sₖ }` becomes the right-nested pairs `⟨⌜s₁⌝, ⟨…, ⟨⌜sₖ⌝, ()⟩⟩⟩` (`recAt`).
- Access to the `i`-th field becomes `i − 1` second projections followed by a first (`projAt`).
- Injection at the `i`-th field becomes `i − 1` right injections followed by a left (`injAt`).
- A `case` becomes nested binary cases (`caseAt`): the first branch on the left, and on the right a case on the rest,
  each later branch weakened past the variable the outer case binds.

The translation of a named APG is `NApg.toCore`, of a context `List.map NTy.bin`, of an environment `envBin`.
-/

namespace ApgModel.Named

open Core

variable {T : Typeside} {L : Type} {S : NSchema T.P L}

/-! ## The four record and variant constructs in binary form -/

/-- A record's field terms as right-nested pairs. -/
def recAt {Γ : List (Ty T.P L)} : (fs : List (Name × NTy T.P L)) →
    ((i : Fin fs.length) → Tm S.bin Γ (NTy.fieldAt fs i).bin) → Tm S.bin Γ (NTy.bin.binRec fs)
  | [], _ => .unit
  | (_, _) :: fs, a => .pair (a ⟨0, Nat.zero_lt_succ _⟩) (recAt fs (fun i => a i.succ))

/-- The field at position `i`: `i − 1` second projections, then a first. -/
def projAt {Γ : List (Ty T.P L)} : (fs : List (Name × NTy T.P L)) → (i : Fin fs.length) →
    Tm S.bin Γ (NTy.bin.binRec fs) → Tm S.bin Γ (NTy.fieldAt fs i).bin
  | [], i, _ => i.elim0
  | (_, _) :: _, ⟨0, _⟩, x => .fst x
  | _ :: fs, ⟨i + 1, h⟩, x => projAt fs ⟨i, Nat.lt_of_succ_lt_succ h⟩ (.snd x)

/-- The injection at position `i`: `i − 1` right injections, then a left. -/
def injAt {Γ : List (Ty T.P L)} : (fs : List (Name × NTy T.P L)) → (i : Fin fs.length) →
    Tm S.bin Γ (NTy.fieldAt fs i).bin → Tm S.bin Γ (NTy.bin.binVar fs)
  | [], i, _ => i.elim0
  | (_, _) :: _, ⟨0, _⟩, x => .inl x
  | _ :: fs, ⟨i + 1, h⟩, x => .inr (injAt fs ⟨i, Nat.lt_of_succ_lt_succ h⟩ x)

/-- Weakening past one variable bound outside a branch's own. -/
def weakenUnder {P L : Type} {Γ : List (Ty P L)} {a b : Ty P L} : Ren (a :: Γ) (a :: b :: Γ) :=
  Ren.lift (fun _ v => .there v)

/-- A case over a variant's fields as nested binary cases. -/
def caseAt {Γ : List (Ty T.P L)} {c : Ty T.P L} : (fs : List (Name × NTy T.P L)) →
    Tm S.bin Γ (NTy.bin.binVar fs) → ((i : Fin fs.length) → Tm S.bin ((NTy.fieldAt fs i).bin :: Γ) c) →
    Tm S.bin Γ c
  | [], x, _ => .absurd x
  | (_, _) :: fs, x, br =>
    .case x (br ⟨0, Nat.zero_lt_succ _⟩) (caseAt fs (.var .here) (fun i => (br i.succ).rename weakenUnder))

/-! ## The translation -/

/-- A named variable as a variable of the translated context. -/
def NVar.bin {P L : Type} : {Γ : List (NTy P L)} → {t : NTy P L} → NVar Γ t → Var (Γ.map NTy.bin) t.bin
  | _, _, .here => .here
  | _, _, .there v => .there v.bin

/-- `⌜s⌝`, proposition 3.3's translation of a named mapping term to the binary grammar. -/
def NTm.bin : {Γ : List (NTy T.P L)} → {t : NTy T.P L} → NTm S Γ t → Tm S.bin (Γ.map NTy.bin) t.bin
  | _, _, .var v => .var v.bin
  | _, _, .lit p a => .lit p a
  | _, _, .app f args => .app f (fun i => (args i).bin)
  | _, _, .record fs args => recAt fs (fun i => (args i).bin)
  | _, _, .field fs i s => projAt fs i s.bin
  | _, _, .inj fs i s => injAt fs i s.bin
  | _, _, .case fs s br => caseAt fs s.bin (fun i => (br i).bin)
  | _, _, .deref l s => .deref l s.bin
  | _, _, .nil => .nil
  | _, _, .cons x xs => .cons x.bin xs.bin
  | _, _, .append xs ys => .append xs.bin ys.bin
  | _, _, .map s u => .map s.bin u.bin
  | _, _, .filter s q => .filter s.bin q.bin
  | _, _, .empty => .empty
  | _, _, .insert x s => .insert x.bin s.bin
  | _, _, .union s s' => .union s.bin s'.bin
  | _, _, .mapSet s u => .mapSet s.bin u.bin
  | _, _, .filterSet s q => .filterSet s.bin q.bin

/-- A named environment translated, every value by `toBin`. -/
def envBin {E : L → Type} : {Γ : List (NTy T.P L)} → NEnv T E Γ → Env T E (Γ.map NTy.bin)
  | [], _ => Env.nil
  | t :: _, ρ => Env.cons (toBin t (ρ t .here)) (envBin (fun u v => ρ u (.there v)))

theorem envBin_var {E : L → Type} : {Γ : List (NTy T.P L)} → {t : NTy T.P L} → (ρ : NEnv T E Γ) → (v : NVar Γ t) →
    envBin ρ _ v.bin = toBin t (ρ t v)
  | _ :: _, _, _, .here => rfl
  | _ :: _, _, ρ, .there v => envBin_var (fun u w => ρ u (.there w)) v

/-! ## Evaluation of the four constructs -/

theorem recAt_eval (G : NApg S) {Γ : List (Ty T.P L)} (env : Env T G.E Γ) : (fs : List (Name × NTy T.P L)) →
    (u : (i : Fin fs.length) → Tm S.bin Γ (NTy.fieldAt fs i).bin) →
    (y : (i : Fin fs.length) → NTy.V T G.E (NTy.fieldAt fs i)) →
    (∀ i, (u i).eval G.toCore env = toBin (NTy.fieldAt fs i) (y i)) →
    (recAt fs u).eval G.toCore env = toBin.toBinRec fs (fun i => NTy.V.VAt.into fs i (y i))
  | [], _, _, _ => rfl
  | (_, _) :: fs, u, y, hu => by
    show ((u ⟨0, _⟩).eval G.toCore env, (recAt fs (fun i => u i.succ)).eval G.toCore env) = (_, _)
    rw [hu ⟨0, _⟩, recAt_eval G env fs (fun i => u i.succ) (fun i => y i.succ) (fun i => hu i.succ)]
    rfl

theorem projAt_eval (G : NApg S) {Γ : List (Ty T.P L)} (env : Env T G.E Γ) : (fs : List (Name × NTy T.P L)) →
    (i : Fin fs.length) → (u : Tm S.bin Γ (NTy.bin.binRec fs)) →
    (f : (i : Fin fs.length) → NTy.V.VAt T G.E fs i) → u.eval G.toCore env = toBin.toBinRec fs f →
    (projAt fs i u).eval G.toCore env = toBin (NTy.fieldAt fs i) (NTy.V.VAt.out fs i (f i))
  | [], i, _, _, _ => i.elim0
  | (_, _) :: _, ⟨0, _⟩, u, f, hu => by
    show Prod.fst (u.eval G.toCore env) = _
    rw [hu]
    rfl
  | _ :: fs, ⟨i + 1, h⟩, u, f, hu =>
    projAt_eval G env fs ⟨i, Nat.lt_of_succ_lt_succ h⟩ (.snd u) (fun j => f j.succ) (by
      show Prod.snd (u.eval G.toCore env) = _
      rw [hu]
      rfl)

theorem injAt_eval (G : NApg S) {Γ : List (Ty T.P L)} (env : Env T G.E Γ) : (fs : List (Name × NTy T.P L)) →
    (i : Fin fs.length) → (u : Tm S.bin Γ (NTy.fieldAt fs i).bin) → (y : NTy.V T G.E (NTy.fieldAt fs i)) →
    u.eval G.toCore env = toBin (NTy.fieldAt fs i) y →
    (injAt fs i u).eval G.toCore env = toBin.toBinVar fs i (NTy.V.VAt.into fs i y)
  | [], i, _, _, _ => i.elim0
  | (_, _) :: _, ⟨0, _⟩, u, y, hu => by
    show Sum.inl (u.eval G.toCore env) = _
    rw [hu]
    rfl
  | _ :: fs, ⟨i + 1, h⟩, u, y, hu => by
    show Sum.inr ((injAt fs ⟨i, _⟩ u).eval G.toCore env) = _
    rw [injAt_eval G env fs ⟨i, Nat.lt_of_succ_lt_succ h⟩ u y hu]
    rfl

theorem caseAt_eval (G : NApg S) {c : Ty T.P L} : {Γ : List (Ty T.P L)} → (env : Env T G.E Γ) →
    (fs : List (Name × NTy T.P L)) → (x : Tm S.bin Γ (NTy.bin.binVar fs)) →
    (br : (i : Fin fs.length) → Tm S.bin ((NTy.fieldAt fs i).bin :: Γ) c) →
    (i : Fin fs.length) → (v : NTy.V.VAt T G.E fs i) → x.eval G.toCore env = toBin.toBinVar fs i v →
    (caseAt fs x br).eval G.toCore env =
      (br i).eval G.toCore (Env.cons (toBin (NTy.fieldAt fs i) (NTy.V.VAt.out fs i v)) env)
  | _, _, [], _, _, i, _, _ => i.elim0
  | _, env, (_, _) :: _, x, br, ⟨0, _⟩, v, hx => by
    simp only [caseAt, Tm.eval]
    rw [hx]
    rfl
  | _, env, _ :: fs, x, br, ⟨i + 1, h⟩, v, hx => by
    simp only [caseAt, Tm.eval]
    rw [hx]
    show (caseAt fs (.var .here) (fun j => (br j.succ).rename weakenUnder)).eval G.toCore
      (Env.cons (toBin.toBinVar fs ⟨i, _⟩ v) env) = _
    rw [caseAt_eval G (Env.cons (toBin.toBinVar fs ⟨i, Nat.lt_of_succ_lt_succ h⟩ v) env) fs (.var .here)
      (fun j => (br j.succ).rename weakenUnder) ⟨i, Nat.lt_of_succ_lt_succ h⟩ v rfl]
    rw [Tm.eval_rename]
    show (br ⟨i + 1, h⟩).eval G.toCore (fun t w => Env.cons _ (Env.cons _ env) t (weakenUnder t w)) = _
    rw [weakenUnder, Env.cons_lift]
    rfl

/-! ## Booleans and lists -/

theorem asBool_toBin {E : L → Type} (b : NTy.V T E NTy.bool) : asBool (toBin NTy.bool b) = nasBool b := by
  obtain ⟨⟨i, h⟩, v⟩ := b
  match i, h with
  | 0, _ => rfl
  | 1, _ => rfl
  | n + 2, h => exact absurd h (by simp [NTy.bool, NTy.enum])

private theorem filter_map_comm {α β : Type} (f : α → β) (p : β → Bool) (l : List α) :
    List.filter p (List.map f l) = List.map f (List.filter (fun x => p (f x)) l) := by
  induction l with
  | nil => rfl
  | cons x l ih =>
    simp only [List.map_cons, List.filter_cons]
    split <;> simp [ih]

/-- Two maps commute when their components do pointwise. -/
private theorem map_comm {α β γ δ : Type} {f : α → β} {h : β → δ} {k : α → γ} {g : γ → δ}
    (w : ∀ x, h (f x) = g (k x)) (l : List α) : List.map h (List.map f l) = List.map g (List.map k l) := by
  rw [List.map_map, List.map_map]
  exact List.map_congr_left (fun x _ => w x)

/-- Mapping after filtering by `p` is filtering by `p'` after mapping, when `p` is `p'` through the map. -/
private theorem map_filter_comm {α β : Type} (f : α → β) {p : α → Bool} {p' : β → Bool} (w : ∀ x, p x = p' (f x))
    (l : List α) : List.map f (List.filter p l) = List.filter p' (List.map f l) := by
  rw [filter_map_comm]
  congr 1
  exact congrArg (List.filter · l) (funext w)

private theorem finset_map_comm {α β γ δ : Type} {f : α → β} {h : β → δ} {k : α → γ} {g : γ → δ}
    (w : ∀ x, h (f x) = g (k x)) (s : FinSet α) : FinSet.map h (FinSet.map f s) = FinSet.map g (FinSet.map k s) := by
  rw [FinSet.map_map, FinSet.map_map]
  exact FinSet.map_congr w s

private theorem finset_map_filter_comm {α β : Type} (f : α → β) {p : α → Bool} {p' : β → Bool}
    (w : ∀ x, p x = p' (f x)) (s : FinSet α) : FinSet.map f (FinSet.filter p s) = FinSet.filter p' (FinSet.map f s) := by
  rw [FinSet.map_filter]
  exact congrArg (fun q => FinSet.map f (FinSet.filter q s)) (funext w)

/-! ## Proposition 3.3, part 2 -/

/-- Proposition 3.3, part 2: evaluation commutes with the translation of terms and the isomorphisms of part 1,
`⌜⟦s⟧_G(ρ)⌝ = ⟦⌜s⌝⟧_⌜G⌝(⌜ρ⌝)`. -/
theorem NTm.eval_bin (G : NApg S) {Γ : List (NTy T.P L)} {t : NTy T.P L} (s : NTm S Γ t) (ρ : NEnv T G.E Γ) :
    toBin t (s.eval G ρ) = s.bin.eval G.toCore (envBin ρ) := by
  induction s with
  | var v => exact (envBin_var ρ v).symm
  | lit p a => rfl
  | app f args ih => exact congrArg (T.interp f) (funext fun i => ih i ρ)
  | record fs args ih =>
    exact (recAt_eval G (envBin ρ) fs (fun i => (args i).bin) (fun i => (args i).eval G ρ)
      (fun i => (ih i ρ).symm)).symm
  | field fs i s ih =>
    exact (projAt_eval G (envBin ρ) fs i s.bin (s.eval G ρ) (ih ρ).symm).symm
  | inj fs i s ih =>
    exact (injAt_eval G (envBin ρ) fs i s.bin (s.eval G ρ) (ih ρ).symm).symm
  | case fs s br ihs ihb =>
    simp only [NTm.eval, NTm.bin]
    have hs := (ihs ρ).symm
    generalize s.eval G ρ = sv at hs ⊢
    obtain ⟨i, v⟩ := sv
    rw [caseAt_eval G (envBin ρ) fs s.bin (fun j => (br j).bin) i v hs]
    exact ihb i (NEnv.cons (NTy.V.VAt.out fs i v) ρ)
  | deref l s ih =>
    show toBin (S.σ l) (G.υ l (s.eval G ρ)) = toBin (S.σ l) (G.υ l (s.bin.eval G.toCore (envBin ρ)))
    rw [← ih ρ]
    rfl
  | nil => rfl
  | cons x xs ihx ihxs =>
    show List.cons _ (List.map _ _) = List.cons _ _
    rw [ihx ρ]
    exact congrArg _ (ihxs ρ)
  | append xs ys ihx ihy =>
    exact (List.map_append _ _ _).trans (congr_two (· ++ ·) (ihx ρ) (ihy ρ))
  | @map Γ a b s u ihs ihu =>
    exact (map_comm (h := toBin b) (f := fun y => u.eval G (NEnv.one y)) (k := toBin a)
        (g := fun y => u.bin.eval G.toCore (Env.one y)) (fun y => ihu (NEnv.one y)) _).trans
      (congrArg (List.map fun y => u.bin.eval G.toCore (Env.one y)) (ihs ρ))
  | @filter Γ a s q ihs ihq =>
    have w : ∀ y : NTy.V T G.E a, nasBool (q.eval G (NEnv.one y)) =
        asBool (q.bin.eval G.toCore (Env.one (toBin a y))) := by
      intro y
      rw [← asBool_toBin]
      exact congrArg asBool (ihq (NEnv.one y))
    exact (map_filter_comm (toBin a) w _).trans
      (congrArg (List.filter fun y => asBool (q.bin.eval G.toCore (Env.one y))) (ihs ρ))
  | empty => rfl
  | insert x s ihx ihs =>
    exact (FinSet.map_insert _ _ _).trans (congr_two FinSet.insert (ihx ρ) (ihs ρ))
  | union s s' ih ih' =>
    exact (FinSet.map_union _ _ _).trans (congr_two FinSet.union (ih ρ) (ih' ρ))
  | @mapSet Γ a b s u ihs ihu =>
    exact (finset_map_comm (h := toBin b) (f := fun y => u.eval G (NEnv.one y)) (k := toBin a)
        (g := fun y => u.bin.eval G.toCore (Env.one y)) (fun y => ihu (NEnv.one y)) _).trans
      (congrArg (FinSet.map fun y => u.bin.eval G.toCore (Env.one y)) (ihs ρ))
  | @filterSet Γ a s q ihs ihq =>
    have w : ∀ y : NTy.V T G.E a, nasBool (q.eval G (NEnv.one y)) =
        asBool (q.bin.eval G.toCore (Env.one (toBin a y))) := by
      intro y
      rw [← asBool_toBin]
      exact congrArg asBool (ihq (NEnv.one y))
    exact (finset_map_filter_comm (toBin a) w _).trans
      (congrArg (FinSet.filter fun y => asBool (q.bin.eval G.toCore (Env.one y))) (ihs ρ))

end ApgModel.Named
