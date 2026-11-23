//! The tab ID of `specs/cli-and-file-open.md#3-tab-ids`: a random version-4 UUID naming one
//! tab for as long as it lives, carried to the tab's terminal sessions in [`WOLF_TAB_ID_ENV`].

use std::fmt;

use serde::de::{self, Visitor};
use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// The variable a terminal session finds its tab's ID in, and the `wolf` command reads.
pub const WOLF_TAB_ID_ENV: &str = "WOLF_TAB_ID";

/// The positions of the hyphens in the `8-4-4-4-12` form.
const HYPHENS: [usize; 4] = [8, 13, 18, 23];

/// The length of the `8-4-4-4-12` form.
const TEXT_LEN: usize = 36;

/// A tab's ID: a UUID, written as lowercase `8-4-4-4-12` hex and serialized as that string.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct TabId(u128);

impl TabId {
    /// A fresh random version-4 UUID.
    ///
    /// # Panics
    ///
    /// When the operating system's entropy source fails, which leaves no way to make an ID.
    #[must_use]
    pub fn random() -> TabId {
        let mut bytes = [0u8; 16];
        // SAFETY: `bytes` is valid for its 16 bytes, under getentropy's 256-byte limit.
        let rc = unsafe { libc::getentropy(bytes.as_mut_ptr().cast(), bytes.len()) };
        assert!(
            rc == 0,
            "getentropy failed: {}",
            std::io::Error::last_os_error()
        );
        bytes[6] = (bytes[6] & 0x0f) | 0x40;
        bytes[8] = (bytes[8] & 0x3f) | 0x80;
        TabId(u128::from_be_bytes(bytes))
    }

    /// The ID `text` writes in the `8-4-4-4-12` form, hex digits in either case; `None` for
    /// anything else, braces and surrounding whitespace included. Any version parses.
    #[must_use]
    pub fn parse(text: &str) -> Option<TabId> {
        let bytes = text.as_bytes();
        if bytes.len() != TEXT_LEN {
            return None;
        }
        let mut value: u128 = 0;
        for (i, &byte) in bytes.iter().enumerate() {
            if HYPHENS.contains(&i) {
                if byte != b'-' {
                    return None;
                }
                continue;
            }
            let digit = char::from(byte).to_digit(16)?;
            value = (value << 4) | u128::from(digit);
        }
        Some(TabId(value))
    }
}

impl fmt::Display for TabId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let hex = format!("{:032x}", self.0);
        write!(
            f,
            "{}-{}-{}-{}-{}",
            &hex[..8],
            &hex[8..12],
            &hex[12..16],
            &hex[16..20],
            &hex[20..]
        )
    }
}

impl Serialize for TabId {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_str(self)
    }
}

impl<'de> Deserialize<'de> for TabId {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<TabId, D::Error> {
        struct TabIdVisitor;

        impl Visitor<'_> for TabIdVisitor {
            type Value = TabId;

            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("a UUID string in the 8-4-4-4-12 form")
            }

            fn visit_str<E: de::Error>(self, text: &str) -> Result<TabId, E> {
                TabId::parse(text).ok_or_else(|| E::invalid_value(de::Unexpected::Str(text), &self))
            }
        }

        deserializer.deserialize_str(TabIdVisitor)
    }
}
