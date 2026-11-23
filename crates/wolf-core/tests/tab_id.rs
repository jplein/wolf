//! Headless tests of `wolf_core::TabId`, the tab ID of `specs/cli-and-file-open.md` §3: a random
//! version-4 UUID, written as lowercase `8-4-4-4-12` hex, parsed from either case and from that
//! form only, and carried through serde as the same string.
//!
//! Every malformed form below is one a lenient parser would plausibly accept: hyphens stripped,
//! a sign `u128::from_str_radix` takes, surrounding whitespace, braces.

use std::collections::HashSet;

use wolf_core::TabId;

/// How many random IDs the format and uniqueness checks draw.
const DRAWS: usize = 1000;

/// A well-formed UUID with every hex letter present, so a case mix-up shows.
const WELL_FORMED: &str = "0123abcd-4567-49ab-8def-0123456789ab";

/// The positions of the four hyphens in the `8-4-4-4-12` form.
const HYPHENS: [usize; 4] = [8, 13, 18, 23];

/// Asserts `text` is a lowercase version-4 UUID with the RFC 4122 variant.
fn assert_v4_form(text: &str) {
    let chars: Vec<char> = text.chars().collect();
    assert_eq!(
        chars.len(),
        36,
        "a TabId must display as 36 characters, 32 hex digits and 4 hyphens; {text:?} is not."
    );
    for (i, c) in chars.iter().enumerate() {
        if HYPHENS.contains(&i) {
            assert_eq!(
                *c, '-',
                "{text:?} must have a hyphen at {i}, the 8-4-4-4-12 form."
            );
        } else {
            assert!(
                c.is_ascii_digit() || ('a'..='f').contains(c),
                "{text:?} must hold only lowercase hex digits outside the hyphens; {c:?} at {i} \
                 is an uppercase or non-hex character."
            );
        }
    }
    assert_eq!(
        chars[14], '4',
        "{text:?} must carry version 4 in the first digit of its third group: another digit means \
         byte 6's high nibble was not set to 4."
    );
    assert!(
        "89ab".contains(chars[19]),
        "{text:?} must carry the RFC 4122 variant, 8, 9, a or b, in the first digit of its fourth \
         group: another digit means byte 8's top two bits were not set to 10."
    );
}

#[test]
fn random_ids_are_version_4_uuids() {
    for _ in 0..DRAWS {
        assert_v4_form(&TabId::random().to_string());
    }
}

#[test]
fn random_ids_differ() {
    let ids: HashSet<String> = (0..DRAWS).map(|_| TabId::random().to_string()).collect();
    assert_eq!(
        ids.len(),
        DRAWS,
        "{DRAWS} random TabIds must all differ; {} distinct means the 122 random bits are not \
         random, such as zeroed or constant bytes.",
        ids.len()
    );
}

#[test]
fn parse_round_trips_and_ignores_case() {
    let id = TabId::random();
    let text = id.to_string();
    assert_eq!(
        TabId::parse(&text),
        Some(id),
        "parse of a TabId's own display {text:?} must give that id back."
    );
    assert_eq!(
        TabId::parse(&text.to_uppercase()),
        Some(id),
        "parse must accept uppercase hex and give the same id: None means uppercase is refused, \
         another id means the case changes the value."
    );

    let fixed = TabId::parse(WELL_FORMED)
        .unwrap_or_else(|| panic!("the well-formed {WELL_FORMED:?} must parse"));
    let upper = TabId::parse(&WELL_FORMED.to_uppercase())
        .unwrap_or_else(|| panic!("the uppercase form of {WELL_FORMED:?} must parse"));
    assert_eq!(
        fixed, upper,
        "the lowercase and uppercase forms of one UUID must parse to the same id."
    );
    assert_eq!(
        upper.to_string(),
        WELL_FORMED,
        "an id parsed from uppercase must display lowercase, as {WELL_FORMED:?}."
    );
}

#[test]
fn parse_accepts_any_well_formed_uuid() {
    // Version 1, variant 0: well-formed, not version 4.
    let v1 = "0123abcd-4567-19ab-0def-0123456789ab";
    let id = TabId::parse(v1).unwrap_or_else(|| {
        panic!("{v1:?} is a well-formed UUID and must parse, whatever its version and variant")
    });
    assert_eq!(
        id.to_string(),
        v1,
        "a parsed id must display as it was written."
    );
}

#[test]
fn parse_rejects_malformed() {
    let rows: [(&str, String); 11] = [
        ("empty", String::new()),
        ("35 characters", WELL_FORMED[..35].to_owned()),
        ("37 characters", format!("{WELL_FORMED}0")),
        (
            "a hyphen replaced by a digit",
            WELL_FORMED.replacen('-', "0", 1),
        ),
        (
            "a misplaced hyphen",
            "0123abc-d4567-49ab-8def-0123456789ab".to_owned(),
        ),
        ("no hyphens", WELL_FORMED.replace('-', "")),
        ("a non-hex character", WELL_FORMED.replacen('a', "g", 1)),
        ("a leading sign", format!("+{}", &WELL_FORMED[1..])),
        ("braces", format!("{{{WELL_FORMED}}}")),
        ("surrounding spaces", format!(" {WELL_FORMED} ")),
        ("a trailing newline", format!("{WELL_FORMED}\n")),
    ];
    for (what, text) in rows {
        assert_eq!(
            TabId::parse(&text),
            None,
            "parse must refuse {what}, {text:?}: Some means it reads hex leniently, such as \
             stripping hyphens or whitespace, or taking a sign as from_str_radix does."
        );
    }
}

#[test]
fn serde_uses_the_display_string() {
    let id = TabId::random();
    let json = serde_json::to_string(&id).expect("a TabId must serialize");
    assert_eq!(
        json,
        format!("\"{id}\""),
        "a TabId must serialize as its display string; a number means the inner u128 is \
         serialized."
    );
    let back: TabId = serde_json::from_str(&json).expect("a TabId's JSON must deserialize");
    assert_eq!(back, id, "a TabId must round-trip through JSON.");

    let upper: TabId = serde_json::from_str(&format!("\"{}\"", WELL_FORMED.to_uppercase()))
        .expect("an uppercase UUID string must deserialize, as parse accepts it");
    assert_eq!(upper.to_string(), WELL_FORMED);

    for bad in ["\"not-a-uuid\"", "\"\"", "42", "null"] {
        assert!(
            serde_json::from_str::<TabId>(bad).is_err(),
            "deserializing {bad} must fail: a TabId is read only from a well-formed UUID string."
        );
    }
}
