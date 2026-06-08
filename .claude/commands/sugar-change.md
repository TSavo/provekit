The user wants to make this change: $ARGUMENTS

You MUST use sugar to make the change, not direct file edits. Run:

```bash
sugar change "$ARGUMENTS"
```

This routes through the bug-fix workflow:
- intake parses the request
- investigate locates the relevant code
- locate pins the symbol
- classify routes by intent kind
- formulate writes invariants (using symbolic primitives via runtime-eval)
- do-the-work writes patch + test
- bundle composes everything

The output is a signed memento containing the diff, the test, and the invariant. Mementos travel with the git commit; verification happens at the commit gate.

If the change is for adding an invariant without a code change, use `sugar must <file> "$ARGUMENTS"` instead.

If the change might violate a known invariant, you can preview by running `sugar refute <propertyHash>` first to see if a counterexample exists.

Do NOT edit files directly with Edit/Write tools for this change. The framework's data-driven workflow IS the change mechanism. Adding invariants by hand-editing .invariant.ts files is also forbidden, use `sugar must` instead.

After sugar completes, the response should report:
- The change's proofHash
- Whether 0 null roots (provably correct) or N null roots (specific gaps)
- The commit message that's ready to be `git commit`'d
