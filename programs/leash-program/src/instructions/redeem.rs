use anchor_lang::prelude::*;
use anchor_spl::token::{self, Transfer};
use anchor_spl::token_2022::{self as token_2022, Burn};

use crate::constants::AUTHORITY_SEED;

/// Anyone holding leash-wrapped-USD may redeem it 1:1 for real USDC from the vault.
/// A separate, explicit instruction — not something the transfer hook can trigger
/// itself, because Token-2022 transfer hooks receive source/destination accounts as
/// read-only during the hook call (see BUILD_PLAN.md §5 D2). This is what makes
/// accepting a Leash payment as good as accepting cash for a merchant.
#[derive(Accounts)]
pub struct Redeem<'info> {
    pub holder: Signer<'info>,

    /// CHECK: holder's Token-2022 account holding wrapped units — burned from here.
    #[account(mut)]
    pub holder_wrapped_account: UncheckedAccount<'info>,

    /// CHECK: leash-wrapped-USD mint. Mutable because burning reduces supply.
    #[account(mut)]
    pub wrapped_mint: UncheckedAccount<'info>,

    /// CHECK: program vault (legacy SPL Token, real USDC) — source of the withdrawal.
    #[account(mut)]
    pub vault: UncheckedAccount<'info>,

    /// CHECK: PDA authority over the vault. Same PDA `issue` uses as mint authority —
    /// one shared authority for the deployment (see constants::AUTHORITY_SEED).
    #[account(seeds = [AUTHORITY_SEED.as_bytes()], bump)]
    pub program_authority: UncheckedAccount<'info>,

    /// CHECK: holder's real-USDC account — destination of the withdrawal.
    #[account(mut)]
    pub holder_deposit_account: UncheckedAccount<'info>,

    /// CHECK: legacy SPL Token program, for the vault withdrawal.
    pub token_program: UncheckedAccount<'info>,
    /// CHECK: Token-2022 program, for the burn.
    pub token_2022_program: UncheckedAccount<'info>,
}

pub fn redeem_handler(ctx: Context<Redeem>, amount: u64) -> Result<()> {
    // 1. Burn `amount` wrapped units from the holder's account. The holder signs this
    // directly (they own the account) — no program authority needed to destroy value.
    token_2022::burn(
        CpiContext::new(
            ctx.accounts.token_2022_program.key(),
            Burn {
                mint: ctx.accounts.wrapped_mint.to_account_info(),
                from: ctx.accounts.holder_wrapped_account.to_account_info(),
                authority: ctx.accounts.holder.to_account_info(),
            },
        ),
        amount,
    )?;

    // 2. Withdraw `amount` real USDC from the vault to the holder, signed by the
    // program's authority PDA (the vault's real owner).
    let bump = ctx.bumps.program_authority;
    let signer_seeds: &[&[u8]] = &[AUTHORITY_SEED.as_bytes(), &[bump]];
    token::transfer(
        CpiContext::new_with_signer(
            ctx.accounts.token_program.key(),
            Transfer {
                from: ctx.accounts.vault.to_account_info(),
                to: ctx.accounts.holder_deposit_account.to_account_info(),
                authority: ctx.accounts.program_authority.to_account_info(),
            },
            &[signer_seeds],
        ),
        amount,
    )?;

    Ok(())
}
