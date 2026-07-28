use anchor_lang::prelude::*;

use crate::state::Capability;
use crate::error::LeashError;

/// Flips `revoked = true`. No token movement. Takes effect on the very next spend
/// attempt against this capability or any descendant, because the hook walks the
/// ancestor chain on every spend (see leash-hook and BUILD_PLAN.md §5 D4).
#[derive(Accounts)]
pub struct Revoke<'info> {
    pub owner: Signer<'info>,

    #[account(
        mut,
        has_one = owner @ LeashError::Unauthorized,
    )]
    pub capability: Account<'info, Capability>,
}

pub fn revoke_handler(ctx: Context<Revoke>) -> Result<()> {
    ctx.accounts.capability.revoked = true;
    Ok(())
}
