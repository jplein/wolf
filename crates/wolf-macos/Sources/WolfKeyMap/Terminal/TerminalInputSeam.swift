// Pure routing of keys, input-method text and focus for terminal panes.

import Foundation

/// The DEL control scalar, the last control scalar committed text drops.
private let delScalar: UInt32 = 0x7F
/// The first printable scalar (space); every scalar below it is a C0 control.
private let firstPrintableScalar: UInt32 = 0x20
/// The last printable ASCII scalar (`~`).
private let lastPrintableASCII: UInt32 = 0x7E

/// Decides where a terminal pane's keys, committed text and focus changes go.
public enum TerminalInputSeam {
  /// Where text an input method committed goes.
  public enum TextRoute: Equatable {
    /// Nothing to send.
    case none
    /// One printable ASCII scalar, sent as a key so core's `<C-w>` router sees it.
    case key(UInt32)
    /// Any other text, sent through `terminal_text`.
    case text(String)
  }

  /// One `terminal_focus` call.
  public struct FocusReport: Equatable {
    public let session: UInt64
    public let gained: Bool
  }

  /// Whether a key down in a terminal pane goes through `interpretKeyEvents`, so input
  /// methods and dead keys see it, rather than straight to `terminal_key`. Marked text
  /// keeps every key in the input method; otherwise a Ctrl, Cmd or option-as-alt chord
  /// and a named key bypass it.
  public static func routesThroughInputContext(
    keyCode: UInt16, ctrl: Bool, meta: Bool, alt: Bool, optionAsAlt: Bool, hasMarkedText: Bool
  ) -> Bool {
    if hasMarkedText { return true }
    return !ctrl && !meta && !(optionAsAlt && alt) && !WolfKeyMap.isNamedKey(keyCode: keyCode)
  }

  /// Routes committed `text`: exactly one printable ASCII scalar is a key, other text
  /// with a printable scalar is text with its C0 and DEL scalars removed, and empty or
  /// control-only text is nothing.
  public static func committedText(_ text: String) -> TextRoute {
    let scalars = text.unicodeScalars.filter {
      $0.value >= firstPrintableScalar && $0.value != delScalar
    }
    guard !scalars.isEmpty else { return .none }
    if text.unicodeScalars.count == 1, let only = scalars.first,
      only.value <= lastPrintableASCII
    {
      return .key(only.value)
    }
    var kept = String.UnicodeScalarView()
    kept.append(contentsOf: scalars)
    return .text(String(kept))
  }

  /// The `terminal_focus` calls for focus moving from session `previous` to `current`,
  /// `nil` meaning no session: none when equal, else the loss and then the gain.
  public static func focusReports(previous: UInt64?, current: UInt64?) -> [FocusReport] {
    if previous == current { return [] }
    var reports: [FocusReport] = []
    if let previous { reports.append(FocusReport(session: previous, gained: false)) }
    if let current { reports.append(FocusReport(session: current, gained: true)) }
    return reports
  }
}
