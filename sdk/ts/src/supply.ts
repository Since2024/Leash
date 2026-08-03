import { TOKEN_2022_PROGRAM_ID, getMint } from "@solana/spl-token";
import { PublicKey } from "@solana/web3.js";
import { MAX_ALLOWLIST_LEN, MAX_DEPTH } from "./constants";
import type { LeashPrograms } from "./programs";
import { type CapabilitySnapshot, decodeCapability } from "./watch";

/** Serialized size of a `Capability`, mirroring `Capability::MAX_SIZE` in `state.rs`.
 * Derived from the same two constants rather than hard-coded, so a change to either is a
 * type-level change here too instead of a silently-wrong `dataSize` filter. */
const CAPABILITY_ACCOUNT_SIZE =
  8 + // discriminator
  32 + // owner
  32 + // parent
  32 * MAX_DEPTH + // ancestors
  32 + // token_account
  32 + // wrapped_mint (docs/ROADMAP.md 0.12)
  8 * 4 + // cap, spent, committed_to_children, expiry
  (4 + 32 * MAX_ALLOWLIST_LEN) + // allowlist Vec<Pubkey>
  1 + // revoked
  1 + // depth
  1; // bump

/** Offset of `amount` in an SPL token account: mint (32) + owner (32). */
const TOKEN_AMOUNT_OFFSET = 64;

export interface StrandedCapability {
  address: PublicKey;
  tokenAccount: PublicKey;
  /** Units sitting in this capability's account that can never reach the vault. */
  amount: bigint;
  reason: "revoked" | "expired";
}

export interface SupplyReport {
  /** The wrapped mint's raw `supply`. Not a solvency figure — see below. */
  totalSupply: bigint;
  /** Units held by accounts that are not capability accounts — merchants who were paid.
   * Redeemable unconditionally. */
  merchantHeld: bigint;
  /** What live capabilities may still put into circulation, summed over the tree. */
  capabilityClaims: bigint;
  /** `merchantHeld + capabilityClaims` — the real outstanding claim on the vault, and the
   * figure to compare the vault balance against. */
  claimable: bigint;
  /** `totalSupply - claimable`: units that exist but can never reach the vault. */
  unbacked: bigint;
  /** The subset of `unbacked` sitting in dead delegated capabilities — the part 0.7's
   * inability to burn is responsible for. The rest is the live-delegation artifact
   * described above. */
  stranded: bigint;
  strandedCapabilities: StrandedCapability[];
}

/**
 * Splits the wrapped mint's supply into the real claim on the vault and the part that is
 * pure accounting artifact. **Compare the vault balance against `claimable`, never against
 * `totalSupply`.**
 *
 * Raw supply is not a solvency figure here, for two separate reasons — the second was
 * missed on the first pass and only showed up when this was run against a live validator,
 * where it reported 1_400 "redeemable" against a 1_000 vault:
 *
 * 1. **Live delegations double-count.** `attenuate` mints the child's units *fresh*
 *    rather than moving the parent's (BUILD_PLAN.md §5 D3), so after delegating 400 of
 *    1_000 the parent still holds 1_000 while only 600 is spendable. Supply is 1_400
 *    against a 1_000 deposit, and always exceeds the vault by the total delegated.
 * 2. **Dead delegations strand units.** `reclaim` releases a dead child's *budget* but
 *    cannot burn its *units* — that token account's authority is the child, not this
 *    program (docs/ROADMAP.md 0.7/0.10). They are inert but still counted.
 *
 * So this computes the claim directly instead of subtracting artifacts from supply:
 * units merchants already hold (unconditionally redeemable), plus what each capability may
 * still put into circulation, which is `cap - spent - committed_to_children`.
 *
 * A dead **delegated** capability contributes nothing: it can neither spend (the hook
 * checks `revoked`/`expiry`) nor redeem (`DelegatedCannotRedeem`). A dead **root** still
 * contributes its free budget, because `redeem` gates on `depth == 0` alone and consults
 * neither flag (`redeem.rs`) — so a root can always unwind. Treating roots as dead would
 * understate the claim, which is the more dangerous direction to be wrong in.
 *
 * Uses `getProgramAccounts`, which scans every account the program owns. Fine for an audit
 * or a dashboard refresh, not for a hot path.
 */
export async function redeemableSupply(
  programs: LeashPrograms,
  wrappedMint: PublicKey,
  nowUnixSeconds: number = Math.floor(Date.now() / 1000),
): Promise<SupplyReport> {
  const mint = await getMint(
    programs.connection,
    wrappedMint,
    "confirmed",
    TOKEN_2022_PROGRAM_ID,
  );
  const totalSupply = mint.supply;

  const accounts = await programs.connection.getProgramAccounts(
    programs.leashProgramId,
    {
      commitment: "confirmed",
      filters: [{ dataSize: CAPABILITY_ACCOUNT_SIZE }],
    },
  );

  const caps: { state: CapabilitySnapshot; address: PublicKey }[] = [];
  for (const { pubkey, account } of accounts) {
    try {
      caps.push({ state: decodeCapability(programs, account.data), address: pubkey });
    } catch {
      continue; // not a Capability, despite matching on size
    }
  }

  // Every capability's token-account balance, batched rather than one round trip each.
  const balances = new Map<string, bigint>();
  for (let i = 0; i < caps.length; i += 100) {
    const batch = caps.slice(i, i + 100);
    const infos = await programs.connection.getMultipleAccountsInfo(
      batch.map((c) => c.state.tokenAccount),
      "confirmed",
    );
    infos.forEach((info, j) => {
      balances.set(
        batch[j].state.tokenAccount.toBase58(),
        info ? info.data.readBigUInt64LE(TOKEN_AMOUNT_OFFSET) : 0n,
      );
    });
  }

  let capabilityHeld = 0n;
  let capabilityClaims = 0n;
  let stranded = 0n;
  const strandedCapabilities: StrandedCapability[] = [];

  for (const { state, address } of caps) {
    const balance = balances.get(state.tokenAccount.toBase58()) ?? 0n;
    capabilityHeld += balance;

    const free = state.cap - state.spent - state.committedToChildren;
    const isRoot = state.depth === 0;
    const dead = state.revoked
      ? "revoked"
      : nowUnixSeconds > state.expiry
        ? "expired"
        : undefined;

    if (isRoot || !dead) {
      // Bounded by the balance actually present: the claim cannot exceed the units.
      capabilityClaims += free < balance ? free : balance;
    } else {
      // Dead and delegated: contributes no claim, and whatever it holds is stranded.
      if (balance > 0n) {
        stranded += balance;
        strandedCapabilities.push({
          address,
          tokenAccount: state.tokenAccount,
          amount: balance,
          reason: dead,
        });
      }
    }
  }

  // Anything not sitting in a capability account is a merchant's, and redeems freely.
  const merchantHeld = totalSupply - capabilityHeld;
  const claimable = merchantHeld + capabilityClaims;

  return {
    totalSupply,
    merchantHeld,
    capabilityClaims,
    claimable,
    unbacked: totalSupply - claimable,
    stranded,
    strandedCapabilities,
  };
}
