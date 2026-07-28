// Re-exports of the Anchor-generated IDL + TypeScript types for both programs.
// Regenerate via `npm run generate-idl` (runs `anchor build` at the workspace root and
// copies the output here) whenever the Rust programs' instructions or account layouts
// change — these files are committed rather than gitignored precisely so the SDK is
// usable without requiring a full Anchor toolchain just to consume it.

import leashProgramIdlJson from "./leash_program.json";
import leashHookIdlJson from "./leash_hook.json";

export type { LeashProgram } from "./leash_program";
export type { LeashHook } from "./leash_hook";

export const leashProgramIdl = leashProgramIdlJson;
export const leashHookIdl = leashHookIdlJson;
