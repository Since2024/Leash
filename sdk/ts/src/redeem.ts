import { BN } from "@coral-xyz/anchor";
import { TOKEN_2022_PROGRAM_ID, TOKEN_PROGRAM_ID } from "@solana/spl-token";
import { Keypair, PublicKey } from "@solana/web3.js";
import type { Deployment } from "./deployment";
import { capabilityPda, programAuthorityPda } from "./pda";
import type { LeashPrograms } from "./programs";

/** `redeem()` — not in BUILD_PLAN.md §6's original short SDK list (mint/attenuate/
 * spend/revoke/watch), but a real instruction merchants need: burns wrapped units and
 * withdraws the deployment's real asset back out 1:1. Included for completeness rather
 * than silently leaving a working instruction unreachable from the SDK.
 *
 * Who may redeem what is enforced on-chain, not here (docs/ROADMAP.md 0.1): a merchant
 * cashes out freely, a root capability's owner may unwind only its uncommitted budget,
 * and a delegated capability cannot redeem at all — it can only spend, through the hook.
 * The capability PDA is always passed, existing or not; the program checks on-chain
 * whether it exists rather than trusting the caller to declare it. It derives from
 * `holderWrappedAccount`, not from the holder (docs/ROADMAP.md 0.3) — so a merchant's
 * plain ATA simply yields an address with nothing at it, and a capability's own token
 * account yields exactly the capability those units are the budget of. */
export async function redeem(
  programs: LeashPrograms,
  holder: Keypair,
  deployment: Deployment,
  holderWrappedAccount: PublicKey,
  holderDepositAccount: PublicKey,
  amount: bigint,
): Promise<string> {
  return programs.leashProgram.methods
    .redeem(new BN(amount.toString()))
    .accounts({
      holder: holder.publicKey,
      holderWrappedAccount,
      capability: capabilityPda(holderWrappedAccount, programs.leashProgramId),
      wrappedMint: deployment.wrappedMint,
      vault: deployment.vault,
      programAuthority: programAuthorityPda(programs.leashProgramId),
      holderDepositAccount,
      tokenProgram: TOKEN_PROGRAM_ID,
      token2022Program: TOKEN_2022_PROGRAM_ID,
    } as never)
    .signers([holder])
    .rpc();
}
