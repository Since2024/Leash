import { BN } from "@coral-xyz/anchor";
import {
  ASSOCIATED_TOKEN_PROGRAM_ID,
  TOKEN_2022_PROGRAM_ID,
  TOKEN_PROGRAM_ID,
  getAssociatedTokenAddressSync,
} from "@solana/spl-token";
import { Keypair, PublicKey, SystemProgram } from "@solana/web3.js";
import type { Deployment } from "./deployment";
import { capabilityPda, programAuthorityPda } from "./pda";
import type { LeashPrograms } from "./programs";

export interface IssueParams {
  principal: Keypair;
  deployment: Deployment;
  /** Amount of the deposit asset to lock up, and the resulting capability's `cap`. */
  budget: bigint;
  /** Unix seconds after which the capability can no longer spend. */
  expiresAt: number;
  /** Wrapped-mint token accounts this capability may pay. */
  allow: PublicKey[];
}

export interface IssueResult {
  capability: PublicKey;
  capabilityTokenAccount: PublicKey;
  signature: string;
}

/**
 * `mint()` — the SDK's name for the `issue` instruction (matches the CLI's `leash mint`
 * and BUILD_PLAN.md §6's planned SDK surface: `mint(), attenuate(), spend(), revoke(),
 * watch()`). Deposits `budget` of the deployment's real asset from the principal's own
 * token account into the vault, and returns a root Capability.
 */
export async function mint(
  programs: LeashPrograms,
  params: IssueParams,
): Promise<IssueResult> {
  const { principal, deployment, budget, expiresAt, allow } = params;
  const { leashProgram, leashProgramId } = programs;

  const principalDepositAccount = getAssociatedTokenAddressSync(
    deployment.depositAssetMint,
    principal.publicKey,
    false,
    TOKEN_PROGRAM_ID,
  );
  const capability = capabilityPda(principal.publicKey, leashProgramId);
  const capabilityTokenAccount = getAssociatedTokenAddressSync(
    deployment.wrappedMint,
    principal.publicKey,
    false,
    TOKEN_2022_PROGRAM_ID,
  );

  const signature = await leashProgram.methods
    .issue(new BN(budget.toString()), new BN(expiresAt), allow)
    .accounts({
      principal: principal.publicKey,
      principalDepositAccount,
      vault: deployment.vault,
      wrappedMint: deployment.wrappedMint,
      programAuthority: programAuthorityPda(leashProgramId),
      capability,
      capabilityTokenAccount,
      tokenProgram: TOKEN_PROGRAM_ID,
      token2022Program: TOKEN_2022_PROGRAM_ID,
      associatedTokenProgram: ASSOCIATED_TOKEN_PROGRAM_ID,
      systemProgram: SystemProgram.programId,
    } as never)
    .signers([principal])
    .rpc();

  return { capability, capabilityTokenAccount, signature };
}
