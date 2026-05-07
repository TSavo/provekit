// @ts-nocheck
import { IsString } from "class-validator";

export class LookupRequest {
  @IsString()
  name: string;
}

export function lookup(input: LookupRequest): string {
  if (input.name == null) {
    throw new TypeError("name must be non-null");
  }
  return "user:" + input.name.toUpperCase();
}
