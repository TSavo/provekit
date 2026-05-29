"use strict";

function assembleResponse(params) {
  if (params === null || typeof params !== "object" || Array.isArray(params)) {
    throw new Error("params must be an object");
  }
  const fileBasename = safeBasename(
    typeof params.file_basename === "string" && params.file_basename !== ""
      ? params.file_basename
      : "module",
  );
  const fragments = Array.isArray(params.fragments) ? params.fragments : [];
  const imports = new Set();
  const helpers = [];
  const sources = [];

  for (const fragment of fragments) {
    if (fragment === null || typeof fragment !== "object" || Array.isArray(fragment)) continue;
    for (const item of stringArray(fragment.imports)) imports.add(item);
    for (const item of stringArray(fragment.helpers)) helpers.push(item);
    if (typeof fragment.source === "string" && fragment.source.trim() !== "") {
      sources.push(fragment.source.trimEnd());
    }
  }

  const sections = [];
  const importBlock = renderImports(imports);
  if (importBlock !== "") sections.push(importBlock);
  if (helpers.length > 0) sections.push(dedupe(helpers).join("\n"));
  if (sources.length > 0) sections.push(sources.join("\n\n"));

  return {
    files: [
      {
        path: `${fileBasename}.ts`,
        content: `${sections.join("\n\n")}${sections.length > 0 ? "\n" : ""}`,
      },
    ],
    compile_classpath: [],
  };
}

function renderImports(imports) {
  return Array.from(imports)
    .sort()
    .map((item) => {
      if (item.startsWith("import ")) return item.endsWith(";") ? item : `${item};`;
      return `import ${JSON.stringify(item)};`;
    })
    .join("\n");
}

function safeBasename(value) {
  return value.replace(/[^A-Za-z0-9_.-]/g, "_").replace(/^\.+/, "") || "module";
}

function stringArray(value) {
  if (!Array.isArray(value)) return [];
  return value.filter((item) => typeof item === "string" && item !== "");
}

function dedupe(values) {
  return Array.from(new Set(values));
}

module.exports = {
  assembleResponse,
};
