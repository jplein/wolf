// Pure, AppKit-free decision of whether a tab's window may close now or must ask its Neovim.

import Foundation

/// What the tab's Neovim answers Close Tab's check, as `WolfCoreHandle.request_close_tab()`
/// reports it. The raw values are the discriminants of wolf-ffi's `mod close_tab_answer` (which
/// mirrors `wolf_core::CloseTabAnswer`); Swift cannot import that module, so this is the
/// hand-kept mirror. An unknown discriminant decodes to `nil`.
public enum CloseTabAnswer: UInt8 {
  /// The ask failed or timed out, or the core is gone: the tab closes unchecked.
  case noAnswer = 0
  /// The tab has unsaved changes, and Neovim showed E37 or E162.
  case refused = 1
  /// Nothing is unsaved, and Neovim notified `close_tab`, whose drain closes the tab.
  case closing = 2
}

/// Whether `windowShouldClose(_:)` lets a tab's window close. A namespace (`enum` with only
/// static members), mirroring the other seams.
public enum CloseTabSeam {
  /// `true` when the window closes now, `false` when it stays open.
  ///
  /// A `confirmed` close, a drained `close_tab` or termination the user has confirmed, closes
  /// without asking, as does a Neovim that is not `.alive`, so a wedged tab stays closable.
  /// Otherwise `ask` runs once: `.refused` keeps the window, `.closing` keeps it too, since the
  /// drained `close_tab` closes it confirmed, and `.noAnswer` closes it unchecked.
  public static func shouldClose(
    confirmed: Bool, liveness: NvimLiveness, ask: () -> CloseTabAnswer
  ) -> Bool {
    if confirmed {
      return true
    }
    switch liveness {
    case .alive:
      break
    case .notResponding, .dead:
      return true
    }
    switch ask() {
    case .refused, .closing:
      return false
    case .noAnswer:
      return true
    }
  }
}
