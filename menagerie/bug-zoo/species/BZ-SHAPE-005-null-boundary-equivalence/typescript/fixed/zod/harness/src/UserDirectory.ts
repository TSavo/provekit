// @ts-nocheck
import { z } from "zod";

export const LookupRequest = z.object({
  name: z.string(),
});

export function lookup(input: unknown): string {
  const request = LookupRequest.parse(input);
  return "user:" + request.name.toUpperCase();
}
