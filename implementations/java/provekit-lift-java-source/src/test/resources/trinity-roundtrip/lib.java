// canonical rewrite: src/lib.rs -> java

// substrate-origin: empty
// @provekit_monitor(concept = "pair")
final class WrapIdentityTransported {
    // concept: pair
    public static long wrap_identity(long x) {
        throw new UnsupportedOperationException("provekit-bind canonical: pair");
    }
}

// substrate-origin: empty
// @provekit_monitor(concept = "unit")
final class DoNothingTransported {
    // concept: unit
    public static void do_nothing() {
        throw new UnsupportedOperationException("provekit-bind canonical: unit");
    }
}

// substrate-origin: empty
// @provekit_monitor(concept = "pair")
final class ToggleTransported {
    // concept: pair
    public static boolean toggle(boolean flag) {
        throw new UnsupportedOperationException("provekit-bind canonical: pair");
    }
}

// substrate-origin: algebra-synthesis[wp_rule.guard-then-commit.v0]
// memento-cid: blake3-512:eacde837aabf5f45507f96049d6a75b59e02aba15f574dc3adf0129186fac93b39c786632883b0bce57f42b2f945e67956c6af0bddcc58ca6a5493646904a2dc
/* @ensures: (out >= 0) || (out == before_state) */
// @provekit_monitor(concept = "assert")
final class AssertPositiveTransported {
    // concept: assert
    public static long assert_positive(long x) {
        throw new UnsupportedOperationException("provekit-bind canonical: assert");
    }
}

// substrate-origin: algebra-synthesis[wp_rule.guard-then-commit.v0]
// memento-cid: blake3-512:aea7d8b1cc3ae1d746709bfa86586c510c5deb47bfda580f74d15a7d5a8c19790a5afbe73d1c97df9ddfe69c9251fefce41fc7623f57b78524a9e2b7cc776a98
/* @ensures: (out >= 0) || (out == before_state) */
// @provekit_monitor(concept = "option")
final class MaybeFirstTransported {
    // concept: option
    public static long maybe_first(&i64 items) {
        throw new UnsupportedOperationException("provekit-bind canonical: option");
    }
}

// substrate-origin: algebra-synthesis[wp_rule.guard-then-commit.v0]
// memento-cid: blake3-512:c61acf52939267a77db043a161f9f751d74f36007c590a474640634725841132328c520467551b9e728805a8e68613f0feb1c006c7f2b4516270d1f8ef4b2e5c
/* @ensures: (out >= 0) || (out == before_state) */
// @provekit_monitor(concept = "option-bind")
final class OptionBindDoubleTransported {
    // concept: option-bind
    public static long option_bind_double(&i64 items) {
        throw new UnsupportedOperationException("provekit-bind canonical: option-bind");
    }
}

// substrate-origin: algebra-synthesis[wp_rule.guard-then-commit.v0]
// memento-cid: blake3-512:c2e867773d27b48acc2cab010ed8817ca27cc56331c161b7bab34955efeb1f5392914f4d31539a967da47ce045800d643a9be12f4799f062712f59cdda907841
/* @ensures: (out >= 0) || (out == before_state) */
// @provekit_monitor(concept = "result")
final class SafeDivideTransported {
    // concept: result
    public static long safe_divide(long num, long denom) {
        throw new UnsupportedOperationException("provekit-bind canonical: result");
    }
}

// substrate-origin: algebra-synthesis[wp_rule.guard-then-commit.v0]
// memento-cid: blake3-512:09c9e626c0679682e8acbdb25ffcacdde4a83f73572b0e943a82513e762ca564829353b216c13928bbd21a250f306cc348804ca0f711de8124f5f8fa25f967b3
/* @ensures: (out >= 0) || (out == before_state) */
// @provekit_monitor(concept = "result-bind")
final class SafeDivideThenDoubleTransported {
    // concept: result-bind
    public static long safe_divide_then_double(long num, long denom) {
        throw new UnsupportedOperationException("provekit-bind canonical: result-bind");
    }
}

// substrate-origin: empty
// @provekit_monitor(concept = "pair")
final class SwapPairTransported {
    // concept: pair
    public static long swap_pair(long a, long b) {
        throw new UnsupportedOperationException("provekit-bind canonical: pair");
    }
}

// substrate-origin: algebra-synthesis[wp_rule.retry-with-bounded-attempts.v0]
// memento-cid: blake3-512:2b9812643379b78b66ed6255d8dc48a638db1d2d2db459620fdd736f61e42b5ff1e5bcb9b588e0c89242a0a3d846ed75d05d2a7f8259a073e4b4190def5870d8
/* @requires: max_attempts >= 0 */
/* @ensures: (out == true) || (out == false) */
// @provekit_monitor(concept = "list")
final class ListSumTransported {
    // concept: list
    public static long list_sum(&i64 items) {
        throw new UnsupportedOperationException("provekit-bind canonical: list");
    }
}

// substrate-origin: algebra-synthesis[wp_rule.guard-then-commit.v0]
// memento-cid: blake3-512:e128fc20e3e2eda3736094717f200892f5352d248be9d9b212b087dc279370aef3549bfbd354b3c186e133d694c7dbe4d83023cf51ec5f81c9577f66de0d200d
/* @ensures: (out >= 0) || (out == before_state) */
// @provekit_monitor(concept = "tagged-union")
final class ClassifyTransported {
    // concept: tagged-union
    public static long classify(long x) {
        throw new UnsupportedOperationException("provekit-bind canonical: tagged-union");
    }
}

// substrate-origin: annotation-lift
// memento-cid: blake3-512:2fc2de58d816dcadb9099fa622ee52a340e79a78342d0ee33693610b050d9cecf45bcb259e0f092b19858d23c0e4a54b782fa8f172b4f2e230f290d8e8ada636
/* @requires: max_attempts > 0 */
/* @ensures: out == true */
// @provekit_monitor(concept = "retry-loop")
final class RetryUntilSuccessTransported {
    // concept: retry-loop
    public static boolean retry_until_success(long max_attempts) {
        throw new UnsupportedOperationException("provekit-bind canonical: retry-loop");
    }
}
