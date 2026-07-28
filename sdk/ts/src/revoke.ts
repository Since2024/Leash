import { Keypair, PublicKey } from "@solana/web3.js";
import type { LeashPrograms } from "./programs";

/** `revoke()` — flips `revoked = true`. No token movement. Takes effect on the very
 * next spend attempt against this capability or, up to MAX_DEPTH, any of its
 * descendants — because leash-hook walks the ancestor chain on every spend. */
export async function revoke(
  programs: LeashPrograms,
  owner: Keypair,
  capability: PublicKey,
): Promise<string> {
  return programs.leashProgram.methods
    .revoke()
    .accounts({
      owner: owner.publicKey,
      capability,
    } as never)
    .signers([owner])
    .rpc();
}
