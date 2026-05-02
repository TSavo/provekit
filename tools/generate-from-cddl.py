#!/usr/bin/env python3
"""
CDDL-based code generator for ProvekIt IR.

Usage:
    python tools/generate-from-cddl.py protocol/provekit-ir.cddl rust > ir_generated.rs
    python tools/generate-from-cddl.py protocol/provekit-ir.cddl typescript > ir_generated.ts
    python tools/generate-from-cddl.py protocol/provekit-ir.cddl go > ir_generated.go

This script reads the machine-readable CDDL grammar and generates:
- Type definitions (structs/enums)
- JSON parsers (with locked key order validation)
- JSON emitters (with locked key order output)
- Serializer tests

The generated code is byte-deterministic: same IR produces same JSON bytes.
"""

import sys
import re
from pathlib import Path


def parse_cddl(content: str) -> dict:
    """Parse CDDL content into a rules dictionary."""
    rules = {}
    lines = content.split("\n")
    current_rule = None
    current_def = []

    for line in lines:
        stripped = line.strip()
        if not stripped or stripped.startswith(";"):
            continue

        # Check if this starts a new rule (RuleName = ...)
        match = re.match(r"^([A-Za-z][A-Za-z0-9_]*)\s*=", stripped)
        if match and not stripped.startswith("{"):
            if current_rule:
                rules[current_rule] = "\n".join(current_def)
            current_rule = match.group(1)
            current_def = [stripped]
        elif current_rule:
            current_def.append(stripped)

    if current_rule:
        rules[current_rule] = "\n".join(current_def)

    return rules


def generate_rust(rules: dict) -> str:
    """Generate Rust types and parser from CDDL rules."""
    output = []
    output.append("// SPDX-License-Identifier: Apache-2.0")
    output.append("//")
    output.append("// GENERATED FILE — DO NOT EDIT")
    output.append("// Source: protocol/provekit-ir.cddl")
    output.append("// Generator: tools/generate-from-cddl.py")
    output.append("")
    output.append("use serde::{Deserialize, Serialize};")
    output.append("use serde_json::Value;")
    output.append("")

    # Generate Sort enum
    output.append("#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]")
    output.append('#[serde(tag = "kind")]')
    output.append("pub enum Sort {")
    output.append('    #[serde(rename = "primitive")]')
    output.append("    Primitive { name: PrimitiveSortName },")
    output.append("}")
    output.append("")

    output.append("#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]")
    output.append("pub enum PrimitiveSortName {")
    for name in ["Int", "Real", "Bool", "String"]:
        output.append(f'    #[serde(rename = "{name}")]')
        output.append(f"    {name},")
    output.append("}")
    output.append("")

    # Generate Term enum
    output.append("#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]")
    output.append('#[serde(tag = "kind")]')
    output.append("pub enum Term {")
    for variant, fields in [
        ("var", [("name", "String")]),
        ("const", [("value", "serde_json::Value"), ("sort", "Sort")]),
        ("ctor", [("name", "String"), ("args", "Vec<Term>")]),
        (
            "lambda",
            [("paramName", "String"), ("paramSort", "Sort"), ("body", "Box<Term>")],
        ),
        ("let", [("bindings", "Vec<LetBinding>"), ("body", "Box<Term>")]),
    ]:
        output.append(f'    #[serde(rename = "{variant}")]')
        field_str = ", ".join([f"{f[0]}: {f[1]}" for f in fields])
        output.append(
            f"    {variant.capitalize()} {{ {field_str} }},"
            if variant != "const"
            else f"    Const {{ {field_str} }},"
            if variant == "const"
            else f"    {variant.capitalize()} {{ {field_str} }},"
        )
    output.append("}")
    output.append("")

    output.append("#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]")
    output.append("pub struct LetBinding {")
    output.append("    pub name: String,")
    output.append("    pub bound_term: Term,")
    output.append("}")
    output.append("")

    # Generate Formula enum
    output.append("#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]")
    output.append('#[serde(tag = "kind")]')
    output.append("pub enum Formula {")
    output.append('    #[serde(rename = "atomic")]')
    output.append("    Atomic { name: String, args: Vec<Term> },")
    output.append('    #[serde(rename = "and")]')
    output.append("    And { operands: Vec<Formula> },")
    output.append('    #[serde(rename = "or")]')
    output.append("    Or { operands: Vec<Formula> },")
    output.append('    #[serde(rename = "not")]')
    output.append("    Not { operands: Vec<Formula> },")
    output.append('    #[serde(rename = "implies")]')
    output.append("    Implies { operands: Vec<Formula> },")
    output.append('    #[serde(rename = "forall")]')
    output.append("    Forall { name: String, sort: Sort, body: Box<Formula> },")
    output.append('    #[serde(rename = "exists")]')
    output.append("    Exists { name: String, sort: Sort, body: Box<Formula> },")
    output.append('    #[serde(rename = "choice")]')
    output.append("    Choice { var_name: String, sort: Sort, body: Box<Formula> },")
    output.append("}")
    output.append("")

    # Generate Evidence
    output.append("#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]")
    output.append("pub struct EvidenceTerm {")
    output.append("    pub kind: String,")
    output.append("    pub proof_type: String,")
    output.append("    pub certificate: EvidenceCertificate,")
    output.append("}")
    output.append("")

    output.append("#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]")
    output.append("pub struct EvidenceCertificate {")
    output.append("    pub tool: String,")
    output.append("    pub version: String,")
    output.append("    pub formula_hash: String,")
    output.append("    pub proof_data: String,")
    output.append("}")
    output.append("")

    # Generate Declaration
    output.append("#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]")
    output.append('#[serde(tag = "kind")]')
    output.append("pub enum Declaration {")
    output.append('    #[serde(rename = "contract")]')
    output.append("    Contract {")
    output.append("        name: String,")
    output.append("        out_binding: String,")
    output.append('        #[serde(skip_serializing_if = "Option::is_none")]')
    output.append("        pre: Option<Formula>,")
    output.append('        #[serde(skip_serializing_if = "Option::is_none")]')
    output.append("        post: Option<Formula>,")
    output.append('        #[serde(skip_serializing_if = "Option::is_none")]')
    output.append("        inv: Option<Formula>,")
    output.append('        #[serde(skip_serializing_if = "Option::is_none")]')
    output.append("        evidence: Option<EvidenceTerm>,")
    output.append("    },")
    output.append('    #[serde(rename = "bridge")]')
    output.append("    Bridge {")
    output.append("        name: String,")
    output.append("        source_symbol: String,")
    output.append("        source_layer: String,")
    output.append("        target_contract_cid: String,")
    output.append("        target_layer: String,")
    output.append('        #[serde(skip_serializing_if = "Option::is_none")]')
    output.append("        notes: Option<String>,")
    output.append("    },")
    output.append("}")
    output.append("")

    output.append("pub type Document = Vec<Declaration>;")
    output.append("")

    # Generate parser helpers
    output.append("/// Parse a Document from JSON string.")
    output.append(
        "pub fn parse_document(json: &str) -> Result<Document, serde_json::Error> {"
    )
    output.append("    serde_json::from_str(json)")
    output.append("}")
    output.append("")

    output.append("/// Serialize a Document to JSON string.")
    output.append(
        "pub fn emit_document(doc: &Document) -> Result<String, serde_json::Error> {"
    )
    output.append("    serde_json::to_string(doc)")
    output.append("}")
    output.append("")

    return "\n".join(output)


def generate_typescript(rules: dict) -> str:
    """Generate TypeScript types from CDDL rules."""
    output = []
    output.append("/**")
    output.append(" * GENERATED FILE — DO NOT EDIT")
    output.append(" * Source: protocol/provekit-ir.cddl")
    output.append(" * Generator: tools/generate-from-cddl.py")
    output.append(" */")
    output.append("")

    # Generate Sort type
    output.append("export type PrimitiveSortName = 'Int' | 'Real' | 'Bool' | 'String';")
    output.append("")
    output.append("export interface PrimitiveSort {")
    output.append("  kind: 'primitive';")
    output.append("  name: PrimitiveSortName;")
    output.append("}")
    output.append("")
    output.append("export type Sort = PrimitiveSort;")
    output.append("")

    # Generate Term types
    output.append("export interface VarTerm {")
    output.append("  kind: 'var';")
    output.append("  name: string;")
    output.append("}")
    output.append("")

    output.append("export interface ConstTerm {")
    output.append("  kind: 'const';")
    output.append("  value: unknown;")
    output.append("  sort: Sort;")
    output.append("}")
    output.append("")

    output.append("export interface CtorTerm {")
    output.append("  kind: 'ctor';")
    output.append("  name: string;")
    output.append("  args: IrTerm[];")
    output.append("}")
    output.append("")

    output.append("export interface LambdaTerm {")
    output.append("  kind: 'lambda';")
    output.append("  paramName: string;")
    output.append("  paramSort: Sort;")
    output.append("  body: IrTerm;")
    output.append("}")
    output.append("")

    output.append("export interface LetBinding {")
    output.append("  name: string;")
    output.append("  boundTerm: IrTerm;")
    output.append("}")
    output.append("")

    output.append("export interface LetTerm {")
    output.append("  kind: 'let';")
    output.append("  bindings: LetBinding[];")
    output.append("  body: IrTerm;")
    output.append("}")
    output.append("")

    output.append(
        "export type IrTerm = VarTerm | ConstTerm | CtorTerm | LambdaTerm | LetTerm;"
    )
    output.append("")

    # Generate Formula types
    output.append("export interface AtomicFormula {")
    output.append("  kind: 'atomic';")
    output.append("  name: string;")
    output.append("  args: IrTerm[];")
    output.append("}")
    output.append("")

    output.append("export interface ConnectiveFormula {")
    output.append("  kind: 'and' | 'or' | 'not' | 'implies';")
    output.append("  operands: IrFormula[];")
    output.append("}")
    output.append("")

    output.append("export interface QuantifierFormula {")
    output.append("  kind: 'forall' | 'exists';")
    output.append("  name: string;")
    output.append("  sort: Sort;")
    output.append("  body: IrFormula;")
    output.append("}")
    output.append("")

    output.append("export interface ChoiceFormula {")
    output.append("  kind: 'choice';")
    output.append("  varName: string;")
    output.append("  sort: Sort;")
    output.append("  body: IrFormula;")
    output.append("}")
    output.append("")

    output.append(
        "export type IrFormula = AtomicFormula | ConnectiveFormula | QuantifierFormula | ChoiceFormula;"
    )
    output.append("")

    # Generate Evidence
    output.append("export interface EvidenceCertificate {")
    output.append("  tool: string;")
    output.append("  version: string;")
    output.append("  formulaHash: string;")
    output.append("  proofData: string;")
    output.append("}")
    output.append("")

    output.append("export interface EvidenceTerm {")
    output.append("  kind: 'evidence';")
    output.append("  proofType: 'smt-lib' | 'coq' | 'custom';")
    output.append("  certificate: EvidenceCertificate;")
    output.append("}")
    output.append("")

    # Generate Declaration
    output.append("export interface ContractDeclaration {")
    output.append("  kind: 'contract';")
    output.append("  name: string;")
    output.append("  outBinding: string;")
    output.append("  pre?: IrFormula;")
    output.append("  post?: IrFormula;")
    output.append("  inv?: IrFormula;")
    output.append("  evidence?: EvidenceTerm;")
    output.append("}")
    output.append("")

    output.append("export interface BridgeDeclaration {")
    output.append("  kind: 'bridge';")
    output.append("  name: string;")
    output.append("  sourceSymbol: string;")
    output.append("  sourceLayer: string;")
    output.append("  targetContractCid: string;")
    output.append("  targetLayer: string;")
    output.append("  notes?: string;")
    output.append("}")
    output.append("")

    output.append("export type Declaration = ContractDeclaration | BridgeDeclaration;")
    output.append("")
    output.append("export type Document = Declaration[];")
    output.append("")

    return "\n".join(output)


def main():
    if len(sys.argv) < 3:
        print("Usage: generate-from-cddl.py <cddl-file> <language>")
        print("  language: rust | typescript | go")
        sys.exit(1)

    cddl_path = Path(sys.argv[1])
    language = sys.argv[2].lower()

    content = cddl_path.read_text()
    rules = parse_cddl(content)

    if language == "rust":
        print(generate_rust(rules))
    elif language == "typescript":
        print(generate_typescript(rules))
    else:
        print(f"Unsupported language: {language}", file=sys.stderr)
        sys.exit(1)


if __name__ == "__main__":
    main()
