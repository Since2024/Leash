use anchor_lang::prelude::*;

declare_id!("9WPQUY6zVRwVZ3eUsDF1aNESWAyZwL8GwKpzd2C66xtS");

/// WEEK 1 SPIKE — proves the primitives compose. No cap/expiry/allowlist/revoked
/// enforcement yet (that's Week 3, once this shape is confirmed). See
/// docs/BUILD_PLAN.md §5 (D1-D4) and CLAUDE.md.
///
/// Finding (this spike): Anchor's `#[program]` macro does NOT dispatch the Transfer
/// Hook Interface's `Execute` instruction directly — Token-2022 CPIs into this program
/// using the SPL interface's own fixed 8-byte discriminator (via `SplDiscriminate`),
/// which is a different scheme from Anchor's sighash. The bridge is Anchor's `fallback`
/// entrypoint: it manually unpacks the raw instruction data with
/// `TransferHookInstruction::unpack` and re-invokes the Anchor-generated private
/// handler. Confirmed against a working reference implementation
/// (pawsengineer/spl-token-2022-transfer-hook-anchor) before writing this.
#[program]
pub mod leash_hook {
    use super::*;
    use anchor_lang::solana_program::program::invoke_signed;
    use anchor_lang::solana_program::system_instruction;
    use spl_tlv_account_resolution::account::ExtraAccountMeta;
    use spl_tlv_account_resolution::state::ExtraAccountMetaList;
    use spl_transfer_hook_interface::instruction::ExecuteInstruction;

    /// D4 spike: register one extra account (a placeholder "spike marker" PDA standing
    /// in for what will be the source token account's Capability PDA in Week 3). Proves
    /// the ExtraAccountMetaList mechanism works before wiring it to real Capability
    /// lookup. NOT yet dynamic per-source-account — see the TODO below and D4's open
    /// question about `Seed::AccountKey`-style dynamic derivation.
    pub fn initialize_extra_account_meta_list(
        ctx: Context<InitializeExtraAccountMetaList>,
    ) -> Result<()> {
        // TODO(week 3, D4): replace this single fixed-address placeholder with a
        // dynamically-derived seed (Seed::AccountKey / Seed::AccountData referencing the
        // `source` token account passed at transfer time) so each transfer resolves to
        // THAT source's own Capability PDA and ancestor chain, not one fixed account.
        let account_metas = vec![ExtraAccountMeta::new_with_pubkey(
            &ctx.accounts.spike_marker.key(),
            false, // is_signer
            false, // is_writable — spike only; Week 3's real Capability account needs `true`
        )?];

        let bump_seed = [ctx.bumps.extra_account_meta_list];
        let mint_key = ctx.accounts.mint.key();
        let signer_seeds =
            spl_transfer_hook_interface::collect_extra_account_metas_signer_seeds(
                &mint_key,
                &bump_seed,
            );
        let account_size = ExtraAccountMetaList::size_of(account_metas.len())?;

        // Fund the PDA to rent-exemption BEFORE allocate/assign. Skipping this is a real
        // bug this spike caught: `allocate` alone sets data size but moves no lamports,
        // so a freshly-derived PDA stays at 0 lamports — and any account at 0 lamports
        // at the end of a transaction is reclaimed by the runtime regardless of its data,
        // silently discarding the whole allocate+assign. The reference implementation
        // this was adapted from has the same gap; confirmed by hitting it here, not by
        // reading about it.
        let rent = Rent::get()?;
        let required_lamports = rent.minimum_balance(account_size);
        if ctx.accounts.extra_account_meta_list.lamports() < required_lamports {
            anchor_lang::solana_program::program::invoke(
                &system_instruction::transfer(
                    ctx.accounts.payer.key,
                    ctx.accounts.extra_account_meta_list.key,
                    required_lamports - ctx.accounts.extra_account_meta_list.lamports(),
                ),
                &[
                    ctx.accounts.payer.to_account_info(),
                    ctx.accounts.extra_account_meta_list.to_account_info(),
                ],
            )?;
        }

        invoke_signed(
            &system_instruction::allocate(
                ctx.accounts.extra_account_meta_list.key,
                account_size as u64,
            ),
            &[ctx.accounts.extra_account_meta_list.to_account_info()],
            &[&signer_seeds],
        )?;
        invoke_signed(
            &system_instruction::assign(
                ctx.accounts.extra_account_meta_list.key,
                ctx.program_id,
            ),
            &[ctx.accounts.extra_account_meta_list.to_account_info()],
            &[&signer_seeds],
        )?;

        let mut data = ctx.accounts.extra_account_meta_list.try_borrow_mut_data()?;
        ExtraAccountMetaList::init::<ExecuteInstruction>(&mut data, &account_metas)?;

        msg!(
            "leash-hook: extra account meta list initialized at {}",
            ctx.accounts.extra_account_meta_list.key()
        );
        Ok(())
    }

    /// Bridges the SPL Transfer Hook Interface's raw `Execute` instruction (fixed
    /// discriminator, arrives via Token-2022's CPI during a real transfer) to
    /// `spike_execute_logic` below. Deliberately does NOT route through Anchor's
    /// generated `__private::__global` dispatcher: that machinery ties the instruction
    /// data's lifetime to the accounts slice's `'info` in a way a freshly-encoded local
    /// byte array can't satisfy (confirmed by hitting E0621/E0597 in this spike — a real
    /// finding, not a guess). Calling the shared logic function directly on the raw
    /// accounts sidesteps it entirely.
    pub fn fallback<'info>(
        _program_id: &Pubkey,
        accounts: &'info [AccountInfo<'info>],
        data: &[u8],
    ) -> Result<()> {
        use spl_transfer_hook_interface::instruction::TransferHookInstruction;

        match TransferHookInstruction::unpack(data)? {
            TransferHookInstruction::Execute { amount } => spike_execute_logic(accounts, amount),
            _ => Err(ProgramError::InvalidInstructionData.into()),
        }
    }
}

/// D2 spike: confirm we can read source/destination during the hook call without doing
/// anything else. Deliberately does NOT check cap/expiry/allowlist/revoked — that's
/// Week 3, once this call path is confirmed to actually fire on a real Token-2022
/// transfer of the wrapped mint. Called only from the `fallback` raw-CPI path above —
/// there is no separately-invokable Anchor instruction for it, since Token-2022 always
/// reaches this through `fallback`, never through Anchor's normal dispatch.
fn spike_execute_logic(accounts: &[AccountInfo], amount: u64) -> Result<()> {
    // Order per the Transfer Hook Interface's Execute instruction: source, mint,
    // destination, owner, extra_account_meta_list, then resolved extra accounts.
    let source = &accounts[0];
    let destination = &accounts[2];
    let spike_marker = accounts.get(5);

    msg!("leash-hook: spike_execute invoked, amount = {}", amount);
    msg!("leash-hook: source = {}", source.key());
    msg!("leash-hook: destination = {}", destination.key());
    if let Some(marker) = spike_marker {
        msg!("leash-hook: spike_marker (extra account) = {}", marker.key());
    }
    // D2 finding to confirm via the LiteSVM test: this function has no mechanism to move
    // tokens itself — it only ever inspects the accounts Token-2022 already passed in.
    // There is deliberately no CPI here that debits/credits anything.
    Ok(())
}

#[derive(Accounts)]
pub struct InitializeExtraAccountMetaList<'info> {
    #[account(mut)]
    pub payer: Signer<'info>,

    /// CHECK: PDA at ["extra-account-metas", mint], required address/seeds by the
    /// Transfer Hook Interface spec — not an Anchor-typed account.
    #[account(
        mut,
        seeds = [b"extra-account-metas", mint.key().as_ref()],
        bump,
    )]
    pub extra_account_meta_list: UncheckedAccount<'info>,

    /// CHECK: the Token-2022 mint this hook is attached to.
    pub mint: UncheckedAccount<'info>,

    /// CHECK: spike placeholder for what becomes the source's Capability PDA in Week 3.
    pub spike_marker: UncheckedAccount<'info>,

    pub system_program: Program<'info, System>,
}

// Note: there is deliberately no `#[derive(Accounts)]` struct for the Execute path.
// Token-2022 always reaches it through `fallback` with a raw `&[AccountInfo]` in the
// interface's fixed order (source, mint, destination, owner, extra_account_meta_list,
// then resolved extras) — see `spike_execute_logic` above.
