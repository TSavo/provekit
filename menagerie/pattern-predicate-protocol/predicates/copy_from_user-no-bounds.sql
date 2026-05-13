-- Pattern: copy_from_user without explicit bounds check
--
-- Kernel functions that consume userspace memory via copy_from_user must
-- ensure the destination buffer is sized for the requested length. The
-- canonical safety pattern is one of: a prior access_ok check, a clamp via
-- min/clamp, an arg-driven kmalloc, or a fixed-size struct copy where the
-- size is a compile-time constant.
--
-- Substrate query: callers of copy_from_user (or _copy_from_user) where the
-- function does not also call any of the bounds-checking helpers. Outliers
-- are candidates for a missing length validation; not every hit is a bug
-- (small fixed structs are fine), but the set is much smaller than the full
-- copy_from_user caller list.

WITH copy_callers AS (
  SELECT DISTINCT caller_function, callsite_path
  FROM call_edges
  WHERE callee_name IN ('copy_from_user','_copy_from_user','strncpy_from_user')
)
SELECT
  c.caller_function AS function,
  c.callsite_path   AS path
FROM copy_callers c
WHERE NOT EXISTS (
  SELECT 1 FROM call_edges g
  WHERE g.caller_function = c.caller_function
    AND g.callee_name IN (
      'access_ok',
      'min', 'min_t', 'umin',
      'clamp', 'clamp_t',
      'kmalloc', 'kzalloc', 'kvmalloc', 'kvzalloc',
      'memdup_user', 'strndup_user'
    )
)
ORDER BY c.callsite_path, c.caller_function;
