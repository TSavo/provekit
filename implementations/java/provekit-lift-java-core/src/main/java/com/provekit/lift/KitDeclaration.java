package com.provekit.lift;

import com.provekit.ir.Jcs;

final class KitDeclaration {
    static final String RPC_METHOD = "provekit.plugin.kit_declaration";

    private KitDeclaration() {}

    static Jcs.Obj toJson() {
        return Jcs.object(
            "kit", Jcs.object(
                "id", Jcs.string("java"),
                "language", Jcs.string("java"),
                "version", Jcs.string("0.1.0")
            ),
            "rpc", Jcs.object(
                "methods", Jcs.array(
                    rpcMethod("initialize", true),
                    rpcMethod(RPC_METHOD, true),
                    rpcMethod("parse", true),
                    rpcMethod("provekit.plugin.lift_implications", true),
                    rpcMethod("lift", false),
                    rpcMethod("provekit.plugin.recognize", false),
                    rpcMethod("shutdown", false)
                )
            ),
            "proofResolution", Jcs.object("strategy", Jcs.string("maven")),
            "effectKinds", Jcs.array(),
            "effectLeaves", Jcs.array(),
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
