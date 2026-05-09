-- Pattern: kmalloc/kzalloc without local kfree
--
-- Functions that allocate kernel memory but never free it locally. False
-- positives are extreme here (most allocs hand ownership to a caller), but
-- the set is interesting as a first lens onto where the allocation flow
-- crosses function boundaries.

WITH allocs AS (
  SELECT DISTINCT caller_function, callsite_path
  FROM call_edges
  WHERE callee_name IN (
      'kmalloc', 'kzalloc', 'kvmalloc', 'kvzalloc',
      'kmalloc_array', 'kcalloc', 'kmemdup', 'kstrdup'
    )
)
SELECT
  a.caller_function AS function,
  a.callsite_path   AS path
FROM allocs a
WHERE NOT EXISTS (
  SELECT 1 FROM call_edges g
  WHERE g.caller_function = a.caller_function
    AND g.callee_name IN ('kfree', 'kvfree', 'kfree_const')
)
ORDER BY a.callsite_path, a.caller_function;
