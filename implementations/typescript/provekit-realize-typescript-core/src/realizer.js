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

  function emitStub({ functionName, params, paramTypes, returnType, conceptName, mode }) {
    const body = bodyTemplateFor(conceptName, params, paramTypes, returnType, mode);
    const isStub = body === null;
    const finalBody = body ?? `throw new Error("provekit-bind canonical: ${conceptName}");`;
    return {
      source: functionSource(functionName, params, finalBody),
      is_stub: isStub,
      extension: "ts",
    };
  }

  function bodyTemplateFor(conceptName, params, paramTypes, returnType, mode) {
    const mappedParamTypes = paramTypes.map(mapSourceType);
    const mappedReturnType = mapSourceType(returnType);
    const candidateNames = [conceptName, conceptName.replace(/^concept:/, "")];
    for (const entry of entries()) {
      if (!candidateNames.includes(entry.concept_name)) continue;
      if (!modeMatches(entry.mode, mode)) continue;
      const guard = entry.signature_guard ?? {};
      if (Number.isInteger(guard.min_params) && params.length < guard.min_params) continue;
      if (Number.isInteger(guard.max_params) && params.length > guard.max_params) continue;
      if (Array.isArray(guard.requires_param_types)) {
        if (!arrayEquals(mappedParamTypes, guard.requires_param_types)) continue;
      }
      if (typeof guard.requires_return_type === "string" && mappedReturnType !== guard.requires_return_type) continue;
      const emissionTemplate = entry.emission_template ?? {};
      if (emissionTemplate.kind !== "verbatim" || typeof emissionTemplate.template !== "string") continue;
      const rendered = renderTemplate(emissionTemplate.template, params, mappedParamTypes, mappedReturnType);
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
