package com.provekit.emit.assertj;

import java.util.ArrayList;
import java.util.List;

import com.provekit.ir.Jcs;

/**
 * The emit request: a contract's neutral predicates plus the target function
 * signature. Mirrors the {@code RealizerPlan} shape in
 * {@code provekit-realize-java-core} but is SMALLER: this kit emits test
 * assertions, not function bodies.
 *
 * <p>JSON shape (the RPC {@code params} object):
 * <pre>
 * {
 *   "contract_id":   "concept:eq",          // concept name OR CID; informational
 *   "function":      "clamp",               // target function name (snake/camel)
 *   "params":        ["x", "lo", "hi"],     // formal parameter names
 *   "param_types":   ["int", "int", "int"], // java parameter types (parallel to params)
 *   "predicates": [                          // neutral predicate terms (catalog form)
 *     {"kind":"op","name":"concept:ge","args":[
 *        {"kind":"var","name":"x"},{"kind":"var","name":"lo"}]},
 *     {"kind":"op","name":"concept:le","args":[
 *        {"kind":"var","name":"x"},{"kind":"var","name":"hi"}]}
 *   ]
 * }
 * </pre>
 *
 * <p>The {@code predicates} array carries the neutral form that flows between
 * languages: {@code kind:"op"}, {@code name:"concept:<head>"}, {@code args}
 * = term subtrees. This kit also tolerates the harvester's internal
 * {@code kind:"atomic"} / bare-name form. The kit reads ONLY which predicate
 * it is emitting from the catalog concept name; the AssertJ spelling is
 * inline java in {@link PredicateAssertionTable}.
 */
public record EmitPlan(
    String contractId,
    String function,
    List<String> params,
    List<String> paramTypes,
    List<Jcs.Obj> predicates
) {
    public EmitPlan {
        params = List.copyOf(params);
        paramTypes = List.copyOf(paramTypes);
        predicates = List.copyOf(predicates);
    }

    /** Parse an EmitPlan from the RPC params object (a JSON object string). */
    public static EmitPlan fromParams(String paramsJson) {
        Jcs.Json doc = Jcs.parse(paramsJson);
        if (!(doc instanceof Jcs.Obj obj)) {
            return new EmitPlan("", "test", List.of(), List.of(), List.of());
        }
        String contractId = orEmpty(obj.stringFieldOrNull("contract_id"),
            obj.stringFieldOrNull("concept_name"));
        String function = orEmpty(obj.stringFieldOrNull("function"),
            obj.stringFieldOrNull("function_name"));
        if (function.isBlank()) function = "test";

        List<String> params = stringArray(obj.get("params"));
        List<String> paramTypes = stringArray(obj.get("param_types"));

        List<Jcs.Obj> predicates = new ArrayList<>();
        Jcs.Json preds = obj.get("predicates");
        if (preds instanceof Jcs.Arr arr) {
            for (Jcs.Json p : arr.values()) {
                if (p instanceof Jcs.Obj po) predicates.add(po);
            }
        }
        return new EmitPlan(contractId, function, params, paramTypes, predicates);
    }

    private static List<String> stringArray(Jcs.Json json) {
        List<String> out = new ArrayList<>();
        if (json instanceof Jcs.Arr arr) {
            for (Jcs.Json v : arr.values()) {
                if (v instanceof Jcs.Str s) out.add(s.value());
            }
        }
        return out;
    }

    private static String orEmpty(String first, String second) {
        if (first != null && !first.isBlank()) return first;
        if (second != null && !second.isBlank()) return second;
        return "";
    }
}
