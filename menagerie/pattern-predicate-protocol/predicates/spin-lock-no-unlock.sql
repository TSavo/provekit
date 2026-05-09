-- Pattern: spin_lock without matching spin_unlock
--
-- Functions that acquire a spin lock but never call its release counterpart.
-- Function-level approximation: a function that calls spin_lock_* but has zero
-- calls to any spin_unlock_* sibling. False positives include functions that
-- deliberately leave the lock held for a caller; false negatives include
-- functions where the unlock is in a callee. Useful as a first pass.

WITH lock_callers AS (
  SELECT DISTINCT caller_function, callsite_path
  FROM call_edges
  WHERE callee_name IN (
      'spin_lock', 'spin_lock_bh', 'spin_lock_irq', 'spin_lock_irqsave',
      'spin_lock_nested'
    )
)
SELECT
  l.caller_function AS function,
  l.callsite_path   AS path
FROM lock_callers l
WHERE NOT EXISTS (
  SELECT 1 FROM call_edges g
  WHERE g.caller_function = l.caller_function
    AND g.callee_name IN (
      'spin_unlock', 'spin_unlock_bh', 'spin_unlock_irq', 'spin_unlock_irqrestore'
    )
)
ORDER BY l.callsite_path, l.caller_function;
