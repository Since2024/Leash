use anchor_lang::prelude::*;

/// Anyone holding leash-wrapped-USD may redeem it 1:1 for real USDC from the vault.
/// A separate, explicit instruction — not something the transfer hook can trigger
/// itself, because Token-2022 transfer hooks receive source/destination accounts as
/// read-only during the hook call (see BUILD_PLAN.md §5 D2). This is what makes
/// accepting a Leash payment as good as accepting cash for a merchant.
#[derive(Accounts)]
pub struct Redeem<'info> {
    #[account(mut)]
    pub holder: Signer<'info>,

    // TODO(week 2, pending D1/D2): holder's wrapped-token account (to burn from),
    // vault token account (real USDC, to withdraw from), mint, token_program.
    pub system_program: Program<'info, System>,
}

pub fn redeem_handler(_ctx: Context<Redeem>, _amount: u64) -> Result<()> {
    // TODO(week 2): burn `amount` wrapped units from holder's token account, transfer
    // `amount` real USDC from the vault to holder.
    todo!("redeem: see docs/BUILD_PLAN.md §4 and §5 (D2) before implementing")
}
