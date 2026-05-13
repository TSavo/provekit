-- Pattern: borrowed-pages-as-scratch
-- nefariousplan catalog: /patterns/borrowed-pages-as-scratch
--
-- A subsystem performs scratch writes into a destination buffer under an
-- internal contract that it owns the memory. Another subsystem supplies that
-- buffer with foreign-owned pages (page cache, mapped device, peer process).
-- The contract is documentation; the legitimate scratch becomes a write
-- primitive across a trust boundary nobody guards.
--
-- Substrate query: receivers that build a scatter-gather list from skb frags
-- and run in-place AEAD/skcipher decryption, without first calling any of the
-- known skb-data-unshare primitives. The set is the cluster of receivers in
-- the borrowed-pages class; the missing-edge functions are candidates.
--
-- Public instances at the time of writing:
--   esp_input  / esp6_input        (CVE-2026-43284, patched at f4c50a4034e6)
--   rxkad_verify_packet_1          (CVE-2026-43500, patch submitted, not yet
--                                    merged at time of this query)

WITH inplace_skb_frag_receivers AS (
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
      SELECT caller_function FROM call_edges
      WHERE callee_name = 'skb_to_sgvec'
    )
)
SELECT
  r.caller_function AS function,
  r.callsite_path   AS path
FROM inplace_skb_frag_receivers r
WHERE NOT EXISTS (
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
ORDER BY r.callsite_path, r.caller_function;
