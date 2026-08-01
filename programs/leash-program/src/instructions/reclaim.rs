use anchor_lang::prelude::*;

use crate::error::LeashError;
use crate::state::Capability;

/// Releases budget a parent reserved for a child that can no longer spend it
/// (docs/ROADMAP.md 0.7).
///
/// `attenuate` increments the parent's `committed_to_children`, and until now nothing
/// ever decremented it. Both critical fixes honour that reservation — 0.2 stops the
/// parent spending it, 0.1 stops the parent redeeming it — so once a child was revoked or
/// expired, its share of the deposit was stranded: correctly withheld, but permanently.
///
/// # Why this does not burn the child's units
///
/// The roadmap originally specified burning the child's unspent units and decrementing in
/// step. That is not implementable as specified. A capability's token account is
/// `token::authority = <holder>` — the bearer-object model of BUILD_PLAN.md §0 — so
/// burning from it needs the *child's* signature, and a child being reclaimed from has
/// just been revoked by its parent. Requiring its cooperation would make the instruction
/// useless in exactly the case it exists for. Making the program the authority instead
/// would break the bearer model outright.
///
/// So this moves accounting only, and leans on the child being provably unable to spend:
///
/// - `revoked` is a one-way flag; there is no un-revoke.
/// - `expiry` is fixed at `attenuate` time and cannot be extended.
/// - leash-hook checks both on every spend, and a delegated capability cannot `redeem`
///   at all (0.1).
///
/// A dead child's units are therefore inert: unspendable and unredeemable, by three
/// independent paths. Releasing the parent's reservation cannot let the tree spend more
/// than the vault backs, which is the invariant 0.2 exists to protect.
///
/// The visible consequence, stated rather than buried: those inert units still count
/// toward the wrapped mint's **total supply**, so supply overstates redeemable value by
/// the amount of every dead child. Nothing on-chain reads total supply, so nothing breaks
/// — but anyone treating supply as a solvency figure would be misled. Tracked as
/// docs/ROADMAP.md 0.10.
///
/// # Why only the immediate parent
///
/// `attenuate` records the reservation on the immediate parent alone, so that is the only
/// account whose `committed_to_children` is wrong. A grandparent has nothing to release.
/// This is deliberately narrower than `revoke_descendant`, which any ancestor may call.
#[derive(Accounts)]
pub struct Reclaim<'info> {
    pub owner: Signer<'info>,

    #[account(mut, has_one = owner @ LeashError::Unauthorized)]
    pub parent_capability: Account<'info, Capability>,

    /// Mutated: its `cap` is written down so the same budget cannot be reclaimed twice.
    #[account(mut)]
    pub child_capability: Account<'info, Capability>,
}

pub fn reclaim_handler(ctx: Context<Reclaim>) -> Result<()> {
    let parent_key = ctx.accounts.parent_capability.key();
    let child = &mut ctx.accounts.child_capability;

    // The reservation lives on the immediate parent, so anything else has nothing to
    // release and must not be able to decrement its own counter using someone else's
    // child.
    require!(child.parent == parent_key, LeashError::NotAChild);

    let now = Clock::get()?.unix_timestamp;
    let dead = child.revoked || now > child.expiry;
    require!(dead, LeashError::ChildStillLive);

    // Release only the child's *free* budget — not everything it hasn't spent.
    //
    // Subtracting `committed_to_children` as well is load-bearing, and its absence was a
    // real bug caught by `fuzz_conservation.rs` (seed 1, step 31). Releasing `cap - spent`
    // and writing `cap` down to `spent` breaks the invariant on any child that is itself a
    // parent: a middle node holding `committed_to_children = 97` ended up with `cap = 0`,
    // so `spent + committed_to_children <= cap` read `0 + 97 <= 0`. No funds were at risk
    // — the grandchildren could not spend, since the hook's ancestor walk sees their dead
    // parent — but the invariant `state.rs` advertises was false, and the arithmetic that
    // depends on it was operating on nonsense.
    //
    // Releasing the free portion instead leaves `cap == spent + committed_to_children`,
    // so the invariant holds with equality. Budget sitting under a grandchild is recovered
    // by reclaiming **bottom-up**: reclaim the grandchild into this child first, which
    // frees it here, then call this again. That is also why calling twice must stay safe.
    let unspent = child
        .cap
        .checked_sub(child.spent)
        .and_then(|v| v.checked_sub(child.committed_to_children))
        .ok_or(LeashError::CapExceeded)?;

    // Idempotence: a second call computes `unspent == 0` and changes nothing. Deriving the
    // write-down from the same quantity as the parent's decrement keeps the two numbers
    // moving together, which is the property 0.2 turns on.
    child.cap = child
        .cap
        .checked_sub(unspent)
        .ok_or(LeashError::CapExceeded)?;

    let parent = &mut ctx.accounts.parent_capability;
    parent.committed_to_children = parent
        .committed_to_children
        .checked_sub(unspent)
        .ok_or(LeashError::CapExceeded)?;

    Ok(())
}
