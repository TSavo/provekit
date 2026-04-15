```smt2
; [NEW] At line 54, projectRoot is non-empty
; PRINCIPLE: P6 - Boundary Input
; resolveProjectRoot always returns a path (always falls back to cwd)
(declare-const projectRoot String)
; The function guarantees non-empty return
; Try to violate: can it be empty?
(assert (= projectRoot ""))
(check-sat)
; sat → but wait, is "" semantically valid? It's a path to root
; This returns sat, showing empty IS possible 
; But resolve() on empty string returns cwd → this is expected behavior
```

```smt2
; [NEW] At line 54 after entry, exit-if-not-git is enforced
; PRINCIPLE: P1 - Precondition Propagation  
; At line 54 we have projectRoot, but line 64 checks isGitRepo()
; The guard ensures subsequent code receives valid git repo
(declare-const is_git Bool)
(assert (= is_git false))
; Can we reach line 54 with non-git projectRoot?
; Yes, but line 64 correctly exits in that case
(assert (not is_git))
(check-sat)
; sat → but this is intentional guard, not a bug
```