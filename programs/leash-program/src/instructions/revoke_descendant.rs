use anchor_lang::prelude::*;

use crate::error::LeashError;
use crate::state::Capability;

/// Revokes a capability *below* you in the tree (docs/ROADMAP.md 0.8).
///
/// `revoke` is `has_one = owner`, so it only ever let a capability's own holder revoke
/// it. That left a principal with one blunt lever: revoke itself, which cascades to every
/// descendant through the hook's ancestor walk. Cutting off one agent — or one of several
/// allowances held by the same agent, which 0.3 made possible — was not expressible.
///
/// Authority comes from the `ancestors` array the descendant already carries. That array
/// exists because leash-hook reads it on every spend (see `state.rs`), so this
/// instruction adds no new state and no new trust: if a capability's spends are already
/// gated on these ancestors being unrevoked, those same ancestors are exactly the set
/// entitled to do the revoking.
///
/// Any ancestor may revoke, not just the immediate parent. A grandparent could already
/// stop the spend by revoking itself; letting it revoke the descendant directly is
/// strictly *less* destructive than the lever it already had.
///
/// Deliberately a separate instruction rather than a branch inside `revoke`: the
/// authority check is genuinely different (`has_one` versus an ancestry proof), and
/// overloading one instruction with an optional second capability account would make the
/// common self-revoke path pay for a check it never needs.
#[derive(Accounts)]
pub struct RevokeDescendant<'info> {
    pub owner: Signer<'info>,

    /// The signer's own capability, somewhere above `descendant_capability` in the tree.
    /// Not mutated — this is the proof of authority, not the target.
    #[account(has_one = owner @ LeashError::Unauthorized)]
    pub ancestor_capability: Account<'info, Capability>,

    #[account(mut)]
    pub descendant_capability: Account<'info, Capability>,
}

pub fn revoke_descendant_handler(ctx: Context<RevokeDescendant>) -> Result<()> {
    let ancestor_key = ctx.accounts.ancestor_capability.key();
    let descendant = &mut ctx.accounts.descendant_capability;

    // Only the first `depth` slots hold real ancestors; the rest are `Pubkey::default()`
    // padding. Scanning the whole array would let a caller whose capability address is
    // somehow the default pubkey pass — impossible in practice, but the bound is free.
    let depth = descendant.depth as usize;
    let is_ancestor = descendant.ancestors[..depth]
        .iter()
        .any(|a| *a == ancestor_key);
    require!(is_ancestor, LeashError::NotAnAncestor);

    // Idempotent by construction: revoking an already-revoked capability is a no-op, not
    // an error. Nothing here can be undone, so a repeated call cannot do harm, and
    // failing would only make the natural "revoke then reclaim" retry loop brittle.
    descendant.revoked = true;
    Ok(())
}
