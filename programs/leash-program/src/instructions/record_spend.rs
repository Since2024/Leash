use anchor_lang::prelude::*;

use crate::constants::{HOOK_AUTHORITY_SEED, HOOK_PROGRAM_ID_STR};
use crate::error::LeashError;
use crate::state::Capability;

/// Commits a spend that leash-hook has already validated (cap/expiry/allowlist/revoked,
/// single ancestor level) but cannot itself write: leash-hook doesn't own Capability
/// accounts, and Solana only lets the owning program mutate an account's data. This is
/// the CPI entrypoint leash-hook calls instead, once its own checks pass.
///
/// Access control: `hook_authority` must be the PDA `["hook-authority"]` derived under
/// leash-hook's own program ID (constants::HOOK_PROGRAM_ID) and signed via
/// `invoke_signed` from within leash-hook. Nothing else can produce that signature, so
/// this instruction can't be called to inflate `spent` by anyone who isn't leash-hook
/// itself. The cap check below is repeated here deliberately (defense in depth) — this
/// program shouldn't blindly trust the hook's arithmetic just because the signer check
/// passed.
#[derive(Accounts)]
pub struct RecordSpend<'info> {
    /// CHECK: PDA seeds-checked against HOOK_PROGRAM_ID below (Anchor's `seeds`/`bump`
    /// constraint can't reference a non-`Pubkey::find_program_address`-under-this-program
    /// scheme directly, so the check is manual in the handler). `signer` is required
    /// here, not just checked manually: without it, Anchor's account-type (a plain
    /// `UncheckedAccount`, not `Signer`) makes the *generated CPI instruction itself*
    /// mark `is_signer: false` in its account metas, so `invoke_signed`'s PDA signature
    /// never gets a chance to matter — confirmed by hitting `is_signer == false` here
    /// with a correctly-matching PDA address, not by guessing.
    #[account(signer)]
    pub hook_authority: UncheckedAccount<'info>,

    #[account(mut)]
    pub capability: Account<'info, Capability>,
}

pub fn record_spend_handler(ctx: Context<RecordSpend>, amount: u64) -> Result<()> {
    let hook_program_id: Pubkey = HOOK_PROGRAM_ID_STR.parse().unwrap();
    let (expected_hook_authority, _) =
        Pubkey::find_program_address(&[HOOK_AUTHORITY_SEED.as_bytes()], &hook_program_id);
    require_keys_eq!(
        ctx.accounts.hook_authority.key(),
        expected_hook_authority,
        LeashError::UnauthorizedCaller
    );
    require!(
        ctx.accounts.hook_authority.is_signer,
        LeashError::UnauthorizedCaller
    );

    let capability = &mut ctx.accounts.capability;
    let new_spent = capability
        .spent
        .checked_add(amount)
        .ok_or(LeashError::CapExceeded)?;
    require!(new_spent <= capability.cap, LeashError::CapExceeded);
    capability.spent = new_spent;

    Ok(())
}
