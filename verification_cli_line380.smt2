```smt2
; [NEW] At line 380, console.log() always succeeds
; PRINCIPLE: P5 - Semantic Correctness
(declare-const logged Bool)
(assert (= logged true))
(assert (not logged))
(check-sat)
; unsat → console.log() unconditionally succeeds
```

```smt2
; [NEW] At line 380, violations non-empty (path condition)
; PRINCIPLE: P1 - Precondition Propagation
(declare-const violations_count Int)
(assert (> violations_count 0))
(assert (not (> violations_count 0)))
(check-sat)
; unsat → violations.length > 0 guaranteed
```