// Pure check for whether a View-menu sidebar entry shows a checkmark.

import Foundation

/// Which sidebar panel a View-menu entry (or the live sidebar) refers to.
public enum SidebarPanel: Equatable {
  case tree
  case search
}

/// Namespace for the sidebar View-menu checkmark decision.
public enum SidebarMenuSeam {
  /// A panel's menu entry is checked iff it is the toolbar control's selected panel,
  /// ``SidebarToolbarSeam/selectedPanel(sidebarVisible:current:)``: the sidebar is visible
  /// and that panel is the active one. A hidden sidebar is never checked, and neither is
  /// any entry when `current` is `nil`.
  public static func isChecked(
    sidebarVisible: Bool, current: SidebarPanel?, item: SidebarPanel
  ) -> Bool {
    SidebarToolbarSeam.selectedPanel(sidebarVisible: sidebarVisible, current: current) == item
  }
}
