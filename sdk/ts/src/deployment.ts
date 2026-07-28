import {
  ExtensionType,
  TOKEN_2022_PROGRAM_ID,
  TOKEN_PROGRAM_ID,
  createInitializeAccount3Instruction,
  createInitializeMint2Instruction,
  createInitializeTransferHookInstruction,
  getMintLen,
} from "@solana/spl-token";
import {
  Connection,
  Keypair,
  PublicKey,
  SystemProgram,
  Transaction,
} from "@solana/web3.js";
import { extraAccountMetaListPda, programAuthorityPda } from "./pda";
import type { LeashPrograms } from "./programs";

/** `connection.sendTransaction` does not set `feePayer`/`recentBlockhash` for you —
 * omitting them doesn't error at compile time, it fails at simulation with a cryptic
 * "Attempt to debit an account but found no record of a prior credit" (the runtime
 * falls back to treating some other account as the fee payer). Confirmed by hitting
 * exactly that running the real CLI against a local validator, not by reading docs. */
async function sendAndConfirm(
  connection: Connection,
  tx: Transaction,
  feePayer: PublicKey,
  signers: Keypair[],
): Promise<string> {
  const { blockhash, lastValidBlockHeight } = await connection.getLatestBlockhash("confirmed");
  tx.recentBlockhash = blockhash;
  tx.feePayer = feePayer;
  tx.sign(...signers);
  const signature = await connection.sendRawTransaction(tx.serialize());
  await connection.confirmTransaction({ signature, blockhash, lastValidBlockHeight }, "confirmed");
  return signature;
}

/**
 * A configured Leash deployment: the vault (legacy SPL Token account holding the real
 * deposited asset) and the wrapped mint (Token-2022, TransferHook -> leash-hook).
 * `issue`/`attenuate`/`redeem` all operate against an already-configured deployment —
 * see docs/BUILD_PLAN.md §4's note that this instruction set deliberately does not
 * include an on-chain "initialize deployment" instruction. This function is that setup
 * step, just done client-side rather than as a program instruction (the same choice the
 * Week 1-4 Rust tests made).
 */
export interface Deployment {
  depositAssetMint: PublicKey;
  wrappedMint: PublicKey;
  vault: PublicKey;
  programAuthority: PublicKey;
}

/** Creates the vault and wrapped mint for a new deployment against an existing
 * deposit-asset mint (e.g. real USDC's mint address, or a devnet test mint). Requires
 * the payer to have enough SOL for the two new accounts' rent. */
export async function createDeployment(
  programs: LeashPrograms,
  payer: Keypair,
  depositAssetMint: PublicKey,
): Promise<Deployment> {
  const { connection, leashProgramId, leashHookId } = programs;
  const programAuthority = programAuthorityPda(leashProgramId);

  // --- Vault: legacy SPL Token account, owned by program_authority. ---
  const vaultKeypair = Keypair.generate();
  const vaultSpace = 165; // legacy SPL Token Account::LEN
  const vaultRent = await connection.getMinimumBalanceForRentExemption(vaultSpace);

  const vaultTx = new Transaction().add(
    SystemProgram.createAccount({
      fromPubkey: payer.publicKey,
      newAccountPubkey: vaultKeypair.publicKey,
      lamports: vaultRent,
      space: vaultSpace,
      programId: TOKEN_PROGRAM_ID,
    }),
    createInitializeAccount3Instruction(
      vaultKeypair.publicKey,
      depositAssetMint,
      programAuthority,
      TOKEN_PROGRAM_ID,
    ),
  );
  await sendAndConfirm(connection, vaultTx, payer.publicKey, [payer, vaultKeypair]);

  // --- Wrapped mint: Token-2022, TransferHook -> leash-hook. ---
  const wrappedMintKeypair = Keypair.generate();
  const wrappedMintSpace = getMintLen([ExtensionType.TransferHook]);
  const wrappedMintRent = await connection.getMinimumBalanceForRentExemption(wrappedMintSpace);

  const mintTx = new Transaction().add(
    SystemProgram.createAccount({
      fromPubkey: payer.publicKey,
      newAccountPubkey: wrappedMintKeypair.publicKey,
      lamports: wrappedMintRent,
      space: wrappedMintSpace,
      programId: TOKEN_2022_PROGRAM_ID,
    }),
    createInitializeTransferHookInstruction(
      wrappedMintKeypair.publicKey,
      programAuthority,
      leashHookId,
      TOKEN_2022_PROGRAM_ID,
    ),
    createInitializeMint2Instruction(
      wrappedMintKeypair.publicKey,
      6,
      programAuthority,
      null,
      TOKEN_2022_PROGRAM_ID,
    ),
  );
  await sendAndConfirm(connection, mintTx, payer.publicKey, [payer, wrappedMintKeypair]);

  // --- Register the real extra-account-meta list on leash-hook. ---
  const extraAccountMetaList = extraAccountMetaListPda(wrappedMintKeypair.publicKey, leashHookId);
  await programs.leashHook.methods
    .initializeExtraAccountMetaList()
    .accounts({
      payer: payer.publicKey,
      extraAccountMetaList,
      mint: wrappedMintKeypair.publicKey,
      systemProgram: SystemProgram.programId,
    } as never)
    .signers([payer])
    .rpc();

  return {
    depositAssetMint,
    wrappedMint: wrappedMintKeypair.publicKey,
    vault: vaultKeypair.publicKey,
    programAuthority,
  };
}
