use anchor_lang::prelude::*;

#[error_code]
pub enum LeashError {
    #[msg("spend would exceed this capability's remaining budget")]
    CapExceeded,
    #[msg("capability has expired")]
    Expired,
    #[msg("destination is not on this capability's allowlist")]
    NotAllowlisted,
    #[msg("capability, or an ancestor of it, has been revoked")]
    Revoked,
    #[msg("attenuation would exceed MAX_DEPTH")]
    DepthExceeded,
    #[msg("child cap/expiry/allowlist is not a subset of the parent's")]
    NotASubset,
    #[msg("signer does not own this capability")]
    Unauthorized,
    #[msg("allowlist exceeds MAX_ALLOWLIST_LEN")]
    AllowlistTooLarge,
    #[msg("only leash-hook may record a spend")]
    UnauthorizedCaller,
    #[msg("a delegated capability cannot redeem its budget; only spend it")]
    DelegatedCannotRedeem,
    // Appended, never inserted: Anchor numbers these from 6000 by position, so inserting
    // a variant above renumbers every error after it and silently invalidates every test
    // asserting a code (docs/ROADMAP.md 0.5) as well as any client decoding them.
    #[msg("signer's capability is not an ancestor of the target capability")]
    NotAnAncestor,
    #[msg("this capability is not a child of the given parent")]
    NotAChild,
    #[msg("child is still live; revoke it or wait for expiry before reclaiming")]
    ChildStillLive,
}
