# Java Bodyguard Precondition

This showcase proves one Java source-body guard as a precondition predicate.

The producer contract is lifted from a real Java method body:

```java
if (radix < Character.MIN_RADIX || radix > Character.MAX_RADIX) {
    throw new IllegalArgumentException("radix");
}
```

The lifter treats the guard condition as a flat FOL predicate over inputs. It
does not model throw, control flow, or effects. The bad caller passes
`radix = 1`, so the existing implication verifier refuses the seam because the
caller cannot establish `radix >= 2 && radix <= 36`.

Claimed: one Java source-body precondition seam, zero Java annotations.

Not claimed: non-flat guards, else branches, switch arms, loops, early-return
reasoning, or exception semantics.
