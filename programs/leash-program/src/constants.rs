use anchor_lang::prelude::*;

#[constant]
pub const CAPABILITY_SEED: &str = "capability";

#[constant]
pub const VAULT_SEED: &str = "vault";

/// Bounded delegation depth for the MVP. See BUILD_PLAN.md §3/§4 — attenuate() and the
/// hook's ancestor-chain walk both rely on this being small and fixed at compile time
/// until D4 (extra-account-metas limits) is validated in the Week 1 spike.
pub const MAX_DEPTH: u8 = 3;

/// Flat allowlist size for the MVP. Deferred: merkle-proof allowlists (BUILD_PLAN.md §12)
/// once a flat Vec<Pubkey> stops being cheap to store/check.
pub const MAX_ALLOWLIST_LEN: usize = 10;
