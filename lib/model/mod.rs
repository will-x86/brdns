//! Everything hangs off an [`Account`], which is identified by
//! account number. The account owns an ordered list of [`Rule`]s and a set of DNS [`Upstream`]s.

mod entities;
mod enums;
mod input;

pub use entities::{Account, AccountPolicy, Member, Rule, Upstream};
pub use enums::{Action, TargetType, UpstreamProtocol, Window};
pub use input::{
    NewAccount, NewMember, NewRule, NewUpstream, generate_account_number, is_valid_account_number,
};
