// SPDX-License-Identifier: Apache-2.0
//
// Slab + ContractDecl + tiny formula DSL for the java self-contracts
// orchestrator. Legacy kit-internal slabs still use this DSL directly.
// New host-language invariants should be authored in the Java IR kit
// (`provekit-ir`) and adapted here only at the packaging boundary.
//
// Formula shapes track the JCS Value tree the cross-kit `formulaToValue`
// peer produces, so the bytes minted by this kit cohabit the same wire
// grammar even though we hand-roll the Value tree directly.
//
// Mirrors implementations/csharp/Provekit.SelfContracts/Program.cs and
// implementations/rust/provekit-self-contracts/src/lib.rs (ContractDecl).

package com.provekit.selfcontracts;

import java.util.ArrayList;
import java.util.Arrays;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.function.Function;

import com.provekit.ir.Declaration;
import com.provekit.ir.Formula;
import com.provekit.ir.Jcs.Value;
import com.provekit.ir.Sort;
import com.provekit.ir.Term;

public final class Slab {

    private Slab() {}

    /**
     * One authored contract declaration. Mirrors the cross-kit ContractDecl;
     * pre/post/inv slots are optional but at least one MUST be set or the
     * mint adapter rejects.
     */
    public static final class ContractDecl {
        public final String name;
        public final Value pre;   // nullable
        public final Value post;  // nullable
        public final Value inv;   // nullable
        public final String outBinding;

        public ContractDecl(String name, Value pre, Value post, Value inv, String outBinding) {
            this.name = name;
            this.pre = pre;
            this.post = post;
            this.inv = inv;
            this.outBinding = outBinding;
        }
    }

    /** A named source of contract authoring. */
    public static final class AuthoredSlab {
        public final String label;
        public final String path;
        public final List<ContractDecl> contracts;

        public AuthoredSlab(String label, String path, List<ContractDecl> contracts) {
            this.label = label;
            this.path = path;
            this.contracts = contracts;
        }
    }

    /** Mutable per-slab collector. Each slab gets its own; orchestrator drains it. */
    public static final class Collector {
        private final List<ContractDecl> decls = new ArrayList<>();

        public void must(String name, Value formula) {
            // `must` is the author-side shorthand for "post-condition holds always".
            // Mirrors rust must() / csharp Must(): all collapse to a contract with
            // post = formula and no pre/inv.
            decls.add(new ContractDecl(name, null, formula, null, "out"));
        }

        public void contract(String name, Value pre, Value post, Value inv) {
            decls.add(new ContractDecl(name, pre, post, inv, "out"));
        }

        public List<ContractDecl> drain() {
            return List.copyOf(decls);
        }
    }

    public static ContractDecl fromIrContract(Declaration.Contract contract) {
        return new ContractDecl(
            contract.name(),
            formulaValue(contract.pre()),
            formulaValue(contract.post()),
            formulaValue(contract.inv()),
            contract.outBinding());
    }

    // ----------------------------------------------------------------
    // Formula DSL  produces JCS Value trees byte-equivalent to the
    // cross-kit `formulaToValue` peer.
    //
    // Shapes (insertion-order JCS keys; the encoder re-sorts by code-point):
    //   var:      {"kind":"var","name":<name>}
    //   const:    {"kind":"const","value":<v>,"sort":<sort>}
    //   ctor:     {"kind":"ctor","name":<name>,"args":[<term>...]}
    //   atomic:   {"kind":"atomic","name":<name>,"args":[<term>...]}
    //   forall:   {"kind":"forall","name":<name>,"sort":<sort>,"body":<formula>}
    //   sort:     {"kind":"primitive","name":<name>}
    // ----------------------------------------------------------------

    /** Primitive sort: {kind:"primitive", name:<name>}. */
    public static Value sort(String name) {
        LinkedHashMap<String, Value> m = new LinkedHashMap<>();
        m.put("kind", Value.string("primitive"));
        m.put("name", Value.string(name));
        return Value.object(m);
    }

    public static final Value SORT_STRING = sort("String");
    public static final Value SORT_BOOL = sort("Bool");
    public static final Value SORT_INT = sort("Int");

    /** Variable term: {kind:"var", name:<name>}. */
    public static Value var_(String name) {
        LinkedHashMap<String, Value> m = new LinkedHashMap<>();
        m.put("kind", Value.string("var"));
        m.put("name", Value.string(name));
        return Value.object(m);
    }

    /** String constant term: {kind:"const", value:<s>, sort:String}. */
    public static Value strConst(String s) {
        LinkedHashMap<String, Value> m = new LinkedHashMap<>();
        m.put("kind", Value.string("const"));
        m.put("value", Value.string(s));
        m.put("sort", SORT_STRING);
        return Value.object(m);
    }

    /** Integer constant term: {kind:"const", value:<n>, sort:Int}. */
    public static Value num(long n) {
        LinkedHashMap<String, Value> m = new LinkedHashMap<>();
        m.put("kind", Value.string("const"));
        m.put("value", Value.integer(n));
        m.put("sort", SORT_INT);
        return Value.object(m);
    }

    /** Constructor term: {kind:"ctor", name:<name>, args:[...]}. */
    public static Value ctor(String name, Value... args) {
        LinkedHashMap<String, Value> m = new LinkedHashMap<>();
        m.put("kind", Value.string("ctor"));
        m.put("name", Value.string(name));
        m.put("args", Value.array(Arrays.asList(args)));
        return Value.object(m);
    }

    /** Atomic predicate: {kind:"atomic", name:<name>, args:[...]}. */
    public static Value atomic(String name, Value... args) {
        LinkedHashMap<String, Value> m = new LinkedHashMap<>();
        m.put("kind", Value.string("atomic"));
        m.put("name", Value.string(name));
        m.put("args", Value.array(Arrays.asList(args)));
        return Value.object(m);
    }

    /**
     * Forall over a fresh variable named {@code varName}. The body builder
     * receives the variable term and returns a formula. Mirrors the rust
     * peer's `forall(String_(), |c| ...)` closure shape.
     */
    public static Value forall(String varName, Value sort, Function<Value, Value> body) {
        LinkedHashMap<String, Value> m = new LinkedHashMap<>();
        m.put("kind", Value.string("forall"));
        m.put("name", Value.string(varName));
        m.put("sort", sort);
        m.put("body", body.apply(var_(varName)));
        return Value.object(m);
    }

    /** Atomic equality: forall a body uses eq(t1, t2) ~ atomic("eq", t1, t2). */
    public static Value eq(Value left, Value right) {
        return atomic("eq", left, right);
    }

    /** Atomic gte: gte(t1, t2). */
    public static Value gte(Value left, Value right) {
        return atomic("gte", left, right);
    }

    /** Atomic starts_with: starts_with(t1, t2). */
    public static Value startsWith(Value left, Value right) {
        return atomic("starts_with", left, right);
    }

    public static Value trueConst() {
        // The rust peer uses ctor("true_const", str_const("")) as a "yes" sentinel
        // for predicates that have no IR-native expressible body. We mirror.
        return ctor("true_const", strConst(""));
    }

    private static Value formulaValue(Formula formula) {
        if (formula == null) return null;
        if (formula instanceof Formula.Atomic atomic) {
            List<Value> args = new ArrayList<>();
            for (Term arg : atomic.args()) {
                args.add(termValue(arg));
            }
            LinkedHashMap<String, Value> m = new LinkedHashMap<>();
            m.put("kind", Value.string("atomic"));
            m.put("name", Value.string(atomic.name()));
            m.put("args", Value.array(args));
            return Value.object(m);
        }
        if (formula instanceof Formula.Connective connective) {
            List<Value> operands = new ArrayList<>();
            for (Formula operand : connective.operands()) {
                operands.add(formulaValue(operand));
            }
            LinkedHashMap<String, Value> m = new LinkedHashMap<>();
            m.put("kind", Value.string(connective.kind().name()));
            m.put("operands", Value.array(operands));
            return Value.object(m);
        }
        if (formula instanceof Formula.Quantifier quantifier) {
            LinkedHashMap<String, Value> m = new LinkedHashMap<>();
            m.put("kind", Value.string(quantifier.kind().name()));
            m.put("name", Value.string(quantifier.name()));
            m.put("sort", sortValue(quantifier.sort()));
            m.put("body", formulaValue(quantifier.body()));
            return Value.object(m);
        }
        if (formula instanceof Formula.Choice choice) {
            LinkedHashMap<String, Value> m = new LinkedHashMap<>();
            m.put("kind", Value.string("choice"));
            m.put("varName", Value.string(choice.varName()));
            m.put("sort", sortValue(choice.sort()));
            m.put("body", formulaValue(choice.body()));
            return Value.object(m);
        }
        throw new IllegalArgumentException("unsupported ProofIR formula: " + formula.getClass());
    }

    private static Value termValue(Term term) {
        if (term instanceof Term.Var var) {
            LinkedHashMap<String, Value> m = new LinkedHashMap<>();
            m.put("kind", Value.string("var"));
            m.put("name", Value.string(var.name()));
            return Value.object(m);
        }
        if (term instanceof Term.Const constant) {
            LinkedHashMap<String, Value> m = new LinkedHashMap<>();
            m.put("kind", Value.string("const"));
            m.put("value", constValue(constant.value()));
            m.put("sort", sortValue(constant.sort()));
            return Value.object(m);
        }
        if (term instanceof Term.Ctor ctor) {
            List<Value> args = new ArrayList<>();
            for (Term arg : ctor.args()) {
                args.add(termValue(arg));
            }
            LinkedHashMap<String, Value> m = new LinkedHashMap<>();
            m.put("kind", Value.string("ctor"));
            m.put("name", Value.string(ctor.name()));
            m.put("args", Value.array(args));
            return Value.object(m);
        }
        if (term instanceof Term.Lambda lambda) {
            LinkedHashMap<String, Value> m = new LinkedHashMap<>();
            m.put("kind", Value.string("lambda"));
            m.put("paramName", Value.string(lambda.paramName()));
            m.put("paramSort", sortValue(lambda.paramSort()));
            m.put("body", termValue(lambda.body()));
            return Value.object(m);
        }
        if (term instanceof Term.Let let) {
            List<Value> bindings = new ArrayList<>();
            for (Term.LetBinding binding : let.bindings()) {
                LinkedHashMap<String, Value> b = new LinkedHashMap<>();
                b.put("name", Value.string(binding.name()));
                b.put("boundTerm", termValue(binding.boundTerm()));
                bindings.add(Value.object(b));
            }
            LinkedHashMap<String, Value> m = new LinkedHashMap<>();
            m.put("kind", Value.string("let"));
            m.put("bindings", Value.array(bindings));
            m.put("body", termValue(let.body()));
            return Value.object(m);
        }
        throw new IllegalArgumentException("unsupported ProofIR term: " + term.getClass());
    }

    private static Value sortValue(Sort sort) {
        if (sort instanceof Sort.Primitive primitive) {
            LinkedHashMap<String, Value> m = new LinkedHashMap<>();
            m.put("kind", Value.string("primitive"));
            m.put("name", Value.string(primitive.name()));
            return Value.object(m);
        }
        throw new IllegalArgumentException("unsupported ProofIR sort: " + sort.getClass());
    }

    private static Value constValue(Term.Value value) {
        if (value instanceof Term.Value.Int i) return Value.integer(i.value());
        if (value instanceof Term.Value.Str s) return Value.string(s.value());
        if (value instanceof Term.Value.Bool b) return Value.bool(b.value());
        throw new IllegalArgumentException("unsupported ProofIR const value: " + value.getClass());
    }
}
