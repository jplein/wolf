//! Headless coverage for the free bridge function `wolf_ffi_library_license_notices()`,
//! whose return value the macOS About panel shows below the font notices.
//!
//! The function must hand Swift `wolf_core::LIBRARY_LICENSE_NOTICES` unchanged, because
//! that text discharges the attribution owed for libghostty-vt and the code its static
//! archive carries. The tests assert byte-for-byte equality with the core constant and,
//! as a second, independently diagnosable check, that every
//! `wolf_core::LIBRARY_LICENSES` row's library is named in what the bridge returns.
//!
//! No nvim, no `WolfCore`, no `tests/common/` and no runfiles: the
//! `//crates/wolf-ffi:library_license_notices` target names `//crates/wolf-core:wolf_core`
//! directly so the test can read the constant it compares against.

use wolf_core::{LIBRARY_LICENSES, LIBRARY_LICENSE_NOTICES};
use wolf_ffi::wolf_ffi_library_license_notices;

/// The bridge returns the core constant byte for byte. A mismatch is reported as both
/// lengths and the first differing byte offset, not as two dumps of several kilobytes.
#[test]
fn the_notices_cross_the_bridge_byte_for_byte() {
    assert!(
        !LIBRARY_LICENSE_NOTICES.is_empty(),
        "wolf_core::LIBRARY_LICENSE_NOTICES is empty, so a bridge returning nothing would equal \
         it. The block must carry every linked library's licence."
    );

    let returned = wolf_ffi_library_license_notices();
    let got = returned.as_bytes();
    let want = LIBRARY_LICENSE_NOTICES.as_bytes();

    let first_difference = got
        .iter()
        .zip(want)
        .position(|(g, w)| g != w)
        .unwrap_or_else(|| got.len().min(want.len()));

    assert!(
        got == want,
        "wolf_ffi_library_license_notices() returned {} bytes, but LIBRARY_LICENSE_NOTICES is \
         {} bytes, and they first differ at byte offset {first_difference}. 0 bytes means the \
         bridge returns an empty string; an offset equal to the shorter length with a shorter \
         return means it truncated the notices; an earlier offset means it altered them, or \
         returns the font notices instead. Either way the macOS About panel would not show the \
         linked libraries' licences.",
        got.len(),
        want.len(),
    );
}

/// Every registered library is named in what the bridge returns, so a reader of the
/// About panel can find each library's licence.
#[test]
fn the_notices_name_every_linked_library() {
    assert!(
        !LIBRARY_LICENSES.is_empty(),
        "wolf_core::LIBRARY_LICENSES is empty, so the per-library check below would pass \
         without looking at anything. The registry must carry one row per linked library."
    );

    let returned = wolf_ffi_library_license_notices();
    for row in LIBRARY_LICENSES {
        assert!(
            returned.contains(row.library),
            "wolf_ffi_library_license_notices() does not name the linked library {:?}, which \
             LIBRARY_LICENSES registers. The bridge returned {} of LIBRARY_LICENSE_NOTICES's {} \
             bytes: fewer means it truncated the block, so the macOS About panel ships {:?} with \
             no visible licence; the same length means it altered the text.",
            row.library,
            returned.len(),
            LIBRARY_LICENSE_NOTICES.len(),
            row.library,
        );
    }
}
