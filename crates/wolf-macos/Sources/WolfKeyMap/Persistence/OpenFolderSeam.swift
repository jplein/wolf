// The pure decision behind Open Folder: which of `specs/application-state.md` §6's steps runs.

import CoreGraphics
import Foundation

/// Decides Open Folder's step for a directory, canonicalizes the directory, and picks the frame a
/// fill applies. AppKit-free: the coordinator passes its live windows as tab ids and directories.
public enum OpenFolderSeam {
  /// One live tab: its id and its live directory, `""` for an empty tab.
  public struct LiveTab: Equatable {
    public let id: Int
    public let directory: String

    public init(id: Int, directory: String) {
      self.id = id
      self.directory = directory
    }
  }

  /// One live window: its tabs in tab-bar order, and the selected tab's id.
  public struct LiveWindow: Equatable {
    public let tabs: [LiveTab]
    public let selected: Int?

    public init(tabs: [LiveTab], selected: Int?) {
      self.tabs = tabs
      self.selected = selected
    }
  }

  /// Open Folder's step: focus an open tab by id, fill the current empty tab, or open a window.
  public enum Step: Equatable {
    case focus(tab: Int)
    case fill
    case newWindow
  }

  /// The step for `directory`, which is already canonical.
  ///
  /// The windows are ordered by `stacking`, each with its selected tab as the active one (its
  /// most recent tab when none is selected) and its tab recency, and handed to
  /// `WindowStateCodec.projectTab`, so the tab focused is the one §4's rule picks. Live
  /// directories, never a tab's cached contribution, which can name another directory.
  /// `current` is the invoking tab, `nil` when no Wolf window is key.
  public static func step(
    directory: String, current: LiveTab?, windows: [LiveWindow], stacking: StackingOrder
  ) -> Step {
    let ids = windows.map { $0.tabs.map(\.id) }
    let ordered = stacking.order(ids).map { windows[$0] }
    let state = ordered.map { window -> WindowStateCodec.Window in
      let tabIds = window.tabs.map(\.id)
      let recency = stacking.recency(tabIds)
      let active = window.selected.flatMap { tabIds.firstIndex(of: $0) } ?? recency.first ?? 0
      return WindowStateCodec.Window(
        frame: .zero, activeTab: active, minimized: false,
        tabs: window.tabs.map {
          RestoreSeam.SavedTab(cwd: $0.directory, sidebarVisible: true, layout: nil)
        },
        recency: recency)
    }
    if let found = WindowStateCodec.projectTab(directory, windows: state),
      ordered.indices.contains(found.window),
      ordered[found.window].tabs.indices.contains(found.tab)
    {
      return .focus(tab: ordered[found.window].tabs[found.tab].id)
    }
    return current?.directory == "" ? .fill : .newWindow
  }

  /// The directory a tab counts as being on: `""` without a folder, else its Neovim's reported
  /// cwd, else, before the first report, the directory its Neovim was launched in. Without the
  /// fallback, two files of one new project opened together would open the project twice.
  public static func liveDirectory(hasFolder: Bool, reportedCwd: String, launchCwd: String)
    -> String
  {
    guard hasFolder else { return "" }
    return reportedCwd.isEmpty ? launchCwd : reportedCwd
  }

  /// `path` as core's `apply_cwd` canonicalizes a directory, through `realpath(3)`, falling back
  /// to `path` as given when it does not resolve.
  ///
  /// Never Foundation's `resolvingSymlinksInPath`, which maps `/private/tmp` to `/tmp`: tab
  /// directories and project-state keys are core's `/private/tmp`.
  public static func canonicalDirectory(_ path: String) -> String {
    guard let resolved = realpath(path, nil) else { return path }
    defer { free(resolved) }
    return String(cString: resolved)
  }

  /// The frame a fill applies to its window: `stored`, validated as at launch, only when the
  /// filled tab is the window's only tab. `nil` keeps the window's frame.
  public static func fillFrame(
    stored: CGRect?, windowTabs: Int, screenVisibleFrames: [CGRect]
  ) -> CGRect? {
    guard windowTabs == 1, let stored else { return nil }
    return StateSeam.restoredWindowFrame(
      x: stored.origin.x, y: stored.origin.y, width: stored.size.width,
      height: stored.size.height, screenVisibleFrames: screenVisibleFrames)
  }
}
