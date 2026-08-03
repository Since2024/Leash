//! Proves the two halves of "a parent can take a delegation back" — docs/ROADMAP.md 0.8
//! (revoke one specific descendant) and 0.7 (release the budget it was holding).
//!
//! Before these, a principal had exactly one lever over a misbehaving agent: revoke
//! *itself*, which cascades to every descendant through the hook's ancestor walk. There
//! was no way to cut off one agent and leave the others running, and no way to ever get
//! the delegated budget back — `attenuate` incremented `committed_to_children` and nothing
//! decremented it, so both critical fixes (0.1 and 0.2, which correctly refuse to let the
//! parent spend or redeem reserved budget) left that share stranded permanently.
//!
//! The pairing is the point, and it is why these land together rather than separately:
//! revoking a child without releasing its reservation strands the budget, and releasing a
//! reservation without the child being provably dead would reopen 0.2 — the parent and
//! the child could each spend the same units. `reclaim` therefore refuses to run until
//! the child is revoked or expired, and `spent + committed_to_children <= cap` is asserted
//! at every step below.
//!
//! Rejections assert specific on-chain error codes (docs/ROADMAP.md 0.5).

mod common;

use anchor_spl::token_2022::spl_token_2022;
use anchor_lang::prelude::Pubkey;
use anchor_lang::solana_program::clock::Clock;
use common::*;
use litesvm::LiteSVM;
use solana_keypair::Keypair;
use solana_signer::Signer;

const FAR_FUTURE: i64 = 4_102_444_800; // 2100-01-01

fn merchant_ata(svm: &mut LiteSVM, s: &Setup) -> Pubkey {
    let merchant = Keypair::new();
    let dest = ata(&merchant.pubkey(), &s.wrapped_mint, &spl_token_2022::id());
    send(
        svm,
        &s.payer,
        &[],
        &[create_ata_ix(
            &s.payer.pubkey(),
            &merchant.pubkey(),
            &s.wrapped_mint,
            &spl_token_2022::id(),
        )],
    )
    .unwrap();
    dest
}

/// Assert the invariant the whole design turns on, at whatever point it is called.
fn assert_conserved(svm: &LiteSVM, capability: &Pubkey) {
    let c = capability_state(svm, capability);
    assert!(
        c.spent + c.committed_to_children <= c.cap,
        "conservation violated: spent={} committed={} cap={}",
        c.spent,
        c.committed_to_children,
        c.cap
    );
}

/// The end-to-end story: delegate, cut the agent off, take the budget back, spend it.
#[test]
fn parent_revokes_one_child_and_reclaims_its_budget() {
    let mut svm = LiteSVM::new();
    let s = setup(&mut svm);
    let dest = merchant_ata(&mut svm, &s);

    let parent_owner = Keypair::new();
    expect_ok(issue(&mut svm, &s, &parent_owner, 0, 1_000, FAR_FUTURE, vec![dest]));
    let (parent, parent_ta) = capability_for(&parent_owner.pubkey(), 0);

    // Two agents, so we can show revocation hits one and spares the other.
    let agent_a = Keypair::new();
    let agent_b = Keypair::new();
    expect_ok(attenuate(&mut svm, &s, &parent_owner, parent, &agent_a, 0, 400, FAR_FUTURE, vec![dest]));
    expect_ok(attenuate(&mut svm, &s, &parent_owner, parent, &agent_b, 0, 200, FAR_FUTURE, vec![dest]));
    let (child_a, child_a_ta) = capability_for(&agent_a.pubkey(), 0);
    let (child_b, child_b_ta) = capability_for(&agent_b.pubkey(), 0);

    assert_eq!(capability_state(&svm, &parent).committed_to_children, 600);
    // Parent may spend 1_000 - 0 - 600 = 400.
    assert_conserved(&svm, &parent);

    // Agent A spends part of its allowance before being cut off. Those units are really
    // gone — a merchant holds them and can redeem them — so they must NOT come back.
    expect_ok(spend(&mut svm, &s, &agent_a, &child_a_ta, &dest, 150));
    assert_eq!(capability_state(&svm, &child_a).spent, 150);

    // --- 0.8: the parent revokes agent A specifically. ---
    expect_ok(revoke_descendant(&mut svm, &s, &parent_owner, parent, child_a));
    assert!(capability_state(&svm, &child_a).revoked);
    assert!(!capability_state(&svm, &child_b).revoked); // B untouched
    assert!(!capability_state(&svm, &parent).revoked); // and the parent did not nuke itself

    expect_err_code(
        spend(&mut svm, &s, &agent_a, &child_a_ta, &dest, 1),
        "revoked agent spending",
        E_HOOK_REVOKED,
    );
    // B is unaffected: the parent cut off one delegation, not the subtree.
    expect_ok(spend(&mut svm, &s, &agent_b, &child_b_ta, &dest, 200));

    // --- 0.7: take back what A can no longer spend. ---
    expect_ok(reclaim(&mut svm, &s, &parent_owner, parent, child_a));

    // Released exactly the unspent 250 (400 cap - 150 spent), not the whole 400.
    assert_eq!(capability_state(&svm, &parent).committed_to_children, 350);
    let child_a_state = capability_state(&svm, &child_a);
    assert_eq!(child_a_state.cap, 150); // written down to `spent`
    assert_eq!(child_a_state.spent, 150);
    assert_conserved(&svm, &parent);

    // The reclaimed budget is genuinely usable again, which is the entire point. The
    // parent may now spend 1_000 - 0 - 350 = 650, up from 400.
    expect_err_code(
        spend(&mut svm, &s, &parent_owner, &parent_ta, &dest, 651),
        "parent spending one past its reclaimed budget",
        E_HOOK_CAP_EXCEEDED,
    );
    expect_ok(spend(&mut svm, &s, &parent_owner, &parent_ta, &dest, 650));
    assert_conserved(&svm, &parent);

    // Nothing over-spent against the deposit: the tree put 150 + 200 + 650 = 1_000 into
    // circulation, exactly the 1_000 deposited.
    assert_eq!(capability_state(&svm, &parent).spent, 650);
    assert_eq!(token_amount(&svm, &dest), 1_000);
}

/// Reclaiming twice must not credit the parent twice — the obvious way to mint budget
/// from nothing.
#[test]
fn reclaim_is_idempotent() {
    let mut svm = LiteSVM::new();
    let s = setup(&mut svm);
    let dest = merchant_ata(&mut svm, &s);

    let parent_owner = Keypair::new();
    expect_ok(issue(&mut svm, &s, &parent_owner, 0, 1_000, FAR_FUTURE, vec![dest]));
    let (parent, parent_ta) = capability_for(&parent_owner.pubkey(), 0);

    let agent = Keypair::new();
    expect_ok(attenuate(&mut svm, &s, &parent_owner, parent, &agent, 0, 400, FAR_FUTURE, vec![dest]));
    let (child, _) = capability_for(&agent.pubkey(), 0);

    expect_ok(revoke_descendant(&mut svm, &s, &parent_owner, parent, child));
    expect_ok(reclaim(&mut svm, &s, &parent_owner, parent, child));
    assert_eq!(capability_state(&svm, &parent).committed_to_children, 0);

    // Second call succeeds but releases nothing: `cap` was written down to `spent`, so
    // the computed unspent amount is zero. Succeeding rather than erroring keeps a
    // retried "revoke then reclaim" from being brittle.
    expect_ok(reclaim(&mut svm, &s, &parent_owner, parent, child));
    assert_eq!(capability_state(&svm, &parent).committed_to_children, 0);
    assert_conserved(&svm, &parent);

    // And the parent still cannot spend more than the deposit backs: exactly 1_000 works,
    // 1_001 does not. Once everything is reclaimed the parent's committed total is back to
    // zero, so its balance and its remaining budget are the same number again and
    // Token-2022's balance check is what fires — asserted as such rather than pretending
    // this exercises the hook's cap arithmetic (docs/ROADMAP.md 0.5). The isolated version
    // of that check lives in `week3_spend_enforcement.rs`.
    expect_err_code(
        spend(&mut svm, &s, &parent_owner, &parent_ta, &dest, 1_001),
        "parent spending past cap after a double reclaim",
        E_TOKEN_INSUFFICIENT_FUNDS,
    );
    expect_ok(spend(&mut svm, &s, &parent_owner, &parent_ta, &dest, 1_000));
}

/// A live child's budget cannot be pulled out from under it — that would reopen 0.2.
#[test]
fn reclaim_refuses_while_the_child_is_live() {
    let mut svm = LiteSVM::new();
    let s = setup(&mut svm);
    let dest = merchant_ata(&mut svm, &s);

    let parent_owner = Keypair::new();
    expect_ok(issue(&mut svm, &s, &parent_owner, 0, 1_000, FAR_FUTURE, vec![dest]));
    let (parent, _) = capability_for(&parent_owner.pubkey(), 0);

    let agent = Keypair::new();
    expect_ok(attenuate(&mut svm, &s, &parent_owner, parent, &agent, 0, 400, FAR_FUTURE, vec![dest]));
    let (child, child_ta) = capability_for(&agent.pubkey(), 0);

    expect_err_code(
        reclaim(&mut svm, &s, &parent_owner, parent, child),
        "reclaiming from a live child",
        E_LEASH_CHILD_STILL_LIVE,
    );
    assert_eq!(capability_state(&svm, &parent).committed_to_children, 400);

    // The child really is still live, which is why the refusal was right.
    expect_ok(spend(&mut svm, &s, &agent, &child_ta, &dest, 400));
}

/// Expiry is the other way a child dies, and it needs no revocation — this is the normal
/// end-of-life path for a time-boxed allowance.
#[test]
fn reclaim_works_on_an_expired_child_without_revoking() {
    let mut svm = LiteSVM::new();
    let s = setup(&mut svm);
    let dest = merchant_ata(&mut svm, &s);

    let mut clock: Clock = svm.get_sysvar();
    let child_expiry = clock.unix_timestamp + 100;

    let parent_owner = Keypair::new();
    expect_ok(issue(&mut svm, &s, &parent_owner, 0, 1_000, FAR_FUTURE, vec![dest]));
    let (parent, parent_ta) = capability_for(&parent_owner.pubkey(), 0);

    let agent = Keypair::new();
    expect_ok(attenuate(&mut svm, &s, &parent_owner, parent, &agent, 0, 400, child_expiry, vec![dest]));
    let (child, _) = capability_for(&agent.pubkey(), 0);

    // Still live: refused.
    expect_err_code(
        reclaim(&mut svm, &s, &parent_owner, parent, child),
        "reclaiming before the child expires",
        E_LEASH_CHILD_STILL_LIVE,
    );

    // One second past expiry: allowed, with no revoke call anywhere.
    clock.unix_timestamp = child_expiry + 1;
    svm.set_sysvar(&clock);
    expect_ok(reclaim(&mut svm, &s, &parent_owner, parent, child));
    assert_eq!(capability_state(&svm, &parent).committed_to_children, 0);
    assert!(!capability_state(&svm, &child).revoked); // never revoked, just aged out

    expect_ok(spend(&mut svm, &s, &parent_owner, &parent_ta, &dest, 1_000));
    assert_conserved(&svm, &parent);
}

/// Reclaiming from a middle node — one that has delegated onward itself — must not break
/// conservation on that node.
///
/// Regression for a bug `fuzz_conservation.rs` found (seed 1, step 31). `reclaim`
/// originally released `cap - spent` and wrote `cap` down to `spent`, which ignored budget
/// the child had already committed to *its* children: the middle node ended up with
/// `cap = 0` while still holding `committed_to_children = 97`, so the invariant read
/// `0 + 97 <= 0`. Nothing could be overspent — the grandchild's spends die on the ancestor
/// walk — but the number the program advertises and computes with was false.
///
/// The fix releases only the free portion, leaving `cap == spent + committed_to_children`,
/// and makes deep recovery a bottom-up walk: reclaim the grandchild first, then the child.
#[test]
fn reclaiming_from_a_middle_node_preserves_conservation() {
    let mut svm = LiteSVM::new();
    let s = setup(&mut svm);
    let dest = merchant_ata(&mut svm, &s);

    let root_owner = Keypair::new();
    expect_ok(issue(&mut svm, &s, &root_owner, 0, 1_000, FAR_FUTURE, vec![dest]));
    let (root, root_ta) = capability_for(&root_owner.pubkey(), 0);

    // root -> mid(500) -> leaf(97)
    let mid_owner = Keypair::new();
    expect_ok(attenuate(&mut svm, &s, &root_owner, root, &mid_owner, 0, 500, FAR_FUTURE, vec![dest]));
    let (mid, _) = capability_for(&mid_owner.pubkey(), 0);

    let leaf_owner = Keypair::new();
    expect_ok(attenuate(&mut svm, &s, &mid_owner, mid, &leaf_owner, 0, 97, FAR_FUTURE, vec![dest]));
    let (leaf, leaf_ta) = capability_for(&leaf_owner.pubkey(), 0);
    assert_eq!(capability_state(&svm, &mid).committed_to_children, 97);

    // The root cuts off the whole mid subtree and reclaims.
    expect_ok(revoke_descendant(&mut svm, &s, &root_owner, root, mid));
    expect_ok(reclaim(&mut svm, &s, &root_owner, root, mid));

    // The invariant must still hold on `mid`, which is what regressed.
    let m = capability_state(&svm, &mid);
    assert!(
        m.spent + m.committed_to_children <= m.cap,
        "conservation violated on the middle node: spent={} committed={} cap={}",
        m.spent, m.committed_to_children, m.cap,
    );
    // Only mid's free 403 came back; the 97 under the leaf stays reserved for now.
    assert_eq!(m.cap, 97);
    assert_eq!(m.committed_to_children, 97);
    assert_eq!(capability_state(&svm, &root).committed_to_children, 97);

    // The leaf cannot spend — its ancestor is revoked — so nothing was over-released.
    expect_err_code(
        spend(&mut svm, &s, &leaf_owner, &leaf_ta, &dest, 1),
        "leaf spending under a revoked middle node",
        E_HOOK_PARENT_REVOKED,
    );

    // Bottom-up recovery. Note the leaf must be revoked *in its own right* first:
    // `reclaim` checks the child's own `revoked`/`expiry`, and revoking an ancestor stops
    // a capability spending without setting its own flag. So "dead because an ancestor
    // died" is not something `reclaim` infers — it would need the ancestor chain passed
    // in to do so. Each level therefore costs an explicit revoke, which is more calls but
    // never guesses that a live capability is finished with.
    expect_err_code(
        reclaim(&mut svm, &s, &mid_owner, mid, leaf),
        "reclaiming a leaf that is unspendable but not itself revoked",
        E_LEASH_CHILD_STILL_LIVE,
    );
    expect_ok(revoke_descendant(&mut svm, &s, &mid_owner, mid, leaf));
    expect_ok(reclaim(&mut svm, &s, &mid_owner, mid, leaf));
    assert_eq!(capability_state(&svm, &mid).committed_to_children, 0);
    expect_ok(reclaim(&mut svm, &s, &root_owner, root, mid));
    assert_eq!(capability_state(&svm, &root).committed_to_children, 0);

    // And the root's full deposit is spendable again.
    expect_ok(spend(&mut svm, &s, &root_owner, &root_ta, &dest, 1_000));
    assert_conserved(&svm, &root);
}

/// Authority: only a real ancestor may revoke, and only the immediate parent may reclaim.
#[test]
fn authority_is_checked_on_both_instructions() {
    let mut svm = LiteSVM::new();
    let s = setup(&mut svm);
    let dest = merchant_ata(&mut svm, &s);

    // Two unrelated trees.
    let owner_x = Keypair::new();
    expect_ok(issue(&mut svm, &s, &owner_x, 0, 1_000, FAR_FUTURE, vec![dest]));
    let (cap_x, _) = capability_for(&owner_x.pubkey(), 0);

    let owner_y = Keypair::new();
    expect_ok(issue(&mut svm, &s, &owner_y, 0, 1_000, FAR_FUTURE, vec![dest]));
    let (cap_y, _) = capability_for(&owner_y.pubkey(), 0);

    let agent = Keypair::new();
    expect_ok(attenuate(&mut svm, &s, &owner_x, cap_x, &agent, 0, 400, FAR_FUTURE, vec![dest]));
    let (child_x, _) = capability_for(&agent.pubkey(), 0);

    // A stranger's capability is not an ancestor of X's child, even though it is a real
    // capability its signer really owns.
    expect_err_code(
        revoke_descendant(&mut svm, &s, &owner_y, cap_y, child_x),
        "unrelated capability revoking someone else's child",
        E_LEASH_NOT_AN_ANCESTOR,
    );
    assert!(!capability_state(&svm, &child_x).revoked);

    // Nor can a stranger reclaim against it — and note this is a *different* error, since
    // the parent linkage is what fails rather than the ancestry proof.
    expect_ok(revoke_descendant(&mut svm, &s, &owner_x, cap_x, child_x));
    expect_err_code(
        reclaim(&mut svm, &s, &owner_y, cap_y, child_x),
        "unrelated capability reclaiming someone else's child",
        E_LEASH_NOT_A_CHILD,
    );
    assert_eq!(capability_state(&svm, &cap_y).committed_to_children, 0);

    // The real parent can.
    expect_ok(reclaim(&mut svm, &s, &owner_x, cap_x, child_x));
    assert_eq!(capability_state(&svm, &cap_x).committed_to_children, 0);
}

/// A grandparent may revoke a grandchild — authority is the whole ancestor chain, not just
/// the immediate parent — but it may not reclaim, because the reservation isn't its own.
#[test]
fn a_grandparent_may_revoke_but_only_the_parent_may_reclaim() {
    let mut svm = LiteSVM::new();
    let s = setup(&mut svm);
    let dest = merchant_ata(&mut svm, &s);

    let root_owner = Keypair::new();
    expect_ok(issue(&mut svm, &s, &root_owner, 0, 1_000, FAR_FUTURE, vec![dest]));
    let (root, _) = capability_for(&root_owner.pubkey(), 0);

    let mid_owner = Keypair::new();
    expect_ok(attenuate(&mut svm, &s, &root_owner, root, &mid_owner, 0, 500, FAR_FUTURE, vec![dest]));
    let (mid, _) = capability_for(&mid_owner.pubkey(), 0);

    let leaf_owner = Keypair::new();
    expect_ok(attenuate(&mut svm, &s, &mid_owner, mid, &leaf_owner, 0, 200, FAR_FUTURE, vec![dest]));
    let (leaf, leaf_ta) = capability_for(&leaf_owner.pubkey(), 0);
    assert_eq!(capability_state(&svm, &leaf).depth, 2);

    // The root is the leaf's grandparent: allowed to revoke it directly.
    expect_ok(revoke_descendant(&mut svm, &s, &root_owner, root, leaf));
    assert!(capability_state(&svm, &leaf).revoked);
    expect_err_code(
        spend(&mut svm, &s, &leaf_owner, &leaf_ta, &dest, 1),
        "spending a grandparent-revoked capability",
        E_HOOK_REVOKED,
    );

    // But the leaf's reservation sits on `mid`, not on `root`, so the root has nothing to
    // release and must not be able to touch its own counter using someone else's child.
    expect_err_code(
        reclaim(&mut svm, &s, &root_owner, root, leaf),
        "grandparent reclaiming a grandchild's reservation",
        E_LEASH_NOT_A_CHILD,
    );
    assert_eq!(capability_state(&svm, &root).committed_to_children, 500);

    // `mid` — the actual parent — can, and its own reserved total drops accordingly.
    assert_eq!(capability_state(&svm, &mid).committed_to_children, 200);
    expect_ok(reclaim(&mut svm, &s, &mid_owner, mid, leaf));
    assert_eq!(capability_state(&svm, &mid).committed_to_children, 0);
    assert_conserved(&svm, &mid);
    assert_conserved(&svm, &root);
}

/// Revocation is total: a revoked capability cannot delegate either.
///
/// This was unchecked until the docs/ROADMAP.md 0.12 sweep. It was never exploitable — the
/// child inherits a revoked ancestor and so can never spend, and the reservation lands on
/// the revoked parent's own counter — but a dead capability could mint fresh units that
/// nothing would ever be able to use, inflating the wrapped supply against an unchanged
/// vault and locking its own remaining budget behind a `revoke_descendant` + `reclaim`
/// round trip. "Revoked" should mean finished.
#[test]
fn a_revoked_capability_cannot_attenuate() {
    let mut svm = LiteSVM::new();
    let s = setup(&mut svm);

    let principal = Keypair::new();
    let agent = Keypair::new();
    expect_ok(issue(&mut svm, &s, &principal, 0, 1_000, FAR_FUTURE, vec![]));
    let (root, _) = capability_for(&principal.pubkey(), 0);

    // Delegating works right up until the moment it shouldn't.
    expect_ok(attenuate(
        &mut svm, &s, &principal, root, &agent, 1, 200, FAR_FUTURE, vec![],
    ));
    assert_eq!(capability_state(&svm, &root).committed_to_children, 200);

    expect_ok(revoke(&mut svm, &s, &principal, root));

    let second_agent = Keypair::new();
    expect_err_code(
        attenuate(
            &mut svm, &s, &principal, root, &second_agent, 2, 300, FAR_FUTURE, vec![],
        ),
        "attenuating from a revoked capability",
        E_LEASH_REVOKED,
    );

    // Nothing moved: no reservation, and no units minted for a child that can never spend.
    assert_eq!(
        capability_state(&svm, &root).committed_to_children,
        200,
        "a rejected attenuation must not reserve budget"
    );
    let (_, second_agent_tokens) = capability_for(&second_agent.pubkey(), 2);
    assert!(
        svm.get_account(&second_agent_tokens)
            .map_or(true, |a| a.data.is_empty()),
        "a rejected attenuation must not mint anything"
    );
}
