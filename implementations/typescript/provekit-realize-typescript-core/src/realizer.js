const fs = require("node:fs");
const path = require("node:path");

const BODY_TEMPLATE_REL = path.join(
  "menagerie",
  "typescript-language-signature",
  "specs",
  "body-templates",
  "typescript-canonical-bodies.json",
);
const PLACEHOLDER_RE = /\$\{[^}]+\}/;

const coreRealizer = createRealizer(BODY_TEMPLATE_REL);

function createRealizer(bodyTemplateRel) {
  let cachedEntries = null;

  function emitStub({ functionName, params, paramTypes, returnType, conceptName, mode, namedTermTree }) {
    const body = bodyTemplateFor(conceptName, params, paramTypes, returnType, mode, namedTermTree);
    const isStub = body === null;
    const finalBody = body ?? `throw new Error("provekit-bind canonical: ${conceptName}");`;
    return {
      source: functionSource(functionName, params, finalBody),
      is_stub: isStub,
      extension: "ts",
    };
  }

  function bodyTemplateFor(conceptName, params, paramTypes, returnType, mode, namedTermTree) {
    const mappedParamTypes = paramTypes.map(mapSourceType);
    const mappedReturnType = mapSourceType(returnType);
    const nttArgsShape = argsShapeFromNamedTermTree(namedTermTree, conceptName);
    const templateSignature = templateLookupSignature(params, mappedParamTypes, namedTermTree, nttArgsShape);
    const candidateArgShapes = argShapeCandidates(nttArgsShape, mappedParamTypes, params.length);
    const candidateNames = [conceptName, conceptName.replace(/^concept:/, "")];
    for (const entry of entries()) {
      if (!candidateNames.includes(entry.concept_name)) continue;
      if (!modeMatches(entry.mode, mode)) continue;
      const guard = entry.signature_guard ?? {};
      if (!signatureGuardMatches(guard, candidateArgShapes)) continue;
      if (typeof guard.requires_return_type === "string" && mappedReturnType !== guard.requires_return_type) continue;
      const emissionTemplate = entry.emission_template ?? {};
      if (emissionTemplate.kind !== "verbatim" || typeof emissionTemplate.template !== "string") continue;
      const rendered = renderTemplate(
        emissionTemplate.template,
        templateSignature.params,
        templateSignature.paramTypes,
        mappedReturnType,
      );
      if (rendered !== null) return rendered;
    }
    return null;
  }

  function entries() {
    if (cachedEntries !== null) return cachedEntries;
    const templatePath = findRepoFile(bodyTemplateRel);
    if (templatePath === null) {
      cachedEntries = [];
      return cachedEntries;
    }
    const root = JSON.parse(fs.readFileSync(templatePath, "utf8"));
    cachedEntries = (((root.header ?? {}).content ?? {}).entries ?? []).filter((entry) => {
      return entry && typeof entry.concept_name === "string";
    });
    return cachedEntries;
  }

  return {
    bodyTemplateFor,
    emitStub,
  };
}

function argShapeCandidates(nttArgsShape, mappedParamTypes, paramCount) {
  const candidates = [];
  if (Array.isArray(nttArgsShape)) {
    candidates.push({ count: nttArgsShape.length, types: nttArgsShape });
  }
  candidates.push({ count: paramCount, types: mappedParamTypes });
  return candidates;
}

function templateLookupSignature(params, mappedParamTypes, namedTermTree, nttArgsShape) {
  if (!Array.isArray(nttArgsShape)) {
    return { params, paramTypes: mappedParamTypes };
  }
  return {
    params: nttTemplateParams(params, namedTermTree, nttArgsShape.length),
    paramTypes: nttArgsShape,
  };
}

function nttTemplateParams(params, namedTermTree, arity) {
  const args = isPlainObject(namedTermTree) ? namedTermTree.args : null;
  if (!Array.isArray(args)) {
    if (params.length === arity) return params;
    return Array.from({ length: arity }, (_, index) => `arg${index}`);
  }

  return Array.from({ length: arity }, (_, index) => {
    const arg = args[index];
    if (isPlainObject(arg)) return nttArgTemplateParam(arg, index);
    return `arg${index}`;
  });
}

function nttArgTemplateParam(arg, index) {
  const source = stringField(arg, ["source"]);
  if (source !== null) return source;

  for (const key of ["name", "paramName", "param_name", "binding", "symbol"]) {
    const value = stringField(arg, [key]);
    if (value !== null) return typescriptIdentifierOrDefault(value, `arg${index}`);
  }

  const descriptor = nttArgDescriptor(arg);
  if (descriptor === null) return `arg${index}`;
  const name = normalizeConceptName(descriptor);
  switch (name) {
    case "sql":
    case "sql-literal":
      return "sql";
    case "sqlargs":
    case "sql-args":
    case "sql_args":
      return "args";
    default:
      return typescriptIdentifierOrDefault(descriptor.replace(/^concept:/, ""), `arg${index}`);
  }
}

function nttArgDescriptor(arg) {
  for (const key of [
    "type",
    "typeName",
    "type_name",
    "sort",
    "sortName",
    "sort_name",
    "conceptName",
    "concept_name",
    "operationKind",
    "operation_kind",
  ]) {
    const value = arg[key];
    if (typeof value === "string" && value.trim() !== "") return value.trim();
    if (isPlainObject(value)) {
      const name = stringField(value, ["name", "sortName", "sort_name"]);
      if (name !== null) return name;
    }
  }
  return null;
}

function typescriptIdentifierOrDefault(value, fallback) {
  const identifier = value.replace(/-/g, "_");
  if (/^[A-Za-z_$][0-9A-Za-z_$]*$/.test(identifier)) return identifier;
  return fallback;
}

function signatureGuardMatches(guard, candidateArgShapes) {
  return candidateArgShapes.some((shape) => {
    if (Number.isInteger(guard.min_params) && shape.count < guard.min_params) return false;
    if (Number.isInteger(guard.max_params) && shape.count > guard.max_params) return false;
    if (Array.isArray(guard.requires_param_types)) {
      return arrayEquals(shape.types, guard.requires_param_types);
    }
    return true;
  });
}

function argsShapeFromNamedTermTree(namedTermTree, fallbackConceptName) {
  if (!isPlainObject(namedTermTree) || !Array.isArray(namedTermTree.args)) return null;
  const parentConceptName = stringField(namedTermTree, ["conceptName", "concept_name"]) ?? fallbackConceptName;
  return namedTermTree.args.map((arg, index) => typeDescriptorForNamedTermArg(arg, parentConceptName, index));
}

function typeDescriptorForNamedTermArg(arg, parentConceptName, index) {
  const explicit = explicitTypeDescriptor(arg);
  if (explicit !== null) return explicit;

  const parentFormal = formalArgType(parentConceptName, index);
  if (parentFormal !== null) return parentFormal;

  if (isPlainObject(arg)) {
    const childConcept = stringField(arg, ["conceptName", "concept_name"]);
    const conceptType = typeDescriptorFromConceptName(childConcept);
    if (conceptType !== null) return conceptType;
  }

  // NamedTermTree currently carries structure, concept names, operation kind,
  // and shape CIDs, but not source language types for arbitrary leaves. Precise
  // guards that cannot use the known concept formal sorts will miss this
  // descriptor and fall back to the legacy function paramTypes path.
  return "object";
}

function explicitTypeDescriptor(value) {
  if (!isPlainObject(value)) return null;
  const direct = stringField(value, [
    "paramType",
    "param_type",
    "type",
    "typeName",
    "type_name",
    "languageType",
    "language_type",
    "sourceType",
    "source_type",
  ]);
  if (direct !== null) return typeDescriptorFromSortName(direct) ?? mapSourceType(direct);

  const sort = value.sort;
  if (typeof sort === "string") return typeDescriptorFromSortName(sort) ?? mapSourceType(sort);
  if (isPlainObject(sort)) {
    const sortName = stringField(sort, ["name", "sortName", "sort_name"]);
    if (sortName !== null) return typeDescriptorFromSortName(sortName) ?? mapSourceType(sortName);
  }
  return null;
}

function formalArgType(conceptName, index) {
  const name = normalizeConceptName(conceptName);
  const sqlArgs = ["string", "unknown[]"];
  switch (name) {
    case "sql-query":
    case "sql-execute":
    case "insert-and-get-id":
      return sqlArgs[index] ?? null;
    default:
      return null;
  }
}

function typeDescriptorFromConceptName(conceptName) {
  return typeDescriptorFromSortName(normalizeConceptName(conceptName));
}

function typeDescriptorFromSortName(sortName) {
  const name = normalizeConceptName(sortName);
  switch (name) {
    case "sql":
    case "sql-literal":
    case "string":
    case "str":
      return "string";
    case "sqlargs":
    case "sql-args":
    case "array":
    case "list":
      return "unknown[]";
    case "bool":
    case "boolean":
      return "boolean";
    case "float":
    case "f32":
    case "f64":
    case "i8":
    case "i16":
    case "i32":
    case "i64":
    case "int":
    case "integer":
    case "number":
    case "u8":
    case "u16":
    case "u32":
    case "u64":
      return "number";
    case "()":
    case "unit":
    case "void":
      return "void";
    default:
      return null;
  }
}

function normalizeConceptName(name) {
  if (typeof name !== "string") return "";
  return name.trim().replace(/^concept:/, "").toLowerCase();
}

function stringField(value, names) {
  if (!isPlainObject(value)) return null;
  for (const name of names) {
    if (typeof value[name] === "string" && value[name].trim() !== "") return value[name];
  }
  return null;
}

function isPlainObject(value) {
  return value !== null && typeof value === "object" && !Array.isArray(value);
}

function mapSourceType(src) {
  switch (src) {
    case "()":
    case "void":
      return "void";
    case "i64":
    case "u64":
    case "i32":
    case "u32":
    case "i16":
    case "u16":
    case "i8":
    case "u8":
    case "int":
    case "float":
    case "f64":
    case "f32":
    case "number":
      return "number";
    case "bool":
    case "boolean":
      return "boolean";
    case "String":
    case "&str":
    case "&String":
    case "str":
    case "string":
      return "string";
    case "dict":
      return "Record<string, unknown>";
    default:
      return src;
  }
}

function renderTemplate(template, params, paramTypes, returnType) {
  let rendered = template;
  params.forEach((name, index) => {
    rendered = rendered.replaceAll(`\${param${index}}`, name);
  });
  paramTypes.forEach((typeName, index) => {
    rendered = rendered.replaceAll(`\${param_type_${index}}`, typeName);
  });
  rendered = rendered.replaceAll("${param_count}", String(params.length));
  rendered = rendered.replaceAll("${return_type}", returnType);
  return PLACEHOLDER_RE.test(rendered) ? null : rendered;
}

function modeMatches(entryMode, requestMode) {
  if (typeof entryMode !== "string" || entryMode === "") return true;
  return typeof requestMode === "string" && requestMode !== "" && entryMode === requestMode;
}

function functionSource(functionName, params, body) {
  const asyncPrefix = /\bawait\b/.test(body) ? "async " : "";
  const bodyLines = body.split("\n");
  const indented = bodyLines.map((line) => (line === "" ? "" : `  ${line}`)).join("\n");
  return `${asyncPrefix}function ${functionName}(${params.join(", ")}) {\n${indented}\n}\n`;
}

function findRepoFile(relativePath) {
  const seen = new Set();
  for (const base of candidateBases()) {
    const candidate = path.resolve(base, relativePath);
    if (seen.has(candidate)) continue;
    seen.add(candidate);
    if (fs.existsSync(candidate)) return candidate;
  }
  return null;
}

function candidateBases() {
  const bases = [];
  if (process.env.PROVEKIT_REPO_ROOT) bases.push(process.env.PROVEKIT_REPO_ROOT);
  let current = process.cwd();
  while (true) {
    bases.push(current);
    const parent = path.dirname(current);
    if (parent === current) break;
    current = parent;
  }
  current = __dirname;
  while (true) {
    bases.push(current);
    const parent = path.dirname(current);
    if (parent === current) break;
    current = parent;
  }
  return bases;
}

function arrayEquals(left, right) {
  return left.length === right.length && left.every((item, index) => item === right[index]);
}

module.exports = {
  bodyTemplateFor: coreRealizer.bodyTemplateFor,
  createRealizer,
  emitStub: coreRealizer.emitStub,
  mapSourceType,
  renderTemplate,
};
