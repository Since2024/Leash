import { PublicKey } from "@solana/web3.js";
import {
  AUTHORITY_SEED,
  CAPABILITY_SEED,
  EXTRA_ACCOUNT_METAS_SEED,
  HOOK_AUTHORITY_SEED,
} from "./constants";

/** The Capability PDA for a given owner. One owner, one active capability — see
 * attenuate.rs's doc comment on why the seed scheme is owner-only, not
 * [parent, owner]. */
export function capabilityPda(owner: PublicKey, programId: PublicKey): PublicKey {
  return PublicKey.findProgramAddressSync(
    [CAPABILITY_SEED, owner.toBuffer()],
    programId,
  )[0];
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
