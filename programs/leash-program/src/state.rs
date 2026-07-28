use anchor_lang::prelude::*;

use crate::constants::MAX_ALLOWLIST_LEN;

/// One node in a capability tree. A root capability has `parent = None`; an attenuated
/// child has `parent = Some(<parent Capability pubkey>)`.
///
/// Invariant (checked by the program, not by convention — see BUILD_PLAN.md §2/§3):
///     spent + committed_to_children <= cap
///
/// TODO(week 2): finalize space calculation once allowlist storage strategy (flat Vec vs.
/// merkle root) is settled by the D1-D4 spike; MAX_ALLOWLIST_LEN keeps this MVP-sized.
#[account]
pub struct Capability {
    /// Signer who may attenuate or revoke this specific node.
    pub owner: Pubkey,
    /// None for root capabilities.
    pub parent: Option<Pubkey>,
    /// The Token-2022 token account holding this capability's spendable balance.
    pub token_account: Pubkey,
    /// Total this capability may ever spend (cumulative, not a rolling window).
    pub cap: u64,
    /// Cumulative amount spent so far via the transfer hook's spend path.
    pub spent: u64,
    /// Sum of `cap` handed to attenuated children. Not spendable by this node directly.
    pub committed_to_children: u64,
    /// Unix timestamp. No spend may execute after this.
    pub expiry: i64,
    /// Flat allowlist of destinations this capability (and, for now, its children) may
    /// pay. MVP: equality-or-subset check only, no arbitrary narrowing logic yet.
    pub allowlist: Vec<Pubkey>, // max MAX_ALLOWLIST_LEN entries — enforced at issue/attenuate time
    /// Set by `revoke`. The hook must also check every ancestor's flag, up to MAX_DEPTH.
    pub revoked: bool,
    /// 0 for a root capability; capped at MAX_DEPTH.
    pub depth: u8,
    pub bump: u8,
}

impl Capability {
    // Anchor account discriminator (8) + fields. Recomputed once allowlist storage is final.
    pub const MAX_SIZE: usize = 8 // discriminator
        + 32 // owner
        + 1 + 32 // parent (Option<Pubkey>)
        + 32 // token_account
        + 8  // cap
        + 8  // spent
        + 8  // committed_to_children
        + 8  // expiry
        + 4 + (32 * MAX_ALLOWLIST_LEN) // allowlist Vec<Pubkey>
        + 1  // revoked
        + 1  // depth
        + 1; // bump
}
