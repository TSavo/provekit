# CCL Meta-Layer Operations (`ccl-meta/1`)

**Status:** v1.0.0 locus contract for meta-layer concept construction.
**Date:** 2026-05-14
**Related:** `docs/papers/25-after-architectures-the-program-was-already-realized.md`, umbrella #893, phase umbrella #894, issues #911 and #912

## Purpose

CCL is Concept Composition Language: the substrate's term algebra recognized as a programming language. Paper 25 names that recognition directly in section 10 and states the consequence in section 11: every other language is sugar over CCL.

This locus introduces the first two meta-layer concept shapes. They let CCL describe its own operation vocabulary and then form applications of those operations as content-addressed terms.

## Op Definition

`concept:op-definition(name, arg_sorts, return_sort, effects, wp_rule)` describes a hub op definition. It is CCL's meta-layer primitive for naming an operator, its argument sorts, its result sort, its effect surface, and the weakest-precondition rule that governs applications.

| Formal | Meaning |
| --- | --- |
| `name` | Stable operation name, without requiring a language prefix. |
| `arg_sorts` | Ordered list of sort CIDs accepted by the operation. |
| `return_sort` | Sort CID returned by a well-formed application. |
| `effects` | Ordered list of effect names exposed by the operation. |
| `wp_rule` | Formula prose for the operation contract at v1.0. |

An op definition is well-formed iff every `arg_sort` and `return_sort` resolves to a minted sort CID, every effect name resolves to a minted `EffectName`, and `wp_rule` is parseable Formula prose at v1.0. Task #61 owns the later machine-checkable formula grammar. A well-formed definition returns the CID of the minted op-definition memento.

## Op Application

`concept:op-application(op_definition_cid, args)` describes an application of one minted op definition to term arguments.

| Formal | Meaning |
| --- | --- |
| `op_definition_cid` | CID of a minted `concept:op-definition` memento. |
| `args` | Ordered list of CCL terms supplied to that definition. |

An op application is well-formed iff `op_definition_cid` resolves to a minted op-definition, `args.length == op_definition.formals.length`, and each `args[i]` sort-checks against `op_definition.formal_sorts[i]`. A well-formed application returns the composition's content-addressed CCL term.

## Effect Inheritance

A `concept:op-application` whose `op_definition_cid` resolves to an op-definition with effects `E` inherits `E`.

At v1.0 this inheritance rule is encoded here as the locus contract. The generated `concept:op-application` shape keeps `effects = {"effects": []}` because the effect set is definition-dependent rather than intrinsic to the application constructor. A future spec extension will encode the inheritance as a `wp_rule` formula after task #61 lands.

## Sort Names

Bare sort names remain admitted at v1.0 for continuity with the current concept-shape catalog. A4 (#914) introduces content-addressed sort instances and retrofits these meta-layer shapes to require those sort instance CIDs everywhere a sort reference appears.

## Paper 25

Paper 25 section 10 is the recognition step: CCL is not another surface language bolted onto the substrate. It is the substrate's term algebra read as the language it already is.

Paper 25 section 11 is the consequence: every other language is sugar. Rust, Java, TypeScript, bytecode, and architecture-specific instruction sets are all renderings through kits or compilers. CCL is the unsugared algebra whose terms are already content-addressed, typed, and dischargeable.

The two concept shapes in this locus are therefore not an external API layered over CCL. They are the first self-describing operators in the language's own meta-layer: one operator mints operation definitions, and the other forms applications of those definitions.
