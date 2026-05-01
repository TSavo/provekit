// SPDX-License-Identifier: Apache-2.0
//
// A small double-entry ledger. The intended invariant: every recorded
// transaction's debits sum to the same total as its credits. Lose
// that and you've lost money.

export interface Entry {
  account: string;
  debit: number;
  credit: number;
}

export interface Transaction {
  id: number;
  entries: Entry[];
}

/**
 * Sum of debits across a transaction's entries.
 */
export function sumDebits(txn: Transaction): number {
  return txn.entries.reduce((acc, e) => acc + e.debit, 0);
}

/**
 * Sum of credits across a transaction's entries.
 */
export function sumCredits(txn: Transaction): number {
  return txn.entries.reduce((acc, e) => acc + e.credit, 0);
}

/**
 * Apply a transaction to a balance map. The intended invariant is
 *
 *   forall txn. sumDebits(txn) === sumCredits(txn)
 *
 * captured by `provekit must doubleledger.ts "not lose money" --agent stub`.
 */
export function apply(
  balances: Record<string, number>,
  txn: Transaction,
): Record<string, number> {
  const next = { ...balances };
  for (const e of txn.entries) {
    next[e.account] = (next[e.account] ?? 0) + e.debit - e.credit;
  }
  return next;
}
