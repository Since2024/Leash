use anchor_lang::prelude::*;
use anchor_spl::token::{self, Transfer};
use anchor_spl::token_2022::{self as token_2022, MintTo};
use anchor_spl::token_interface::{Mint, TokenAccount};

use crate::constants::{AUTHORITY_SEED, CAPABILITY_SEED, MAX_ALLOWLIST_LEN, TOKEN_ACCOUNT_SEED};
use crate::error::LeashError;
use crate::state::Capability;

/// Principal deposits `cap` real-USDC-like tokens into the vault and receives a root
/// Capability (parent = Pubkey::default(), depth = 0, spent = 0, committed_to_children = 0,
/// revoked = false), plus `cap` units of leash-wrapped-USD minted to a fresh token
/// account they hold directly.
///
/// See BUILD_PLAN.md §4 "issue" and §5 D1/D3. The vault and `wrapped_mint` are created
/// once, off-chain/client-side (exactly as in the Week 1 spike test) — this instruction
/// operates against an already-configured deployment, it doesn't set one up.
#[derive(Accounts)]
#[instruction(nonce: u64, cap: u64, expiry: i64, allowlist: Vec<Pubkey>)]
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

    /// leash-wrapped-USD mint (Token-2022, TransferHook extension already configured at
    /// deployment time). Mutable because minting increases supply. Typed rather than
    /// unchecked so Anchor can read its extensions to size the token account below.
    #[account(mut)]
    pub wrapped_mint: InterfaceAccount<'info, Mint>,

    /// CHECK: PDA that is the wrapped mint's mint authority. Not read, only signs.
    #[account(seeds = [AUTHORITY_SEED.as_bytes()], bump)]
    pub program_authority: UncheckedAccount<'info>,

    /// This capability's own wrapped-token account, at `[TOKEN_ACCOUNT_SEED, principal,
    /// nonce]`. Declared *before* `capability` because the capability's seeds reference
    /// this account's key, and Anchor resolves fields in declaration order.
    ///
    /// It used to be the principal's associated token account, created by hand via a CPI
    /// to the ATA program. An ATA is unique per (owner, mint), so it could only ever
    /// represent one capability — the root of docs/ROADMAP.md 0.3. Anchor's `init` +
    /// `token::*` constraints replace that CPI outright and size the account for the
    /// mint's TransferHook extension automatically.
    ///
    /// `token::authority = principal` keeps the bearer-object model from BUILD_PLAN.md §0
    /// intact: the holder controls the account directly, leash-program does not gate
    /// access to it. Only the *address* is program-derived, not the authority.
    #[account(
        init,
        payer = principal,
        seeds = [TOKEN_ACCOUNT_SEED.as_bytes(), principal.key().as_ref(), &nonce.to_le_bytes()],
        bump,
        token::mint = wrapped_mint,
        token::authority = principal,
        token::token_program = token_2022_program,
    )]
    pub capability_token_account: InterfaceAccount<'info, TokenAccount>,

    #[account(
        init,
        payer = principal,
        space = Capability::MAX_SIZE,
        seeds = [CAPABILITY_SEED.as_bytes(), capability_token_account.key().as_ref()],
        bump,
    )]
    pub capability: Account<'info, Capability>,

    /// CHECK: legacy SPL Token program, for the deposit transfer.
    pub token_program: UncheckedAccount<'info>,
    /// CHECK: Token-2022 program, for minting wrapped units.
    pub token_2022_program: UncheckedAccount<'info>,
    pub system_program: Program<'info, System>,
}

pub fn issue_handler(
    ctx: Context<Issue>,
    _nonce: u64,
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

    // 2. The capability's wrapped-token account is created by Anchor's `init` +
    // `token::*` constraints above, before this handler runs — no manual CPI needed.

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
    capability.parent = Pubkey::default(); // root: no parent (state.rs — not Option<Pubkey>)
    capability.ancestors = [Pubkey::default(); crate::constants::ANCESTOR_SLOTS]; // root: no ancestors
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
