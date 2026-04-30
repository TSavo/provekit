/**
 * Stryker-style mutation amplification for the corpus harness (Leak 4 layer).
 *
 * Why this exists
 * ───────────────
 * The base corpus is hand-curated. A passing principle on the base scenario
 * isn't strong evidence the principle generalises — it might overfit to the
 * exact source string. Stryker's published mutation operator catalog is the
 * standard tool for poking at that kind of overfit. We don't run Stryker's
 * full test-runner stack (it's tightly coupled to its own framework and we
 * already have our own corpus runner); we apply the same operator catalog
 * directly to scenario.files using ts-morph (which is already a dep).
 *
 * What it does
 * ────────────
 * For each base scenario, walk every (file, AST node) pair and try every
 * mutation operator. Each successful mutation produces ONE amplified scenario
 * differing from base by exactly one operator application. Mutations are LOCAL
 * and never composed.
 *
 * What "preservation" means here
 * ──────────────────────────────
 * The scenario's LLM stubs encode the answer to the ORIGINAL bug — they don't
 * react to the mutated source. So amplification mostly probes the integration
 * seam (SAST builder → locate → C5 mutation runner): does the pipeline still
 * land on the same node and produce the same outcome when source is perturbed
 * around the bug?
 *
 *   - preserves_bug: mutation does not remove the bug-causing operator. The
 *     scenario's expected outcome should still apply.
 *   - removes_bug:   mutation eliminates the bug (e.g. `/ → *` on a
 *     divide-by-zero scenario). The scenario should now classify out_of_scope
 *     or reject; we don't amplify these because the LLM stubs still claim the
 *     bug exists, which would produce noise rather than signal.
 *   - uncertain:     heuristic can't decide. Skipped from automated runs;
 *     the IDs are still emitted for human-review fixtures.
 *
 * Filtering
 * ─────────
 * Bases with expected.outcome === "out_of_scope" produce no useful signal —
 * they short-circuit at classify before any code-touching stage. The amplifier
 * skips them; callers should filter the corpus before invoking.
 */

import { Project, SyntaxKind, Node, SourceFile } from "ts-morph";
import type { CorpusScenario } from "./scenarios.js";

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

export interface AmplifiedScenario extends CorpusScenario {
  baseScenarioId: string;
  /** "<operator>:<from>->[to]"; arrow rendered as "=>" to keep ascii. */
  mutationKind: string;
  preservation: "preserves_bug" | "removes_bug" | "uncertain";
}

export interface AmplifyOptions {
  /** Cap on amplified scenarios returned per base (default 10). */
  maxMutations?: number;
  /** If set, only operators whose name appears in this list are applied. */
  operators?: string[];
  /**
   * If true, also return mutations classified as "removes_bug" / "uncertain".
   * Default false — those are skipped from automated runs.
   */
  includeNonPreserving?: boolean;
}

// ---------------------------------------------------------------------------
// Operator catalog — Stryker-aligned subset
// ---------------------------------------------------------------------------

interface OperatorApplication {
  /** Human-readable mutationKind tag, e.g. "arith:+=>-". */
  kind: string;
  /** SyntaxKind of the node being mutated. */
  nodeKind: SyntaxKind;
  /** Test if this operator applies to the node. */
  matches(node: Node): boolean;
  /**
   * Mutate the node in-place. Caller emits the source after.
   * Should be reversible — do not throw; if the mutation cannot be applied,
   * leave the node alone and return false.
   */
  apply(node: Node): boolean;
}

/**
 * Replace a binary operator token. Returns a new operator application keyed on
 * the BinaryExpression's operatorToken kind.
 */
function binaryOpReplacer(
  category: string,
  fromTok: SyntaxKind,
  fromText: string,
  toText: string,
): OperatorApplication {
  return {
    kind: `${category}:${fromText}=>${toText}`,
    nodeKind: SyntaxKind.BinaryExpression,
    matches(node) {
      if (!Node.isBinaryExpression(node)) return false;
      return node.getOperatorToken().getKind() === fromTok;
    },
    apply(node) {
      if (!Node.isBinaryExpression(node)) return false;
      const tok = node.getOperatorToken();
      if (tok.getKind() !== fromTok) return false;
      tok.replaceWithText(toText);
      return true;
    },
  };
}

/** Replace a `true`/`false` literal. */
function booleanLiteralReplacer(
  fromKind: SyntaxKind,
  fromText: string,
  toText: string,
): OperatorApplication {
  return {
    kind: `bool:${fromText}=>${toText}`,
    nodeKind: fromKind,
    matches(node) {
      return node.getKind() === fromKind;
    },
    apply(node) {
      if (node.getKind() !== fromKind) return false;
      node.replaceWithText(toText);
      return true;
    },
  };
}

/** Negate a unary `!x` to `x` (drop the bang). */
const unaryNotDropper: OperatorApplication = {
  kind: "unary:!x=>x",
  nodeKind: SyntaxKind.PrefixUnaryExpression,
  matches(node) {
    if (!Node.isPrefixUnaryExpression(node)) return false;
    return node.getOperatorToken() === SyntaxKind.ExclamationToken;
  },
  apply(node) {
    if (!Node.isPrefixUnaryExpression(node)) return false;
    if (node.getOperatorToken() !== SyntaxKind.ExclamationToken) return false;
    const operand = node.getOperand().getText();
    node.replaceWithText(operand);
    return true;
  },
};

/**
 * Stryker's ConditionalExpression mutator: force the ternary's condition to
 * true or false. We implement only the "force true" variant (drops the false
 * branch) — Stryker emits both, but for our purposes one is sufficient signal
 * per ternary site.
 */
const conditionalForceTrue: OperatorApplication = {
  kind: "conditional:cond=>true",
  nodeKind: SyntaxKind.ConditionalExpression,
  matches(node) {
    return Node.isConditionalExpression(node);
  },
  apply(node) {
    if (!Node.isConditionalExpression(node)) return false;
    const cond = node.getCondition();
    if (cond.getText() === "true") return false;
    cond.replaceWithText("true");
    return true;
  },
};

const conditionalForceFalse: OperatorApplication = {
  kind: "conditional:cond=>false",
  nodeKind: SyntaxKind.ConditionalExpression,
  matches(node) {
    return Node.isConditionalExpression(node);
  },
  apply(node) {
    if (!Node.isConditionalExpression(node)) return false;
    const cond = node.getCondition();
    if (cond.getText() === "false") return false;
    cond.replaceWithText("false");
    return true;
  },
};

const OPERATORS: OperatorApplication[] = [
  // Arithmetic
  binaryOpReplacer("arith", SyntaxKind.PlusToken, "+", "-"),
  binaryOpReplacer("arith", SyntaxKind.MinusToken, "-", "+"),
  binaryOpReplacer("arith", SyntaxKind.AsteriskToken, "*", "/"),
  binaryOpReplacer("arith", SyntaxKind.SlashToken, "/", "*"),
  binaryOpReplacer("arith", SyntaxKind.PercentToken, "%", "*"),
  // Equality / relational
  binaryOpReplacer("equality", SyntaxKind.EqualsEqualsEqualsToken, "===", "!=="),
  binaryOpReplacer("equality", SyntaxKind.ExclamationEqualsEqualsToken, "!==", "==="),
  binaryOpReplacer("equality", SyntaxKind.EqualsEqualsToken, "==", "!="),
  binaryOpReplacer("equality", SyntaxKind.ExclamationEqualsToken, "!=", "=="),
  binaryOpReplacer("relational", SyntaxKind.LessThanToken, "<", ">="),
  binaryOpReplacer("relational", SyntaxKind.GreaterThanToken, ">", "<="),
  binaryOpReplacer("relational", SyntaxKind.LessThanEqualsToken, "<=", ">"),
  binaryOpReplacer("relational", SyntaxKind.GreaterThanEqualsToken, ">=", "<"),
  // Logical
  binaryOpReplacer("logical", SyntaxKind.AmpersandAmpersandToken, "&&", "||"),
  binaryOpReplacer("logical", SyntaxKind.BarBarToken, "||", "&&"),
  // Boolean literal
  booleanLiteralReplacer(SyntaxKind.TrueKeyword, "true", "false"),
  booleanLiteralReplacer(SyntaxKind.FalseKeyword, "false", "true"),
  // Unary not drop
  unaryNotDropper,
  // Conditional expression force
  conditionalForceTrue,
  conditionalForceFalse,
];

// ---------------------------------------------------------------------------
// Per-bugClass preservation table
//
// Returns the preservation classification for a (bugClass, operatorKind, nodeText)
// triple. Default is "uncertain" so we err on the side of skipping rather than
// confidently mis-classifying.
// ---------------------------------------------------------------------------

function classifyPreservation(
  bugClass: string,
  mutationKind: string,
  /** Source text of the mutated node before the mutation, for context. */
  nodePreText: string,
  /** Source text of the file before mutation, used for surrounding-context heuristics. */
  fileText: string,
): "preserves_bug" | "removes_bug" | "uncertain" {
  void fileText;
  void nodePreText;

  switch (bugClass) {
    case "division-by-zero": {
      // The bug-causing operator is `/`. Mutating it removes the bug.
      if (mutationKind === "arith:/=>*") return "removes_bug";
      if (mutationKind === "arith:/=>%") return "removes_bug";
      // Any other operator mutation does not touch the divide. Whether the
      // remaining `/` stays unguarded is what we want to probe — preserves_bug.
      if (mutationKind.startsWith("arith:")) return "preserves_bug";
      // Equality flips (e.g. on a `b === 0` guard) potentially DISABLE the
      // guard, which keeps the bug present (still preserves_bug for our
      // principle's purposes). The amplifier's job is to verify the principle
      // still flags the unguarded division.
      if (mutationKind.startsWith("equality:")) return "preserves_bug";
      if (mutationKind.startsWith("relational:")) return "preserves_bug";
      if (mutationKind.startsWith("logical:")) return "preserves_bug";
      if (mutationKind.startsWith("bool:")) return "preserves_bug";
      if (mutationKind.startsWith("unary:")) return "preserves_bug";
      return "uncertain";
    }
    case "null-assertion": {
      // Bug-causing site is the unguarded property access — not a binary op.
      // Any operator mutation around it preserves the dereference shape.
      if (mutationKind.startsWith("equality:")) return "preserves_bug";
      if (mutationKind.startsWith("relational:")) return "preserves_bug";
      if (mutationKind.startsWith("logical:")) return "preserves_bug";
      if (mutationKind.startsWith("bool:")) return "preserves_bug";
      if (mutationKind.startsWith("unary:")) return "preserves_bug";
      if (mutationKind.startsWith("arith:")) return "preserves_bug";
      return "uncertain";
    }
    case "ternary-branch-collapse": {
      // Bug is the two equal branch literals. Operator mutations elsewhere
      // (e.g. on the condition) leave the collapsed branches intact.
      if (mutationKind.startsWith("equality:")) return "preserves_bug";
      if (mutationKind.startsWith("relational:")) return "preserves_bug";
      if (mutationKind.startsWith("bool:")) return "preserves_bug";
      if (mutationKind.startsWith("logical:")) return "preserves_bug";
      if (mutationKind.startsWith("unary:")) return "preserves_bug";
      // Forcing the condition to a literal still leaves the two equal branches
      // present — collapse persists at the AST level even if one becomes
      // unreachable.
      if (mutationKind.startsWith("conditional:")) return "preserves_bug";
      // Arith on the branches could change literal values — uncertain.
      if (mutationKind.startsWith("arith:")) return "uncertain";
      return "uncertain";
    }
    case "novel": {
      // Imported semgrep scenarios are out_of_scope — pre-filtered at the
      // amplifier entry, but be defensive.
      return "uncertain";
    }
    default: {
      // Multi-file / adversarial / anything else: be conservative.
      return "uncertain";
    }
  }
}

// ---------------------------------------------------------------------------
// Core: enumerate mutations on a single source string
// ---------------------------------------------------------------------------

interface RawMutation {
  mutationKind: string;
  /** Mutated source text. */
  newSource: string;
  /** Source text of the targeted node before mutation, for preservation heuristic. */
  nodePreText: string;
}

function enumerateMutationsForFile(source: string, allowedOperators?: Set<string>): RawMutation[] {
  const out: RawMutation[] = [];

  // Each mutation needs a fresh AST so we don't accumulate edits.
  // We first scan once to find candidate (operator, nodePosition) pairs, then
  // apply each one to a fresh project.
  const scoutProject = new Project({
    useInMemoryFileSystem: true,
    skipAddingFilesFromTsConfig: true,
    compilerOptions: { allowJs: true, checkJs: false, noEmit: true },
  });
  let scoutFile: SourceFile;
  try {
    scoutFile = scoutProject.createSourceFile("__scout__.ts", source);
  } catch {
    return out;
  }

  type Candidate = { operatorIdx: number; pos: number; preText: string };
  const candidates: Candidate[] = [];

  scoutFile.forEachDescendant((node) => {
    for (let i = 0; i < OPERATORS.length; i++) {
      const op = OPERATORS[i];
      if (allowedOperators && !allowedOperators.has(op.kind.split(":")[0])) continue;
      if (node.getKind() !== op.nodeKind) continue;
      if (!op.matches(node)) continue;
      // For BinaryExpression the operator-token position is what we need; for
      // simplicity we record the binary expression position itself and re-find
      // by traversal.
      candidates.push({ operatorIdx: i, pos: node.getStart(), preText: node.getText() });
    }
  });

  // De-dup candidates that share (operatorIdx, pos) — should be unique already
  // but make it explicit.
  const seen = new Set<string>();

  for (const c of candidates) {
    const dedup = `${c.operatorIdx}|${c.pos}`;
    if (seen.has(dedup)) continue;
    seen.add(dedup);

    const op = OPERATORS[c.operatorIdx];
    const proj = new Project({
      useInMemoryFileSystem: true,
      skipAddingFilesFromTsConfig: true,
      compilerOptions: { allowJs: true, checkJs: false, noEmit: true },
    });
    let file: SourceFile;
    try {
      file = proj.createSourceFile("__mut__.ts", source);
    } catch {
      continue;
    }

    let applied = false;
    file.forEachDescendant((node, traversal) => {
      if (applied) {
        traversal.stop();
        return;
      }
      if (node.getKind() !== op.nodeKind) return;
      if (node.getStart() !== c.pos) return;
      if (!op.matches(node)) return;
      if (op.apply(node)) {
        applied = true;
        traversal.stop();
      }
    });

    if (!applied) continue;

    const mutated = file.getFullText();
    if (mutated === source) continue;

    out.push({
      mutationKind: op.kind,
      newSource: mutated,
      nodePreText: c.preText,
    });
  }

  return out;
}

// ---------------------------------------------------------------------------
// Public: amplifyScenario
// ---------------------------------------------------------------------------

export function amplifyScenario(
  base: CorpusScenario,
  opts: AmplifyOptions = {},
): AmplifiedScenario[] {
  // Do not amplify out_of_scope bases — no signal.
  if (base.expected.outcome === "out_of_scope") return [];

  const maxMutations = opts.maxMutations ?? 10;
  const allowedOperators = opts.operators ? new Set(opts.operators) : undefined;

  const out: AmplifiedScenario[] = [];
  let counter = 0;

  // Iterate files in stable order so amplified IDs are deterministic across
  // runs (keys() preserves insertion order).
  for (const [path, source] of Object.entries(base.files)) {
    // Only mutate TS/JS source. Other files (e.g. configs) are passed through.
    if (!/\.(tsx?|jsx?|mts|cts)$/.test(path)) continue;

    const rawMutations = enumerateMutationsForFile(source, allowedOperators);
    for (const m of rawMutations) {
      if (out.length >= maxMutations) break;

      const preservation = classifyPreservation(
        base.bugClass,
        m.mutationKind,
        m.nodePreText,
        source,
      );

      if (preservation !== "preserves_bug" && !opts.includeNonPreserving) {
        continue;
      }

      counter++;
      const amplified: AmplifiedScenario = {
        ...base,
        files: { ...base.files, [path]: m.newSource },
        id: `${base.id}+m${counter}`,
        baseScenarioId: base.id,
        mutationKind: m.mutationKind,
        preservation,
        // Preserves_bug => same expected outcome as base.
        // removes_bug   => bug should be gone; expect rejection or out_of_scope.
        // uncertain     => keep base expected; caller should skip these.
        expected: preservation === "removes_bug"
          ? { ...base.expected, outcome: "rejected" as const, completes: [] }
          : base.expected,
      };
      out.push(amplified);
    }
    if (out.length >= maxMutations) break;
  }

  return out;
}

// ---------------------------------------------------------------------------
// Public: amplifyCorpus — convenience for the runner.
// ---------------------------------------------------------------------------

export function amplifyCorpus(
  corpus: CorpusScenario[],
  opts: AmplifyOptions = {},
): { base: CorpusScenario[]; amplified: AmplifiedScenario[] } {
  const amplified: AmplifiedScenario[] = [];
  for (const s of corpus) {
    amplified.push(...amplifyScenario(s, opts));
  }
  return { base: corpus, amplified };
}
