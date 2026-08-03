use anchor_lang::prelude::*;
use anchor_spl::token::{self, Token, TokenAccount as SplTokenAccount, Transfer};
use anchor_spl::token_2022::{self as token_2022, MintTo, Token2022};
use anchor_spl::token_interface::{Mint, TokenAccount};

use crate::constants::{
    AUTHORITY_SEED, CAPABILITY_SEED, MAX_ALLOWLIST_LEN, TOKEN_ACCOUNT_SEED, VAULT_SEED,
};
use crate::error::LeashError;
use crate::state::Capability;

/// Principal deposits `cap` real-USDC-like tokens into the vault and receives a root
/// Capability (parent = Pubkey::default(), depth = 0, spent = 0, committed_to_children = 0,
/// revoked = false), plus `cap` units of leash-wrapped-USD minted to a fresh token
/// account they hold directly.
///
/// See BUILD_PLAN.md §4 "issue" and §5 D1/D3. This instruction operates against an
/// already-configured deployment; it doesn't set one up. `wrapped_mint` is still created
/// client-side, but the **vault is not** — it is a PDA at `[VAULT_SEED, wrapped_mint]`
/// created by `initialize_vault`. That changed with docs/ROADMAP.md 0.11: a
/// client-created vault is one the caller can substitute, and substituting it was worth
/// the entire deposit.
#[derive(Accounts)]
#[instruction(nonce: u64, cap: u64, expiry: i64, allowlist: Vec<Pubkey>)]
pub struct Issue<'info> {
    #[account(mut)]
    pub principal: Signer<'info>,

    /// CHECK: the real (legacy SPL Token) asset being deposited, e.g. USDC.
    #[account(mut)]
    pub principal_deposit_account: UncheckedAccount<'info>,

    /// leash-wrapped-USD mint (Token-2022, TransferHook extension already configured at
    /// deployment time). Mutable because minting increases supply. Typed rather than
    /// unchecked so Anchor can read its extensions to size the token account below.
    ///
    /// Declared *before* `vault` because the vault's seeds reference it, and Anchor
    /// resolves fields in declaration order. That ordering is the fix for
    /// docs/ROADMAP.md 0.11 — see the vault below.
    #[account(mut)]
    pub wrapped_mint: InterfaceAccount<'info, Mint>,

    /// The program's vault: the legacy SPL Token account holding the real deposited
    /// asset, created once per wrapped mint by `initialize_vault`.
    ///
    /// The `seeds` constraint is load-bearing and was absent (docs/ROADMAP.md 0.11). As a
    /// bare `UncheckedAccount` this took whatever vault the caller named, so a caller
    /// could deposit into an account they owned and still be minted genuine wrapped units
    /// against the real mint — a fully-backed-looking capability with nothing behind it,
    /// redeemable from the real vault by the ordinary path. Deriving the vault from
    /// `wrapped_mint` makes the deposit and the mint provably part of the same
    /// deployment.
    #[account(
        mut,
        seeds = [VAULT_SEED.as_bytes(), wrapped_mint.key().as_ref()],
        bump,
    )]
    pub vault: Account<'info, SplTokenAccount>,

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

    /// Typed rather than unchecked: both of these are CPI targets, and an unchecked
    /// account that gets invoked is the same bug class as an unchecked vault. The
    /// instruction builders in `spl-token`/`spl-token-2022` happen to reject a foreign
    /// program id themselves, so this is belt-and-braces — but it makes the requirement
    /// visible at the account list instead of buried in a dependency's internals.
    pub token_program: Program<'info, Token>,
    pub token_2022_program: Program<'info, Token2022>,
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
