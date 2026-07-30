import { BN } from "@coral-xyz/anchor";
import {
  TOKEN_2022_PROGRAM_ID,
  TOKEN_PROGRAM_ID,
  getAssociatedTokenAddressSync,
} from "@solana/spl-token";
import { Keypair, PublicKey, SystemProgram } from "@solana/web3.js";
import type { Deployment } from "./deployment";
import { capabilityFor, programAuthorityPda, randomNonce } from "./pda";
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
  /** Distinguishes this capability from others the same principal holds. Random if
   * omitted; reusing one that is already taken fails on-chain rather than overwriting. */
  nonce?: bigint;
}

export interface IssueResult {
  capability: PublicKey;
  capabilityTokenAccount: PublicKey;
  /** The nonce actually used — worth keeping, since it is what re-derives the
   * capability's addresses later (docs/ROADMAP.md 0.6 tracks discovery without it). */
  nonce: bigint;
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
  const nonce = params.nonce ?? randomNonce();

  const principalDepositAccount = getAssociatedTokenAddressSync(
    deployment.depositAssetMint,
    principal.publicKey,
    false,
    TOKEN_PROGRAM_ID,
  );
  // The capability's token account is a program-derived account created by `issue`
  // itself, not an ATA — an ATA is unique per (owner, mint) and so could only ever back
  // one capability (docs/ROADMAP.md 0.3).
  const { capability, tokenAccount: capabilityTokenAccount } = capabilityFor(
    principal.publicKey,
    nonce,
    leashProgramId,
  );

  const signature = await leashProgram.methods
    .issue(
      new BN(nonce.toString()),
      new BN(budget.toString()),
      new BN(expiresAt),
      allow,
    )
    .accounts({
      principal: principal.publicKey,
      principalDepositAccount,
      vault: deployment.vault,
      wrappedMint: deployment.wrappedMint,
      programAuthority: programAuthorityPda(leashProgramId),
      capabilityTokenAccount,
      capability,
      tokenProgram: TOKEN_PROGRAM_ID,
      token2022Program: TOKEN_2022_PROGRAM_ID,
      systemProgram: SystemProgram.programId,
    } as never)
    .signers([principal])
    .rpc();

  return { capability, capabilityTokenAccount, nonce, signature };
}
