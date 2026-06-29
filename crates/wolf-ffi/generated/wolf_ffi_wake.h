// Hand-written (NOT swift-bridge-generated) C header for the wake-registration
// seam. Swift's `WakeBridge` calls `wolf_ffi_set_wake` once at startup with a
// thunk that dispatches a repaint onto the AppKit main thread. Passing NULL
// clears the callback. See `crates/wolf-ffi/src/lib.rs` (`wolf_ffi_set_wake`).
#ifndef WOLF_FFI_WAKE_H
#define WOLF_FFI_WAKE_H

void wolf_ffi_set_wake(void (*wake)(void));

#endif /* WOLF_FFI_WAKE_H */
