import { PublicKey } from "@solana/web3.js";
import {
  AUTHORITY_SEED,
  CAPABILITY_SEED,
  EXTRA_ACCOUNT_METAS_SEED,
  HOOK_AUTHORITY_SEED,
  TOKEN_ACCOUNT_SEED,
} from "./constants";

/** Encodes a nonce the way the on-chain seeds do: little-endian `u64`. */
function nonceLe(nonce: bigint | number): Buffer {
  const buf = Buffer.alloc(8);
  buf.writeBigUInt64LE(BigInt(nonce));
  return buf;
}

/** A capability's own wrapped-token account, at `[TOKEN_ACCOUNT_SEED, owner, nonce]`.
 * The nonce lives here rather than on the Capability itself so that leash-hook can
 * recover it from the transfer's source account — see `capabilityPda`. */
export function capabilityTokenAccountPda(
  owner: PublicKey,
  nonce: bigint | number,
  programId: PublicKey,
): PublicKey {
  return PublicKey.findProgramAddressSync(
    [TOKEN_ACCOUNT_SEED, owner.toBuffer(), nonceLe(nonce)],
    programId,
  )[0];
}

/** The Capability PDA for a given *token account* — not for an owner.
 *
 * An owner may hold any number of capabilities (docs/ROADMAP.md 0.3), so the owner alone
 * no longer identifies one. Keying on the capability's own token account keeps a single
 * derivation formula for roots and children alike, which is what lets leash-hook re-derive
 * the capability at transfer time from base account 0. */
export function capabilityPda(
  tokenAccount: PublicKey,
  programId: PublicKey,
): PublicKey {
  return PublicKey.findProgramAddressSync(
    [CAPABILITY_SEED, tokenAccount.toBuffer()],
    programId,
  )[0];
}

/** Both PDAs for an (owner, nonce) pair — the common case for callers that just issued
 * or attenuated and need to name the capability afterwards. */
export function capabilityFor(
  owner: PublicKey,
  nonce: bigint | number,
  programId: PublicKey,
): { capability: PublicKey; tokenAccount: PublicKey } {
  const tokenAccount = capabilityTokenAccountPda(owner, nonce, programId);
  return { capability: capabilityPda(tokenAccount, programId), tokenAccount };
}

/** A random `u64` nonce, for callers that don't want to manage their own. Collision
 * would surface as an `init` failure on an address already in use, not as silent reuse. */
export function randomNonce(): bigint {
  const bytes = new Uint8Array(8);
  globalThis.crypto.getRandomValues(bytes);
  return Buffer.from(bytes).readBigUInt64LE(0);
}

/** leash-program's shared PDA: both the wrapped mint's mint authority and the vault's
 * token-account authority. */
export function programAuthorityPda(programId: PublicKey): PublicKey {
  return PublicKey.findProgramAddressSync([AUTHORITY_SEED], programId)[0];
}

/** leash-hook's own signing PDA, used as the CPI-caller proof-of-identity in
 * leash-program's `record_spend`. */
export function hookAuthorityPda(hookProgramId: PublicKey): PublicKey {
  return PublicKey.findProgramAddressSync([HOOK_AUTHORITY_SEED], hookProgramId)[0];
}

/** The Transfer Hook Interface's required ExtraAccountMetaList PDA for a given mint. */
export function extraAccountMetaListPda(
  mint: PublicKey,
  hookProgramId: PublicKey,
): PublicKey {
  return PublicKey.findProgramAddressSync(
    [EXTRA_ACCOUNT_METAS_SEED, mint.toBuffer()],
    hookProgramId,
  )[0];
}
