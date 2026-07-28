import { AnchorProvider, Program } from "@coral-xyz/anchor";
import { Connection, PublicKey } from "@solana/web3.js";
import type { LeashProgram } from "./idl";
import type { LeashHook } from "./idl";
import { leashHookIdl, leashProgramIdl } from "./idl";

export interface LeashPrograms {
  connection: Connection;
  provider: AnchorProvider;
  leashProgram: Program<LeashProgram>;
  leashHook: Program<LeashHook>;
  leashProgramId: PublicKey;
  leashHookId: PublicKey;
}

/** Builds typed Anchor `Program` clients for both leash-program and leash-hook from a
 * single `AnchorProvider`. Program IDs come from the IDL's own `address` field (set by
 * `declare_id!` on the Rust side), not passed separately, so there's one source of
 * truth. */
export function loadPrograms(provider: AnchorProvider): LeashPrograms {
  const leashProgram = new Program(leashProgramIdl as never, provider) as Program<LeashProgram>;
  const leashHook = new Program(leashHookIdl as never, provider) as Program<LeashHook>;
  return {
    connection: provider.connection,
    provider,
    leashProgram,
    leashHook,
    leashProgramId: leashProgram.programId,
    leashHookId: leashHook.programId,
  };
}
