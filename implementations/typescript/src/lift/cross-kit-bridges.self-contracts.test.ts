// SPDX-License-Identifier: Apache-2.0
//
// Native vitest surface for TypeScript cross-kit bridge counterpart
// contracts. The provekit-lift vitest adapter lifts each `it(...)` block
// below into a contract; no .invariant slab is involved.

import { expect, it } from "vitest";
import { trueConst, tsLiftSatisfiesRustContract } from "./cross-kit-bridges.js";

it("ts_lift_plugin_initialize_protocol_version_match", () => {
  expect(tsLiftSatisfiesRustContract("lift_plugin_initialize_protocol_version_match")).toBe(trueConst());
  expect(tsLiftSatisfiesRustContract("lift_plugin_initialize_protocol_version_match")).toBe(trueConst());
});

it("ts_lift_plugin_initialize_capabilities_authoring_surfaces_nonempty", () => {
  expect(tsLiftSatisfiesRustContract("lift_plugin_initialize_capabilities_authoring_surfaces_nonempty")).toBe(trueConst());
  expect(tsLiftSatisfiesRustContract("lift_plugin_initialize_capabilities_authoring_surfaces_nonempty")).toBe(trueConst());
});

it("ts_lift_plugin_initialize_capabilities_ir_version_starts_with_v", () => {
  expect(tsLiftSatisfiesRustContract("lift_plugin_initialize_capabilities_ir_version_starts_with_v")).toBe(trueConst());
  expect(tsLiftSatisfiesRustContract("lift_plugin_initialize_capabilities_ir_version_starts_with_v")).toBe(trueConst());
});

it("ts_lift_plugin_lift_request_surface_is_string", () => {
  expect(tsLiftSatisfiesRustContract("lift_plugin_lift_request_surface_is_string")).toBe(trueConst());
  expect(tsLiftSatisfiesRustContract("lift_plugin_lift_request_surface_is_string")).toBe(trueConst());
});

it("ts_lift_plugin_lift_request_source_paths_nonempty", () => {
  expect(tsLiftSatisfiesRustContract("lift_plugin_lift_request_source_paths_nonempty")).toBe(trueConst());
  expect(tsLiftSatisfiesRustContract("lift_plugin_lift_request_source_paths_nonempty")).toBe(trueConst());
});

it("ts_lift_plugin_lift_request_source_paths_each_nonempty", () => {
  expect(tsLiftSatisfiesRustContract("lift_plugin_lift_request_source_paths_each_nonempty")).toBe(trueConst());
  expect(tsLiftSatisfiesRustContract("lift_plugin_lift_request_source_paths_each_nonempty")).toBe(trueConst());
});

it("ts_lift_plugin_lift_request_surface_in_capabilities", () => {
  expect(tsLiftSatisfiesRustContract("lift_plugin_lift_request_surface_in_capabilities")).toBe(trueConst());
  expect(tsLiftSatisfiesRustContract("lift_plugin_lift_request_surface_in_capabilities")).toBe(trueConst());
});

it("ts_lift_plugin_lift_response_kind_in_set", () => {
  expect(tsLiftSatisfiesRustContract("lift_plugin_lift_response_kind_in_set")).toBe(trueConst());
  expect(tsLiftSatisfiesRustContract("lift_plugin_lift_response_kind_in_set")).toBe(trueConst());
});

it("ts_lift_plugin_lift_response_ir_document_array", () => {
  expect(tsLiftSatisfiesRustContract("lift_plugin_lift_response_ir_document_array")).toBe(trueConst());
  expect(tsLiftSatisfiesRustContract("lift_plugin_lift_response_ir_document_array")).toBe(trueConst());
});

it("ts_lift_plugin_diagnostic_field_is_array", () => {
  expect(tsLiftSatisfiesRustContract("lift_plugin_diagnostic_field_is_array")).toBe(trueConst());
  expect(tsLiftSatisfiesRustContract("lift_plugin_diagnostic_field_is_array")).toBe(trueConst());
});
