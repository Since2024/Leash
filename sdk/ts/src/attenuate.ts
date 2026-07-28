import { BN } from "@coral-xyz/anchor";
import {
  ASSOCIATED_TOKEN_PROGRAM_ID,
  TOKEN_2022_PROGRAM_ID,
  getAssociatedTokenAddressSync,
} from "@solana/spl-token";
import { Keypair, PublicKey, SystemProgram } from "@solana/web3.js";
import type { Deployment } from "./deployment";
import { capabilityPda, programAuthorityPda } from "./pda";
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
}

export interface AttenuateResult {
  childCapability: PublicKey;
  childTokenAccount: PublicKey;
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

  const childCapability = capabilityPda(childOwner, leashProgramId);
  const childTokenAccount = getAssociatedTokenAddressSync(
    deployment.wrappedMint,
    childOwner,
    false,
    TOKEN_2022_PROGRAM_ID,
  );

  const signature = await leashProgram.methods
    .attenuate(new BN(childBudget.toString()), new BN(childExpiresAt), childAllow)
    .accounts({
      owner: owner.publicKey,
      parentCapability,
      childOwner,
      childCapability,
      wrappedMint: deployment.wrappedMint,
      programAuthority: programAuthorityPda(leashProgramId),
      childTokenAccount,
      token2022Program: TOKEN_2022_PROGRAM_ID,
      associatedTokenProgram: ASSOCIATED_TOKEN_PROGRAM_ID,
      systemProgram: SystemProgram.programId,
    } as never)
    .signers([owner])
    .rpc();

  return { childCapability, childTokenAccount, signature };
}
