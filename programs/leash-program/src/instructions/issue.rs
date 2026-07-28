use anchor_lang::prelude::*;
use anchor_spl::associated_token::spl_associated_token_account;
use anchor_spl::token::{self, Transfer};
use anchor_spl::token_2022::{self as token_2022, MintTo};

use crate::constants::{AUTHORITY_SEED, CAPABILITY_SEED, MAX_ALLOWLIST_LEN};
use crate::error::LeashError;
use crate::state::Capability;

/// Principal deposits `cap` real-USDC-like tokens into the vault and receives a root
/// Capability (parent = None, depth = 0, spent = 0, committed_to_children = 0,
/// revoked = false), plus `cap` units of leash-wrapped-USD minted to a fresh token
/// account they hold directly.
///
/// See BUILD_PLAN.md §4 "issue" and §5 D1/D3. The vault and `wrapped_mint` are created
/// once, off-chain/client-side (exactly as in the Week 1 spike test) — this instruction
/// operates against an already-configured deployment, it doesn't set one up.
#[derive(Accounts)]
#[instruction(cap: u64, expiry: i64, allowlist: Vec<Pubkey>)]
pub struct Issue<'info> {
    #[account(mut)]
    pub principal: Signer<'info>,

    /// CHECK: the real (legacy SPL Token) asset being deposited, e.g. USDC.
    #[account(mut)]
    pub principal_deposit_account: UncheckedAccount<'info>,

    /// CHECK: the program's vault (legacy SPL Token account), created once at deployment
    /// time. Its authority is `program_authority` below.
    #[account(mut)]
    pub vault: UncheckedAccount<'info>,

    /// CHECK: leash-wrapped-USD mint (Token-2022, TransferHook extension already
    /// configured at deployment time). Mutable because minting increases supply.
    #[account(mut)]
    pub wrapped_mint: UncheckedAccount<'info>,

    /// CHECK: PDA that is the wrapped mint's mint authority. Not read, only signs.
    #[account(seeds = [AUTHORITY_SEED.as_bytes()], bump)]
    pub program_authority: UncheckedAccount<'info>,

    #[account(
        init,
        payer = principal,
        space = Capability::MAX_SIZE,
        seeds = [CAPABILITY_SEED.as_bytes(), principal.key().as_ref()], // TODO: real seed scheme (nonce for multiple root caps per principal)
        bump,
    )]
    pub capability: Account<'info, Capability>,

    /// CHECK: fresh Token-2022 account holding this capability's wrapped balance,
    /// created here via CPI to the associated-token-account program. Owned by
    /// `principal` directly — the capability is a bearer object the holder controls,
    /// not something leash-program gates access to (BUILD_PLAN.md §0).
    #[account(mut)]
    pub capability_token_account: UncheckedAccount<'info>,

    /// CHECK: legacy SPL Token program, for the deposit transfer.
    pub token_program: UncheckedAccount<'info>,
    /// CHECK: Token-2022 program, for minting wrapped units.
    pub token_2022_program: UncheckedAccount<'info>,
    /// CHECK: associated-token-account program, for creating capability_token_account.
    pub associated_token_program: UncheckedAccount<'info>,
    pub system_program: Program<'info, System>,
}

pub fn issue_handler(
    ctx: Context<Issue>,
    cap: u64,
    expiry: i64,
    allowlist: Vec<Pubkey>,
) -> Result<()> {
    require!(
        allowlist.len() <= MAX_ALLOWLIST_LEN,
        LeashError::AllowlistTooLarge
    );

    // 1. Deposit: principal -> vault, in the real (legacy SPL Token) asset.
    token::transfer(
        CpiContext::new(
            ctx.accounts.token_program.key(),
            Transfer {
                from: ctx.accounts.principal_deposit_account.to_account_info(),
                to: ctx.accounts.vault.to_account_info(),
                authority: ctx.accounts.principal.to_account_info(),
            },
        ),
        cap,
    )?;

    // 2. Create the capability's wrapped-token account (ATA, owned by principal).
    anchor_lang::solana_program::program::invoke(
        &spl_associated_token_account::instruction::create_associated_token_account(
            ctx.accounts.principal.key,
            ctx.accounts.principal.key,
            ctx.accounts.wrapped_mint.key,
            ctx.accounts.token_2022_program.key,
        ),
        &[
            ctx.accounts.principal.to_account_info(),
            ctx.accounts.capability_token_account.to_account_info(),
            ctx.accounts.principal.to_account_info(),
            ctx.accounts.wrapped_mint.to_account_info(),
            ctx.accounts.system_program.to_account_info(),
            ctx.accounts.token_2022_program.to_account_info(),
        ],
    )?;

    // 3. Mint `cap` wrapped units to that fresh account, signed by the program's
    // mint-authority PDA.
    let bump = ctx.bumps.program_authority;
    let signer_seeds: &[&[u8]] = &[AUTHORITY_SEED.as_bytes(), &[bump]];
    token_2022::mint_to(
        CpiContext::new_with_signer(
            ctx.accounts.token_2022_program.key(),
            MintTo {
                mint: ctx.accounts.wrapped_mint.to_account_info(),
                to: ctx.accounts.capability_token_account.to_account_info(),
                authority: ctx.accounts.program_authority.to_account_info(),
            },
            &[signer_seeds],
        ),
        cap,
    )?;

    // 4. Initialize the root Capability.
    let capability = &mut ctx.accounts.capability;
    capability.owner = ctx.accounts.principal.key();
    capability.parent = None;
    capability.token_account = ctx.accounts.capability_token_account.key();
    capability.cap = cap;
    capability.spent = 0;
    capability.committed_to_children = 0;
    capability.expiry = expiry;
    capability.allowlist = allowlist;
    capability.revoked = false;
    capability.depth = 0;
    capability.bump = ctx.bumps.capability;

    Ok(())
}
