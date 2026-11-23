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

  /// The window's normal frame after a geometry change: `frame` when the window is neither
  /// zoomed nor full screen, else the `previous` normal frame, which a zoom or full screen keeps.
  public static func normalFrame(
    previous: CGRect?, frame: CGRect, zoomed: Bool, fullScreen: Bool
  ) -> CGRect? {
    zoomed || fullScreen ? previous : frame
  }

  /// The active window a new window takes its size from (`specs/application-state.md` §7): its
  /// recorded normal frame, its live frame, and whether it is zoomed or full screen.
  public struct ActiveWindow: Equatable {
    /// The normal frame recorded before the last zoom or full screen, `nil` when none was.
    public let normalFrame: CGRect?
    /// The window's live frame, which is the zoomed or full-screen frame while it is either.
    public let frame: CGRect
    /// Whether the window is zoomed.
    public let zoomed: Bool
    /// Whether the window is full screen.
    public let fullScreen: Bool

    public init(normalFrame: CGRect?, frame: CGRect, zoomed: Bool, fullScreen: Bool) {
      self.normalFrame = normalFrame
      self.frame = frame
      self.zoomed = zoomed
      self.fullScreen = fullScreen
    }
  }

  /// Where a new window opens, as `newWindowFrame` decides it.
  public enum NewWindowFrame: Equatable {
    /// At a stored frame that intersects a screen.
    case stored(CGRect)
    /// At the active window's normal size, cascaded from `topLeft`, the top-left of the active
    /// window's live frame: a zoomed or full-screen window cascades from where it is.
    case cascaded(topLeft: CGPoint, size: CGSize)
    /// At the built default size, where AppKit places it.
    case system
  }

  /// Where a new window opens (`specs/application-state.md` §7): at `stored` when
  /// `restoredWindowFrame` accepts it, else at `active`'s normal size, never its zoomed or
  /// full-screen size, cascaded from the top-left of its live frame, so a zoomed or full-screen
  /// window cascades from where it is; else at the default size and placement.
  public static func newWindowFrame(
    stored: CGRect?, active: ActiveWindow?, screenVisibleFrames: [CGRect]
  ) -> NewWindowFrame {
    if let stored,
      let frame = restoredWindowFrame(
        x: stored.origin.x, y: stored.origin.y, width: stored.size.width,
        height: stored.size.height, screenVisibleFrames: screenVisibleFrames)
    {
      return .stored(frame)
    }
    if let active,
      let normal = normalFrame(
        previous: active.normalFrame, frame: active.frame, zoomed: active.zoomed,
        fullScreen: active.fullScreen)
    {
      return .cascaded(
        topLeft: CGPoint(x: active.frame.minX, y: active.frame.maxY), size: normal.size)
    }
    return .system
  }
}
