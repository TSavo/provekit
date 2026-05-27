Require Import Arith Lia.
Fixpoint sum (n:nat) : nat := match n with O => 0 | S k => S k + sum k end.
(* Same obligation SMT returned unknown on: forall n, 2*sum(n) = n*(n+1). *)
Goal forall n, 2 * sum n = n * (n + 1).
Proof. induction n; simpl; lia. Qed.
