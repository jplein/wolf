//! Headless coverage for the free bridge function `wolf_ffi_font_license_notices()`,
//! whose return value the macOS About panel shows as its credits.
//!
//! The function must hand Swift `wolf_core::FONT_LICENSE_NOTICES` unchanged, because
//! that text discharges the bundled fonts' attribution obligation: a shortened or
//! re-encoded copy ships a font with no visible licence. The tests assert byte-for-byte
//! equality with the core constant, and, as a second, independently diagnosable check,
//! that every `wolf_core::FONT_LICENSES` row's font is named in what the bridge returns.
//!
//! No nvim, no `WolfCore`, no `tests/common/` and no runfiles: the
//! `//crates/wolf-ffi:font_license_notices` target names `//crates/wolf-core:wolf_core`
//! directly so the test can read the constant it compares against.

use wolf_core::{FONT_LICENSES, FONT_LICENSE_NOTICES};
use wolf_ffi::wolf_ffi_font_license_notices;

/// The bridge returns the core constant byte for byte. A mismatch is reported as both
/// lengths and the first differing byte offset, not as two ~14 KB dumps.
#[test]
fn the_notices_cross_the_bridge_byte_for_byte() {
    let returned = wolf_ffi_font_license_notices();
    let got = returned.as_bytes();
    let want = FONT_LICENSE_NOTICES.as_bytes();

    let first_difference = got
        .iter()
        .zip(want)
        .position(|(g, w)| g != w)
        .unwrap_or_else(|| got.len().min(want.len()));

    assert!(
        got == want,
        "wolf_ffi_font_license_notices() returned {} bytes, but FONT_LICENSE_NOTICES is {} \
         bytes, and they first differ at byte offset {first_difference}. An offset equal to the \
         shorter length with a shorter return means the bridge truncated the notices; an \
         earlier offset means it altered or re-encoded them. Either way the macOS About panel \
         would show licence text that is not the one the fonts ship under.",
        got.len(),
        want.len(),
    );
}

/// Every registered font is named in what the bridge returns, so a reader of the About
/// panel can find each font's licence.
#[test]
fn the_notices_name_every_bundled_font() {
    assert!(
        !FONT_LICENSES.is_empty(),
        "wolf_core::FONT_LICENSES is empty, so the per-font check below would pass without \
         looking at anything. The registry must carry one row per bundled font."
    );

    let returned = wolf_ffi_font_license_notices();
    for row in FONT_LICENSES {
        assert!(
            returned.contains(row.font),
            "wolf_ffi_font_license_notices() does not name the bundled font {:?}, which \
             FONT_LICENSES registers. The bridge returned {} of FONT_LICENSE_NOTICES's {} \
             bytes: fewer means it truncated the block, so the macOS About panel ships {:?} \
             with no visible licence; the same length means it altered the text.",
            row.font,
            returned.len(),
            FONT_LICENSE_NOTICES.len(),
            row.font,
        );
    }
}
