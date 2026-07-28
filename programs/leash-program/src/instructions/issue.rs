use anchor_lang::prelude::*;

use crate::state::Capability;
use crate::constants::{CAPABILITY_SEED, MAX_ALLOWLIST_LEN};
use crate::error::LeashError;

/// Principal deposits `cap` USDC into the vault and receives a root Capability
/// (parent = None, depth = 0, spent = 0, committed_to_children = 0, revoked = false),
/// plus `cap` units of leash-wrapped-USD minted to a fresh token account.
///
/// See BUILD_PLAN.md §4 "issue" and §5 D1/D3 — the wrapping mechanism (real USDC can't
/// carry a TransferHook directly) is the first thing to validate, not assume, in Week 1.
#[derive(Accounts)]
pub struct Issue<'info> {
    #[account(mut)]
    pub principal: Signer<'info>,

    #[account(
        init,
        payer = principal,
        space = Capability::MAX_SIZE,
        seeds = [CAPABILITY_SEED.as_bytes(), principal.key().as_ref()], // TODO: real seed scheme (nonce for multiple root caps per principal)
        bump,
    )]
    pub capability: Account<'info, Capability>,

    // TODO(week 1-2, pending D1): vault token account (real USDC), the leash-wrapped
    // Token-2022 mint, the new capability's own wrapped-token account, token_program,
    // associated_token_program, system_program.
    pub system_program: Program<'info, System>,
}

pub fn issue_handler(
    _ctx: Context<Issue>,
    _cap: u64,
    _expiry: i64,
    _allowlist: Vec<Pubkey>,
) -> Result<()> {
    // TODO(week 2): validate allowlist.len() <= MAX_ALLOWLIST_LEN (LeashError::AllowlistTooLarge),
    // deposit `cap` real USDC into the vault, mint `cap` wrapped units to a new token
    // account, initialize the Capability fields per BUILD_PLAN.md §4.
    let _ = MAX_ALLOWLIST_LEN;
    let _ = LeashError::AllowlistTooLarge;
    todo!("issue: see docs/BUILD_PLAN.md §4 and §5 (D1) before implementing")
}
