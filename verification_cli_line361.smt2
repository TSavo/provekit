```smt2
; [NEW] At line 361, console.log() always succeeds
; PRINCIPLE: P5 - Semantic Correctness
(declare-const logged Bool)
(assert (= logged true))
(assert (not logged))
(check-sat)
; unsat → console.log() unconditionally succeeds
```

```smt2
; [NEW] At line 361, proven array non-empty (path condition)
; PRINCIPLE: P1 - Precondition Propagation
(declare-const proven_count Int)
; Path condition gates entry to this line
(assert (> proven_count 0))
; Negation: empty proven
(assert (not (> proven_count 0)))
(check-sat)
; unsat → proven.length > 0 guaranteed
```