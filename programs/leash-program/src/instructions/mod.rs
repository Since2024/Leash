pub mod issue;
pub mod attenuate;
pub mod revoke;
pub mod redeem;
pub mod record_spend;

// Full glob re-export is required here: Anchor's #[derive(Accounts)] macro generates
// hidden `__client_accounts_*` modules that the #[program] macro in lib.rs expects to
// find via `instructions::*`. Each module's handler function has a unique name
// (issue_handler, attenuate_handler, ...) specifically so this glob has no ambiguity.
pub use issue::*;
pub use attenuate::*;
pub use revoke::*;
pub use redeem::*;
pub use record_spend::*;
