pub mod constants;
pub mod error;
pub mod instructions;
pub mod state;

use anchor_lang::prelude::*;

pub use constants::*;
pub use error::*;
pub use instructions::*;
pub use state::*;

declare_id!("Gbx7nEL2rxWUTj7LnqRQtBDU7yi8oF3miYmjKGncsDXk");

/// Capability program: issue / attenuate / revoke / redeem, plus `record_spend` — the
/// CPI entrypoint leash-hook calls to commit a spend it has already validated. The
/// actual cap/expiry/allowlist/revoked checks happen in leash-hook, inside the
/// Token-2022 transfer itself (docs/BUILD_PLAN.md §4); this program owns the Capability
/// accounts and is the only one allowed to write `spent`.
#[program]
pub mod leash_program {
    use super::*;

    /// Creates the single vault backing `wrapped_mint`, at `[VAULT_SEED, wrapped_mint]`.
    /// Must be called once per deployment, in the same transaction that creates the mint —
    /// see initialize_vault.rs. docs/ROADMAP.md 0.11.
    pub fn initialize_vault(ctx: Context<InitializeVault>) -> Result<()> {
        instructions::initialize_vault::initialize_vault_handler(ctx)
    }

    /// `nonce` distinguishes this capability's token account from any other the same
    /// principal holds, and so distinguishes the capability itself — see
    /// constants::CAPABILITY_SEED. Callers who don't care may pass any value they
    /// haven't used before; the SDK generates a random `u64`.
    pub fn issue(
        ctx: Context<Issue>,
        nonce: u64,
        cap: u64,
        expiry: i64,
        allowlist: Vec<Pubkey>,
    ) -> Result<()> {
        instructions::issue::issue_handler(ctx, nonce, cap, expiry, allowlist)
    }

    /// `nonce` plays the same role as in `issue`, scoped to `child_owner` — which is what
    /// lets one parent delegate to the same owner more than once.
    pub fn attenuate(
        ctx: Context<Attenuate>,
        nonce: u64,
        child_cap: u64,
        child_expiry: i64,
        child_allowlist: Vec<Pubkey>,
    ) -> Result<()> {
        instructions::attenuate::attenuate_handler(
            ctx,
            nonce,
            child_cap,
            child_expiry,
            child_allowlist,
        )
    }

    pub fn revoke(ctx: Context<Revoke>) -> Result<()> {
        instructions::revoke::revoke_handler(ctx)
    }

    /// Revoke a capability below you in the tree. Authority is proved by the target's own
    /// `ancestors` array — the same one leash-hook already gates spends on. See
    /// docs/ROADMAP.md 0.8.
    pub fn revoke_descendant(ctx: Context<RevokeDescendant>) -> Result<()> {
        instructions::revoke_descendant::revoke_descendant_handler(ctx)
    }

    /// Release budget reserved for a child that can no longer spend it, so the parent can
    /// use or redeem it again. Accounting only — see reclaim.rs for why the child's units
    /// cannot be burned. docs/ROADMAP.md 0.7.
    pub fn reclaim(ctx: Context<Reclaim>) -> Result<()> {
        instructions::reclaim::reclaim_handler(ctx)
    }

    pub fn redeem(ctx: Context<Redeem>, amount: u64) -> Result<()> {
        instructions::redeem::redeem_handler(ctx, amount)
    }

    pub fn record_spend(ctx: Context<RecordSpend>, amount: u64) -> Result<()> {
        instructions::record_spend::record_spend_handler(ctx, amount)
    }
}
