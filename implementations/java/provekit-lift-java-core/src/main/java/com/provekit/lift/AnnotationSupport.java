package com.provekit.lift;

import java.util.*;
import com.github.javaparser.ast.CompilationUnit;
import com.github.javaparser.ast.ImportDeclaration;
import com.github.javaparser.ast.expr.AnnotationExpr;

public final class AnnotationSupport {
    private AnnotationSupport() {}

    public static boolean belongsToFamily(
            CompilationUnit cu,
            AnnotationExpr annotation,
            String packageName,
            Set<String> annotationNames,
            Set<String> competingPackages) {
        String name = annotation.getNameAsString();
        String simple = simpleName(name);
        if (!annotationNames.contains(simple)) return false;

        if (name.contains(".")) {
            return name.equals(packageName + "." + simple);
        }

        String currentPackage = cu.getPackageDeclaration()
            .map(pd -> pd.getNameAsString())
            .orElse("");
        if (currentPackage.equals(packageName)) return true;

        if (!hasImport(cu, packageName, simple)) return false;
        for (String competingPackage : competingPackages) {
            if (hasImport(cu, competingPackage, simple)) return false;
        }
        return true;
    }

    private static boolean hasImport(CompilationUnit cu, String packageName, String simpleName) {
        for (ImportDeclaration importDecl : cu.getImports()) {
            if (importDecl.isStatic()) continue;
            String imported = importDecl.getNameAsString();
            if (importDecl.isAsterisk() && imported.equals(packageName)) return true;
            if (!importDecl.isAsterisk() && imported.equals(packageName + "." + simpleName)) return true;
        }
        return false;
    }

    private static String simpleName(String fq) {
        int dot = fq.lastIndexOf('.');
        return dot >= 0 ? fq.substring(dot + 1) : fq;
    }
}
