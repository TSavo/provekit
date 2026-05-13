// @ts-nocheck
import { z } from "zod";

export const LookupRequest = z.object({
  name: z.string(),
});

export function lookup(input: { name: string }): string {
  return "user:" + input.name.toUpperCase();
}
