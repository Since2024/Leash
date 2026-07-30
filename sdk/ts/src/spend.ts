import {
  TOKEN_2022_PROGRAM_ID,
  addExtraAccountMetasForExecute,
  createTransferCheckedInstruction,
} from "@solana/spl-token";
import { Keypair, PublicKey, Transaction } from "@solana/web3.js";
import type { Deployment } from "./deployment";
import type { LeashPrograms } from "./programs";

/**
 * `spend()` — a normal Token-2022 `transfer_checked` of the wrapped asset, with the
 * Transfer Hook Interface's extra accounts resolved and attached so leash-hook actually
 * gets invoked. This is the whole point: enforcement happens inside this transfer, at
 * the token-program level, not in this SDK function. If cap/expiry/allowlist/revoked
 * (or any ancestor's revoked, up to MAX_DEPTH) fails, the transaction fails — `spend()`
 * does not pre-check any of that itself.
 *
 * `source` is the capability's own token account, and it is explicit rather than derived
 * from `sourceOwner`: an owner may hold several capabilities (docs/ROADMAP.md 0.3), so
 * the owner alone no longer names one. Use `capabilityFor(owner, nonce, programId)` or
 * the `capabilityTokenAccount` returned by `mint`/`attenuate`. Which capability is
 * charged follows from this account — leash-hook re-derives it from exactly this address.
 */
export async function spend(
  programs: LeashPrograms,
  deployment: Deployment,
  sourceOwner: Keypair,
  source: PublicKey,
  destination: PublicKey,
  amount: bigint,
  decimals = 6,
): Promise<string> {
  const { connection, leashHookId } = programs;

  const transferIx = createTransferCheckedInstruction(
    source,
    deployment.wrappedMint,
    destination,
    sourceOwner.publicKey,
    amount,
    decimals,
    [],
    TOKEN_2022_PROGRAM_ID,
  );

  await addExtraAccountMetasForExecute(
    connection,
    transferIx,
    leashHookId,
    source,
    deployment.wrappedMint,
    destination,
    sourceOwner.publicKey,
    amount,
    "confirmed",
  );

  // `connection.sendTransaction` does not set feePayer/recentBlockhash itself — see the
  // matching note in deployment.ts's `sendAndConfirm` for what happens if you skip this.
  const tx = new Transaction().add(transferIx);
  const { blockhash, lastValidBlockHeight } = await connection.getLatestBlockhash("confirmed");
  tx.recentBlockhash = blockhash;
  tx.feePayer = sourceOwner.publicKey;
  tx.sign(sourceOwner);
  const signature = await connection.sendRawTransaction(tx.serialize());
  await connection.confirmTransaction({ signature, blockhash, lastValidBlockHeight }, "confirmed");
  return signature;
}
