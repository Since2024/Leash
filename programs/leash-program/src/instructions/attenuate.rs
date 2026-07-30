use anchor_lang::prelude::*;
use anchor_spl::token_2022::{self as token_2022, MintTo};
use anchor_spl::token_interface::{Mint, TokenAccount};

use crate::constants::{
    AUTHORITY_SEED, CAPABILITY_SEED, MAX_ALLOWLIST_LEN, MAX_DEPTH, TOKEN_ACCOUNT_SEED,
};
use crate::error::LeashError;
use crate::state::Capability;

/// Mints a new child Capability whose cap/expiry/allowlist are a subset of the parent's,
/// and increments the parent's `committed_to_children`. Does NOT go through the transfer
/// hook — attenuation is a mint to a new account, not a transfer to a destination. See
/// BUILD_PLAN.md §4 "attenuate" and §5 D3 for why that asymmetry is the point.
#[derive(Accounts)]
#[instruction(nonce: u64, child_cap: u64, child_expiry: i64, child_allowlist: Vec<Pubkey>)]
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

    /// leash-wrapped-USD mint. Mutable because minting increases supply. Typed so Anchor
    /// can read its extensions to size the child's token account.
    #[account(mut)]
    pub wrapped_mint: InterfaceAccount<'info, Mint>,

    /// CHECK: PDA that is the wrapped mint's mint authority.
    #[account(seeds = [AUTHORITY_SEED.as_bytes()], bump)]
    pub program_authority: UncheckedAccount<'info>,

    /// The child's own wrapped-token account, at `[TOKEN_ACCOUNT_SEED, child_owner,
    /// nonce]`, owned by `child_owner` directly (same bearer-object model as `issue`).
    /// Declared before `child_capability`, whose seeds reference it.
    #[account(
        init,
        payer = owner,
        seeds = [TOKEN_ACCOUNT_SEED.as_bytes(), child_owner.key().as_ref(), &nonce.to_le_bytes()],
        bump,
        token::mint = wrapped_mint,
        token::authority = child_owner,
        token::token_program = token_2022_program,
    )]
    pub child_token_account: InterfaceAccount<'info, TokenAccount>,

    // Root and child capabilities share one derivation — `[CAPABILITY_SEED, <the
    // capability's own token account>]` — and that sameness is load-bearing, not tidiness.
    // leash-hook derives "the Capability for this transfer" from ONE fixed seed formula,
    // registered into the mint's ExtraAccountMetaList at deployment, resolvable only from
    // accounts the transfer already carries. Root and child cannot use different schemes
    // and both still be that one formula.
    //
    // What changed in docs/ROADMAP.md 0.3 is *which* account both are keyed on. It used
    // to be the owner, which meant one capability per owner forever — attenuating twice
    // to the same agent collided on the second `init`. Keying on the capability's own
    // token account keeps the single formula (the hook reads base account 0, the source
    // token account) while letting the nonce in that account's seeds do the
    // disambiguating.
    #[account(
        init,
        payer = owner,
        space = Capability::MAX_SIZE,
        seeds = [CAPABILITY_SEED.as_bytes(), child_token_account.key().as_ref()],
        bump,
    )]
    pub child_capability: Account<'info, Capability>,

    /// CHECK: Token-2022 program, for minting child_cap wrapped units.
    pub token_2022_program: UncheckedAccount<'info>,
    pub system_program: Program<'info, System>,
}

pub fn attenuate_handler(
    ctx: Context<Attenuate>,
    _nonce: u64,
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

    // The child's wrapped-token account is created by Anchor's `init` + `token::*`
    // constraints above, before this handler runs.

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
