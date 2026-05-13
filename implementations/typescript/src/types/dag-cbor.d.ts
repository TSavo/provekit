declare module "@ipld/dag-cbor" {
  export function encode(value: unknown): Uint8Array;
  export function decode(bytes: Uint8Array): unknown;
}
