import { Keypair, PublicKey } from "@solana/web3.js";
import type { LeashPrograms } from "./programs";

/** `revoke()` — flips `revoked = true`. No token movement. Takes effect on the very
 * next spend attempt against this capability or, up to MAX_DEPTH, any of its
 * descendants — because leash-hook walks the ancestor chain on every spend. */
export async function revoke(
  programs: LeashPrograms,
  owner: Keypair,
  capability: PublicKey,
): Promise<string> {
  return programs.leashProgram.methods
    .revoke()
    .accounts({
      owner: owner.publicKey,
      capability,
    } as never)
    .signers([owner])
    .rpc();
}

/** `revokeDescendant()` — revoke a capability *below* you in the tree, without revoking
 * yourself (docs/ROADMAP.md 0.8).
 *
 * `revoke` above only ever works on a capability you hold, so a principal's sole lever
 * over a misbehaving agent used to be revoking itself — which cascades to every
 * descendant. This cuts off one delegation and leaves the siblings running.
 *
 * `ancestorCapability` must be one of `descendantCapability`'s recorded ancestors; any
 * ancestor qualifies, not just the immediate parent. Note this only stops future
 * spending — to get the reserved budget back, follow it with `reclaim`. */
export async function revokeDescendant(
  programs: LeashPrograms,
  owner: Keypair,
  ancestorCapability: PublicKey,
  descendantCapability: PublicKey,
): Promise<string> {
  return programs.leashProgram.methods
    .revokeDescendant()
    .accounts({
      owner: owner.publicKey,
      ancestorCapability,
      descendantCapability,
    } as never)
    .signers([owner])
    .rpc();
}

/** `reclaim()` — release budget reserved for a child that can no longer spend it
 * (docs/ROADMAP.md 0.7), so the parent can spend or redeem it again.
 *
 * Requires the child to be revoked or past its expiry; against a live child the program
 * rejects with `ChildStillLive`, because releasing a reservation the child could still
 * draw on would let parent and child spend the same units.
 *
 * Only the **immediate** parent may call this — `attenuate` records the reservation there
 * and nowhere else. Safe to call twice: the second call releases nothing rather than
 * crediting the parent again.
 *
 * Accounting only. The child's unspent units are not burned and cannot be: the token
 * account's authority is the child, not this program. They are inert (unspendable and
 * unredeemable), but they do still count toward the mint's total supply — see
 * `reclaim.rs` and docs/ROADMAP.md 0.10. */
export async function reclaim(
  programs: LeashPrograms,
  owner: Keypair,
  parentCapability: PublicKey,
  childCapability: PublicKey,
): Promise<string> {
  return programs.leashProgram.methods
    .reclaim()
    .accounts({
      owner: owner.publicKey,
      parentCapability,
      childCapability,
    } as never)
    .signers([owner])
    .rpc();
}
