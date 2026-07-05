//
//  PickerRefreshCoalescer.swift
//  A pure, headless debounce/coalescing seam SHARED by BOTH the file picker and
//  the file-tree sidebar for their streaming refreshes (plan Fix #1). The name is
//  historical (it was introduced for the picker), but the seam is generic — the
//  file tree reuses it verbatim because both stream a large traversal and would
//  otherwise re-do O(N) work per batch.
//
//  During a large traversal the file-search / tree-traversal wake fires many
//  times (once per batch), and each bare `refresh()` re-ranks (picker) or
//  reloads/rebuilds (tree) the whole accumulated snapshot. This seam collapses a
//  BURST of `request()` calls into a SINGLE trailing `perform()`, while still
//  guaranteeing that a later wake reschedules — so the final, complete snapshot
//  is never dropped.
//
//  It is toolkit-free (Foundation only): the caller injects `schedule` (which in
//  production arms a `DispatchQueue.main.asyncAfter` timer) and `perform` (the
//  work to run once per coalesced burst), so the seam is unit-testable without a
//  real run loop — the test drives the "timer fired" edge by hand.
//

import Foundation

/// Coalesces a burst of `request()` calls into one trailing `perform()`.
///
/// Shared by BOTH the file picker (`PickerView`) and the file-tree sidebar
/// (`FileTreeView`): the name is historical, but the seam is generic (its
/// `schedule` / `perform` collaborators are injected), so both streaming
/// traversals reuse it to avoid re-doing O(N) work on every batch wake.
///
/// The first `request()` while nothing is pending marks a fire as scheduled and
/// asks `schedule` to run the fire after the debounce window; subsequent
/// `request()`s during that window are dropped (coalesced). When the fire runs it
/// FIRST clears the scheduled flag (so a later `request()` can reschedule) and
/// THEN calls `perform`, which reads the latest snapshot.
public final class PickerRefreshCoalescer {
    /// The debounce window (~30fps). A burst of wakes within this window collapses
    /// to a single trailing refresh.
    public static let refreshInterval: TimeInterval = 0.033

    /// Arms the trailing fire after the debounce window (production: a
    /// `DispatchQueue.main.asyncAfter`). Receives the fire closure to run.
    private let schedule: (@escaping () -> Void) -> Void
    /// The work to run once per coalesced burst.
    private let perform: () -> Void
    /// Whether a fire is already armed; further `request()`s coalesce into it.
    private var scheduled = false

    /// Builds the coalescer with its two injected collaborators.
    public init(
        schedule: @escaping (@escaping () -> Void) -> Void, perform: @escaping () -> Void
    ) {
        self.schedule = schedule
        self.perform = perform
    }

    /// Requests a refresh. If a fire is already armed, coalesces (does nothing);
    /// otherwise arms a trailing fire that clears the flag and performs the work.
    public func request() {
        if scheduled { return }
        scheduled = true
        schedule { [weak self] in
            self?.scheduled = false
            self?.perform()
        }
    }

    /// Clears the scheduled flag so a pending (already-armed) fire is treated as
    /// cancelled — the next `request()` re-arms. (The armed closure itself still
    /// runs; clearing the flag makes it a no-op-relevant reschedule point.)
    public func cancelPending() {
        scheduled = false
    }
}
