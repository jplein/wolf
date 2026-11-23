// The pure routing of a path opened from outside Wolf: `specs/cli-and-file-open.md` §4 and §5.

import Foundation

/// Routes an open request to Open Folder, to a named tab, or to a file's project, and decodes
/// core's pane choice for a tab. AppKit-free: the coordinator passes its live windows as tab ids
/// and directories, as it does to `OpenFolderSeam`.
public enum FileOpenSeam {
  /// What a request asks for, as core resolved it.
  public enum Target {
    /// No path: launch or activate Wolf.
    case activate
    /// An existing directory, canonical.
    case directory(String)
    /// A file, existing or not.
    case file(String)
  }

  /// One request from the socket or from Finder.
  public struct Request {
    public let target: Target
    /// The request's `WOLF_TAB_ID`, lowercase; `""` when it has none.
    public let tabId: String

    init(target: Target, tabId: String) {
      self.target = target
      self.tabId = tabId
    }

    /// Decodes a drained or Finder item. An unknown kind is an activate.
    public init(_ item: FfiOpenItemRef) {
      let path = item.path().toString()
      switch OpenKind(rawValue: item.kind()) {
      case .directory: target = .directory(path)
      case .file: target = .file(path)
      case .activate, nil: target = .activate
      }
      tabId = item.tab_id().toString()
    }
  }

  /// Mirrors wolf-ffi's `mod open_kind`.
  private enum OpenKind: UInt8 {
    case activate = 0
    case directory = 1
    case file = 2
  }

  /// What the coordinator does with a request. `current` is §4's current tab, `nil` with no
  /// live window.
  public enum Route {
    /// Raise the current tab, or activate the app.
    case activate(current: Int?)
    /// Open Folder on `directory` from `current`.
    case openFolder(directory: String, current: Int?)
    /// §5.1 rule 1: open `file` in the live tab `tab` named by the request.
    case openFile(file: String, tab: Int)
    /// Open Folder on `file`'s project, `project`, from `current`, then open `file` in the tab
    /// that lands.
    case openFileInProject(file: String, project: String, current: Int?)
  }

  /// The route for `request`. `tabIds` maps each live tab's `WOLF_TAB_ID` to its tab id; an ID
  /// is live only when its tab is in `windows`, and a request with no live ID is handled as if
  /// it had none (§3).
  public static func route(
    _ request: Request, windows: [OpenFolderSeam.LiveWindow], tabIds: [String: Int],
    stacking: StackingOrder
  ) -> Route {
    let named = request.tabId.isEmpty ? nil : tabIds[request.tabId]
    let live = named.flatMap { id in
      windows.contains { $0.tabs.contains { $0.id == id } } ? id : nil
    }
    let current = live ?? currentTab(windows: windows, stacking: stacking)
    switch request.target {
    case .activate:
      return .activate(current: current)
    case .directory(let directory):
      return .openFolder(directory: directory, current: current)
    case .file(let file):
      if let live { return .openFile(file: file, tab: live) }
      let open = windows.map { window in
        WindowStateCodec.Window(
          frame: .zero, activeTab: 0, minimized: false,
          tabs: window.tabs.map {
            RestoreSeam.SavedTab(cwd: $0.directory, sidebarVisible: true, layout: nil)
          },
          recency: [])
      }
      return .openFileInProject(
        file: file, project: WindowStateCodec.projectDirectory(file, windows: open),
        current: current)
    }
  }

  /// The frontmost window's active tab: its selected tab, else its most recent one, as
  /// `OpenFolderSeam.step` reads them. `nil` with no window or an empty one.
  public static func currentTab(windows: [OpenFolderSeam.LiveWindow], stacking: StackingOrder)
    -> Int?
  {
    let ids = windows.map { $0.tabs.map(\.id) }
    guard let front = stacking.order(ids).first else { return nil }
    let tabs = ids[front]
    if let selected = windows[front].selected, tabs.contains(selected) { return selected }
    return stacking.recency(tabs).first.map { tabs[$0] }
  }

  /// The pane of a `PaneChoice`; each index counts `PaneTree.leaves()` depth-first.
  public enum Placement {
    /// An editor pane already shows the file: focus it.
    case focus(Int)
    /// Open the file in this editor pane.
    case open(Int)
    /// Make this preview pane an editor pane showing the file.
    case convert(Int)
    /// Split the whole tab, the file in a new editor pane on the left.
    case splitLeft
  }

  /// Where §5.2 puts a file in a tab.
  public struct PaneChoice {
    /// Whether the tab's zoom, a pane's or the drawer's, is undone first.
    public let unzoom: Bool
    public let placement: Placement
  }

  /// Mirrors wolf-ffi's `mod pane_placement`.
  private enum PanePlacement: UInt8 {
    case focus = 0
    case open = 1
    case convert = 2
    case splitLeft = 3
  }

  /// Core's §5.2 choice for `file` in a tab laid out as `layout`, whose zoomed leaf is marked,
  /// with leaf `focused` active, `nil` when the drawer is, and its drawer zoomed when
  /// `drawerZoomed`. `file` is in core's resolved form, as a request's is.
  public static func choosePane(
    _ file: String, layout: SavedPaneNode, focused: Int?, drawerZoomed: Bool
  ) -> PaneChoice {
    let choice = WindowStateCodec.choosePane(
      file, layout: layout, focused: focused, drawerZoomed: drawerZoomed)
    let index = Int(choice.pane())
    let placement: Placement
    switch PanePlacement(rawValue: choice.placement()) {
    case .focus: placement = .focus(index)
    case .open: placement = .open(index)
    case .convert: placement = .convert(index)
    case .splitLeft, nil: placement = .splitLeft
    }
    return PaneChoice(unzoom: choice.unzoom(), placement: placement)
  }
}
