/-
 SPDX-License-Identifier: Apache-2.0
 Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
-/
import ApgModel.Core.Apg

/-!
# Homomorphisms, isomorphisms and coalgebras

`SCHEMA-APG.md` definitions 7.1 and 7.3, and propositions 6.3, 7.2 and 8.1.

A homomorphism sends each element to an element of the same label and commutes with the values: the image's value is
the element's value with every reference mapped (`Hom.comm`). This is the second generation of the paper's Coq
development, `APG.v`'s `apgMorphism` with `PFNAT`; its first generation's morphisms, which ignore the values, are not
homomorphisms here. APGs on a schema with their homomorphisms form a category (proposition 7.2), whose laws are
stated as equations, since the model holds no category library.

The two forms of an APG are inverse up to isomorphism (proposition 6.3): the family form taken through the single-set
form and back is isomorphic to itself, and the single-set form taken through the family form and back holds the same
elements under a bijection, with the same labels and values.

An APG on `S` is exactly a coalgebra of `F_S`, `F_S(E)(l) = V_E(σ(l))`, and a homomorphism exactly a coalgebra
morphism (proposition 8.1). That `F_S` is a polynomial functor is the paper's remark and is not restated.
-/

namespace ApgModel.Core

variable {T : Typeside} {L : Type} {S : Schema T.P L}

/-- A homomorphism of APGs on `S`, definition 7.1: a function per label commuting with the values. -/
structure Hom (G G' : Apg S) where
  /-- The function on each label's elements. -/
  h : ∀ l, G.E l → G'.E l
  /-- `υ'_l ∘ h_l = V_h(σ(l)) ∘ υ_l`. -/
  comm : ∀ l e, G'.υ l (h l e) = V.map h (S.σ l) (G.υ l e)

namespace Hom

variable {G G' G'' G''' : Apg S}

/-- Two homomorphisms with the same functions are equal: the commuting square is a proof. -/
theorem ext {f g : Hom G G'} (w : f.h = g.h) : f = g := by
  cases f
  cases g
  cases w
  rfl

/-- The identity homomorphism. -/
def id (G : Apg S) : Hom G G where
  h _ e := e
  comm l e := (V.map_id (S.σ l) (G.υ l e)).symm

/-- The composite of two homomorphisms, `f` first. -/
def comp (f : Hom G G') (g : Hom G' G'') : Hom G G'' where
  h l e := g.h l (f.h l e)
  comm l e := by rw [g.comm, f.comm, ← V.map_comp]

/-- Proposition 7.2, the left identity law. -/
theorem id_comp (f : Hom G G') : (id G).comp f = f := ext rfl

/-- Proposition 7.2, the right identity law. -/
theorem comp_id (f : Hom G G') : f.comp (id G') = f := ext rfl

/-- Proposition 7.2, associativity. -/
theorem comp_assoc (f : Hom G G') (g : Hom G' G'') (k : Hom G'' G''') :
    (f.comp g).comp k = f.comp (g.comp k) := ext rfl

end Hom

/-- An isomorphism of APGs, definition 7.3: homomorphisms both ways whose components are mutually inverse, so the two
APGs differ by a renaming of their elements. -/
structure Iso (G G' : Apg S) where
  /-- The homomorphism forth. -/
  hom : Hom G G'
  /-- The homomorphism back. -/
  inv : Hom G' G
  /-- Back after forth is the identity. -/
  inv_hom : ∀ l e, inv.h l (hom.h l e) = e
  /-- Forth after back is the identity. -/
  hom_inv : ∀ l e, hom.h l (inv.h l e) = e

namespace Iso

variable {G G' G'' : Apg S}

/-- Every APG is isomorphic to itself. -/
def refl (G : Apg S) : Iso G G := ⟨Hom.id G, Hom.id G, fun _ _ => rfl, fun _ _ => rfl⟩

/-- An isomorphism read backwards. -/
def symm (i : Iso G G') : Iso G' G := ⟨i.inv, i.hom, i.hom_inv, i.inv_hom⟩

/-- Two isomorphisms composed. -/
def trans (i : Iso G G') (j : Iso G' G'') : Iso G G'' where
  hom := i.hom.comp j.hom
  inv := j.inv.comp i.inv
  inv_hom l e := by simp only [Hom.comp, j.inv_hom, i.inv_hom]
  hom_inv l e := by simp only [Hom.comp, i.hom_inv, j.hom_inv]

/-- A homomorphism whose components have inverse functions is an isomorphism: the inverse functions commute with the
values too. -/
def ofHom (f : Hom G G') (g : ∀ l, G'.E l → G.E l) (gf : ∀ l e, g l (f.h l e) = e) (fg : ∀ l e, f.h l (g l e) = e) :
    Iso G G' where
  hom := f
  inv := {
    h := g
    comm := by
      intro l e
      have inj : ∀ x y : V T G.E (S.σ l), V.map f.h (S.σ l) x = V.map f.h (S.σ l) y → x = y := by
        intro x y hxy
        have back : ∀ z, V.map g (S.σ l) (V.map f.h (S.σ l) z) = z := fun z => by
          rw [← V.map_comp]
          exact (V.map_congr (fun l' e' => gf l' e') (S.σ l) z).trans (V.map_id (S.σ l) z)
        rw [← back x, hxy, back y]
      apply inj
      rw [← f.comm, fg, ← V.map_comp]
      exact ((V.map_congr (fun l' e' => fg l' e') (S.σ l) _).trans (V.map_id (S.σ l) _)).symm }
  inv_hom := gf
  hom_inv := fg

end Iso

/-! ## Proposition 6.3: the two forms of an APG -/

/-- An element of the disjoint union carrying the label `l`, as an element of `l`. -/
def fromSigma (E : L → Type) (l : L) : { x : (Σ l, E l) // x.1 = l } → E l
  | ⟨⟨_, e⟩, rfl⟩ => e

/-- Proposition 6.3, the family form through the single-set form and back: isomorphic to the APG it started from. -/
def ofSingleSet_toSingleSet (G : Apg S) : Iso (ofSingleSet (toSingleSet G)) G where
  hom := {
    h := fromSigma G.E
    comm := by
      intro l y
      obtain ⟨⟨l', e⟩, h⟩ := y
      cases h
      show G.υ l' e = V.map (fromSigma G.E) (S.σ l') (V.map (toSigma G.E) (S.σ l') (G.υ l' e))
      rw [← V.map_comp]
      exact (V.map_id (S.σ l') (G.υ l' e)).symm }
  inv := {
    h := toSigma G.E
    comm := fun _ _ => rfl }
  inv_hom := by
    intro l y
    obtain ⟨⟨l', e⟩, h⟩ := y
    cases h
    rfl
  hom_inv := fun _ _ => rfl

/-- A bijection between two types. -/
structure Bij (α β : Type) where
  /-- Forth. -/
  toFun : α → β
  /-- Back. -/
  invFun : β → α
  /-- Back after forth is the identity. -/
  left : ∀ a, invFun (toFun a) = a
  /-- Forth after back is the identity. -/
  right : ∀ b, toFun (invFun b) = b

/-- The elements of the single-set form taken through the family form and back, in bijection with the elements it
started from: each element `x` is the element `⟨λ x, x⟩` of its label. -/
def toSingleSet_ofSingleSet_elements (H : SingleSet S) : Bij (toSingleSet (ofSingleSet H)).X H.X where
  toFun x := x.2.1
  invFun x := ⟨H.lam x, ⟨x, rfl⟩⟩
  left := by
    intro x
    obtain ⟨l, ⟨y, h⟩⟩ := x
    cases h
    rfl
  right := fun _ => rfl

/-- Proposition 6.3, the single-set form through the family form and back: the bijection keeps every element's
label. -/
theorem toSingleSet_ofSingleSet_lam (H : SingleSet S) (x : H.X) :
    (toSingleSet (ofSingleSet H)).lam ((toSingleSet_ofSingleSet_elements H).invFun x) = H.lam x :=
  rfl

/-- Proposition 6.3, the single-set form through the family form and back: an element's value is its value, every
reference renamed by `toSigma`. -/
theorem toSingleSet_ofSingleSet_value (H : SingleSet S) (x : H.X) :
    (toSingleSet (ofSingleSet H)).υ ((toSingleSet_ofSingleSet_elements H).invFun x) =
      V.map (toSigma (ofSingleSet H).E) (S.σ (H.lam x)) (H.υ x) :=
  rfl

/-- The renaming of references in `toSingleSet_ofSingleSet_value` is the bijection of the elements: a reference to
`y` becomes a reference to `y`'s image. -/
theorem toSigma_ofSingleSet (H : SingleSet S) (l : L) (y : { y : H.X // H.lam y = l }) :
    (toSigma (ofSingleSet H).E l y).1 = (toSingleSet_ofSingleSet_elements H).invFun y.1 := by
  obtain ⟨y, h⟩ := y
  cases h
  rfl

/-! ## Proposition 8.1: APGs are coalgebras -/

/-- `F_S`, the functor on families of proposition 8.1: `F_S(E)(l) = V_E(σ(l))`. -/
def F (S : Schema T.P L) (E : L → Type) : L → Type := fun l => V T E (S.σ l)

/-- `F_S` on morphisms of families: `F_S(h)_l = V_h(σ(l))`. -/
def F.map (S : Schema T.P L) {E E' : L → Type} (h : ∀ l, E l → E' l) : ∀ l, F S E l → F S E' l :=
  fun l => V.map h (S.σ l)

theorem F.map_id (S : Schema T.P L) {E : L → Type} (l : L) (x : F S E l) : F.map S (fun _ y => y) l x = x :=
  V.map_id (S.σ l) x

theorem F.map_comp (S : Schema T.P L) {E E' E'' : L → Type} (h : ∀ l, E l → E' l) (h' : ∀ l, E' l → E'' l)
    (l : L) (x : F S E l) : F.map S (fun l y => h' l (h l y)) l x = F.map S h' l (F.map S h l x) :=
  V.map_comp h h' (S.σ l) x

/-- An `F_S`-coalgebra: a family with a structure map into its image under `F_S`. -/
structure Coalg (S : Schema T.P L) where
  /-- The carrier family. -/
  E : L → Type
  /-- The structure map. -/
  str : ∀ l, E l → F S E l

/-- A morphism of `F_S`-coalgebras: a morphism of families commuting with the structure maps. -/
structure Coalg.Hom (C C' : Coalg S) where
  /-- The morphism of families. -/
  h : ∀ l, C.E l → C'.E l
  /-- `str' ∘ h = F_S(h) ∘ str`. -/
  comm : ∀ l e, C'.str l (h l e) = F.map S h l (C.str l e)

/-- An APG as an `F_S`-coalgebra. -/
def Apg.toCoalg (G : Apg S) : Coalg S := ⟨G.E, G.υ⟩

/-- An `F_S`-coalgebra as an APG. -/
def Coalg.toApg (C : Coalg S) : Apg S := ⟨C.E, C.str⟩

/-- Proposition 8.1, objects: an APG is exactly an `F_S`-coalgebra, both ways the identity. -/
theorem Coalg.toApg_toCoalg (G : Apg S) : G.toCoalg.toApg = G := rfl

theorem Apg.toCoalg_toApg (C : Coalg S) : C.toApg.toCoalg = C := rfl

/-- Proposition 8.1, morphisms: a homomorphism is exactly a coalgebra morphism. -/
def Hom.toCoalg {G G' : Apg S} (f : Hom G G') : Coalg.Hom G.toCoalg G'.toCoalg := ⟨f.h, f.comm⟩

def Coalg.Hom.toApg {C C' : Coalg S} (f : Coalg.Hom C C') : Core.Hom C.toApg C'.toApg := ⟨f.h, f.comm⟩

theorem Hom.toCoalg_toApg {G G' : Apg S} (f : Hom G G') : f.toCoalg.toApg = f := rfl

theorem Coalg.Hom.toApg_toCoalg {C C' : Coalg S} (f : Coalg.Hom C C') : f.toApg.toCoalg = f := rfl

end ApgModel.Core
