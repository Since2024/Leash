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

/// Capability program: issue / attenuate / revoke / redeem. Spending is enforced
/// separately, inside the Token-2022 transfer itself — see the `leash-hook` program
/// and docs/BUILD_PLAN.md §4. This program never checks a spend; it only ever
/// mints, revokes, and redeems.
#[program]
pub mod leash_program {
    use super::*;

    pub fn issue(
        ctx: Context<Issue>,
        cap: u64,
        expiry: i64,
        allowlist: Vec<Pubkey>,
    ) -> Result<()> {
        instructions::issue::issue_handler(ctx, cap, expiry, allowlist)
    }

    pub fn attenuate(
        ctx: Context<Attenuate>,
        child_cap: u64,
        child_expiry: i64,
        child_allowlist: Vec<Pubkey>,
    ) -> Result<()> {
        instructions::attenuate::attenuate_handler(ctx, child_cap, child_expiry, child_allowlist)
    }

    pub fn revoke(ctx: Context<Revoke>) -> Result<()> {
        instructions::revoke::revoke_handler(ctx)
    }

    pub fn redeem(ctx: Context<Redeem>, amount: u64) -> Result<()> {
        instructions::redeem::redeem_handler(ctx, amount)
    }
}
