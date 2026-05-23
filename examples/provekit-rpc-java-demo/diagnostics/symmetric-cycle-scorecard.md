# Substrate-symmetric cycle scorecard (no @substrate-term-shape sidecar)

Date: 2026-05-22 — after json! + match recognizers landed.

## Method
Strip @substrate-term-shape block comments from the substrate-emitted
java; lift via syntax-driven path only; lower back to rust; compare
to original.

## Result on libprovekit-rpc-cross-platform (10 @sugar functions):

| function | diff | status |
|---|---|---|
| ok_response       | +8b  | NEAR — single tail-vs-return wrapper diff |
| error_response    | +9b  | NEAR — same shape as ok_response |
| initialize_result | +328b | partial — nested json! reconstructs but not byte-identical |
| slot_cid          | -87b  | partial — match works, body reconstruction loses some detail |
| content_addressed_name | -259b | substantial — method chains still asymmetric |
| blake3_512_cid    | -261b | substantial — for-loop + indexing patterns missing |
| handle_line       | -1097b | major — nested matches + Result destructure not yet symmetric |
| build_ir_document | -812b  | major — struct destructure + iteration chains not symmetric |
| lift              | -959b  | major — ?-operator chains + method chains not symmetric |
| run_server        | -582b  | major — while-let + complex closure body not symmetric |

Strict byte-identical: 0/10
Loss/refuse records: 7/10 (rust lower bails on un-recognized java body shapes)

## Closed recognizers
- json! macro Supplier-closure → concept:macro-call(json, body)
- match expression scrut+if/else → concept:match(scrut, arm1, arm2)

## Pending recognizers (each closes a slice)
- return-statement-at-fn-tail → strip outer concept:return wrapper
  (closes ok_response, error_response — currently +8/+9b)
- iterator chains: Stream.filter().map().collect() → rust .iter().filter_map().collect()
- struct destructure: getter sequence → concept:destructure-struct
- Result method chains: instanceof Ok/Err + .unwrap() → concept:try
- while-let with __raw → concept:while-let with explicit pattern

## What the data says
The substrate-symmetric closure path is real and bounded. Each recognizer
closes a specific slice. The metadata-sidecar path (@substrate-term-shape)
remains the byte-identical-guaranteed cycle; the symmetric path is the
"no-sidecar" research direction.

For the demo cycle to work TODAY: use the sidecar. The symmetric closure
is incremental work — each recognizer is bounded, each pattern is named.
