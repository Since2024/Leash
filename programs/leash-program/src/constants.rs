use anchor_lang::prelude::*;

/// A Capability PDA is `[CAPABILITY_SEED, its own token account]` — *not* keyed on the
/// owner. Keying on the owner is what limited each owner to a single capability forever
/// (docs/ROADMAP.md 0.3): a second `issue` collided with the first PDA. The nonce that
/// makes several capabilities possible lives in the token account's seeds below, and the
/// capability inherits the distinction by being derived from that address.
///
/// This indirection is not stylistic. leash-hook must re-derive "the Capability for this
/// transfer" from **one fixed seed formula**, registered into the mint's
/// `ExtraAccountMetaList` at deployment and resolvable only from accounts the transfer
/// already carries — it cannot be handed a client-chosen nonce. The source token account
/// *is* one of those accounts (base account 0), so folding the nonce into its address
/// puts the disambiguator somewhere the hook can already reach.
#[constant]
pub const CAPABILITY_SEED: &str = "capability";

/// Seeds a capability's own wrapped-token account: `[TOKEN_ACCOUNT_SEED, owner, nonce]`,
/// nonce as little-endian `u64`. One per capability, rather than the single ATA per
/// (owner, mint) this used to rely on — an ATA is unique per owner, so it could not
/// represent more than one capability. See CAPABILITY_SEED above for why the nonce goes
/// here and not on the capability itself.
#[constant]
pub const TOKEN_ACCOUNT_SEED: &str = "capability-token";

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

/// Same value as `MAX_DEPTH`, as a `usize` for array sizing (`Capability.ancestors`).
/// Kept as a separate constant rather than casting `MAX_DEPTH` inline everywhere it's
/// needed for an array size.
pub const ANCESTOR_SLOTS: usize = MAX_DEPTH as usize;

/// Flat allowlist size for the MVP. Deferred: merkle-proof allowlists (BUILD_PLAN.md §12)
/// once a flat Vec<Pubkey> stops being cheap to store/check.
pub const MAX_ALLOWLIST_LEN: usize = 10;

/// leash-hook's program ID, as a string (not a `const Pubkey` — parsing a base58 string
/// isn't `const fn` in the solana-pubkey version this workspace pins, and chasing the
/// right `pubkey!` macro import path wasn't worth it for one constant). leash-hook owns
/// no Capability accounts (leash-program does), so it can't write `spent` directly —
/// Solana only lets the owning program mutate an account. `record_spend` is the CPI
/// entrypoint leash-hook calls instead, and this constant is how `record_spend` verifies
/// the caller really is leash-hook: it checks the provided authority PDA is
/// `find_program_address(["hook-authority"], HOOK_PROGRAM_ID)`, which only leash-hook
/// can sign for. Keep in sync with leash-hook's declare_id!.
pub const HOOK_PROGRAM_ID_STR: &str = "9WPQUY6zVRwVZ3eUsDF1aNESWAyZwL8GwKpzd2C66xtS";

/// Seed for leash-hook's own signing PDA (derived under HOOK_PROGRAM_ID, not this
/// program), used as the CPI caller's proof-of-identity in `record_spend`.
pub const HOOK_AUTHORITY_SEED: &str = "hook-authority";
