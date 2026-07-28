use anchor_lang::prelude::*;
use anchor_spl::associated_token::spl_associated_token_account;
use anchor_spl::token_2022::{self as token_2022, MintTo};

use crate::constants::{AUTHORITY_SEED, CAPABILITY_SEED, MAX_ALLOWLIST_LEN, MAX_DEPTH};
use crate::error::LeashError;
use crate::state::Capability;

/// Mints a new child Capability whose cap/expiry/allowlist are a subset of the parent's,
/// and increments the parent's `committed_to_children`. Does NOT go through the transfer
/// hook — attenuation is a mint to a new account, not a transfer to a destination. See
/// BUILD_PLAN.md §4 "attenuate" and §5 D3 for why that asymmetry is the point.
#[derive(Accounts)]
#[instruction(child_cap: u64, child_expiry: i64, child_allowlist: Vec<Pubkey>)]
pub struct Attenuate<'info> {
    #[account(mut)]
    pub owner: Signer<'info>,

    #[account(
        mut,
        has_one = owner @ LeashError::Unauthorized,
    )]
    pub parent_capability: Account<'info, Capability>,

    /// CHECK: whoever is being delegated to — the child's owner. Not required to sign;
    /// the parent's owner is the one authorizing this attenuation, not the child.
    pub child_owner: UncheckedAccount<'info>,

    // Seeded on `child_owner` alone (same scheme as issue's root capability), NOT
    // [parent, child_owner]. This is load-bearing, not a style choice: leash-hook has to
    // derive "the Capability for this transfer" using only the transfer's own accounts
    // (specifically the token account's owner, per the Transfer Hook Interface), with
    // ONE fixed seed formula it registers once at mint-creation time. Two different
    // derivation schemes for root vs. child capabilities can't both be that one formula.
    // Consequence, documented rather than hidden: one owner can hold at most one active
    // capability (root or child) at a time — attenuating to an owner who already has one
    // fails the `init` constraint below, by design, not by oversight.
    #[account(
        init,
        payer = owner,
        space = Capability::MAX_SIZE,
        seeds = [CAPABILITY_SEED.as_bytes(), child_owner.key().as_ref()],
        bump,
    )]
    pub child_capability: Account<'info, Capability>,

    /// CHECK: leash-wrapped-USD mint. Mutable because minting increases supply.
    #[account(mut)]
    pub wrapped_mint: UncheckedAccount<'info>,

    /// CHECK: PDA that is the wrapped mint's mint authority.
    #[account(seeds = [AUTHORITY_SEED.as_bytes()], bump)]
    pub program_authority: UncheckedAccount<'info>,

    /// CHECK: fresh Token-2022 account holding the child's wrapped balance, created here
    /// via CPI, owned by `child_owner` directly (same bearer-object model as `issue`).
    #[account(mut)]
    pub child_token_account: UncheckedAccount<'info>,

    /// CHECK: Token-2022 program, for minting child_cap wrapped units.
    pub token_2022_program: UncheckedAccount<'info>,
    /// CHECK: associated-token-account program, for creating child_token_account.
    pub associated_token_program: UncheckedAccount<'info>,
    pub system_program: Program<'info, System>,
}

pub fn attenuate_handler(
    ctx: Context<Attenuate>,
    child_cap: u64,
    child_expiry: i64,
    child_allowlist: Vec<Pubkey>,
) -> Result<()> {
    require!(
        child_allowlist.len() <= MAX_ALLOWLIST_LEN,
        LeashError::AllowlistTooLarge
    );

    let parent = &ctx.accounts.parent_capability;
    require!(parent.depth < MAX_DEPTH, LeashError::DepthExceeded);

    let parent_remaining = parent
        .cap
        .checked_sub(parent.spent)
        .and_then(|v| v.checked_sub(parent.committed_to_children))
        .ok_or(LeashError::CapExceeded)?;
    require!(child_cap <= parent_remaining, LeashError::CapExceeded);
    require!(child_expiry <= parent.expiry, LeashError::NotASubset);
    require!(
        child_allowlist.iter().all(|d| parent.allowlist.contains(d)),
        LeashError::NotASubset
    );

    // Create the child's wrapped-token account (ATA, owned by child_owner).
    anchor_lang::solana_program::program::invoke(
        &spl_associated_token_account::instruction::create_associated_token_account(
            ctx.accounts.owner.key,
            ctx.accounts.child_owner.key,
            ctx.accounts.wrapped_mint.key,
            ctx.accounts.token_2022_program.key,
        ),
        &[
            ctx.accounts.owner.to_account_info(),
            ctx.accounts.child_token_account.to_account_info(),
            ctx.accounts.child_owner.to_account_info(),
            ctx.accounts.wrapped_mint.to_account_info(),
            ctx.accounts.system_program.to_account_info(),
            ctx.accounts.token_2022_program.to_account_info(),
        ],
    )?;

    // Mint child_cap fresh wrapped units to it — attenuation is a mint, not a transfer
    // of the parent's existing balance (BUILD_PLAN.md §5 D3).
    let bump = ctx.bumps.program_authority;
    let signer_seeds: &[&[u8]] = &[AUTHORITY_SEED.as_bytes(), &[bump]];
    token_2022::mint_to(
        CpiContext::new_with_signer(
            ctx.accounts.token_2022_program.key(),
            MintTo {
                mint: ctx.accounts.wrapped_mint.to_account_info(),
                to: ctx.accounts.child_token_account.to_account_info(),
                authority: ctx.accounts.program_authority.to_account_info(),
            },
            &[signer_seeds],
        ),
        child_cap,
    )?;

    // Initialize the child Capability.
    let parent_key = ctx.accounts.parent_capability.key();
    let parent_depth = ctx.accounts.parent_capability.depth;
    let parent_ancestors = ctx.accounts.parent_capability.ancestors;
    let child_token_account_key = ctx.accounts.child_token_account.key();
    let child_owner_key = ctx.accounts.child_owner.key();
    let child_bump = ctx.bumps.child_capability;

    // Shift: [immediate parent, then the parent's own ancestor list, minus its last
    // slot]. See state.rs's doc comment on `ancestors` for why each capability carries
    // its full chain directly rather than leash-hook re-deriving it by chaining through
    // each ancestor's `parent` field at spend time.
    let mut child_ancestors = [Pubkey::default(); crate::constants::ANCESTOR_SLOTS];
    child_ancestors[0] = parent_key;
    for i in 1..crate::constants::ANCESTOR_SLOTS {
        child_ancestors[i] = parent_ancestors[i - 1];
    }

    let child = &mut ctx.accounts.child_capability;
    child.owner = child_owner_key;
    child.parent = parent_key;
    child.ancestors = child_ancestors;
    child.token_account = child_token_account_key;
    child.cap = child_cap;
    child.spent = 0;
    child.committed_to_children = 0;
    child.expiry = child_expiry;
    child.allowlist = child_allowlist;
    child.revoked = false;
    child.depth = parent_depth + 1;
    child.bump = child_bump;

    // Reserve child_cap out of the parent's remaining budget — accounting only, no
    // token movement on the parent's side (BUILD_PLAN.md §4).
    ctx.accounts.parent_capability.committed_to_children = ctx
        .accounts
        .parent_capability
        .committed_to_children
        .checked_add(child_cap)
        .ok_or(LeashError::CapExceeded)?;

    Ok(())
}
