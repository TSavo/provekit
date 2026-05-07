// @ts-nocheck
import { IsString } from "class-validator";

export class LookupRequest {
  @IsString()
  name: string;
}

export function lookup(input: LookupRequest): string {
  return "user:" + input.name.toUpperCase();
}
