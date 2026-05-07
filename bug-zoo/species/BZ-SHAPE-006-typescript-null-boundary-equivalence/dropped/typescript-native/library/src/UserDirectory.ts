export function lookup(name: string | null | undefined): string {
  if (name == null) {
    throw new TypeError("name must be non-null");
  }
  return "user:" + name.toUpperCase();
}
