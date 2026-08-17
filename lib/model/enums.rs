//! Serde-friendly enums

use serde::{Deserialize, Serialize};
use std::str::FromStr;

macro_rules! str_enum {
    ($name:ident { $($variant:ident => $s:literal),+ $(,)? }) => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
        #[serde(rename_all = "lowercase")]
        pub enum $name { $($variant),+ }

        impl $name {
            pub fn as_str(&self) -> &'static str {
                match self { $(Self::$variant => $s),+ }
            }
        }

        impl std::fmt::Display for $name {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str(self.as_str())
            }
        }

        impl FromStr for $name {
            type Err = String;
            fn from_str(s: &str) -> Result<Self, Self::Err> {
                match s {
                    $($s => Ok(Self::$variant),)+
                    _ => Err(format!(
                        "unknown {}: {s:?} (expected one of {})",
                        stringify!($name),
                        [$($s),+].join(", ")
                    )),
                }
            }
        }
    };
}

str_enum!(Action {
    Allow => "allow",
    Deny => "deny",
    Limit => "limit",
});

str_enum!(TargetType {
    Domain => "domain",
    Wildcard => "wildcard",
    Category => "category",
});

str_enum!(Window {
    Hour => "hour",
    Day => "day",
    Week => "week",
    Month => "month",
});

str_enum!(UpstreamProtocol {
    Dot => "dot",
    Doh => "doh",
    Udp => "udp",
});

impl std::hash::Hash for UpstreamProtocol {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.as_str().hash(state);
    }
}
