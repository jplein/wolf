// Pure, AppKit-free decisions of Close Window and Quit over each tab's unsaved answer, and of
// what a close request from a window of a tab group closes.

import Foundation

/// What the tab's Neovim answers Close Window's and Quit's silent ask, as
/// `WolfCoreHandle.request_unsaved()` reports it. The raw values are the discriminants of
/// wolf-ffi's `mod unsaved_answer` (which mirrors `wolf_core::UnsavedAnswer`); Swift cannot
/// import that module, so this is the hand-kept mirror. An unknown discriminant decodes to `nil`.
public enum UnsavedAnswer: UInt8 {
  /// The ask failed or timed out, or the core is gone: counted as clean.
  case noAnswer = 0
  /// The tab holds a modified buffer, shown or hidden.
  case unsaved = 1
  /// Nothing is unsaved.
  case clean = 2
}

/// One tab's answer for Close Window and Quit.
public enum TabUnsaved: Equatable {
  /// The tab's Neovim holds a modified buffer: the close is refused.
  case unsaved
  /// Nothing is unsaved.
  case clean
  /// The tab's Neovim is dead, stalled, gone or gave no answer: counted as clean, so a wedged
  /// tab never keeps its window or the app open.
  case unreachable
}

/// Close Window's and Quit's decisions over each tab's answer, the same rule as wolf-gnome's
/// `tabs_ui::window_close` and `quit`. A namespace (`enum` with only static members), mirroring
/// the other seams.
public enum WindowCloseSeam {
  /// How long `CloseScopeSeam` waits for the other windows of a tab group to ask, before it
  /// decides a lone request is a tab's own close. The debounce Ghostty's tab-group close uses.
  public static let closeScopeSettle: TimeInterval = 0.1

  /// One tab's answer, given its Neovim's `liveness`. Only an `.alive` Neovim is asked, once,
  /// through `ask`; any other is `.unreachable`, as is an `.noAnswer`.
  public static func tabUnsaved(liveness: NvimLiveness, ask: () -> UnsavedAnswer) -> TabUnsaved {
    switch liveness {
    case .alive:
      break
    case .notResponding, .dead:
      return .unreachable
    }
    switch ask() {
    case .unsaved:
      return .unsaved
    case .clean:
      return .clean
    case .noAnswer:
      return .unreachable
    }
  }

  /// What Close Window does once the window's tabs have answered.
  public enum WindowClose: Equatable {
    /// Close every tab, and the window with them.
    case closeAll
    /// Close nothing, and show the tab at this index of the order given.
    case show(tab: Int)
  }

  /// Close Window's decision over the window's tabs, in tab order: `.show` for the first
  /// `.unsaved`, else `.closeAll`. `tabs` is pulled lazily and not past the first unsaved tab, so
  /// no tab after it is asked and a wedged one there costs no RPC timeout.
  public static func closeWindow<Tabs: Sequence>(_ tabs: Tabs) -> WindowClose
  where Tabs.Element == TabUnsaved {
    for (index, tab) in tabs.enumerated() where tab == .unsaved {
      return .show(tab: index)
    }
    return .closeAll
  }

  /// What Quit does once every tab of every window has answered.
  public enum Quit: Equatable {
    /// Quit: commit the session and shut every Neovim down.
    case quit
    /// Quit nothing, and show tab `tab` of window `window`, both indices of the order given.
    case show(window: Int, tab: Int)
  }

  /// Quit's decision over every window's tabs, the windows in the order given (frontmost first)
  /// and each window's tabs in tab order: `.show` for the first window whose `closeWindow`
  /// shows a tab, else `.quit`. Pulls no window after the one shown.
  public static func quit<Windows: Sequence>(_ windows: Windows) -> Quit
  where Windows.Element: Sequence, Windows.Element.Element == TabUnsaved {
    for (index, tabs) in windows.enumerated() {
      if case .show(let tab) = closeWindow(tabs) {
        return .show(window: index, tab: tab)
      }
    }
    return .quit
  }
}

/// Tells a tab group's own close, which AppKit sends to every window of the group, from one
/// tab's close, which it sends to that window alone. Requests are collected until a scheduled
/// end, `WindowCloseSeam.closeScopeSettle` later in production, and then decided per group.
public final class CloseScopeSeam<Window: Hashable> {
  /// What one settled batch decides for a group.
  public enum Decision: Equatable {
    /// Only this window of its group asked: close that tab, through Close Tab's check.
    case tab(Window)
    /// Every window of the group asked: close the whole group, in the group's order.
    case window([Window])
  }

  private let schedule: (@escaping () -> Void) -> Void
  private let decide: (Decision) -> Void
  /// The windows that asked in this batch, in request order, each with the group it gave.
  private var requests: [(window: Window, group: [Window])] = []
  private var asked: Set<Window> = []

  /// `schedule` runs the batch's end later, once per batch; `decide` receives each decision.
  public init(
    schedule: @escaping (@escaping () -> Void) -> Void, decide: @escaping (Decision) -> Void
  ) {
    self.schedule = schedule
    self.decide = decide
  }

  /// Files `window`'s close request, `group` being its tab group's windows in tab order. The
  /// first request of a batch schedules its end; a window that asks twice counts once.
  public func request(_ window: Window, group: [Window]) {
    let first = requests.isEmpty
    if asked.insert(window).inserted {
      requests.append((window, group))
    }
    if first {
      schedule { [weak self] in self?.end() }
    }
  }

  /// Decides each distinct group, in first-request order, and empties the batch before any
  /// decision runs, so a decision that closes windows starts a batch of its own.
  private func end() {
    let batch = requests
    let askedNow = asked
    requests = []
    asked = []
    var groups: [[Window]] = []
    for request in batch where !groups.contains(request.group) {
      groups.append(request.group)
    }
    for group in groups {
      if group.allSatisfy(askedNow.contains) {
        decide(.window(group))
      } else {
        for request in batch where request.group == group {
          decide(.tab(request.window))
        }
      }
    }
  }
}
