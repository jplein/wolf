// The window state's write to and read from the app-state store, through wolf-ffi's builder.

import CoreGraphics
import Foundation

/// Converts between the front-end's persistence types and the `wolf_ffi_app_state_*` surface,
/// in the shape `specs/application-state.md` §2 defines.
///
/// A pane zoom travels as the tab's `zoomed` pane index, the depth-first index of the first leaf
/// `SavedPaneNode` marks zoomed, and is read back onto that leaf; a drawer zoom travels as the
/// `drawer` reference and is read back as `SavedDrawer.zoomed`. An empty tab (`cwd == ""`)
/// writes its sidebar and no root, focus, zoom or drawer, whatever its cache holds. A folder tab
/// with no layout writes a lone edit pane on no file, which reads back as no layout.
public enum WindowStateCodec {
  /// One window a commit writes, frontmost first.
  public struct Window: Equatable {
    /// The window's normal frame.
    public let frame: CGRect
    /// The index into `tabs` of the selected tab.
    public let activeTab: Int
    public let minimized: Bool
    /// The window's tabs, in tab-bar order.
    public let tabs: [RestoreSeam.SavedTab]
    /// Indices into `tabs`, most recently selected first, for §4's project-state rule.
    public let recency: [Int]

    public init(
      frame: CGRect, activeTab: Int, minimized: Bool, tabs: [RestoreSeam.SavedTab],
      recency: [Int]
    ) {
      self.frame = frame
      self.activeTab = activeTab
      self.minimized = minimized
      self.tabs = tabs
      self.recency = recency
    }
  }

  /// What a restored tab rebuilds: the layout the view builds its panes from, and the bridge tab
  /// `WolfCoreHandle.rebuild_tab` realizes. Both are the tab after `FfiAppTab.restored()`.
  public struct Rebuild {
    public let layout: SavedPaneNode
    public let tab: FfiAppTab
    /// The restored tab's focused pane.
    public let focused: RestoreSeam.SavedPaneRef?
    /// The restored tab's drawer.
    public let drawer: RestoreSeam.SavedDrawer?
  }

  /// One saved tab, as saved and as it restores.
  public struct LoadedTab {
    /// The tab as saved, which a view caches until it can gather its own.
    public let saved: RestoreSeam.SavedTab
    /// The restored tab to rebuild, `nil` when it has no root.
    public let rebuild: Rebuild?
  }

  /// One saved window.
  public struct LoadedWindow {
    /// The saved frame, `nil` when it has no position.
    public let frame: CGRect?
    public let activeTab: Int
    public let minimized: Bool
    /// The window's tabs, in tab-bar order; never empty.
    public let tabs: [LoadedTab]
  }

  /// One directory's project state, as Open Folder restores it.
  public struct LoadedProject {
    /// The stored frame, `nil` when it has no position.
    public let frame: CGRect?
    public let tab: LoadedTab
  }

  /// Mirrors wolf-ffi's `mod pane_kind`.
  private enum PaneKind: UInt8 {
    case edit = 0
    case preview = 1
    case terminal = 2
  }

  /// Mirrors wolf-ffi's `mod pane_ref`.
  private enum PaneRef: UInt8 {
    case none = 0
    case pane = 1
    case drawer = 2
  }

  /// Mirrors wolf-ffi's `mod sidebar_mode`.
  private enum SidebarMode: UInt8 {
    case unset = 0
    case tree = 1
    case search = 2
  }

  /// Mirrors wolf-ffi's `mod tristate`.
  private enum Tristate: UInt8 {
    case unset = 0
    case off = 1
    case on = 2
  }

  /// Submits `windows`, in list order, to the shared app-state store, which writes the window
  /// state and the project states in one transaction. It does not flush.
  public static func submit(_ windows: [Window]) {
    feed(windows)
    wolf_ffi_app_state_commit()
  }

  /// Replaces the builder's scratch with `windows`, in list order.
  private static func feed(_ windows: [Window]) {
    wolf_ffi_app_state_begin()
    for window in windows {
      wolf_ffi_app_state_add_window(
        true, window.frame.origin.x, window.frame.origin.y, window.frame.size.width,
        window.frame.size.height, UInt32(clamping: window.activeTab))
      if window.minimized { wolf_ffi_app_state_window_minimized(true) }
      for tab in window.tabs {
        submitTab(tab)
      }
      for index in window.recency {
        wolf_ffi_app_state_push_recency(UInt32(clamping: index))
      }
    }
  }

  /// The window and tab indices into `windows` of the tab Open Folder focuses for `directory`,
  /// by §4's rule, or `nil` when no tab is open on it. Feeds the builder as `submit` does, minus
  /// the commit, so it writes nothing.
  public static func projectTab(_ directory: String, windows: [Window]) -> (window: Int, tab: Int)?
  {
    feed(windows)
    let answer = wolf_ffi_app_state_project_tab(directory)
    guard answer.found() else { return nil }
    return (Int(answer.window()), Int(answer.tab()))
  }

  /// Reads a directory's project state, `nil` when it has none.
  public static func loadProject(_ project: FfiProjectState) -> LoadedProject? {
    guard project.found() else { return nil }
    let frame =
      project.has_position()
      ? CGRect(x: project.x(), y: project.y(), width: project.width(), height: project.height())
      : nil
    return LoadedProject(frame: frame, tab: loadedTab(project.tab()))
  }

  /// Reads the saved window state, frontmost first. A window with no tabs is dropped.
  public static func load(_ state: FfiAppState) -> [LoadedWindow] {
    // Element refs must not outlive their vec, so `windows` and `tabs` stay bound.
    let windows = state.windows()
    var result: [LoadedWindow] = []
    for i in 0..<windows.len() {
      guard let window = windows.get(index: UInt(i)) else { continue }
      let tabs = window.tabs()
      var loaded: [LoadedTab] = []
      for j in 0..<tabs.len() {
        guard let tab = tabs.get(index: UInt(j)) else { continue }
        loaded.append(loadedTab(tab))
      }
      if loaded.isEmpty { continue }
      let frame =
        window.has_position()
        ? CGRect(x: window.x(), y: window.y(), width: window.width(), height: window.height())
        : nil
      result.append(
        LoadedWindow(
          frame: frame, activeTab: Int(window.active_tab()), minimized: window.minimized(),
          tabs: loaded))
    }
    return result
  }

  /// Pushes `tab` onto the last window: its references, sidebar and drawer, then its root.
  private static func submitTab(_ tab: RestoreSeam.SavedTab) {
    let folder = !tab.cwd.isEmpty
    let focused = folder ? reference(tab.focused) : (PaneRef.none, 0)
    let zoomed: (PaneRef, UInt32)
    if folder, let index = tab.layout?.zoomedLeafIndex {
      zoomed = (.pane, UInt32(clamping: index))
    } else if folder, tab.drawer?.zoomed == true {
      zoomed = (.drawer, 0)
    } else {
      zoomed = (.none, 0)
    }
    wolf_ffi_app_state_add_tab(
      tab.cwd, tab.sidebarVisible, focused.0.rawValue, focused.1, zoomed.0.rawValue, zoomed.1)
    let sidebar = tab.sidebar
    if sidebar != RestoreSeam.SavedSidebar() {
      let mode: SidebarMode
      switch sidebar.mode {
      case .tree: mode = .tree
      case .search: mode = .search
      case nil: mode = .unset
      }
      wolf_ffi_app_state_tab_sidebar(
        mode.rawValue, sidebar.query ?? "", sidebar.caseSensitive ?? false,
        sidebar.useRegex ?? false, sidebar.allFiles ?? false)
    }
    guard folder else { return }
    if let drawer = tab.drawer {
      wolf_ffi_app_state_tab_drawer(drawer.open, drawer.share)
    }
    pushRoot(tab.layout ?? .leaf(path: "", preview: false))
  }

  /// A pane reference as a `pane_ref` kind and its index.
  private static func reference(_ ref: RestoreSeam.SavedPaneRef?) -> (PaneRef, UInt32) {
    switch ref {
    case .pane(let index): return (.pane, UInt32(clamping: index))
    case .drawer: return (.drawer, 0)
    case nil: return (.none, 0)
    }
  }

  /// Replays `node` through the builder in post order onto the last tab's root, each pane
  /// followed by its view's cursor or scroll.
  private static func pushRoot(_ node: SavedPaneNode) {
    for token in PaneLayoutCodec.postOrderTokens(node) {
      switch token {
      case .leaf(let path, let preview, let terminal, _, let view, let weight):
        let kind: PaneKind = terminal ? .terminal : preview ? .preview : .edit
        wolf_ffi_app_state_push_pane(kind.rawValue, path, weight)
        if kind == .edit, let cursor = view.cursor, let topLine = view.topLine {
          wolf_ffi_app_state_pane_cursor(cursor.line, cursor.col, topLine)
        } else if kind == .preview, let scroll = view.scroll {
          wolf_ffi_app_state_pane_scroll(scroll)
        }
      case .split(let orientation, let childCount, let weight):
        wolf_ffi_app_state_push_layout(
          orientation == .vertical ? 1 : 0, UInt32(clamping: childCount), weight)
      }
    }
    wolf_ffi_app_state_commit_root()
  }

  /// One saved tab, as saved and after `restored()`.
  private static func loadedTab(_ tab: FfiAppTabRef) -> LoadedTab {
    var layout: SavedPaneNode?
    if tab.has_root() {
      let node = project(tab.root(), zoomed: zoomedIndex(tab))
      layout = node.isEmptyLeaf ? nil : node
    }
    let saved = RestoreSeam.SavedTab(
      cwd: tab.directory().toString(), sidebarVisible: tab.sidebar_visible(), layout: layout,
      focused: focusedRef(tab), sidebar: sidebar(tab), drawer: drawer(tab))
    let restored = tab.restored()
    let rebuild =
      restored.has_root()
      ? Rebuild(
        layout: project(restored.root(), zoomed: zoomedIndex(restored)), tab: restored,
        focused: focusedRef(restored), drawer: drawer(restored))
      : nil
    return LoadedTab(saved: saved, rebuild: rebuild)
  }

  /// The tab's focused pane, `nil` when it saved none.
  private static func focusedRef(_ tab: FfiAppTabRef) -> RestoreSeam.SavedPaneRef? {
    switch PaneRef(rawValue: tab.focused_kind()) {
    case .pane: return .pane(Int(tab.focused_index()))
    case .drawer: return .drawer
    case .some(.none), nil: return nil
    }
  }

  /// The tab's sidebar mode, query and toggles, each `nil` when unset.
  private static func sidebar(_ tab: FfiAppTabRef) -> RestoreSeam.SavedSidebar {
    let mode: SidebarPanel?
    switch SidebarMode(rawValue: tab.sidebar_mode()) {
    case .tree: mode = .tree
    case .search: mode = .search
    case .unset, nil: mode = nil
    }
    func flag(_ raw: UInt8) -> Bool? {
      switch Tristate(rawValue: raw) {
      case .on: return true
      case .off: return false
      case .unset, nil: return nil
      }
    }
    return RestoreSeam.SavedSidebar(
      mode: mode, query: tab.has_query() ? tab.query().toString() : nil,
      caseSensitive: flag(tab.case_sensitive()), useRegex: flag(tab.use_regex()),
      allFiles: flag(tab.all_files()))
  }

  /// The tab's drawer, zoomed when the tab's zoom is the drawer; `nil` when it saved none.
  private static func drawer(_ tab: FfiAppTabRef) -> RestoreSeam.SavedDrawer? {
    guard tab.has_drawer() else { return nil }
    return RestoreSeam.SavedDrawer(
      open: tab.drawer_open(), share: tab.drawer_share(),
      zoomed: tab.zoomed_kind() == PaneRef.drawer.rawValue)
  }

  /// The depth-first index of the tab's zoomed pane, `nil` when it zooms none.
  private static func zoomedIndex(_ tab: FfiAppTabRef) -> Int? {
    tab.zoomed_kind() == PaneRef.pane.rawValue ? Int(tab.zoomed_index()) : nil
  }

  /// Projects a bridge node into a `SavedPaneNode`, marking the leaf at depth-first index
  /// `zoomed`.
  private static func project(_ root: FfiAppNode, zoomed: Int?) -> SavedPaneNode {
    var next = 0
    func walk(_ node: FfiAppNodeRef) -> SavedPaneNode {
      if node.is_pane() {
        defer { next += 1 }
        let kind = PaneKind(rawValue: node.kind())
        var view = SavedPaneView()
        if node.has_cursor() {
          view.cursor = SavedPaneView.Cursor(line: node.cursor_line(), col: node.cursor_col())
        }
        if node.has_top_line() { view.topLine = node.top_line() }
        if node.has_scroll() { view.scroll = node.scroll() }
        return .leaf(
          path: node.path().toString(), preview: kind == .preview, terminal: kind == .terminal,
          zoomed: next == zoomed, view: view)
      }
      var weights: [Double] = []
      var children: [SavedPaneNode] = []
      for i in 0..<node.child_count() {
        weights.append(node.weight(i))
        children.append(walk(node.child(i)))
      }
      return .split(
        orientation: Orientation(ffi: node.orientation()) ?? .horizontal, weights: weights,
        children: children)
    }
    return walk(root)
  }
}
