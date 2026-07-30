// Must exactly match programs/leash-program/src/constants.rs and
// programs/leash-hook/src/lib.rs — these are the seed strings the on-chain programs use
// for PDA derivation. There is no single source of truth shared between Rust and
// TypeScript in this MVP; if one side changes, the other has to change by hand.

export const CAPABILITY_SEED = Buffer.from("capability");
export const TOKEN_ACCOUNT_SEED = Buffer.from("capability-token");
export const AUTHORITY_SEED = Buffer.from("authority");
export const HOOK_AUTHORITY_SEED = Buffer.from("hook-authority");
export const EXTRA_ACCOUNT_METAS_SEED = Buffer.from("extra-account-metas");

/** Must match constants::MAX_DEPTH / ANCESTOR_SLOTS in leash-program. */
export const MAX_DEPTH = 3;

/** Must match constants::MAX_ALLOWLIST_LEN in leash-program. */
export const MAX_ALLOWLIST_LEN = 10;
