// Must exactly match programs/leash-program/src/constants.rs and
// programs/leash-hook/src/lib.rs — these are the seed strings the on-chain programs use
// for PDA derivation. There is no single source of truth shared between Rust and
// TypeScript in this MVP; if one side changes, the other has to change by hand.

export const CAPABILITY_SEED = Buffer.from("capability");
export const TOKEN_ACCOUNT_SEED = Buffer.from("capability-token");
export const AUTHORITY_SEED = Buffer.from("authority");
/** `[VAULT_SEED, wrappedMint]` — one vault per wrapped mint (docs/ROADMAP.md 0.11). */
export const VAULT_SEED = Buffer.from("vault");
export const HOOK_AUTHORITY_SEED = Buffer.from("hook-authority");
export const EXTRA_ACCOUNT_METAS_SEED = Buffer.from("extra-account-metas");

/** Must match constants::MAX_DEPTH / ANCESTOR_SLOTS in leash-program. */
export const MAX_DEPTH = 3;

/** Must match constants::MAX_ALLOWLIST_LEN in leash-program. */
export const MAX_ALLOWLIST_LEN = 10;

/** Serialized size of a `Capability`, mirroring `Capability::MAX_SIZE` in `state.rs`.
 * Derived from the two constants above rather than hard-coded, so a layout change is a
 * type-level change here too.
 *
 * **Always filter `getProgramAccounts` on this**, not just on `owner`. A capability
 * written under an older layout still passes an owner-offset `memcmp`, and handing its
 * bytes to the Anchor decoder is not a recoverable error: the `allowlist` length prefix is
 * read at the wrong offset, lands inside `cap`/`expiry`, and yields a huge count that the
 * decoder tries to allocate — so the process dies of heap exhaustion *before* any
 * `try`/`catch` around the decode can run. That is not hypothetical; it killed `leash list`
 * against devnet the first time it met the two capabilities left over from the pre-0.12
 * deployment. */
export const CAPABILITY_ACCOUNT_SIZE =
  8 + // discriminator
  32 + // owner
  32 + // parent
  32 * MAX_DEPTH + // ancestors
  32 + // token_account
  32 + // wrapped_mint (docs/ROADMAP.md 0.12)
  8 * 4 + // cap, spent, committed_to_children, expiry
  (4 + 32 * MAX_ALLOWLIST_LEN) + // allowlist Vec<Pubkey>
  1 + // revoked
  1 + // depth
  1; // bump
