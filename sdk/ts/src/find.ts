import { PublicKey } from "@solana/web3.js";
import { capabilityTokenAccountPda } from "./pda";
import type { LeashPrograms } from "./programs";
import { type CapabilitySnapshot, decodeCapability } from "./watch";

/** Byte offset of `Capability.owner` in raw account data: straight after Anchor's 8-byte
 * discriminator. Must match `state.rs` — the layout there is fixed and documented as
 * load-bearing (leash-hook reads `parent`/`ancestors` out of raw bytes at fixed offsets),
 * so filtering on it is safe in the same way. */
const OWNER_OFFSET = 8;

export interface FoundCapability {
  /** The Capability account's address. */
  address: PublicKey;
  state: CapabilitySnapshot;
  /** The nonce this capability was created with, recovered by search — `undefined` if it
   * wasn't found within the scanned range. See `findCapabilitiesByOwner`. */
  nonce?: bigint;
}

/**
 * Every capability owned by `owner`, found by scanning program accounts.
 *
 * This exists because 0.3 made an owner's key stop identifying a single capability, which
 * left callers holding a nonce they must not lose (docs/ROADMAP.md 0.6). Losing it made
 * the capability unreachable from the SDK/CLI even though it was perfectly alive on-chain.
 * This is the cheap answer the roadmap calls for — a `memcmp` filter — rather than a
 * per-owner registry account, which would reintroduce a hot account and a write on every
 * `issue`.
 *
 * Two things to know before relying on it:
 *
 * - **`getProgramAccounts` is a heavy RPC call** and many public endpoints rate-limit or
 *   disable it. It scans every account owned by the program. Fine for a CLI or a
 *   dashboard refresh; not something to put on a hot path.
 * - **It finds capabilities, not nonces.** The nonce is an input to the token account's
 *   address, and hashing doesn't run backwards, so it cannot be read off the account. It
 *   is recovered only if `recoverNonceLimit` is set and the value is inside that range —
 *   see below. Everything else about the capability is returned regardless.
 */
export async function findCapabilitiesByOwner(
  programs: LeashPrograms,
  owner: PublicKey,
  opts: {
    /** Try to recover each capability's nonce by testing `0..limit`. Off by default: it
     * is a brute-force scan, and it only ever succeeds for small, sequential nonces —
     * which is exactly what `--nonce 0`, `--nonce 1` CLI usage produces, and never what
     * the SDK's `randomNonce()` produces. */
    recoverNonceLimit?: number;
  } = {},
): Promise<FoundCapability[]> {
  const accounts = await programs.connection.getProgramAccounts(
    programs.leashProgramId,
    {
      commitment: "confirmed",
      filters: [{ memcmp: { offset: OWNER_OFFSET, bytes: owner.toBase58() } }],
    },
  );

  const found: FoundCapability[] = [];
  for (const { pubkey, account } of accounts) {
    // The owner filter is an offset match, not a type check: any account this program
    // owns whose bytes 8..40 happen to equal `owner` comes back. Capability is the only
    // account type leash-program defines today, but decoding is what actually confirms
    // that, so a failure here is skipped rather than thrown.
    let state: CapabilitySnapshot;
    try {
      state = decodeCapability(programs, account.data);
    } catch {
      continue;
    }
    found.push({ address: pubkey, state });
  }

  if (opts.recoverNonceLimit !== undefined) {
    for (const entry of found) {
      entry.nonce = recoverNonce(
        owner,
        entry.state.tokenAccount,
        programs.leashProgramId,
        opts.recoverNonceLimit,
      );
    }
  }

  return found;
}

/**
 * Recovers the nonce behind a known token account by deriving `0..limit` and comparing.
 *
 * Brute force is the only option — the nonce is hashed into the address, so it cannot be
 * read back out. Worth being blunt about the consequence: this finds sequential nonces a
 * human typed and will essentially never find one from `randomNonce()`. It is a
 * convenience for the common CLI case, not a recovery mechanism to depend on. The durable
 * answer is to keep the nonce that `mint`/`attenuate` return.
 */
export function recoverNonce(
  owner: PublicKey,
  tokenAccount: PublicKey,
  programId: PublicKey,
  limit: number,
): bigint | undefined {
  for (let n = 0; n < limit; n++) {
    if (capabilityTokenAccountPda(owner, BigInt(n), programId).equals(tokenAccount)) {
      return BigInt(n);
    }
  }
  return undefined;
}
