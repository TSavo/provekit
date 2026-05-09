-- Pattern: borrowed-pages-as-scratch (v2 — caller-aware)
-- nefariousplan catalog: /patterns/borrowed-pages-as-scratch
--
-- v1 was function-local: a candidate is flagged when no local protective
-- primitive (skb_cow_data / skb_unshare / skb_make_writable / shared-frag
-- check) appears in its body. That is correct as a function-level shape,
-- but it produces false positives where the protection lives in the
-- candidate's caller, either as a sibling that mutates the same skb
-- before the candidate runs (esp_output -> esp_output_head -> skb_cow_data
-- before esp_output -> esp_output_tail), or because the candidate's caller
-- constructed the skb itself with a kernel allocator (rxkad_send_response
-- alloc_skb_with_frags before rxkad_encrypt_response).
--
-- v2 adds a caller-aware mitigation step: walk the direct caller's
-- transitive callees up to depth 5; if the closure contains any of the
-- known unshare primitives or kernel-skb allocators, treat the candidate
-- as mitigated. The walk stops naturally at indirect calls (function-
-- pointer dispatch through an ops table is not represented as a call_edge),
-- which is exactly why rxkad_verify_packet_1 / _2 stay flagged: their
-- caller is the rxkad-internal dispatcher rxkad_verify_packet, and the
-- upstream rxrpc_input_call_event / rxrpc_verify_response unshare gates
-- live behind a function-pointer dispatch the call graph cannot see.

WITH RECURSIVE
inplace_skb_frag_receivers AS (
  SELECT DISTINCT
    caller_function,
    callsite_path
  FROM call_edges
  WHERE callee_name IN (
      'crypto_aead_decrypt',
      'crypto_aead_encrypt',
      'crypto_skcipher_decrypt',
      'crypto_skcipher_encrypt'
    )
    AND caller_function IN (
      SELECT caller_function FROM call_edges WHERE callee_name = 'skb_to_sgvec'
    )
),
candidate_parents AS (
  SELECT DISTINCT
    r.caller_function AS candidate,
    c.caller_function AS parent
  FROM inplace_skb_frag_receivers r
  JOIN call_edges c ON c.callee_name = r.caller_function
),
parent_descendants(parent, descendant, depth) AS (
  SELECT parent, parent, 0 FROM candidate_parents
  UNION
  SELECT pd.parent, c.callee_name, pd.depth + 1
  FROM parent_descendants pd
  JOIN call_edges c ON c.caller_function = pd.descendant
  WHERE pd.depth < 5
),
parents_with_mitigation AS (
  SELECT DISTINCT pd.parent
  FROM parent_descendants pd
  WHERE pd.descendant IN (
      'skb_cow_data',
      'skb_unshare',
      'skb_make_writable',
      'skb_check_shared_frag',
      'skb_has_shared_frag',
      'alloc_skb',
      '__alloc_skb',
      'alloc_skb_with_frags',
      'alloc_skb_for_msg',
      'alloc_skb_fclone'
    )
)
SELECT
  r.caller_function AS function,
  r.callsite_path   AS path
FROM inplace_skb_frag_receivers r
WHERE NOT EXISTS (
  -- Local mitigation: candidate calls an unshare primitive in its own body.
  SELECT 1 FROM call_edges g
  WHERE g.caller_function = r.caller_function
    AND g.callee_name IN (
      'skb_cow_data',
      'skb_unshare',
      'skb_make_writable',
      'skb_check_shared_frag',
      'skb_has_shared_frag'
    )
)
AND NOT EXISTS (
  -- Caller-aware mitigation: a direct caller's transitive call closure
  -- (bounded depth) contains an unshare primitive or kernel-skb allocator.
  SELECT 1
  FROM candidate_parents cp
  JOIN parents_with_mitigation pwm ON pwm.parent = cp.parent
  WHERE cp.candidate = r.caller_function
)
ORDER BY r.callsite_path, r.caller_function;
