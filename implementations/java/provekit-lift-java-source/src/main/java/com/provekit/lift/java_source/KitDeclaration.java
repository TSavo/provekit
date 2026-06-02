// SPDX-License-Identifier: Apache-2.0

package com.provekit.lift.java_source;

import com.provekit.ir.Jcs;

final class KitDeclaration {
    static final String RPC_METHOD = "provekit.plugin.kit_declaration";

    private KitDeclaration() {}

    static Jcs.Obj toJson() {
        return Jcs.object(
            "kit", Jcs.object(
                "id", Jcs.string("java-source"),
                "language", Jcs.string("java"),
                "version", Jcs.string("0.1.0")
            ),
            "rpc", Jcs.object(
                "methods", Jcs.array(
                    rpcMethod("initialize", true),
                    rpcMethod(RPC_METHOD, true),
                    rpcMethod("lift", true),
                    rpcMethod("shutdown", false)
                )
            ),
            "proofResolution", Jcs.object("strategy", Jcs.string("maven")),
            "effectKinds", Jcs.array(Jcs.string("concept:panic-freedom")),
            "effectLeaves", Jcs.array(
                Jcs.object(
                    "surface", Jcs.string("java-source"),
                    "local", Jcs.string("java:throw"),
                    "concept", Jcs.string("concept:panic-freedom.leaf.runtime-failure-site")
                )
            ),
            "guardPredicates", Jcs.array(),
            "controlCarriers", Jcs.array(),
            "residueCategories", Jcs.array()
        );
    }

    private static Jcs.Obj rpcMethod(String name, boolean required) {
        return Jcs.object(
            "name", Jcs.string(name),
            "required", Jcs.bool(required)
        );
    }
}
