use anchor_lang::prelude::*;

#[error_code]
pub enum LeashHookError {
    #[msg("capability has been revoked")]
    Revoked,
    #[msg("this capability's parent has been revoked")]
    ParentRevoked,
    #[msg("capability has expired")]
    Expired,
    #[msg("destination is not on this capability's allowlist")]
    NotAllowlisted,
    #[msg("spend would exceed this capability's remaining budget")]
    CapExceeded,
}
