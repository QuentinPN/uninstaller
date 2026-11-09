//! Other accounts of this PC whose registry hive is loaded (signed in, or kept loaded by
//! a service): their per-user installs are listed too (F1). Hives of signed-out accounts
//! need elevation to load and wait for the broker.

use crate::win::registry::{Hive, Key, View};
use crate::win::token;

/// Profile folders by SID, with the path of each.
const PROFILE_LIST: &str = r"SOFTWARE\Microsoft\Windows NT\CurrentVersion\ProfileList";

/// SIDs of real accounts: local or domain (`S-1-5-21-`), Microsoft Entra (`S-1-12-1-`).
/// Built-in service accounts (`S-1-5-18` to `-20`) install nothing of their own.
const ACCOUNT_SID_PREFIXES: [&str; 2] = ["S-1-5-21-", "S-1-12-1-"];

#[derive(Clone, Debug, PartialEq)]
pub struct Profile {
    pub sid: String,
    /// The profile folder's name, usually the account name.
    pub name: Option<String>,
}

/// Loaded hives of accounts other than the current one (which is read through HKCU).
pub fn others() -> Vec<Profile> {
    let (Some(own), Some(users)) = (
        token::user_sid(),
        Key::open(Hive::Users, "", View::Native64),
    ) else {
        return Vec::new();
    };
    users
        .subkey_names()
        .into_iter()
        .filter(|sid| !sid.eq_ignore_ascii_case(&own))
        .filter_map(|sid| profile(&sid))
        .collect()
}

/// The profile of an account SID; `None` for anything else, such as a `_Classes` hive or
/// text that is not a SID (ids from the UI go through here).
pub fn profile(sid: &str) -> Option<Profile> {
    is_account_sid(sid).then(|| Profile {
        sid: sid.to_owned(),
        name: folder_name(sid),
    })
}

fn is_account_sid(text: &str) -> bool {
    ACCOUNT_SID_PREFIXES.iter().any(|prefix| {
        text.strip_prefix(prefix).is_some_and(|rest| {
            !rest.is_empty() && rest.bytes().all(|b| b.is_ascii_digit() || b == b'-')
        })
    })
}

fn folder_name(sid: &str) -> Option<String> {
    let path = Key::open(
        Hive::LocalMachine,
        &format!(r"{PROFILE_LIST}\{sid}"),
        View::Native64,
    )?
    .string("ProfileImagePath")?;
    path.rsplit('\\')
        .next()
        .filter(|name| !name.is_empty())
        .map(Into::into)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_account_sids_are_profiles() {
        for sid in [
            "S-1-5-21-2990840499-3796560361-3692091661-1001",
            "S-1-12-1-111-222-333-444",
        ] {
            assert!(is_account_sid(sid), "{sid}");
        }
        for other in [
            ".DEFAULT",
            "S-1-5-18",
            "S-1-5-21-",
            "S-1-5-21-1-1001_Classes",
            r"S-1-5-21-1\..\x",
            "",
        ] {
            assert!(profile(other).is_none(), "{other}");
        }
    }

    /// Real registry, read-only: this account has a named profile and is not "another".
    #[test]
    fn names_this_account_and_leaves_it_out_of_others() {
        let own = token::user_sid().unwrap();
        let folder = std::env::var("USERPROFILE").unwrap();
        assert_eq!(
            profile(&own).and_then(|p| p.name),
            folder.rsplit('\\').next().map(Into::into)
        );
        assert!(
            others()
                .iter()
                .all(|p| p.sid != own && is_account_sid(&p.sid))
        );
    }
}
