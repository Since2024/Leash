use anchor_lang::prelude::*;

use crate::constants::{ANCESTOR_SLOTS, MAX_ALLOWLIST_LEN};

/// Byte offset (within an account's raw data, discriminator included) where `parent`
/// begins. Fixed and load-bearing: leash-hook's extra-account-meta config reads this
/// field directly out of raw account bytes (via `PubkeyData::AccountData`) to locate a
/// capability's parent without deserializing the whole struct. See the doc comment on
/// `parent` below for why it can't be an `Option<Pubkey>`.
pub const PARENT_FIELD_OFFSET: u8 = 8 + 32; // discriminator (8) + owner (32)

/// Byte offset where the `ancestors` array begins. Also fixed and load-bearing — see
/// the doc comment on `ancestors` below for why leash-hook reads these directly out of
/// the *source* capability's own data instead of chaining through each ancestor's
/// `parent` field one hop at a time.
pub const ANCESTORS_FIELD_OFFSET: u8 = PARENT_FIELD_OFFSET + 32;

/// One node in a capability tree. A root capability has `parent = Pubkey::default()`; an
/// attenuated child has `parent = <parent Capability pubkey>`.
///
/// Invariant (checked by the program, not by convention — see BUILD_PLAN.md §2/§3):
///     spent + committed_to_children <= cap
#[account]
pub struct Capability {
    /// Signer who may attenuate or revoke this specific node.
    pub owner: Pubkey,
    /// `Pubkey::default()` (all-zero) for root capabilities, never a real Capability
    /// address otherwise. Deliberately NOT `Option<Pubkey>`: Borsh encodes `Option<T>`
    /// as a 1-byte tag plus 32 bytes only when `Some`, so its on-disk size — and every
    /// field after it — would shift depending on whether a capability has a parent.
    /// leash-hook's extra-account-meta resolution needs `parent` at a **fixed** byte
    /// offset to read it directly out of raw account data (see `PARENT_FIELD_OFFSET`),
    /// so it has to be a fixed-size `Pubkey`, not an `Option`. Found by working through
    /// the hook's account-resolution design, not by guessing — see BUILD_PLAN.md §5.
    pub parent: Pubkey,
    /// `[immediate parent, grandparent, great-grandparent]` (up to `ANCESTOR_SLOTS` =
    /// `MAX_DEPTH`), each `Pubkey::default()` past this capability's actual depth.
    /// Deliberately a **direct copy on this account**, not something leash-hook chains
    /// together by reading each ancestor's own `parent` field one hop at a time — that
    /// approach was tried first (Week 4) and broke: a root capability's `parent` is the
    /// System Program placeholder address, which has zero account data, so trying to
    /// read a "further parent" *out of* that placeholder's data fails at the
    /// client-side resolution step before a transaction is even submitted. Storing the
    /// whole chain here means every ancestor is read from *this* capability's own data
    /// (always real, always populated), never from a potentially-empty placeholder.
    /// Populated by `attenuate`: `[parent, parent.ancestors[0], parent.ancestors[1]]`.
    pub ancestors: [Pubkey; ANCESTOR_SLOTS],
    /// The Token-2022 token account holding this capability's spendable balance.
    pub token_account: Pubkey,
    /// The wrapped mint this capability was issued against — i.e. which deployment's
    /// collateral its units are a claim on.
    ///
    /// **Placed after `token_account`, and that is load-bearing.** leash-hook reads
    /// `parent` and `ancestors` straight out of raw account bytes at
    /// `PARENT_FIELD_OFFSET` / `ANCESTORS_FIELD_OFFSET`; a field inserted before them
    /// shifts both, and the hook would resolve its ancestor accounts from whatever bytes
    /// now sit at those offsets — silently, at transfer time, with no compile error.
    ///
    /// Added for docs/ROADMAP.md 0.12. Without it there was nothing to check `attenuate`'s
    /// `wrapped_mint` against: the mint authority for every deployment is the same
    /// `program_authority` PDA, and `TOKEN_ACCOUNT_SEED` contains no mint either, so a
    /// capability issued against a worthless mint could mint children of the *real* one.
    pub wrapped_mint: Pubkey,
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
    pub const MAX_SIZE: usize = 8 // discriminator
        + 32 // owner
        + 32 // parent (fixed-size Pubkey, see doc comment — not Option<Pubkey>)
        + (32 * ANCESTOR_SLOTS) // ancestors
        + 32 // token_account
        + 32 // wrapped_mint
        + 8  // cap
        + 8  // spent
        + 8  // committed_to_children
        + 8  // expiry
        + 4 + (32 * MAX_ALLOWLIST_LEN) // allowlist Vec<Pubkey>
        + 1  // revoked
        + 1  // depth
        + 1; // bump

    pub fn is_root(&self) -> bool {
        self.parent == Pubkey::default()
    }
}
