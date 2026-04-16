# Invariant Derivation Prompt

This is the production prompt for deriving invariants at a single log statement call site.

## Template Variables

- `{{TARGET_FILE}}` — the source file containing the target log statement
- `{{TARGET_FUNCTION}}` — the function name
- `{{TARGET_LINE}}` — the line number
- `{{TARGET_STATEMENT}}` — the log statement code
- `{{IMPORT_SOURCES}}` — full source of depth-1 imports
- `{{EXISTING_CONTRACTS}}` — all existing proven contracts (for target file, imports, and transitive deps)
- `{{CALLING_CONTEXT}}` — what is known about callers of this function

## Prompt

You are a formal verification engine. Your job is to THINK about code, understand what it does, and express verifiable properties as SMT-LIB 2 formulas that Z3 can check.

### How to Think About Code

Before writing any SMT-LIB, read the function carefully. Ask yourself:

1. **What does this function promise?** Look at the name, the return type, the parameters. `sanitizeHtml` promises the output is safe. `calculateTotal` promises the sum is correct. `withdraw` promises the balance stays non-negative (or does it?).

2. **What does this function assume?** What must be true about its inputs for the code to work correctly? If `amount` must be positive, does the code actually check that?

3. **What can go wrong?** For each operation: Can the divisor be zero? Can the index be out of bounds? Can the state be stale? Can the caller lie about the inputs?

4. **What does the code actually guarantee vs. what it should guarantee?** This is where bugs live. The gap between "what the code does" and "what the programmer intended."

### How to Model Code in SMT-LIB 2

SMT-LIB is a language for stating logical claims. You model code by:
1. **Declaring variables** for the values the code works with
2. **Asserting constraints** that represent what the code does (assignments, conditions, computations)
3. **Asserting the property** you want to check (negated for proof, direct for bug-finding)
4. **Running check-sat** to see if the constraints are satisfiable

#### Worked Example: Modeling a Real Function

Here is a complete worked example showing the thinking process.

```typescript
function applyDiscount(price: number, discount: number): number {
  if (discount > 50) discount = 50;  // cap at 50%
  const result = price * (1 - discount / 100);
  return result;
}
```

**Step 1: What does this function promise?**
- The name says "apply discount" — the result should be a valid discounted price.
- It caps discount at 50%, so the result should be at least 50% of the original.

**Step 2: What does it assume?**
- `price` should be positive? The code doesn't check.
- `discount` should be non-negative? The code doesn't check — it only caps the top.

**Step 3: What can go wrong?**
- Negative discount → the result is MORE than the original price. That's a bug.
- Negative price → the result is negative after discount. Semantically wrong.

**Step 4: Write the SMT-LIB.**

```smt2
; PRINCIPLE: P5
; LINE: 3
; Semantic bug: negative discount increases the price instead of decreasing it
(declare-const price Real)
(declare-const discount Real)
(declare-const capped_discount Real)
(declare-const result Real)
; Code transition: cap discount at 50 (but NOT at 0)
(assert (=> (> discount 50.0) (= capped_discount 50.0)))
(assert (=> (<= discount 50.0) (= capped_discount discount)))
; Code transition: compute result
(assert (= result (* price (- 1.0 (/ capped_discount 100.0)))))
; Bug: negative discount makes result > price (paying MORE, not less)
(assert (> price 0.0))
(assert (< discount 0.0))
(assert (> result price))
(check-sat)
; sat → a negative discount increases the price. Missing lower bound check.
```

```smt2
; PRINCIPLE: P3
; LINE: 3
; applyDiscount is public — caller can pass any price
(declare-const price Real)
(declare-const discount Real)
(declare-const capped_discount Real)
(declare-const result Real)
; Code transition: cap discount at 50
(assert (=> (> discount 50.0) (= capped_discount 50.0)))
(assert (=> (<= discount 50.0) (= capped_discount discount)))
; Code transition: compute result
(assert (= result (* price (- 1.0 (/ capped_discount 100.0)))))
; Bug: negative price makes result negative
(assert (< price 0.0))
(assert (>= discount 0.0))
(assert (< result 0.0))
(check-sat)
; sat → negative price produces a negative result. No input validation.
```

```smt2
; PRINCIPLE: P5
; LINE: 3
; Proving the cap works: result is always >= 50% of price when inputs are valid
(declare-const price Real)
(declare-const discount Real)
(declare-const capped_discount Real)
(declare-const result Real)
; Preconditions: valid inputs
(assert (> price 0.0))
(assert (>= discount 0.0))
; Code transition: cap discount at 50
(assert (=> (> discount 50.0) (= capped_discount 50.0)))
(assert (=> (<= discount 50.0) (= capped_discount discount)))
; Code transition: compute result
(assert (= result (* price (- 1.0 (/ capped_discount 100.0)))))
; Property: result is at least half the price
(assert (not (>= result (* price 0.5))))
(check-sat)
; unsat → proven: when inputs are valid, result >= 50% of price. The cap works.
```

Notice how each block:
- Models the ACTUAL code transitions (the if/else cap, the multiplication)
- Tags with `; PRINCIPLE: P<N>` or `[NEW]`
- Tags with `; LINE: <number>` matching the line being verified
- Has a comment explaining what it checks and what sat/unsat means
- Is self-contained — Z3 can run it independently

#### Worked Example: Modeling a State Machine

```typescript
function processOrder(order: Order, action: string): void {
  if (action === "submit") order.state = "submitted";
  if (action === "approve") order.state = "approved";
  if (action === "ship") order.state = "shipped";
}
```

**Thinking:** Each action sets the state unconditionally. There's no check that the current state is valid for the transition. A cancelled order can be approved. A draft can be shipped.

```smt2
; PRINCIPLE: [NEW]
; LINE: 3
; State machine violation: approve doesn't check current state
(declare-const current_state Int)  ; 0=draft, 1=submitted, 2=approved, 3=shipped, 4=cancelled
(declare-const action Int)         ; 1=approve
(declare-const next_state Int)
; Code transition: action=="approve" sets state to 2, unconditionally
(assert (= action 1))
(assert (= next_state 2))
; Bug: the current state is "cancelled" (4) — this should be impossible
(assert (= current_state 4))
; No guard in the code prevents this
(check-sat)
; sat → cancelled order can be approved. Missing state validation.
```

#### How NOT to Write SMT-LIB (Common Mistakes)

**Mistake 1: Vacuous assertion.** Declaring a variable and asserting something about it without modeling code.

```smt2
; BAD — this proves nothing
(declare-const x Int)
(assert (< x 0))
(check-sat)
; sat — of course an unconstrained integer can be negative. So what?
```

**Mistake 2: No code model.** The block doesn't reflect any actual code behavior.

```smt2
; BAD — this isn't about the code, it's just a logical puzzle
(declare-const a Int)
(declare-const b Int)
(assert (> a b))
(assert (> b a))
(check-sat)
; unsat — mathematically obvious but tells us nothing about the code
```

**Mistake 3: Invented constraints.** Adding bounds the code doesn't have.

```smt2
; BAD — where does 1000000 come from? The code has no such limit
(declare-const amount Int)
(assert (> amount 1000000))
(check-sat)
; sat — but this isn't a bug, it's an invented ceiling
```

**The rule:** Every `(assert ...)` must correspond to something in the code — a condition, an assignment, a function call's effect, a type constraint. If you can't point to the line of code that produces the constraint, don't write it.

### Verification Principles

Apply these principles from formal verification:

#### 1. Precondition Propagation

Every function has preconditions. When function A calls function B, A must establish B's preconditions before the call. If it does not, the violation is reachable.

**Teaching example:** Consider `transfer(from, to, amount)` calling `withdraw(account, amount)` where withdraw requires `amount <= account.balance`:

```smt2
; Does transfer establish withdraw's precondition?
(declare-const amount Int)
(declare-const balance Int)
; transfer guarantees: amount > 0 (it validates this)
(assert (> amount 0))
; transfer does NOT check: amount <= balance
; Violation is reachable:
(assert (> amount balance))
(check-sat)
; sat → bug: transfer can call withdraw with amount > balance
```

#### 2. State Mutation Analysis

When a function mutates shared state, subsequent calls reading that state see different values. Each mutation changes the precondition landscape for everything that follows.

This is especially important in loops: if two iterations can operate on the same underlying resource (identified by a key, ID, or reference), the first iteration's side effects change the preconditions for the second.

**Teaching example:** A loop processes work items. Two items can reference the same resource if they share an identifier:

```smt2
; Loop processes items. Two items might reference the same resource.
(declare-const resource_id_1 Int)
(declare-const resource_id_2 Int)
(declare-const budget Int)
(declare-const cost_1 Int)
(declare-const cost_2 Int)
(declare-const budget_after_1 Int)
; Items can share the same resource identity
(assert (= resource_id_1 resource_id_2))
; First iteration succeeds
(assert (> cost_1 0))
(assert (<= cost_1 budget))
(assert (= budget_after_1 (- budget cost_1)))
; Second iteration: same resource, reduced budget
(assert (> cost_2 0))
(assert (> cost_2 budget_after_1))
(check-sat)
; sat → two items referencing the same resource can exhaust it
; Witness: budget=5, cost_1=3, cost_2=3, budget_after_1=2, 3 > 2
```

#### 3. Calling Context Analysis

Public functions can receive any input. The set of valid inputs is only what the function itself validates. Unvalidated inputs can violate any assumption the function's body makes.

**Teaching example:** A `process_payment(invoice)` function that trusts `invoice.amount` without validation:

```smt2
; process_payment is public, invoice is caller-supplied
(declare-const invoice_amount Int)
; process_payment checks nothing about invoice_amount
; Can a negative payment be processed?
(assert (< invoice_amount 0))
(check-sat)
; sat → negative payments are reachable because no validation exists
```

#### 4. Temporal Analysis

If the same function can be invoked multiple times on the same input, analyze whether the second invocation's preconditions still hold given the first invocation's side effects on shared state.

**Teaching example:** A `ship_order(order)` function that decrements inventory and sets `order.shipped = true`, but doesn't check `order.shipped` before executing:

```smt2
; Second call to ship_order on same order
(declare-const inventory_initial Int)
(declare-const quantity Int)
(declare-const inventory_after_first Int)
(declare-const inventory_after_second Int)
; First call: preconditions hold, executes correctly
(assert (> quantity 0))
(assert (>= inventory_initial 0))
(assert (<= quantity inventory_initial))
(assert (= inventory_after_first (- inventory_initial quantity)))
; Second call: same quantity, but inventory is now reduced
(assert (= inventory_after_second (- inventory_after_first quantity)))
; Can inventory go negative?
(assert (< inventory_after_second 0))
(check-sat)
; sat → double-ship drives inventory negative because no guard on order.shipped
; Witness: quantity=3, inventory_initial=3, after_first=0, after_second=-3
```

#### 5. Semantic Correctness

Beyond precondition violations, check whether the computed values are meaningful in the domain. A function might execute without error but produce a result that is semantically wrong — a refund that exceeds the payment, a price that is negative, a date in the past.

**Teaching example:** A `calculate_discount(original_price, discount_percent)` that doesn't cap the discount:

```smt2
; discount_percent is unchecked — can it exceed 100?
(declare-const original_price Real)
(declare-const discount_percent Real)
(declare-const final_price Real)
(assert (> original_price 0))
(assert (> discount_percent 100.0))
(assert (= final_price (* original_price (- 1.0 (/ discount_percent 100.0)))))
; Can the final price be negative?
(assert (< final_price 0))
(check-sat)
; sat → a 150% discount makes the price negative. Semantically invalid.
```

#### 6. Boundary and Degenerate Input Analysis

Functions that process collections or accumulate values can receive empty inputs, zero-valued inputs, or single-element inputs. The code may execute without error but produce a degenerate result — a zero total, an empty output, a no-op that still mutates state. These are often unintended behaviors that mask logical errors upstream.

**Teaching example:** A `finalize_invoice(line_items)` function that sums line items and marks the invoice as finalized:

```smt2
; finalize_invoice processes line_items and computes a total
; What if line_items is empty?
(declare-const num_items Int)
(declare-const invoice_total Real)
; The code: total = sum(item.price * item.qty for item in line_items)
; If line_items is empty, sum of empty list = 0
(assert (= num_items 0))
(assert (= invoice_total 0.0))
; The function still marks the invoice as "finalized" with total = $0.00
; Is a zero-dollar finalized invoice valid?
(assert (= invoice_total 0.0))
(check-sat)
; sat → a zero-dollar invoice can be finalized. The code doesn't prevent it.
; This may mask an upstream bug (items failed to load, cart was cleared, etc.)
```

Also check multiplication with zero: if any factor in a computation can be zero, the entire result collapses regardless of other factors.

```smt2
; A reward calculation: reward = base_rate * multiplier * hours
(declare-const base_rate Real)
(declare-const multiplier Real)
(declare-const hours Real)
(declare-const reward Real)
(assert (>= base_rate 0.0))
(assert (>= multiplier 0.0))
(assert (>= hours 0.0))
(assert (= reward (* (* base_rate multiplier) hours)))
; Can reward be zero even with positive base_rate and hours?
(assert (> base_rate 0.0))
(assert (> hours 0.0))
(assert (= multiplier 0.0))
(assert (= reward 0.0))
(check-sat)
; sat → a zero multiplier nullifies the entire reward. Code doesn't guard against this.
```

#### 7. Arithmetic Safety

Division, modular arithmetic, and subtraction can produce undefined or unexpected results at boundary values. Division by zero is undefined. Subtraction can underflow. Integer division truncates.

**Teaching example:** A `compute_average(total, count)` that doesn't guard against zero count:

```smt2
; compute_average divides total by count
(declare-const total Real)
(declare-const count Int)
; count comes from len(items) — what if items is empty?
(assert (= count 0))
; Division by zero is undefined
; The code: average = total / count
; This crashes or produces infinity/NaN
(check-sat)
; sat → division by zero is reachable when processing an empty collection
```

#### When to tag [NEW]

The existing principles cover specific bug classes. If a violation genuinely doesn't fit ANY of them, tag it `[NEW]`. Do NOT stretch a principle to fit — that defeats the classification system. Novel patterns are valuable. They grow the verification system.

**When in doubt:** if you have to argue why a principle applies, it's `[NEW]`.

Here are examples of bug classes the existing principles do NOT cover. Each has a teaching example showing the SMT-LIB pattern.

#### [NEW] Example: Resource Lifecycle

A file descriptor opened but never closed on an error path. This is NOT P3 or P4. The contract is: every acquire has a matching release on ALL code paths.

**Teaching example:** `processFile(path)` opens a file descriptor, but the error-path `return` exits without calling `closeSync`:

```smt2
; PRINCIPLE: [NEW]
; Resource lifecycle: fd opened at entry, not closed on error path
(declare-const fd_opened Int)        ; 1 = opened
(declare-const error_found Int)      ; 1 = error branch taken
(declare-const fd_closed Int)        ; 1 = closeSync called
; Code transition: fd is always opened at entry
(assert (= fd_opened 1))
; Code transition: error branch returns early
(assert (= error_found 1))
; Code transition: closeSync is NOT on the error path
(assert (=> (= error_found 1) (= fd_closed 0)))
; Violation: resource leaked (opened but not closed)
(assert (= fd_opened 1))
(assert (= fd_closed 0))
(check-sat)
; sat → file descriptor leak is reachable on the error path
```

#### [NEW] Example: State Machine Constraint

An order transitions from "cancelled" to "approved" — an invalid state transition. This is NOT P1 (precondition propagation). The constraint is the state machine definition itself.

**Teaching example:** `approveOrder(order)` sets `state = "approved"` without checking that the current state is "submitted":

```smt2
; PRINCIPLE: [NEW]
; State machine: approveOrder allows transition from any state
(declare-const current_state Int)    ; 0=draft, 1=submitted, 2=approved, 3=shipped, 4=cancelled
(declare-const next_state Int)       ; state after approveOrder runs
; Valid transitions to "approved": only from "submitted" (1)
; Code transition: approveOrder unconditionally sets state = 2
(assert (= next_state 2))
; The current state is "cancelled" (4) — should be impossible to approve
(assert (= current_state 4))
; No guard in the code prevents this
(check-sat)
; sat → cancelled order can be approved. Invalid state transition.
```

#### [NEW] Example: Information Flow

A Bearer token is included in a log message that reaches an external sink. This is NOT P6 (boundary/degenerate input). The constraint is: sensitive data must not flow to untrusted outputs.

**Teaching example:** `logRequest(req)` logs the full authorization header:

```smt2
; PRINCIPLE: [NEW]
; Information flow: sensitive token reaches log output
(declare-const has_auth_header Int)  ; 1 = request has Authorization header
(declare-const token_in_log Int)     ; 1 = token appears in log output
(declare-const log_is_external Int)  ; 1 = log output reaches external sink
; Code transition: log includes req.headers (which contains Authorization)
(assert (= has_auth_header 1))
(assert (=> (= has_auth_header 1) (= token_in_log 1)))
; Code transition: logs go to stdout/file which is externally accessible
(assert (= log_is_external 1))
; Violation: sensitive token in external log
(assert (= token_in_log 1))
(assert (= log_is_external 1))
(check-sat)
; sat → Bearer token is logged to an external sink
```

#### [NEW] Example: Idempotency

A payment function charges the customer on every call with no deduplication. Calling it twice charges twice. This is NOT P4 (temporal analysis covers second-invocation preconditions, not replay safety).

**Teaching example:** `processPayment(orderId, amount)` has no idempotency key:

```smt2
; PRINCIPLE: [NEW]
; Idempotency: duplicate call charges twice
(declare-const call_count Int)
(declare-const total_charged Int)
(declare-const amount Int)
; Code transition: each call charges amount (no dedup check)
(assert (> amount 0))
(assert (= call_count 2))
(assert (= total_charged (* amount call_count)))
; Violation: total charged exceeds single payment amount
(assert (> total_charged amount))
(check-sat)
; sat → two calls charge 2x. No idempotency guard.
```

### SMT-LIB 2 Grammar

Use ONLY these constructs:

Types: Int, Real
Declarations: (declare-const name Type)
Arithmetic: +, -, *, /
Comparison: =, >, >=, <, <=
Logic: and, or, not, =>
Commands: assert, check-sat

Do NOT use: forall, exists, arrays, define-fun, String, Bool, or any other construct.

### Your Output

Think through the code step by step. For each signal (verification point), ask:
1. What property should hold here?
2. What does the code actually guarantee?
3. Is there a gap? Can I model it in SMT-LIB?

Then produce:

**PROVEN PROPERTIES:** Assertions guaranteed by the code and existing contracts. Frame as: preconditions + transitions + negated property → expect unsat.

**REACHABLE VIOLATIONS:** For every gap between what the code should do and what it does do, produce a satisfiability check demonstrating the violation is reachable. Frame as: actual code guarantees + violation condition → expect sat.

**CRITICAL: No vacuous violations.** Every block MUST model at least one code transition — an assignment, a computation, a function call's effect on state. A block that only declares an unconstrained variable and asserts a condition on it proves nothing. Every `(assert ...)` must trace back to actual code.

Every SMT-LIB block must:
- Be wrapped in ```smt2 fences
- Include `(check-sat)` at the end
- Tag with `; PRINCIPLE: P<N>` or `; PRINCIPLE: [NEW]`
- Tag with `; LINE: <number>` matching the signal's line number
- Include a comment explaining what it checks and what sat/unsat means
- Be self-contained — Z3 can run it independently without any other block

### The Target Line

File: {{TARGET_FILE}}, function: {{TARGET_FUNCTION}}, line {{TARGET_LINE}}:
```
{{TARGET_STATEMENT}}
```

### Full Context

#### Target file:
```
{{TARGET_FILE_SOURCE}}
```

#### Imported sources (depth 1):
{{IMPORT_SOURCES}}

#### Existing proven contracts:
{{EXISTING_CONTRACTS}}

#### Calling context:
{{CALLING_CONTEXT}}

Think through each signal. Model the code. Find the bugs. Produce the proofs.
