// Pure rules for the toolbar's sidebar panel control: its selection, clicks and segments.

import Foundation

/// Namespace for the toolbar's two-segment sidebar control, Project then Search.
///
/// The selected segment is derived from the sidebar's visibility and mode here, never set by the
/// path that changed them, so every path that shows, hides or switches the sidebar leaves the
/// control right.
public enum SidebarToolbarSeam {
  /// The control's `NSToolbarItem.Identifier` raw value.
  public static let identifier = "com.wolf.editor.toolbar.sidebarPanels"

  /// The panels in segment order, left to right.
  public static let panels: [SidebarPanel] = [.tree, .search]

  /// What a click on a segment does.
  public enum Click: Equatable {
    /// Hide the sidebar.
    case hide
    /// Show the sidebar on this panel.
    case show(SidebarPanel)
  }

  /// The segment index of `panel`.
  public static func segment(of panel: SidebarPanel) -> Int {
    panels.firstIndex(of: panel) ?? 0
  }

  /// The panel at segment `segment`, or `nil` for an index out of range, `-1` included, which is
  /// a control with nothing selected.
  public static func panel(atSegment segment: Int) -> SidebarPanel? {
    panels.indices.contains(segment) ? panels[segment] : nil
  }

  /// The panel the control and the View menu show as selected: `current` while the sidebar is
  /// visible, and `nil` while it is hidden.
  public static func selectedPanel(sidebarVisible: Bool, current: SidebarPanel?) -> SidebarPanel? {
    sidebarVisible ? current : nil
  }

  /// The segment to select, or `nil` for none. See ``selectedPanel(sidebarVisible:current:)``.
  public static func selectedSegment(sidebarVisible: Bool, current: SidebarPanel?) -> Int? {
    selectedPanel(sidebarVisible: sidebarVisible, current: current).map(segment(of:))
  }

  /// The effect of clicking `panel`'s segment: a click on the selected segment hides the sidebar,
  /// and any other click shows the clicked panel.
  public static func click(
    _ panel: SidebarPanel, sidebarVisible: Bool, current: SidebarPanel?
  ) -> Click {
    selectedPanel(sidebarVisible: sidebarVisible, current: current) == panel
      ? .hide : .show(panel)
  }

  /// The SF Symbol of `panel`'s segment.
  public static func symbolName(for panel: SidebarPanel) -> String {
    switch panel {
    case .tree: return "folder"
    case .search: return "magnifyingglass"
    }
  }

  /// The name of `panel`, the View menu's title for it.
  public static func label(for panel: SidebarPanel) -> String {
    switch panel {
    case .tree: return "Project"
    case .search: return "Search"
    }
  }

  /// The tooltip of `panel`'s segment: its name and its shortcut.
  public static func toolTip(for panel: SidebarPanel) -> String {
    switch panel {
    case .tree: return "Project (⇧⌘E)"
    case .search: return "Search (⇧⌘F)"
    }
  }
}
