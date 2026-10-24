//! Borrowed identifier kinds preserve argument wire rules without collector rules.

use std::{borrow::Borrow, fmt};

macro_rules! identifier {
    ($name:ident) => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
        pub struct $name<'a>(&'a str);
        impl<'a> $name<'a> {
            pub fn parse(value: &'a str) -> Option<Self> {
                valid_id(value).then_some(Self(value))
            }
            pub const fn as_str(self) -> &'a str {
                self.0
            }
            pub(crate) const fn reference(value: &'a str) -> Self {
                Self(value)
            }
        }
        impl Borrow<str> for $name<'_> {
            fn borrow(&self) -> &str {
                self.0
            }
        }
        impl fmt::Display for $name<'_> {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(self.0)
            }
        }
    };
}
identifier!(ArgumentId);
identifier!(EvidenceId);
identifier!(NeedId);

pub(crate) fn valid_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 64
        && id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
}
