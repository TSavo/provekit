/**
 * Type-dialect brands. Zero runtime cost — the brand field exists only
 * in TypeScript's type system. Constructor functions carry the runtime
 * check that constitutes the proof for the brand.
 *
 * Note: `__brand` uses a unique symbol so two different packages cannot
 * accidentally unify branded types even if they spell the brand name
 * the same way.
 */

declare const __brand: unique symbol;

// ---------------------------------------------------------------------------
// Generic brand constructor
// ---------------------------------------------------------------------------

/**
 * The root branded-type constructor. Every other brand in this file
 * is a specialization of this type.
 *
 * A `Branded<T, BrandName>` is assignment-compatible with `T` but
 * not the other way around — tsserver enforces the brand at every
 * consumption site.
 */
export type Branded<T, BrandName extends string> = T & {
  readonly [__brand]: BrandName;
};

// ---------------------------------------------------------------------------
// Standard brands
// ---------------------------------------------------------------------------

/** A number or bigint that is guaranteed not to be zero. */
export type NonZero<T extends number | bigint> = Branded<T, "non-zero">;

/** An array guaranteed to have at least one element. */
export type NonEmpty<T> = T extends readonly (infer _U)[] ? Branded<T, "non-empty"> : never;

/** An array guaranteed to be in sorted order. */
export type Sorted<T> = T extends readonly (infer _U)[] ? Branded<T, "sorted"> : never;

/**
 * A value that is guaranteed not to be null or undefined.
 * The Exclude removes null/undefined from the type union, and the
 * brand makes the proof explicit to tsserver.
 */
export type NonNull<T> = Exclude<T, null | undefined> & Branded<T, "non-null">;

/** A value that has been validated against a schema. */
export type Validated<T, Schema> = Branded<T, "validated"> & {
  readonly __schema: Schema;
};

/** A value satisfying a named predicate (open-ended). */
export type Refined<T, Description extends string> = Branded<T, `refined:${Description}`>;

/** A numeric value in the inclusive range [lo, hi]. */
export type Range<T extends number | bigint, lo extends number, hi extends number> = Branded<
  T,
  `range:${lo}..${hi}`
>;

// ---------------------------------------------------------------------------
// Constructor functions
// ---------------------------------------------------------------------------

/**
 * Wrap `x` as NonZero if it is not zero; return null otherwise.
 * The null return is the constructor's way of surfacing the failure
 * without throwing — callers must handle the null case, which
 * forces them to acknowledge the possibility.
 */
export function nonZero<T extends number | bigint>(x: T): NonZero<T> | null {
  return x === 0 ? null : (x as NonZero<T>);
}

/**
 * Wrap `x` as NonZero or throw if it is zero.
 * Use this when a zero value is a programmer error, not a
 * recoverable condition.
 */
export function assertNonZero<T extends number | bigint>(x: T): NonZero<T> {
  if (x === 0) throw new Error("assertNonZero: expected non-zero value");
  return x as NonZero<T>;
}

/** Wrap `arr` as NonEmpty if it has at least one element; null otherwise. */
export function nonEmpty<T>(arr: T[]): NonEmpty<T[]> | null {
  return arr.length === 0 ? null : (arr as NonEmpty<T[]>);
}

/** Wrap `arr` as NonEmpty or throw. */
export function assertNonEmpty<T>(arr: T[]): NonEmpty<T[]> {
  if (arr.length === 0) throw new Error("assertNonEmpty: expected non-empty array");
  return arr as NonEmpty<T[]>;
}

/**
 * Wrap `arr` as Sorted. This is a trust-the-caller constructor —
 * runtime verification of sorted-ness is O(n) and not always desired.
 * Use `assertSorted` if you want the check.
 */
export function sorted<T>(arr: T[]): Sorted<T[]> {
  return arr as Sorted<T[]>;
}

/** Wrap `arr` as Sorted, verifying the ordering with a comparator. */
export function assertSorted<T>(
  arr: T[],
  comparator: (a: T, b: T) => number = (a, b) => (a < b ? -1 : a > b ? 1 : 0),
): Sorted<T[]> {
  for (let i = 1; i < arr.length; i++) {
    if (comparator(arr[i - 1], arr[i]) > 0) {
      throw new Error(`assertSorted: array is not sorted at index ${i}`);
    }
  }
  return arr as Sorted<T[]>;
}

/** Wrap `x` as NonNull if it is not null or undefined; null otherwise. */
export function nonNull<T>(x: T): NonNull<T> | null {
  return x == null ? null : (x as NonNull<T>);
}

/** Wrap `x` as NonNull or throw. */
export function assertNonNull<T>(x: T): NonNull<T> {
  if (x == null) throw new Error("assertNonNull: expected non-null value");
  return x as NonNull<T>;
}

/**
 * Trust-the-caller: mark `x` as Refined<T, Description>.
 * The proof that `x` satisfies Description is on the caller.
 */
export function refined<T, Description extends string>(
  x: T,
  _description: Description,
): Refined<T, Description> {
  return x as Refined<T, Description>;
}

/**
 * Trust-the-caller: mark `x` as Range<T, lo, hi>.
 * Does NOT do a runtime bounds check — use `assertRange` for that.
 */
export function range<T extends number | bigint, lo extends number, hi extends number>(
  x: T,
  _lo: lo,
  _hi: hi,
): Range<T, lo, hi> {
  return x as Range<T, lo, hi>;
}

/** Wrap `x` as Range or throw if out of bounds. */
export function assertRange<T extends number | bigint, lo extends number, hi extends number>(
  x: T,
  lo: lo,
  hi: hi,
): Range<T, lo, hi> {
  if (x < lo || x > hi) {
    throw new Error(`assertRange: ${String(x)} is not in [${lo}, ${hi}]`);
  }
  return x as Range<T, lo, hi>;
}
