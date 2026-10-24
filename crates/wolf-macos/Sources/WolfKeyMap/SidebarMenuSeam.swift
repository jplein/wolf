// Pure check for whether a View-menu sidebar entry shows a checkmark.

import Foundation

/// Which sidebar panel a View-menu entry (or the live sidebar) refers to.
public enum SidebarPanel: Equatable {
  case tree
  case search
}

/// Namespace for the sidebar View-menu checkmark decision.
public enum SidebarMenuSeam {
  /// A panel's menu entry is checked iff the sidebar is visible AND that panel
  /// is the active one. A hidden sidebar (`sidebarVisible == false`) is never
  /// checked, regardless of `current`; likewise `current == nil` (no active
  /// panel) is never checked.
  public static func isChecked(
    sidebarVisible: Bool, current: SidebarPanel?, item: SidebarPanel
  ) -> Bool {
    sidebarVisible && current == item
  }
}
