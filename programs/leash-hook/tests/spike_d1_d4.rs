//! Week 1 spike (docs/BUILD_PLAN.md §5, §7). Proves the primitives compose:
//!
//! D1: a Token-2022 mint with the TransferHook extension is created fresh (nothing here
//!     retrofits a hook onto an existing plain-SPL-Token mint — confirming wrapping is
//!     required, not optional).
//! D4: `initialize_extra_account_meta_list` registers one extra account, and a real
//!     `transfer_checked` resolves + passes it into the hook automatically.
//! D2: the hook (`leash-hook`'s `spike_execute_logic`) only reads source/destination —
//!     there is no CPI inside it that moves tokens itself. The transfer's actual value
//!     movement is entirely Token-2022's own doing, before/around the hook CPI.
//! D3: a separate mint-authority-signed `mint_to` succeeds against the same wrapped
//!     mint, standing in for what `attenuate` will do in Week 3 (mint to a new account,
//!     not transfer — no hook involvement expected for a mint, only for a transfer).

use anchor_lang::{InstructionData, ToAccountMetas};
use litesvm::LiteSVM;
use solana_keypair::Keypair;
use solana_message::{Message, VersionedMessage};
use solana_signer::Signer;
use solana_transaction::versioned::VersionedTransaction;
// Reuse anchor-spl's own re-export of spl-token-2022-interface rather than adding a
// second, separately-versioned direct dependency — pulling in our own copy caused a
// real duplicate-crate-version conflict (two incompatible `wincode` versions) that this
// spike hit and is worth recording: pin to what anchor-spl already provides internally
// rather than adding parallel SPL crates alongside it.
use anchor_spl::token_2022::spl_token_2022 as token_2022;

use anchor_lang::prelude::Pubkey;

const HOOK_ID: Pubkey = leash_hook::ID;

fn send(svm: &mut LiteSVM, payer: &Keypair, signers: &[&Keypair], ixs: &[anchor_lang::solana_program::instruction::Instruction]) {
    let blockhash = svm.latest_blockhash();
    let msg = Message::new_with_blockhash(ixs, Some(&payer.pubkey()), &blockhash);
    let mut all_signers = vec![payer];
    all_signers.extend_from_slice(signers);
    let tx = VersionedTransaction::try_new(VersionedMessage::Legacy(msg), &all_signers).unwrap();
    let res = svm.send_transaction(tx);
    assert!(res.is_ok(), "transaction failed: {:?}", res.err());
    for line in &res.unwrap().logs {
        eprintln!("  log: {}", line);
    }
}

#[test]
fn d1_d2_d4_wrapped_mint_with_hook_and_real_transfer() {
    let mut svm = LiteSVM::new();

    let payer = Keypair::new();
    svm.airdrop(&payer.pubkey(), 10_000_000_000).unwrap();

    let hook_bytes = include_bytes!("../../../target/deploy/leash_hook.so");
    svm.add_program(HOOK_ID, hook_bytes).unwrap();

    // --- D1: create a brand-new Token-2022 mint with the TransferHook extension. ---
    // This is the wrapped asset (BUILD_PLAN.md §3/§5 D1) — not real USDC, which is a
    // plain SPL Token mint and cannot take this extension.
    let mint = Keypair::new();
    let mint_authority = Keypair::new();

    let extensions = [token_2022::extension::ExtensionType::TransferHook];
    let space = token_2022::extension::ExtensionType::try_calculate_account_len::<
        token_2022::state::Mint,
    >(&extensions)
    .unwrap();
    let rent = svm.minimum_balance_for_rent_exemption(space);

    let create_mint_account_ix = anchor_lang::solana_program::system_instruction::create_account(
        &payer.pubkey(),
        &mint.pubkey(),
        rent,
        space as u64,
        &token_2022::id(),
    );

    let init_transfer_hook_ix = token_2022::extension::transfer_hook::instruction::initialize(
        &token_2022::id(),
        &mint.pubkey(),
        Some(mint_authority.pubkey()),
        Some(HOOK_ID),
    )
    .unwrap();

    let init_mint_ix = token_2022::instruction::initialize_mint2(
        &token_2022::id(),
        &mint.pubkey(),
        &mint_authority.pubkey(),
        None,
        0,
    )
    .unwrap();

    send(
        &mut svm,
        &payer,
        &[&mint],
        &[create_mint_account_ix, init_transfer_hook_ix, init_mint_ix],
    );

    // --- D4: register the extra account meta list on leash-hook. ---
    let (extra_account_meta_list, _) = Pubkey::find_program_address(
        &[b"extra-account-metas", mint.pubkey().as_ref()],
        &HOOK_ID,
    );
    let spike_marker = Keypair::new().pubkey(); // stand-in for the future Capability PDA

    let init_extra_metas_ix = anchor_lang::solana_program::instruction::Instruction {
        program_id: HOOK_ID,
        accounts: leash_hook::accounts::InitializeExtraAccountMetaList {
            payer: payer.pubkey(),
            extra_account_meta_list,
            mint: mint.pubkey(),
            spike_marker,
            system_program: anchor_lang::solana_program::system_program::ID,
        }
        .to_account_metas(None),
        data: leash_hook::instruction::InitializeExtraAccountMetaList {}.data(),
    };
    send(&mut svm, &payer, &[], &[init_extra_metas_ix]);

    // --- Set up source/destination token accounts and mint some supply to source. ---
    let source_owner = Keypair::new();
    let dest_owner = Keypair::new();
    let source_ata = anchor_spl::associated_token::spl_associated_token_account::address::get_associated_token_address_with_program_id(
        &source_owner.pubkey(),
        &mint.pubkey(),
        &token_2022::id(),
    );
    let dest_ata = anchor_spl::associated_token::spl_associated_token_account::address::get_associated_token_address_with_program_id(
        &dest_owner.pubkey(),
        &mint.pubkey(),
        &token_2022::id(),
    );

    let create_source_ata_ix = anchor_spl::associated_token::spl_associated_token_account::instruction::create_associated_token_account(
        &payer.pubkey(),
        &source_owner.pubkey(),
        &mint.pubkey(),
        &token_2022::id(),
    );
    let create_dest_ata_ix = anchor_spl::associated_token::spl_associated_token_account::instruction::create_associated_token_account(
        &payer.pubkey(),
        &dest_owner.pubkey(),
        &mint.pubkey(),
        &token_2022::id(),
    );
    send(&mut svm, &payer, &[], &[create_source_ata_ix, create_dest_ata_ix]);

    let mint_to_ix = token_2022::instruction::mint_to(
        &token_2022::id(),
        &mint.pubkey(),
        &source_ata,
        &mint_authority.pubkey(),
        &[],
        1_000,
    )
    .unwrap();
    send(&mut svm, &payer, &[&mint_authority], &[mint_to_ix]);

    // --- D2/D4: a real transfer_checked, with hook accounts resolved and attached. ---
    let mut transfer_ix = token_2022::instruction::transfer_checked(
        &token_2022::id(),
        &source_ata,
        &mint.pubkey(),
        &dest_ata,
        &source_owner.pubkey(),
        &[],
        100,
        0,
    )
    .unwrap();

    eprintln!(
        "mint={} extra_account_meta_list={} source_ata={} dest_ata={} hook={}",
        mint.pubkey(),
        extra_account_meta_list,
        source_ata,
        dest_ata,
        HOOK_ID
    );

    // Resolve the extra accounts the hook needs, reading the on-chain
    // ExtraAccountMetaList this test just initialized — the same resolution a real
    // client (or our future TS SDK) has to do before submitting a transfer. This helper
    // is async (it's designed for a real RPC client); LiteSVM's `get_account` is
    // synchronous, so a minimal `futures::executor::block_on` is enough here — no need
    // for a full async runtime just to drive one already-resolved future.
    futures::executor::block_on(
        spl_transfer_hook_interface::offchain::add_extra_account_metas_for_execute(
            &mut transfer_ix,
            &HOOK_ID,
            &source_ata,
            &mint.pubkey(),
            &dest_ata,
            &source_owner.pubkey(),
            100,
            |pubkey| {
                let account = svm.get_account(&pubkey);
                async move { Ok(account.map(|a| a.data)) }
            },
        ),
    )
    .unwrap();

    send(&mut svm, &payer, &[&source_owner], &[transfer_ix]);

    // If we got here, the hook fired (LiteSVM would have failed the transaction if the
    // hook program rejected it or the extra accounts didn't resolve) — D1, D2, D4 hold
    // for this minimal shape.

    // --- D3: mint-authority-signed mint_to against the same wrapped mint again,
    // standing in for what `attenuate` does (mint to a new account) in Week 3. No
    // transfer is involved, so no hook CPI should occur for this instruction at all. ---
    let child_owner = Keypair::new();
    let child_ata = anchor_spl::associated_token::spl_associated_token_account::address::get_associated_token_address_with_program_id(
        &child_owner.pubkey(),
        &mint.pubkey(),
        &token_2022::id(),
    );
    let create_child_ata_ix = anchor_spl::associated_token::spl_associated_token_account::instruction::create_associated_token_account(
        &payer.pubkey(),
        &child_owner.pubkey(),
        &mint.pubkey(),
        &token_2022::id(),
    );
    send(&mut svm, &payer, &[], &[create_child_ata_ix]);

    let attenuate_style_mint_ix = token_2022::instruction::mint_to(
        &token_2022::id(),
        &mint.pubkey(),
        &child_ata,
        &mint_authority.pubkey(),
        &[],
        50,
    )
    .unwrap();
    send(&mut svm, &payer, &[&mint_authority], &[attenuate_style_mint_ix]);
}
