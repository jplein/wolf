// The window state's write to and read from the app-state store, through wolf-ffi's builder.

import CoreGraphics
import Foundation

/// Converts between the front-end's persistence types and the `wolf_ffi_app_state_*` surface,
/// in the shape `specs/application-state.md` §2 defines.
///
/// The zoom travels as the tab's `zoomed` pane index, the depth-first index of the first leaf
/// `SavedPaneNode` marks zoomed, and is read back onto that leaf. An empty tab (`cwd == ""`)
/// writes no root. A folder tab with no layout writes a lone edit pane on no file, which reads
/// back as no layout. The per-tab extras (`focused`, cursor, scroll, the sidebar's mode, query and
/// toggles, the drawer) are written as absent.
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

  /// Submits `windows`, in list order, to the shared app-state store, which writes the window
  /// state and the project states in one transaction. It does not flush.
  public static func submit(_ windows: [Window]) {
    wolf_ffi_app_state_begin()
    for window in windows {
      wolf_ffi_app_state_add_window(
        true, window.frame.origin.x, window.frame.origin.y, window.frame.size.width,
        window.frame.size.height, UInt32(clamping: window.activeTab))
      if window.minimized { wolf_ffi_app_state_window_minimized(true) }
      for tab in window.tabs {
        let folder = !tab.cwd.isEmpty
        let zoomed = folder ? tab.layout?.zoomedLeafIndex : nil
        wolf_ffi_app_state_add_tab(
          tab.cwd, tab.sidebarVisible, PaneRef.none.rawValue, 0,
          (zoomed == nil ? PaneRef.none : PaneRef.pane).rawValue, UInt32(clamping: zoomed ?? 0))
        guard folder else { continue }
        pushRoot(tab.layout ?? .leaf(path: "", preview: false))
      }
      for index in window.recency {
        wolf_ffi_app_state_push_recency(UInt32(clamping: index))
      }
    }
    wolf_ffi_app_state_commit()
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

  /// Replays `node` through the builder in post order onto the last tab's root.
  private static func pushRoot(_ node: SavedPaneNode) {
    for token in PaneLayoutCodec.postOrderTokens(node) {
      switch token {
      case .leaf(let path, let preview, let terminal, _, let weight):
        let kind: PaneKind = terminal ? .terminal : preview ? .preview : .edit
        wolf_ffi_app_state_push_pane(kind.rawValue, path, weight)
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
      cwd: tab.directory().toString(), sidebarVisible: tab.sidebar_visible(), layout: layout)
    let restored = tab.restored()
    let rebuild =
      restored.has_root()
      ? Rebuild(layout: project(restored.root(), zoomed: zoomedIndex(restored)), tab: restored)
      : nil
    return LoadedTab(saved: saved, rebuild: rebuild)
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
        return .leaf(
          path: node.path().toString(), preview: kind == .preview, terminal: kind == .terminal,
          zoomed: next == zoomed)
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
