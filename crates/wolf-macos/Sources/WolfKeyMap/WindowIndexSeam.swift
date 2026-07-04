//
//  WindowIndexSeam.swift
//  Pure, AppKit-free seam that picks a window's `state.json` slot (plan §5).
//
//  A pure, display-free seam exercised with scalar array inputs only — NO
//  AppKit, NO window. It picks the smallest non-negative integer NOT already
//  occupied, so slots stay dense as windows open and close (the lowest freed
//  hole is reused).
//
//  Imports ONLY Foundation, so it lives in the Foundation-only `WolfKeyMap`
//  module alongside `PickerSeam.swift` / `ShortcutSeam.swift`.
//

import Foundation

/// The pure seam picking a new window's index. A namespace (`enum` with only
/// static members) mirroring `PickerSeam` / `ShortcutSeam`.
public enum WindowIndexSeam {
    /// The smallest non-negative integer NOT present in `occupied` (the window
    /// indices already in use). Order and duplicates in `occupied` are
    /// irrelevant: `[]` -> 0, `[0,1,2]` -> 3 (append), `[0,2]` -> 1 (reuse the
    /// lowest hole).
    public static func nextFreeIndex(occupied: [Int]) -> Int {
        let taken = Set(occupied)
        var candidate = 0
        while taken.contains(candidate) {
            candidate += 1
        }
        return candidate
    }
}
