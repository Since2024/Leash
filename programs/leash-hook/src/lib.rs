use anchor_lang::prelude::*;

pub mod error;
use error::LeashHookError;

declare_id!("9WPQUY6zVRwVZ3eUsDF1aNESWAyZwL8GwKpzd2C66xtS");

const HOOK_AUTHORITY_SEED: &[u8] = b"hook-authority";

/// Real spend-path enforcement (Week 3/4, BUILD_PLAN.md §4/§7). Checks cap/expiry/
/// allowlist/revoked, and now the full ancestor chain up to MAX_DEPTH (Week 3 checked
/// only one level). See docs/BUILD_PLAN.md §5 "Week 1 spike results" and the doc
/// comments below for what changed since the Week 1 placeholder.
#[program]
pub mod leash_hook {
    use super::*;
    use anchor_lang::solana_program::program::invoke_signed;
    use anchor_lang::solana_program::system_instruction;
    use spl_tlv_account_resolution::account::ExtraAccountMeta;
    use spl_tlv_account_resolution::pubkey_data::PubkeyData;
    use spl_tlv_account_resolution::seeds::Seed;
    use spl_tlv_account_resolution::state::ExtraAccountMetaList;
    use spl_transfer_hook_interface::instruction::ExecuteInstruction;

    /// D4, resolved for real: registers the source's own Capability PDA and its parent,
    /// derived dynamically per-transfer instead of Week 1's fixed placeholder.
    ///
    /// Every Capability (root from `issue`, or child from `attenuate`) is seeded as
    /// `[CAPABILITY_SEED, owner]` — one fixed formula regardless of root/child status,
    /// which is exactly what makes deriving it here possible (see attenuate.rs's doc
    /// comment on why the child seed scheme had to change to match).
    ///
    /// Extra accounts registered, in order (indices 5-10, after the 5 base accounts):
    ///   5. leash_program's own ID — needed as the "owning program" anchor for the PDA
    ///      derivation below (Capability accounts belong to leash-program, not this
    ///      hook), and reused as the CPI target account in `spend_logic`.
    ///   6. source_capability — external PDA, `[CAPABILITY_SEED, owner]` under
    ///      leash-program. Writable: `record_spend` (via CPI) mutates it.
    ///   7-9. ancestor1 / ancestor2 / ancestor3 — each read directly out of
    ///      **source_capability's own** `ancestors` array (`ANCESTORS_FIELD_OFFSET`),
    ///      not chained through each ancestor's own `parent` field one hop at a time.
    ///      The chained version was tried first and broke: a root capability's `parent`
    ///      resolves to the System Program's placeholder address, which has zero
    ///      account data, so trying to read a "further parent" out of *that* data fails
    ///      at the client-side resolution step (confirmed by hitting exactly this
    ///      failure, not by anticipating it — see docs/BUILD_PLAN.md §5 Week 4 results).
    ///      Storing the full ancestor chain directly on each Capability (`state.rs`)
    ///      avoids ever reading from a potentially-empty account. Whichever slots a
    ///      transfer's capability doesn't actually have resolve to Pubkey::default()
    ///      (harmless here, since they're read from real, always-populated data) —
    ///      `spend_logic` only checks as many as `capability.depth` says exist.
    ///   10. hook_authority — this program's own PDA, used purely as the CPI signer for
    ///      `record_spend`. Not derived from anything transfer-specific.
    pub fn initialize_extra_account_meta_list(
        ctx: Context<InitializeExtraAccountMetaList>,
    ) -> Result<()> {
        let account_metas = vec![
            ExtraAccountMeta::new_with_pubkey(&leash_program::ID, false, false)?,
            ExtraAccountMeta::new_external_pda_with_seeds(
                5,
                &[
                    Seed::Literal {
                        bytes: leash_program::CAPABILITY_SEED.as_bytes().to_vec(),
                    },
                    Seed::AccountKey { index: 3 }, // "owner" — base account 3
                ],
                false,
                true,
            )?,
            // ancestor1 (index 7): source_capability's (index 6) ancestors[0].
            ExtraAccountMeta::new_with_pubkey_data(
                &PubkeyData::AccountData {
                    account_index: 6,
                    data_index: leash_program::state::ANCESTORS_FIELD_OFFSET,
                },
                false,
                false,
            )?,
            // ancestor2 (index 8): source_capability's (index 6) ancestors[1].
            ExtraAccountMeta::new_with_pubkey_data(
                &PubkeyData::AccountData {
                    account_index: 6,
                    data_index: leash_program::state::ANCESTORS_FIELD_OFFSET + 32,
                },
                false,
                false,
            )?,
            // ancestor3 (index 9): source_capability's (index 6) ancestors[2].
            ExtraAccountMeta::new_with_pubkey_data(
                &PubkeyData::AccountData {
                    account_index: 6,
                    data_index: leash_program::state::ANCESTORS_FIELD_OFFSET + 64,
                },
                false,
                false,
            )?,
            ExtraAccountMeta::new_with_seeds(
                &[Seed::Literal {
                    bytes: HOOK_AUTHORITY_SEED.to_vec(),
                }],
                false,
                false,
            )?,
        ];

        let bump_seed = [ctx.bumps.extra_account_meta_list];
        let mint_key = ctx.accounts.mint.key();
        let signer_seeds =
            spl_transfer_hook_interface::collect_extra_account_metas_signer_seeds(
                &mint_key,
                &bump_seed,
            );
        let account_size = ExtraAccountMetaList::size_of(account_metas.len())?;

        // Fund the PDA to rent-exemption before allocate/assign — see Week 1's finding
        // in docs/BUILD_PLAN.md §5 on why skipping this silently discards the account.
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

    /// Bridges the SPL Transfer Hook Interface's raw `Execute` instruction into
    /// `spend_logic` below. See Week 1's finding (docs/BUILD_PLAN.md §5) on why this
    /// can't route through Anchor's generated `__private::__global` dispatcher.
    pub fn fallback<'info>(
        _program_id: &Pubkey,
        accounts: &'info [AccountInfo<'info>],
        data: &[u8],
    ) -> Result<()> {
        use spl_transfer_hook_interface::instruction::TransferHookInstruction;

        match TransferHookInstruction::unpack(data)? {
            TransferHookInstruction::Execute { amount } => spend_logic(accounts, amount),
            _ => Err(ProgramError::InvalidInstructionData.into()),
        }
    }
}

/// The six non-negotiable properties from BUILD_PLAN.md §2, including — as of Week 4 —
/// the full ancestor chain up to MAX_DEPTH, not just one level. Reads each capability's
/// fields via normal Anchor/Borsh deserialization (only the seed *derivation* step
/// needed fixed byte offsets, not this) and, if every check passes, commits the spend
/// via CPI into leash-program's `record_spend` — this program owns no Capability
/// accounts and cannot write `spent` itself.
fn spend_logic(accounts: &[AccountInfo], amount: u64) -> Result<()> {
    let destination = &accounts[2];
    // accounts[5] (leash_program's own account) doesn't need to be referenced here:
    // invoke_signed resolves the CPI target via its Pubkey (leash_program::ID, used
    // below) plus whatever's already loaded in the transaction's account set — it
    // doesn't need the program's AccountInfo re-passed explicitly (confirmed already in
    // Week 1/2: anchor_spl's own transfer/mint_to/burn CPI helpers never pass the token
    // program's AccountInfo either). Registering it as extra account #5 is still
    // required, though — that's what guarantees it's loaded in the transaction at all,
    // which CPI-ing into it depends on.
    let source_capability_info = &accounts[6];
    // ancestor1/2/3 live at 7/8/9 (see initialize_extra_account_meta_list's doc
    // comment); hook_authority shifted to 10 to make room for them.
    let hook_authority_info = &accounts[10];

    let capability: leash_program::state::Capability = {
        let data = source_capability_info.try_borrow_data()?;
        AnchorDeserialize::deserialize(&mut &data[8..])
            .map_err(|_| ProgramError::InvalidAccountData)?
    };

    require!(!capability.revoked, LeashHookError::Revoked);
    let now = Clock::get()?.unix_timestamp;
    require!(now <= capability.expiry, LeashHookError::Expired);
    require!(
        capability
            .allowlist
            .iter()
            .any(|d| d == destination.key),
        LeashHookError::NotAllowlisted
    );
    // A spend has to fit in the budget that is actually still free — the cap minus what
    // has already been spent AND minus what `attenuate` has already reserved for
    // children. Checking only `spent + amount <= cap` lets a parent spend budget it
    // already delegated: since attenuation *mints* the child's units rather than moving
    // the parent's, the parent's token account still holds the full original balance,
    // so nothing else stops it. Parent and child could then each spend that same $20,
    // putting more units into circulation than the vault backs.
    //
    // This is the invariant `state.rs` states on the Capability struct and BUILD_PLAN §2
    // promises: `spent + committed_to_children <= cap`. See docs/ROADMAP.md 0.2.
    let obligations = amount
        .checked_add(capability.spent)
        .and_then(|v| v.checked_add(capability.committed_to_children))
        .ok_or(LeashHookError::CapExceeded)?;
    require!(obligations <= capability.cap, LeashHookError::CapExceeded);

    // Walk every real ancestor, bounded by `capability.depth` (never MAX_DEPTH itself —
    // a depth-1 capability has exactly one real ancestor, at accounts[7]; anything
    // beyond `depth` resolves to the System Program placeholder and must never be
    // deserialized as a Capability). `level` 0 -> accounts[7] (immediate parent),
    // `level` 1 -> accounts[8] (grandparent), `level` 2 -> accounts[9]
    // (great-grandparent) — exactly matching MAX_DEPTH = 3's three ancestor slots.
    for level in 0..capability.depth {
        let ancestor_info = &accounts[7 + level as usize];
        let ancestor: leash_program::state::Capability = {
            let data = ancestor_info.try_borrow_data()?;
            AnchorDeserialize::deserialize(&mut &data[8..])
                .map_err(|_| ProgramError::InvalidAccountData)?
        };
        require!(!ancestor.revoked, LeashHookError::ParentRevoked);
    }

    msg!(
        "leash-hook: spend ok, amount = {}, capability = {}",
        amount,
        source_capability_info.key()
    );

    let (_, bump) = Pubkey::find_program_address(&[HOOK_AUTHORITY_SEED], &crate::ID);
    let signer_seeds: &[&[u8]] = &[HOOK_AUTHORITY_SEED, &[bump]];

    leash_program::cpi::record_spend(
        CpiContext::new_with_signer(
            leash_program::ID,
            leash_program::cpi::accounts::RecordSpend {
                hook_authority: hook_authority_info.clone(),
                capability: source_capability_info.clone(),
            },
            &[signer_seeds],
        ),
        amount,
    )?;

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

    pub system_program: Program<'info, System>,
}

// Note: there is deliberately no `#[derive(Accounts)]` struct for the Execute path.
// Token-2022 always reaches it through `fallback` with a raw `&[AccountInfo]` in the
// interface's fixed order (source, mint, destination, owner, extra_account_meta_list,
// then the six resolved extras above: leash_program, source_capability, ancestor1-3,
// hook_authority) — see `spend_logic`.
