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
}
