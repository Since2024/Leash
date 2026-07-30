import { BN } from "@coral-xyz/anchor";
import { TOKEN_2022_PROGRAM_ID } from "@solana/spl-token";
import { Keypair, PublicKey, SystemProgram } from "@solana/web3.js";
import type { Deployment } from "./deployment";
import { capabilityFor, programAuthorityPda, randomNonce } from "./pda";
import type { LeashPrograms } from "./programs";

export interface AttenuateParams {
  /** Signer for the parent capability (must be its owner). */
  owner: Keypair;
  parentCapability: PublicKey;
  deployment: Deployment;
  /** Who the new, narrower capability is delegated to. Does not need to sign. */
  childOwner: PublicKey;
  childBudget: bigint;
  childExpiresAt: number;
  childAllow: PublicKey[];
  /** Distinguishes this delegation from others the same `childOwner` holds — which is
   * what lets one parent delegate to the same agent more than once. Random if omitted. */
  nonce?: bigint;
}

export interface AttenuateResult {
  childCapability: PublicKey;
  childTokenAccount: PublicKey;
  /** The nonce actually used — required to re-derive the child's addresses later. */
  nonce: bigint;
  signature: string;
}

/** `attenuate()` — mints a smaller, narrower, earlier-expiring child capability from a
 * parent, without exposing the parent's full budget. See BUILD_PLAN.md §4/§5 D3: this
 * mints fresh wrapped units to the child, it does not move the parent's existing
 * balance. */
export async function attenuate(
  programs: LeashPrograms,
  params: AttenuateParams,
): Promise<AttenuateResult> {
  const { owner, parentCapability, deployment, childOwner, childBudget, childExpiresAt, childAllow } = params;
  const { leashProgram, leashProgramId } = programs;
  const nonce = params.nonce ?? randomNonce();

  // Program-derived, not an ATA — see mint.ts for why, and note this is what makes a
  // second delegation to the same childOwner possible at all.
  const { capability: childCapability, tokenAccount: childTokenAccount } = capabilityFor(
    childOwner,
    nonce,
    leashProgramId,
  );

  const signature = await leashProgram.methods
    .attenuate(
      new BN(nonce.toString()),
      new BN(childBudget.toString()),
      new BN(childExpiresAt),
      childAllow,
    )
    .accounts({
      owner: owner.publicKey,
      parentCapability,
      childOwner,
      wrappedMint: deployment.wrappedMint,
      programAuthority: programAuthorityPda(leashProgramId),
      childTokenAccount,
      childCapability,
      token2022Program: TOKEN_2022_PROGRAM_ID,
      systemProgram: SystemProgram.programId,
    } as never)
    .signers([owner])
    .rpc();

  return { childCapability, childTokenAccount, nonce, signature };
}
