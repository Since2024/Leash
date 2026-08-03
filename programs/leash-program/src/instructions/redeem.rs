use anchor_lang::prelude::*;
use anchor_spl::token::{self, Token, TokenAccount as SplTokenAccount, Transfer};
use anchor_spl::token_2022::{self as token_2022, Burn, Token2022};

use crate::constants::{AUTHORITY_SEED, CAPABILITY_SEED, VAULT_SEED};
use crate::error::LeashError;
use crate::state::Capability;

/// Redeems leash-wrapped-USD 1:1 for real USDC from the vault. A separate, explicit
/// instruction — not something the transfer hook can trigger itself, because Token-2022
/// transfer hooks receive source/destination accounts as read-only during the hook call
/// (see BUILD_PLAN.md §5 D2). This is what makes accepting a Leash payment as good as
/// accepting cash for a merchant.
///
/// Not "anyone holding wrapped units," though — that was the hole in docs/ROADMAP.md 0.1.
/// Burning does not fire the transfer hook, so redemption is a path around every check
/// the hook exists to enforce. This instruction originally never looked at a `Capability`
/// at all, which meant a delegated agent could burn its *unspent budget* and receive
/// unrestricted real USDC at any address — defeating the allowlist and revocation for
/// the entire un-spent balance.
///
/// The rule that closes it, in terms of who actually funded the vault:
///
///   - **A merchant** (no capability at `[CAPABILITY_SEED, holder]`) redeems freely.
///     Their units arrived through a real, hook-checked transfer; they earned them.
///   - **A root capability's owner** redeems freely too, but only out of budget that is
///     genuinely free (`cap - spent - committed_to_children`), and `cap` shrinks to
///     match. They deposited into the vault, so unwinding is theirs to do; the bound is
///     what stops them draining collateral their children's units are minted against.
///   - **A delegated (child) capability's owner cannot redeem its budget at all.** It
///     never deposited anything. It may *spend*, through the hook, to an allowlisted
///     destination — that is the whole point of the capability.
///
/// The capability account is derived and passed unconditionally rather than made
/// optional: a caller who could simply omit it could opt out of the check entirely.
/// A holder with no capability passes a PDA that does not exist, and the handler
/// verifies that on-chain rather than taking the caller's word for it.
#[derive(Accounts)]
pub struct Redeem<'info> {
    pub holder: Signer<'info>,

    /// CHECK: holder's Token-2022 account holding wrapped units — burned from here.
    /// Declared before `capability`, whose seeds derive from it.
    #[account(mut)]
    pub holder_wrapped_account: UncheckedAccount<'info>,

    /// CHECK: the Capability belonging to `holder_wrapped_account`, address-verified by
    /// the seeds constraint. May legitimately not exist (a merchant holding received
    /// units) — the handler distinguishes the two by inspecting owner/data rather than
    /// trusting the caller, so this cannot be typed as `Account<Capability>` or
    /// `Option<_>`. Mutable because a root's redemption writes `cap` back down.
    ///
    /// Derived from the token account rather than the holder (docs/ROADMAP.md 0.3), which
    /// makes the association exact: if a capability exists at this address then this
    /// account *is* its token account, so the "is this unspent budget?" question is
    /// answered by the derivation itself. Under the old owner-keyed scheme it could only
    /// be answered as "the one capability this holder happens to have," which stops being
    /// good enough the moment an owner can hold several.
    #[account(
        mut,
        seeds = [CAPABILITY_SEED.as_bytes(), holder_wrapped_account.key().as_ref()],
        bump,
    )]
    pub capability: UncheckedAccount<'info>,

    /// CHECK: leash-wrapped-USD mint. Mutable because burning reduces supply. Not typed,
    /// because it needs no field read — what makes it trustworthy is that the vault below
    /// is derived from it, so naming a counterfeit mint here names a vault that is not the
    /// real one.
    #[account(mut)]
    pub wrapped_mint: UncheckedAccount<'info>,

    /// Program vault (legacy SPL Token, real USDC) — source of the withdrawal, and the
    /// account docs/ROADMAP.md 0.11 was about.
    ///
    /// This was a bare `UncheckedAccount`, and the combination with an unchecked
    /// `wrapped_mint` was the worst hole in the program: the burn took whatever mint the
    /// caller named and the payout came from whatever vault the caller named, with
    /// nothing requiring the two to be related. Burn a Token-2022 mint you created
    /// yourself, name the real vault, and the program pays you out of a stranger's
    /// deposit — one instruction, no capability, no prior state. `tests/deployment_binding.rs`
    /// demonstrates it against the real binary.
    ///
    /// `program_authority`'s own `seeds` constraint does not help, and the reason is the
    /// instructive part: it proves the *signer* is canonical, not which account that
    /// signer is being made to pay out of. It is seeded `[AUTHORITY_SEED]` with no mint in
    /// the seeds, so one PDA is the authority for every vault the program will ever have.
    ///
    /// Deriving the vault from `wrapped_mint` is what ties them together: the seeds that
    /// say which mint say which vault.
    #[account(
        mut,
        seeds = [VAULT_SEED.as_bytes(), wrapped_mint.key().as_ref()],
        bump,
    )]
    pub vault: Account<'info, SplTokenAccount>,

    /// CHECK: PDA authority over the vault. Same PDA `issue` uses as mint authority —
    /// one shared authority for the deployment (see constants::AUTHORITY_SEED).
    #[account(seeds = [AUTHORITY_SEED.as_bytes()], bump)]
    pub program_authority: UncheckedAccount<'info>,

    /// CHECK: holder's real-USDC account — destination of the withdrawal.
    #[account(mut)]
    pub holder_deposit_account: UncheckedAccount<'info>,

    /// Typed for the same reason as in `issue`: these are CPI targets, and an unchecked
    /// account that gets invoked is the same bug class as an unchecked vault.
    pub token_program: Program<'info, Token>,
    pub token_2022_program: Program<'info, Token2022>,
}

pub fn redeem_handler(ctx: Context<Redeem>, amount: u64) -> Result<()> {
    // 0. Authorize the redemption itself (docs/ROADMAP.md 0.1). See the struct doc above
    // for the rule; this is where it is enforced.
    //
    // The account at this address is a capability only if leash-program owns it and it
    // holds data — nobody else can create one there, since producing that PDA's signature
    // requires this program. An empty/system-owned account means "these units are not
    // some capability's unspent budget," which is checked here rather than asserted by
    // the caller. That is the merchant case, and it stays unrestricted.
    let capability_info = ctx.accounts.capability.to_account_info();
    let is_capability_budget =
        capability_info.owner == &crate::ID && !capability_info.data_is_empty();

    if is_capability_budget {
        let mut capability: Capability = {
            let data = capability_info.try_borrow_data()?;
            Capability::try_deserialize(&mut &data[..])?
        };

        // Derivation already ties this capability to `holder_wrapped_account`; re-checking
        // the stored field costs nothing and catches it being written wrong at issue time
        // (the same reasoning as leash-hook's check — docs/ROADMAP.md 0.4).
        require!(
            ctx.accounts.holder_wrapped_account.key() == capability.token_account,
            LeashError::Unauthorized
        );

        // The capability must be redeeming against its own deployment. Token-2022 would
        // catch the mismatch anyway when burning from an account of a different mint, so
        // this is defence in depth rather than the load-bearing check (docs/ROADMAP.md
        // 0.12) — but it names the failure instead of leaving it as a token-program error
        // that reads like insufficient funds.
        require!(
            capability.wrapped_mint == ctx.accounts.wrapped_mint.key(),
            LeashError::WrongMint
        );

        // A delegatee never funded the vault, so it has no claim on it. It can still
        // spend through the hook — that path is untouched.
        require!(capability.depth == 0, LeashError::DelegatedCannotRedeem);

        // A root may unwind only what it hasn't already spent or promised away. Without
        // the `committed_to_children` term, a parent could drain the vault and strand the
        // units minted against it for its children — the same collateral shortfall as
        // docs/ROADMAP.md 0.2, reached through redemption instead of spending.
        let free = capability
            .cap
            .checked_sub(capability.spent)
            .and_then(|v| v.checked_sub(capability.committed_to_children))
            .ok_or(LeashError::CapExceeded)?;
        require!(amount <= free, LeashError::CapExceeded);

        // Shrink the budget to match the collateral actually left behind. Skipping this
        // would leave the capability advertising spending power the vault no longer backs
        // (docs/ROADMAP.md 0.7).
        capability.cap = capability
            .cap
            .checked_sub(amount)
            .ok_or(LeashError::CapExceeded)?;

        let mut data = capability_info.try_borrow_mut_data()?;
        capability.try_serialize(&mut &mut data[..])?;
    }

    // 1. Burn `amount` wrapped units from the holder's account. The holder signs this
    // directly (they own the account) — no program authority needed to destroy value.
    token_2022::burn(
        CpiContext::new(
            ctx.accounts.token_2022_program.key(),
            Burn {
                mint: ctx.accounts.wrapped_mint.to_account_info(),
                from: ctx.accounts.holder_wrapped_account.to_account_info(),
                authority: ctx.accounts.holder.to_account_info(),
            },
        ),
        amount,
    )?;

    // 2. Withdraw `amount` real USDC from the vault to the holder, signed by the
    // program's authority PDA (the vault's real owner).
    let bump = ctx.bumps.program_authority;
    let signer_seeds: &[&[u8]] = &[AUTHORITY_SEED.as_bytes(), &[bump]];
    token::transfer(
        CpiContext::new_with_signer(
            ctx.accounts.token_program.key(),
            Transfer {
                from: ctx.accounts.vault.to_account_info(),
                to: ctx.accounts.holder_deposit_account.to_account_info(),
                authority: ctx.accounts.program_authority.to_account_info(),
            },
            &[signer_seeds],
        ),
        amount,
    )?;

    Ok(())
}
