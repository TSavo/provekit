import Mathlib

set_option autoImplicit false

theorem provekit_obligation : ∀ (x : Int), (x = x) := by
  aesop

#print axioms provekit_obligation
