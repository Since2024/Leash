use anchor_lang::prelude::*;
use anchor_spl::token::{Mint as SplMint, Token, TokenAccount as SplTokenAccount};
use anchor_spl::token_interface::Mint;

use crate::constants::{AUTHORITY_SEED, VAULT_SEED};
use crate::error::LeashError;

/// Creates the one vault that backs a given wrapped mint, at `[VAULT_SEED, wrapped_mint]`
/// (docs/ROADMAP.md 0.11).
///
/// # Why this instruction has to exist
///
/// It exists to give the vault an *address the program can re-derive*, which is the only
/// thing that makes "is this the real vault?" an answerable question.
///
/// Before this, a deployment's vault was a freshly generated keypair account created
/// entirely client-side (`sdk/ts/src/deployment.ts`), and `issue`/`redeem` took it as an
/// unconstrained `UncheckedAccount`. Nothing on-chain recorded which vault belonged to
/// which mint, so nothing could reject a substitute. The consequences were not subtle:
/// `redeem` would burn units of any Token-2022 mint the caller handed it and pay out from
/// any vault the caller handed it, which meant burning a mint you created yourself and
/// withdrawing somebody else's deposit — the whole vault, in one instruction, with no
/// capability and no prior state. See `tests/deployment_binding.rs`.
///
/// The near-miss worth recording is `program_authority`. It *is* seeds-checked, and that
/// looks like it settles the matter; it does not. It proves the signer is the canonical
/// authority PDA, not which token account that PDA is being made to sign a withdrawal
/// from. And since it is seeded `[AUTHORITY_SEED]` alone — no mint, no deployment — one
/// PDA is the authority for every vault the program will ever have, so the real vault
/// satisfies the check no matter what mint the caller is burning.
///
/// # Why derivation rather than a stored pubkey
///
/// A `Deployment` account recording `vault` would work too, and was the obvious
/// alternative. Deriving is better here for the reason docs/ROADMAP.md 0.4 already paid
/// for once: a stored pubkey is a field written at setup and trusted forever, and a field
/// nothing re-checks is exactly the kind that quietly stops being true. With the vault at
/// `[VAULT_SEED, wrapped_mint]`, the seeds that answer "which mint" *are* the seeds that
/// answer "which vault" — the binding cannot drift, because there is nothing to drift.
///
/// # Deliberately permissionless, and why that is safe
///
/// Anyone may create a vault for any wrapped mint. A caller who does this for their own
/// mint gets a self-consistent little deployment holding only their own money, which is
/// harmless — theft requires reaching the *real* vault, and the real vault is reachable
/// only by passing the real wrapped mint.
///
/// The one ordering hazard is that whoever calls this first for a given wrapped mint
/// chooses its `deposit_mint`, and could pick a worthless one. That window is closed by
/// construction rather than by a check: a wrapped mint is a freshly generated keypair, so
/// its address is unknown to anyone else until it exists, and `createDeployment` creates
/// the mint and calls this in the same transaction. There is no point at which a third
/// party knows the address and the vault does not yet exist.
#[derive(Accounts)]
pub struct InitializeVault<'info> {
    #[account(mut)]
    pub payer: Signer<'info>,

    /// The Token-2022 wrapped mint this vault backs. Required to be a mint whose mint
    /// authority is this program's `program_authority` PDA — that is what makes it a real
    /// leash wrapped mint rather than an arbitrary token, and it keeps the set of vaults
    /// that can ever exist tied to actual deployments.
    #[account(
        constraint = wrapped_mint.mint_authority == anchor_lang::solana_program::program_option::COption::Some(program_authority.key())
            @ LeashError::Unauthorized,
    )]
    pub wrapped_mint: InterfaceAccount<'info, Mint>,

    /// The real deposited asset (legacy SPL Token), e.g. USDC.
    pub deposit_mint: Account<'info, SplMint>,

    /// CHECK: PDA that owns the vault and signs withdrawals from it. Seeds-checked; not
    /// read.
    #[account(seeds = [AUTHORITY_SEED.as_bytes()], bump)]
    pub program_authority: UncheckedAccount<'info>,

    /// `init` here is what makes the address canonical: exactly one vault can ever exist
    /// per wrapped mint, and a second call fails on the account already being in use.
    #[account(
        init,
        payer = payer,
        seeds = [VAULT_SEED.as_bytes(), wrapped_mint.key().as_ref()],
        bump,
        token::mint = deposit_mint,
        token::authority = program_authority,
        token::token_program = token_program,
    )]
    pub vault: Account<'info, SplTokenAccount>,

    pub token_program: Program<'info, Token>,
    pub system_program: Program<'info, System>,
}

pub fn initialize_vault_handler(_ctx: Context<InitializeVault>) -> Result<()> {
    // Nothing to do: the account constraints above are the entire instruction. The vault
    // holds no leash-specific state — its address is the state.
    Ok(())
}
