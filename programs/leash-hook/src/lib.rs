use anchor_lang::prelude::*;

declare_id!("9WPQUY6zVRwVZ3eUsDF1aNESWAyZwL8GwKpzd2C66xtS");

/// The Token-2022 TransferHook program: enforcement lives here, inside every transfer of
/// leash-wrapped-USD, not in a server or in the agent's own code. See docs/BUILD_PLAN.md
/// §4 "spend" and §5 (D1-D4) — every design decision below is unvalidated until the
/// Week 1 spike confirms the Transfer Hook Interface actually supports it as assumed.
///
/// On every transfer this must (BUILD_PLAN.md §2, the six non-negotiable properties):
///   1. Load the source token account's associated Capability (extra account).
///   2. Load the ancestor chain up to MAX_DEPTH (extra accounts, D4).
///   3. Reject if `revoked` on the capability or any ancestor.
///   4. Reject if `now > expiry`.
///   5. Reject if `destination` not in `allowlist`.
///   6. Reject if `amount + spent > cap`.
///   7. Otherwise increment `spent` and allow the transfer.
///
/// Attenuation (mint, not transfer — D3) never reaches this program at all.
#[program]
pub mod leash_hook {
    use super::*;

    /// TODO(week 1, D4): this needs to match whatever entry point shape the Transfer Hook
    /// Interface actually requires (likely `execute`, per the SPL interface) — Anchor's
    /// `#[program]` macro may or may not be the right fit for that interface directly.
    /// Confirm in the Week 1 spike before writing real enforcement logic here.
    pub fn execute(_ctx: Context<Execute>, _amount: u64) -> Result<()> {
        todo!("leash-hook execute: see docs/BUILD_PLAN.md §4 and §5 (D2, D4) before implementing")
    }
}

#[derive(Accounts)]
pub struct Execute<'info> {
    // TODO(week 1, D1/D2/D4): source token account, destination token account (both
    // read-only per D2), the source's Capability PDA, ancestor Capability PDAs up to
    // MAX_DEPTH, the ExtraAccountMetaList PDA. Exact shape depends on what the ambient
    // Transfer Hook Interface / anchor-spl support actually provides — do not assume this
    // struct is correct until D1-D4 are validated against the real interface.
    pub source: UncheckedAccount<'info>,
    pub destination: UncheckedAccount<'info>,
}
