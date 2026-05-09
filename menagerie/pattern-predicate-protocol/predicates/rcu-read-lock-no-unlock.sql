-- Pattern: rcu_read_lock without matching rcu_read_unlock
--
-- A read-side critical section that's opened but never closed at the same
-- function level. Same false-positive caveats as the spin lock variant.

WITH rcu_locks AS (
  SELECT DISTINCT caller_function, callsite_path
  FROM call_edges
  WHERE callee_name IN ('rcu_read_lock', 'rcu_read_lock_bh', 'rcu_read_lock_sched')
)
SELECT
  r.caller_function AS function,
  r.callsite_path   AS path
FROM rcu_locks r
WHERE NOT EXISTS (
  SELECT 1 FROM call_edges g
  WHERE g.caller_function = r.caller_function
    AND g.callee_name IN (
      'rcu_read_unlock', 'rcu_read_unlock_bh', 'rcu_read_unlock_sched'
    )
)
ORDER BY r.callsite_path, r.caller_function;
