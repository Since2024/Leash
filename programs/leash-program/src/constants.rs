use anchor_lang::prelude::*;

#[constant]
pub const CAPABILITY_SEED: &str = "capability";

#[constant]
pub const VAULT_SEED: &str = "vault";

/// Single shared PDA used as both the wrapped mint's mint authority (issue mints,
/// redeem burns don't need it, but a future re-mint might) and the vault's token-account
/// authority (redeem withdraws real USDC from the vault, signed by this PDA). One PDA
/// instead of two — nothing here needs them to be separate for the MVP.
#[constant]
pub const AUTHORITY_SEED: &str = "authority";

/// Bounded delegation depth for the MVP. See BUILD_PLAN.md §3/§4 — attenuate() and the
/// hook's ancestor-chain walk both rely on this being small and fixed at compile time
/// until D4 (extra-account-metas limits) is validated in the Week 1 spike.
pub const MAX_DEPTH: u8 = 3;

/// Flat allowlist size for the MVP. Deferred: merkle-proof allowlists (BUILD_PLAN.md §12)
/// once a flat Vec<Pubkey> stops being cheap to store/check.
pub const MAX_ALLOWLIST_LEN: usize = 10;
