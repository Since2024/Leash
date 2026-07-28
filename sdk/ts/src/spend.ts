import {
  TOKEN_2022_PROGRAM_ID,
  addExtraAccountMetasForExecute,
  createTransferCheckedInstruction,
  getAssociatedTokenAddressSync,
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
 */
export async function spend(
  programs: LeashPrograms,
  deployment: Deployment,
  sourceOwner: Keypair,
  destination: PublicKey,
  amount: bigint,
  decimals = 6,
): Promise<string> {
  const { connection, leashHookId } = programs;
  const sourceAta = getAssociatedTokenAddressSync(
    deployment.wrappedMint,
    sourceOwner.publicKey,
    false,
    TOKEN_2022_PROGRAM_ID,
  );

  const transferIx = createTransferCheckedInstruction(
    sourceAta,
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
    sourceAta,
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
