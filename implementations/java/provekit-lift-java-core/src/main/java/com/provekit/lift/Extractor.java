package com.provekit.lift;

import java.util.List;
import com.github.javaparser.ast.CompilationUnit;

public interface Extractor {
    String name();
    List<ContractDecl> extract(CompilationUnit cu, String rawSource);
}
