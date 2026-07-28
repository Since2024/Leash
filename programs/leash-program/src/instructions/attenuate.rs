use anchor_lang::prelude::*;

use crate::state::Capability;
use crate::constants::{CAPABILITY_SEED, MAX_DEPTH};
use crate::error::LeashError;

/// Mints a new child Capability whose cap/expiry/allowlist are a subset of the parent's,
/// and increments the parent's `committed_to_children`. Does NOT go through the transfer
/// hook — attenuation is a mint to a new account, not a transfer to a destination. See
/// BUILD_PLAN.md §4 "attenuate" and §5 D3 for why that asymmetry is the point.
#[derive(Accounts)]
pub struct Attenuate<'info> {
    #[account(mut)]
    pub owner: Signer<'info>,

    #[account(
        mut,
        has_one = owner @ LeashError::Unauthorized,
    )]
    pub parent_capability: Account<'info, Capability>,

    #[account(
        init,
        payer = owner,
        space = Capability::MAX_SIZE,
        seeds = [CAPABILITY_SEED.as_bytes(), parent_capability.key().as_ref()], // TODO: real seed scheme (child index)
        bump,
    )]
    pub child_capability: Account<'info, Capability>,

    // TODO(week 3, pending D3): mint authority PDA, child's new wrapped-token account,
    // token_program, associated_token_program.
    pub system_program: Program<'info, System>,
}

pub fn attenuate_handler(
    _ctx: Context<Attenuate>,
    _child_cap: u64,
    _child_expiry: i64,
    _child_allowlist: Vec<Pubkey>,
) -> Result<()> {
    // TODO(week 3): enforce child_cap <= parent.cap - parent.spent - parent.committed_to_children
    // (LeashError::CapExceeded), child_expiry <= parent.expiry, child_allowlist subset of
    // parent's (LeashError::NotASubset), parent.depth < MAX_DEPTH (LeashError::DepthExceeded).
    // Mint child_cap wrapped units to the new child token account, increment
    // parent.committed_to_children.
    let _ = MAX_DEPTH;
    let _ = LeashError::DepthExceeded;
    todo!("attenuate: see docs/BUILD_PLAN.md §4 and §5 (D3) before implementing")
}
