// Pure, AppKit-free decision seams for the state-restore path.

import CoreGraphics
import Foundation

/// The pure decision functions backing macOS state restore. A namespace
/// (`enum` with only static members) mirroring `PickerSeam`.
public enum StateSeam {
  /// Validates a saved window frame for restore. Returns the frame
  /// `CGRect(x:y:width:height)` when it is usable, or `nil` when the saved
  /// geometry should be IGNORED (the caller then falls back to the default
  /// centered placement):
  ///
  /// - a non-positive `width` or `height` is the "unset" sentinel (the
  ///   geometry was never reported), and
  /// - a frame intersecting NO screen visible frame would restore the window
  ///   fully offscreen (e.g. a disconnected display).
  public static func restoredWindowFrame(
    x: Double, y: Double, width: Double, height: Double,
    screenVisibleFrames: [CGRect]
  ) -> CGRect? {
    guard width > 0, height > 0 else { return nil }
    let frame = CGRect(x: x, y: y, width: width, height: height)
    guard screenVisibleFrames.contains(where: { $0.intersects(frame) }) else {
      return nil
    }
    return frame
  }

  /// The launch decision for one window: whether it opens a real folder or the
  /// "no folder open" empty state.
  ///
  /// - `.folder(cwd:)` — a restored (or freshly chosen) directory to open, with
  ///   the sidebar + file tree rooted there.
  /// - `.emptyState(nvimCwd:)` — no folder is open: the UI shows the centered
  ///   "Open folder…" button with the sidebar hidden, while the embedded Neovim
  ///   still launches at `nvimCwd` (a benign real directory — home) because it
  ///   always needs a working directory. This passive launch cwd is deliberately
  ///   never persisted or recorded in the recent-dirs MRU.
  public enum LaunchDecision: Equatable {
    /// No folder is open; nvim launches at `nvimCwd` (home) for the empty state.
    case emptyState(nvimCwd: String)
    /// A real folder is open at `cwd`.
    case folder(cwd: String)
  }

  /// Decides how a window launches: reopen the `restored` folder only when it is
  /// non-empty AND `isDirectory(restored)` (it may have been deleted — or
  /// replaced by a plain file — since the last run); otherwise the empty state,
  /// with nvim launched at `home`. This NEVER returns the process working
  /// directory, which would pollute the recent-dirs MRU.
  /// The injected closure keeps the seam pure (the caller passes a
  /// FileManager-backed check; tests pass a set lookup).
  public static func launchDecision(
    restored: String, home: String, isDirectory: (String) -> Bool
  ) -> LaunchDecision {
    guard !restored.isEmpty, isDirectory(restored) else {
      return .emptyState(nvimCwd: home)
    }
    return .folder(cwd: restored)
  }
}
