/*
 * Base64Walker — a throwaway AST walker for the ProvekIt/Sugar generalization
 * experiment. JDK-only (com.sun.source), --release 21.
 *
 * LAW: every constraint emitted downstream must be traceable to an AST node of
 * the vendored commons-codec source, walked here. No constraint is hand-authored
 * from base64 knowledge; if it is not in the tree, it is not in the constraint set.
 *
 * What this tool does:
 *   1. Parses Base64.java + BaseNCodec.java with the javac compiler API.
 *   2. Locates STANDARD_ENCODE_TABLE; checks static+final; extracts the 64 byte
 *      literals (as walked char/int literal nodes) and the pad byte.
 *   3. Locates the encode(byte[],int,int,Context) method and walks:
 *        - the 3-byte full-block branch (modulus==0): the 4 encodeTable[...] index
 *          expressions (shift op + amount + mask), in emission order;
 *        - the modulus==1 tail branch: 2 chars + 2 pads (STANDARD);
 *        - the modulus==2 tail branch: 3 chars + 1 pad (STANDARD).
 *   4. Extracts the constants MASK_6BITS, BITS_PER_ENCODED_BYTE,
 *      BYTES_PER_UNENCODED_BLOCK, BYTES_PER_ENCODED_BLOCK and PAD_DEFAULT.
 *
 * Output: a JSON document of tree-derived facts on stdout. Each fact records the
 * source file + line of the AST node it came from. The SMT emission step
 * (emit_smt.js) consumes ONLY this JSON — it never re-reads the Java source.
 */

import com.sun.source.tree.*;
import com.sun.source.util.*;
import javax.tools.*;
import java.io.*;
import java.util.*;

public class Base64Walker {

    // ---- collected facts -------------------------------------------------
    static List<Integer> standardTable = new ArrayList<>(); // 64 byte values
    static String tableModifiers = "";                      // e.g. "private static final"
    static int tableLine = -1;
    static Map<String,Long> constants = new LinkedHashMap<>();
    static Map<String,Integer> constantLines = new LinkedHashMap<>();
    static int padDefault = -1;
    static int padLine = -1;

    // Per-branch emitted-char expressions. Each entry: {op, amount, file, line}
    // op = "SHR" (>>), "SHL" (<<), "ID" (no shift, bare & MASK).
    static List<Map<String,Object>> block3 = new ArrayList<>();   // modulus==0
    static List<Map<String,Object>> tailMod1 = new ArrayList<>(); // 8 bits
    static int tailMod1Pads = 0;
    static List<Map<String,Object>> tailMod2 = new ArrayList<>(); // 16 bits
    static int tailMod2Pads = 0;
    static boolean tailMod1PadGuardedStandard = false;
    static boolean tailMod2PadGuardedStandard = false;

    static Trees trees;
    static SourcePositions positions;
    static CompilationUnitTree currentCU;

    public static void main(String[] args) throws Exception {
        if (args.length < 2) {
            System.err.println("usage: Base64Walker <Base64.java> <BaseNCodec.java>");
            System.exit(2);
        }
        JavaCompiler compiler = ToolProvider.getSystemJavaCompiler();
        if (compiler == null) { System.err.println("no system java compiler"); System.exit(3); }
        StandardJavaFileManager fm = compiler.getStandardFileManager(null, null, null);
        List<File> files = new ArrayList<>();
        for (String a : args) files.add(new File(a));
        Iterable<? extends JavaFileObject> units = fm.getJavaFileObjectsFromFiles(files);

        // --release 21, proc:none — we only want parse + name resolution of trees.
        JavacTask task = (JavacTask) compiler.getTask(
                null, fm, d -> {/* ignore vendor-source diagnostics; we only parse */},
                Arrays.asList("--release", "21", "-proc:none"), null, units);
        trees = Trees.instance(task);
        positions = trees.getSourcePositions();

        Iterable<? extends CompilationUnitTree> parsed = task.parse();

        for (CompilationUnitTree cu : parsed) {
            currentCU = cu;
            new Scanner().scan(cu, null);
        }

        emitJson();
    }

    static long lineOf(Tree t) {
        long pos = positions.getStartPosition(currentCU, t);
        if (pos < 0) return -1;
        return currentCU.getLineMap().getLineNumber(pos);
    }

    static String fileOf() {
        String n = currentCU.getSourceFile().getName();
        int slash = Math.max(n.lastIndexOf('/'), n.lastIndexOf(File.separatorChar));
        return slash >= 0 ? n.substring(slash + 1) : n;
    }

    // ---- the tree scanner ------------------------------------------------
    static class Scanner extends TreePathScanner<Void, Void> {

        @Override
        public Void visitVariable(VariableTree node, Void p) {
            String name = node.getName().toString();
            // The encode table literal.
            if (name.equals("STANDARD_ENCODE_TABLE") && node.getInitializer() instanceof NewArrayTree) {
                tableModifiers = node.getModifiers().getFlags().toString()
                        .replace("[", "").replace("]", "");
                tableLine = (int) lineOf(node);
                NewArrayTree arr = (NewArrayTree) node.getInitializer();
                for (ExpressionTree e : arr.getInitializers()) {
                    standardTable.add(evalByteLiteral(e));
                }
            }
            // Integer/byte constants of interest.
            if ((name.equals("MASK_6BITS") || name.equals("BITS_PER_ENCODED_BYTE")
                    || name.equals("BYTES_PER_UNENCODED_BLOCK") || name.equals("BYTES_PER_ENCODED_BLOCK"))
                    && node.getInitializer() instanceof LiteralTree) {
                constants.put(name, ((Number) ((LiteralTree) node.getInitializer()).getValue()).longValue());
                constantLines.put(name, (int) lineOf(node));
            }
            if (name.equals("PAD_DEFAULT") && node.getInitializer() instanceof LiteralTree) {
                Object v = ((LiteralTree) node.getInitializer()).getValue();
                padDefault = (v instanceof Character) ? (int) (char) (Character) v
                        : ((Number) v).intValue();
                padLine = (int) lineOf(node);
            }
            return super.visitVariable(node, p);
        }

        @Override
        public Void visitMethod(MethodTree node, Void p) {
            if (node.getName().toString().equals("encode")
                    && node.getParameters().size() == 4) {
                walkEncode(node);
            }
            return super.visitMethod(node, p);
        }
    }

    static int evalByteLiteral(ExpressionTree e) {
        // STANDARD_ENCODE_TABLE entries are char literals 'A'.. or '+' '/'.
        if (e instanceof LiteralTree) {
            Object v = ((LiteralTree) e).getValue();
            if (v instanceof Character) return (int) (char) (Character) v;
            if (v instanceof Number) return ((Number) v).intValue();
        }
        throw new IllegalStateException("non-literal table entry: " + e);
    }

    // ---- walk the encode() method body for the index expressions ---------
    static void walkEncode(MethodTree m) {
        // We look for assignment statements of the shape:
        //     buffer[context.pos++] = encodeTable[ <indexExpr> & MASK_6BITS ];
        //     buffer[context.pos++] = pad;   (or PAD)
        // Inside the modulus switch (tail) and the modulus==0 if (full block).
        // We classify each emitted char by which structural region it sits in,
        // using the enclosing if/case condition walked from the tree.
        new TreeScanner<Void, String>() {
            @Override
            public Void visitIf(IfTree node, String region) {
                // The full-block branch: condition "0 == context.modulus".
                String cond = node.getCondition().toString().replaceAll("\\s+", "");
                if (cond.contains("0==context.modulus") || cond.contains("context.modulus==0")) {
                    scan(node.getThenStatement(), "BLOCK3");
                    return null;
                }
                return super.visitIf(node, region);
            }

            @Override
            public Void visitCase(CaseTree node, String region) {
                // tail branches keyed by modulus literal 1 or 2
                String label = node.getExpressions().toString();
                String r = region;
                if (label.contains("1")) r = "MOD1";
                else if (label.contains("2")) r = "MOD2";
                else if (label.contains("0")) r = "MOD0"; // nothing emitted
                for (StatementTree s : node.getStatements()) scan(s, r);
                return null;
            }

            @Override
            public Void visitAssignment(AssignmentTree node, String region) {
                if (region == null) return super.visitAssignment(node, region);
                // LHS must be buffer[context.pos++]
                if (!(node.getVariable() instanceof ArrayAccessTree)) {
                    return super.visitAssignment(node, region);
                }
                ExpressionTree rhs = node.getExpression();
                Map<String,Object> rec = classifyEmit(rhs);
                if (rec == null) return super.visitAssignment(node, region);
                switch (region) {
                    case "BLOCK3": block3.add(rec); break;
                    case "MOD1":
                        if (rec.get("op").equals("PAD")) tailMod1Pads++;
                        else tailMod1.add(rec);
                        break;
                    case "MOD2":
                        if (rec.get("op").equals("PAD")) tailMod2Pads++;
                        else tailMod2.add(rec);
                        break;
                }
                return super.visitAssignment(node, region);
            }
        }.scan(m.getBody(), null);
    }

    // Classify an RHS expression as a table-index emit or a pad emit.
    static Map<String,Object> classifyEmit(ExpressionTree rhs) {
        // pad / PAD identifier
        if (rhs instanceof IdentifierTree) {
            String id = ((IdentifierTree) rhs).getName().toString();
            if (id.equals("pad") || id.equals("PAD")) {
                Map<String,Object> r = new LinkedHashMap<>();
                r.put("op", "PAD");
                r.put("file", fileOf());
                r.put("line", (int) lineOf(rhs));
                return r;
            }
            return null;
        }
        // encodeTable[ indexExpr ]
        if (rhs instanceof ArrayAccessTree) {
            ArrayAccessTree aat = (ArrayAccessTree) rhs;
            if (!(aat.getExpression() instanceof IdentifierTree)) return null;
            if (!((IdentifierTree) aat.getExpression()).getName().toString().equals("encodeTable")) return null;
            ExpressionTree idx = aat.getIndex();
            return walkIndexExpr(idx);
        }
        return null;
    }

    // Walk `context.ibitWorkArea OP amount & MASK_6BITS` (or bare `... & MASK_6BITS`).
    static Map<String,Object> walkIndexExpr(ExpressionTree idx) {
        Map<String,Object> r = new LinkedHashMap<>();
        r.put("file", fileOf());
        r.put("line", (int) lineOf(idx));
        // The top operator is the bitwise AND with the mask (due to Java precedence:
        // >> and << bind tighter than &). So idx = (shiftExpr) & MASK.
        if (idx instanceof BinaryTree) {
            BinaryTree and = (BinaryTree) idx;
            if (and.getKind() == Tree.Kind.AND) {
                ExpressionTree maskOperand = and.getRightOperand();
                r.put("mask", maskOperand.toString());
                ExpressionTree shiftExpr = and.getLeftOperand();
                if (shiftExpr instanceof BinaryTree) {
                    BinaryTree sh = (BinaryTree) shiftExpr;
                    Tree.Kind k = sh.getKind();
                    if (k == Tree.Kind.RIGHT_SHIFT) {
                        r.put("op", "SHR");
                        r.put("amount", litInt(sh.getRightOperand()));
                        r.put("source", sh.getLeftOperand().toString());
                        return r;
                    } else if (k == Tree.Kind.LEFT_SHIFT) {
                        r.put("op", "SHL");
                        r.put("amount", litInt(sh.getRightOperand()));
                        r.put("source", sh.getLeftOperand().toString());
                        return r;
                    }
                }
                // bare `ibitWorkArea & MASK` (the last char of the full block)
                r.put("op", "ID");
                r.put("amount", 0);
                r.put("source", shiftExpr.toString());
                return r;
            }
        }
        return null;
    }

    static int litInt(ExpressionTree e) {
        if (e instanceof LiteralTree) {
            Object v = ((LiteralTree) e).getValue();
            if (v instanceof Number) return ((Number) v).intValue();
        }
        return -1;
    }

    // ---- emit the facts as JSON -----------------------------------------
    static void emitJson() {
        StringBuilder sb = new StringBuilder();
        sb.append("{\n");
        sb.append("  \"table\": {\n");
        sb.append("    \"name\": \"STANDARD_ENCODE_TABLE\",\n");
        sb.append("    \"modifiers\": \"").append(tableModifiers).append("\",\n");
        sb.append("    \"file\": \"Base64.java\",\n");
        sb.append("    \"line\": ").append(tableLine).append(",\n");
        sb.append("    \"length\": ").append(standardTable.size()).append(",\n");
        sb.append("    \"bytes\": ").append(standardTable.toString()).append("\n");
        sb.append("  },\n");
        sb.append("  \"pad\": {\"value\": ").append(padDefault)
          .append(", \"file\": \"BaseNCodec.java\", \"line\": ").append(padLine).append("},\n");
        sb.append("  \"constants\": {\n");
        int ci = 0;
        for (Map.Entry<String,Long> e : constants.entrySet()) {
            sb.append("    \"").append(e.getKey()).append("\": {\"value\": ")
              .append(e.getValue()).append(", \"line\": ").append(constantLines.get(e.getKey())).append("}");
            sb.append(++ci < constants.size() ? ",\n" : "\n");
        }
        sb.append("  },\n");
        sb.append("  \"block3\": ").append(emitRecs(block3)).append(",\n");
        sb.append("  \"tailMod1\": {\"chars\": ").append(emitRecs(tailMod1))
          .append(", \"pads\": ").append(tailMod1Pads).append("},\n");
        sb.append("  \"tailMod2\": {\"chars\": ").append(emitRecs(tailMod2))
          .append(", \"pads\": ").append(tailMod2Pads).append("}\n");
        sb.append("}\n");
        System.out.print(sb);
    }

    static String emitRecs(List<Map<String,Object>> recs) {
        StringBuilder sb = new StringBuilder("[");
        for (int i = 0; i < recs.size(); i++) {
            Map<String,Object> r = recs.get(i);
            sb.append("{");
            int j = 0;
            for (Map.Entry<String,Object> e : r.entrySet()) {
                Object v = e.getValue();
                sb.append("\"").append(e.getKey()).append("\": ");
                if (v instanceof Number) sb.append(v);
                else sb.append("\"").append(v.toString().replace("\\", "\\\\").replace("\"", "\\\"")).append("\"");
                if (++j < r.size()) sb.append(", ");
            }
            sb.append("}");
            if (i + 1 < recs.size()) sb.append(", ");
        }
        return sb.append("]").toString();
    }
}
