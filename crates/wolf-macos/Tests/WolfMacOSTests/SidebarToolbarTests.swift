// Checks that the toolbar's sidebar control follows every path that shows, hides or switches it.

import AppKit
import WolfKeyMap
import XCTest

@testable import WolfMacOS

/// A production `MainWindow` over a `LiveCore` on the staged `init.lua`, never shown, whose
/// toolbar holds the sidebar panel control. The core is real because a search click seeds the
/// search toggles, which reads `ignorecase` from Neovim.
@MainActor
private final class SidebarToolbarFixture {
  /// How long a sidebar change gets to reach the split item and the container.
  static let timeout: TimeInterval = 15
  /// The size `MainWindow` gives the editor pane, as `WolfWindowController` builds it.
  private static let cols = 80
  private static let rows = 24

  let live: LiveCore
  let sidebar: SidebarContainerView
  let window: MainWindow

  /// Starts a core and hosts a sidebar and a pane container in a `MainWindow`; `nil`, with the
  /// test failed, when the core does not start.
  init?(file: StaticString = #filePath, line: UInt = #line) {
    guard let live = LiveCore.start(file: file, line: line) else { return nil }
    self.live = live
    // AppKit's drawing machinery is set up by the shared application.
    _ = NSApplication.shared
    let handle = live.handle
    let container = PaneContainerView(
      handle: handle, metrics: live.metrics, tabpageHandle: handle.current_tabpage(),
      optionAsAlt: false)
    sidebar = SidebarContainerView(
      tree: FileTreeView(handle: handle, paneContainer: container),
      search: SearchView(handle: handle, paneContainer: container))
    window = MainWindow(
      sidebar: sidebar, paneContainer: container, metrics: live.metrics, cols: Self.cols,
      rows: Self.rows)
    window.layoutIfNeeded()
  }

  /// The toolbar item that holds the sidebar panel control, or `nil` when the toolbar has none.
  var item: NSToolbarItem? {
    window.toolbar?.items.first { $0.itemIdentifier == MainWindow.sidebarPanelsItemIdentifier }
  }

  /// The sidebar panel control, or `nil` when its item is missing or holds another view.
  var control: NSSegmentedControl? {
    item?.view as? NSSegmentedControl
  }

  /// Whether the sidebar is shown.
  var isVisible: Bool {
    !window.sidebarItem.isCollapsed
  }

  /// Waits until the sidebar's visibility is `visible` and, when `mode` is given, the container
  /// shows `mode`; returns whether it did.
  func reach(visible: Bool, mode: SidebarContainerView.Mode?) -> Bool {
    live.waitFor(timeout: Self.timeout) {
      isVisible == visible && (mode.map { sidebar.mode == $0 } ?? true)
    }
  }

  func shutdown() {
    live.shutdown()
  }
}

/// The toolbar's sidebar panel control in a production `MainWindow`: its place after Back and
/// Forward, its selection after each path that changes the sidebar, a click's effect, and the
/// sidebar's content, which starts at the safe area's top with no header strip above it.
///
/// `WolfWindowController` cannot be built offscreen: its `build()` needs a `Project` started
/// against `AppDelegate` and orders its window front. So the command paths enter at the
/// `MainWindow` members its `toggleSidebar()` and `showSidebar(panel:)` forward to, and the
/// `CommandDispatcher` arms and those forwarders are checked only by the compiler.
///
/// A click is `selectedSegment` set to the clicked index followed by `sendAction`, which is what
/// a `.selectOne` click does; AppKit routes no mouse event to a window that is never shown.
/// Selection is read per segment with `isSelected(forSegment:)`, because `selectedSegment` keeps
/// its old value after a segment is deselected.
///
/// Premise, measured by each step's wait: `NSSplitViewController.toggleSidebar(_:)` flips the
/// sidebar item's `isCollapsed` in a window that is never shown.
@MainActor
final class SidebarToolbarTests: XCTestCase {
  /// How long the control gets to follow a sidebar change that has already happened.
  private static let selectionTimeout: TimeInterval = 2
  /// The largest distance, in points, a panel's top may be from the safe area's top.
  private static let edgeTolerance: CGFloat = 0.5

  /// Which segments are selected, tree first.
  private static func selection(_ control: NSSegmentedControl) -> [Bool] {
    (0..<control.segmentCount).map { control.isSelected(forSegment: $0) }
  }

  /// `selection` in words, for a message.
  private static func describe(_ selection: [Bool]) -> String {
    guard selection.count == 2 else { return "\(selection.count) segments \(selection)" }
    let state = { (on: Bool) in on ? "selected" : "clear" }
    return "tree segment \(state(selection[0])), search segment \(state(selection[1]))"
  }

  /// The control, or `nil` after failing the test as a premise failure when the toolbar lacks it.
  private func requireControl(
    _ fixture: SidebarToolbarFixture, file: StaticString = #filePath, line: UInt = #line
  ) -> NSSegmentedControl? {
    guard let control = fixture.control else {
      let ids = fixture.window.toolbar?.items.map(\.itemIdentifier.rawValue) ?? []
      XCTFail(
        "premise: the toolbar must hold an NSSegmentedControl under"
          + " \(MainWindow.sidebarPanelsItemIdentifier.rawValue), but its items are \(ids) and"
          + " that item's view is \(String(describing: fixture.item?.view))",
        file: file, line: line)
      return nil
    }
    return control
  }

  /// Waits for the sidebar to reach `visible` on `mode`, then expects exactly the segments in
  /// `selected` to be selected; returns whether the step held. `meaning` says what the wrong
  /// selection would mean, and `unreached` what a sidebar that never gets there would mean.
  /// Without `unreached`, the sidebar's change is the step's premise and fails as one.
  @discardableResult
  private func expect(
    _ fixture: SidebarToolbarFixture, _ control: NSSegmentedControl, step: String,
    visible: Bool, mode: SidebarContainerView.Mode?, selected: [Bool], meaning: String,
    unreached: String? = nil, file: StaticString = #filePath, line: UInt = #line
  ) -> Bool {
    guard fixture.reach(visible: visible, mode: mode) else {
      let wanted = (visible ? "visible" : "hidden") + (mode.map { " on \($0)" } ?? "")
      let state = (fixture.isVisible ? "visible" : "hidden") + " on \(fixture.sidebar.mode)"
      let why =
        unreached
        ?? "The path itself did not change the sidebar, so the control is not what failed."
      XCTFail(
        "\(unreached == nil ? "premise, " : "")\(step): the sidebar must become \(wanted), but"
          + " it is \(state) after \(SidebarToolbarFixture.timeout) s. \(why)",
        file: file, line: line)
      return false
    }
    fixture.live.waitFor(timeout: Self.selectionTimeout) { Self.selection(control) == selected }
    let actual = Self.selection(control)
    XCTAssertEqual(
      actual, selected,
      "\(step): the control must show \(Self.describe(selected)), but it shows"
        + " \(Self.describe(actual)). \(meaning)",
      file: file, line: line)
    return actual == selected
  }

  /// Clicks segment `segment` as a `.selectOne` click does; returns whether the action was sent.
  private func click(
    _ control: NSSegmentedControl, segment: Int, file: StaticString = #filePath,
    line: UInt = #line
  ) -> Bool {
    control.selectedSegment = segment
    let sent = control.sendAction(control.action, to: control.target)
    XCTAssertTrue(
      sent,
      "premise: clicking segment \(segment) must send the control's action, but it has action"
        + " \(String(describing: control.action)) and target \(String(describing: control.target))",
      file: file, line: line)
    return sent
  }

  /// The toolbar runs separator, Back, Forward, then the control, with no system sidebar toggle;
  /// the control's item is navigational, and its two segments carry the seam's tooltips.
  func testTheToolbarHoldsTheControlAfterBackAndForward() {
    guard let fixture = SidebarToolbarFixture() else { return }
    defer { fixture.shutdown() }
    let ids = fixture.window.toolbar?.items.map(\.itemIdentifier) ?? []
    XCTAssertEqual(
      ids,
      [
        .sidebarTrackingSeparator, MainWindow.backItemIdentifier,
        MainWindow.forwardItemIdentifier, MainWindow.sidebarPanelsItemIdentifier,
      ],
      "the toolbar must run tracking separator, Back, Forward, sidebar control, but it runs"
        + " \(ids.map(\.rawValue)). A toggleSidebar entry means the system toggle is still"
        + " installed; the control ahead of Back means it does not follow Back and Forward")
    XCTAssertFalse(
      ids.contains(.toggleSidebar), "the system sidebar toggle must be gone from the toolbar")

    guard let control = requireControl(fixture) else { return }
    XCTAssertEqual(
      fixture.item?.isNavigational, true,
      "the control's item must be navigational, or AppKit places it after the title instead of"
        + " beside Back and Forward")
    XCTAssertEqual(
      control.segmentCount, SidebarToolbarSeam.panels.count,
      "the control must have one segment per sidebar panel")
    for (index, panel) in SidebarToolbarSeam.panels.enumerated()
    where index < control.segmentCount {
      XCTAssertEqual(
        control.toolTip(forSegment: index), SidebarToolbarSeam.toolTip(for: panel),
        "segment \(index) must carry the \(panel) tooltip naming the panel and its shortcut;"
          + " nil means the tooltips are never set on the segments")
    }
  }

  /// The paths of the `toggleSidebar`, `sidebarShowProject` and `searchInProject` commands leave
  /// the control on the visible panel, and on neither segment while the sidebar is hidden.
  func testTheSidebarCommandsLeaveTheControlOnTheVisiblePanel() {
    guard let fixture = SidebarToolbarFixture() else { return }
    defer { fixture.shutdown() }
    guard let control = requireControl(fixture) else { return }
    let window = fixture.window

    guard
      expect(
        fixture, control, step: "at start", visible: true, mode: .tree,
        selected: [true, false],
        meaning: "Neither means the control is built without the derived selection; search means"
          + " the segments are in the wrong order")
    else { return }

    window.showSidebar(panel: .search)
    expect(
      fixture, control, step: "showSidebar(.search) from tree", visible: true, mode: .search,
      selected: [false, true],
      meaning: "Tree still selected means showSidebar(panel:) switches the mode without"
        + " refreshing the control, so searchInProject leaves it stale")

    window.showSidebar(panel: .tree)
    expect(
      fixture, control, step: "showSidebar(.tree) from search", visible: true, mode: .tree,
      selected: [true, false],
      meaning: "Search still selected means showSidebar(panel:) switches the mode without"
        + " refreshing the control, so sidebarShowProject leaves it stale")

    window.toggleSidebar()
    expect(
      fixture, control, step: "toggleSidebar() hiding the sidebar", visible: false, mode: nil,
      selected: [false, false],
      meaning: "Tree still selected means the control is not refreshed when isCollapsed changes,"
        + " so it stays lit after the toggleSidebar command hides the sidebar")

    window.toggleSidebar()
    expect(
      fixture, control, step: "toggleSidebar() showing the sidebar again", visible: true,
      mode: .tree, selected: [true, false],
      meaning: "Neither means the control is not refreshed when isCollapsed changes back")

    window.toggleSidebar()
    guard
      expect(
        fixture, control, step: "toggleSidebar() hiding the sidebar again", visible: false,
        mode: nil, selected: [false, false],
        meaning: "Tree still selected means the control is not refreshed when isCollapsed"
          + " changes")
    else { return }

    window.showSidebar(panel: .search)
    expect(
      fixture, control, step: "showSidebar(.search) while hidden on tree", visible: true,
      mode: .search, selected: [false, true],
      meaning: "Tree means the control was refreshed when isCollapsed changed, before the mode"
        + " switched, and not after it; neither means showing the sidebar refreshed nothing")
  }

  /// Writing `isCollapsed` directly, as a state restore and a divider drag do, clears the control
  /// and then restores it.
  func testADirectCollapseClearsTheControl() {
    guard let fixture = SidebarToolbarFixture() else { return }
    defer { fixture.shutdown() }
    guard let control = requireControl(fixture) else { return }
    guard
      expect(
        fixture, control, step: "at start", visible: true, mode: .tree, selected: [true, false],
        meaning: "Neither means the control is built without the derived selection")
    else { return }

    fixture.window.sidebarItem.isCollapsed = true
    expect(
      fixture, control, step: "isCollapsed = true", visible: false, mode: nil,
      selected: [false, false],
      meaning: "Tree still selected means the control follows only the commands and not the"
        + " split item's isCollapsed, so a restore or a drag that hides the sidebar leaves it lit")

    fixture.window.sidebarItem.isCollapsed = false
    expect(
      fixture, control, step: "isCollapsed = false", visible: true, mode: .tree,
      selected: [true, false],
      meaning: "Neither means the control does not follow isCollapsed when the sidebar comes"
        + " back")
  }

  /// A click on a segment switches to its panel, a click on the selected one hides the sidebar,
  /// and a click while hidden shows the clicked panel.
  func testAToolbarClickTakesTheSamePath() {
    guard let fixture = SidebarToolbarFixture() else { return }
    defer { fixture.shutdown() }
    guard let control = requireControl(fixture) else { return }
    guard
      expect(
        fixture, control, step: "at start", visible: true, mode: .tree, selected: [true, false],
        meaning: "Neither means the control is built without the derived selection")
    else { return }

    guard click(control, segment: 1) else { return }
    expect(
      fixture, control, step: "click search while tree shows", visible: true, mode: .search,
      selected: [false, true],
      meaning: "The click selects search itself, so a missing refresh does not show here; any"
        + " other selection means a refresh overwrote the click's with one derived from the mode"
        + " before the switch",
      unreached: "Hidden on tree means the action acted on the tree panel instead of the clicked"
        + " segment's, and took the click for one on the active segment")

    guard click(control, segment: 1) else { return }
    expect(
      fixture, control, step: "click search while search shows", visible: false, mode: nil,
      selected: [false, false],
      meaning: "Search still selected means hiding from the toolbar does not refresh the control",
      unreached: "Still visible means a click on the active segment shows its panel again"
        + " instead of hiding the sidebar")

    guard click(control, segment: 0) else { return }
    expect(
      fixture, control, step: "click tree while hidden", visible: true, mode: .tree,
      selected: [true, false],
      meaning: "The click selects tree itself, so a missing refresh does not show here; any"
        + " other selection means a refresh overwrote the click's with one derived from the"
        + " sidebar still counted as hidden, which clears the control",
      unreached: "Still hidden means a click while hidden is taken for a click on the active"
        + " segment instead of showing the clicked panel")

    guard click(control, segment: 1) else { return }
    expect(
      fixture, control, step: "click search while tree shows again", visible: true,
      mode: .search, selected: [false, true],
      meaning: "The click selects search itself, so a missing refresh does not show here; any"
        + " other selection means a refresh overwrote the click's with one derived from the mode"
        + " before the switch",
      unreached: "Any other state means a click on the other segment does not switch to it")
  }

  /// With no header strip, the top of both panels is the top of the sidebar's safe area, which
  /// clears the transparent titlebar.
  func testSidebarContentStartsBelowTheTitlebar() {
    guard let fixture = SidebarToolbarFixture() else { return }
    defer { fixture.shutdown() }
    let sidebar = fixture.sidebar
    sidebar.layoutSubtreeIfNeeded()
    let inset = sidebar.safeAreaInsets.top
    guard inset > 0, sidebar.bounds.height > inset else {
      XCTFail(
        "premise: the sidebar must sit under the transparent titlebar, with a safe-area top inset"
          + " above 0 and room below it, but its inset is \(inset) and its bounds"
          + " \(sidebar.bounds); the window is not laid out as it is on screen")
      return
    }
    let safe = sidebar.safeAreaRect
    // The distance below the safe area's top, positive downward in either orientation.
    let below = { (rect: NSRect) -> CGFloat in
      sidebar.isFlipped ? rect.minY - safe.minY : safe.maxY - rect.maxY
    }
    let panels: [(String, NSView)] = [("tree", sidebar.tree), ("search", sidebar.search)]
    for (name, panel) in panels {
      let offset = below(sidebar.convert(panel.bounds, from: panel))
      XCTAssertEqual(
        offset, 0, accuracy: Self.edgeTolerance,
        "the \(name) panel's top must be the sidebar's safe-area top, but it is \(offset) points"
          + " below it with a safe-area inset of \(inset). About 30 below means the header"
          + " strip's space is still reserved; about -\(inset) means the panel is pinned to the"
          + " view's top, under the transparent titlebar")
    }
  }
}
