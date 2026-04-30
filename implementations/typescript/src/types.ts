import { VerificationResult } from "./verifier";

export interface AnalysisResult {
  derivation: {
    filePath: string;
    callSite: { functionName: string; line: number; logText: string };
  };
  verifications: VerificationResult[];
}
