import Mathlib

set_option autoImplicit false

theorem sugar_obligation : ∀ (x : Int), (x = x) := by
  aesop

#print axioms sugar_obligation
