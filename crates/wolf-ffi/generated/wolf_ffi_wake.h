// Hand-written (NOT swift-bridge-generated) C header for the wake-registration
// seam. Swift's `WakeBridge` calls `wolf_ffi_set_wake` once at startup with a
// thunk that dispatches a repaint onto the AppKit main thread. Passing NULL
// clears the callback. See `crates/wolf-ffi/src/lib.rs` (`wolf_ffi_set_wake`).
//
// Each callback carries the firing core's per-window wake `token` (a `uint64_t`)
// so a single process-global callback can route each wake to the right window
// WITHOUT clobbering (the multi-window wake-routing layer).
#ifndef WOLF_FFI_WAKE_H
#define WOLF_FFI_WAKE_H

#include <stdint.h>

void wolf_ffi_set_wake(void (*wake)(uint64_t token));

// The picker's file-search wake (see `wolf_ffi_set_filesearch_wake`): Swift's
// `PickerWakeBridge` registers a thunk that dispatches a picker refresh onto the
// AppKit main thread, routed by `token`. Passing NULL clears it.
void wolf_ffi_set_filesearch_wake(void (*wake)(uint64_t token));

// The file-tree sidebar's wake (see `wolf_ffi_set_tree_wake`): Swift's
// `TreeWakeBridge` registers a thunk that dispatches a tree refresh onto the
// AppKit main thread, routed by `token`. Passing NULL clears it.
void wolf_ffi_set_tree_wake(void (*wake)(uint64_t token));

#endif /* WOLF_FFI_WAKE_H */
