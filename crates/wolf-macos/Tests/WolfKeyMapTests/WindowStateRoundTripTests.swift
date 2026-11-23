// See crates/wolf-macos/Tests/WolfKeyMapTests/README.md

import Foundation
import XCTest

@testable import WolfKeyMap

/// Points `WOLF_STATE_DIR` at a fresh directory, once per process, before the process-wide
/// app-state store first opens `wolf.sqlite` there. Every test calls `ready()` before its first
/// `wolf_ffi_app_state_*` call; no other file in this target touches the store.
private enum StateDirFence {
  static let directory: String = {
    let env = ProcessInfo.processInfo.environment
    let base = env["TEST_TMPDIR"].flatMap { $0.isEmpty ? nil : $0 } ?? NSTemporaryDirectory()
    let dir = (base as NSString).appendingPathComponent("wolf-window-state-\(UUID().uuidString)")
    try? FileManager.default.createDirectory(atPath: dir, withIntermediateDirectories: true)
    setenv("WOLF_STATE_DIR", dir, 1)
    return dir
  }()

  static func ready() { _ = directory }
}

/// The macOS save and restore of `specs/application-state.md` §4 and §5 over a real
/// `wolf.sqlite`, through the production seams and the bridge, with no window or Neovim.
///
/// A quit is the terminate path's calls: `SessionSeam.windowState`, `WindowStateCodec.submit`,
/// then `wolf_ffi_app_state_flush`. A relaunch is the launch's pure steps over the saved window
/// state, `WindowStateCodec.load`, `RestoreSeam.plan`, `RestoreSeam.selectedPosition` and
/// `StackingOrder(seed:)` over fresh tab ids, followed by another quit, so what it writes is
/// what a second launch reads.
///
/// The store and its project-state rows outlive a test, so every test names its directories
/// with a UUID, and every live window list is passed out of stacking order, so only the
/// stacking can put it right.
final class WindowStateRoundTripTests: XCTestCase {

  /// One loaded window without its rebuilds, so that two loads compare.
  private struct Loaded: Equatable, CustomStringConvertible {
    let frame: CGRect?
    let activeTab: Int
    let minimized: Bool
    let tabs: [RestoreSeam.SavedTab]

    var description: String {
      "frame \(String(describing: frame)), active \(activeTab), minimized \(minimized), "
        + "tabs \(tabs.map(\.cwd))"
    }
  }

  /// The empty-state launch directory the planner falls back to.
  private let home = "/Users/nobody"

  /// Two frames with every field distinct.
  private let frame1 = CGRect(x: 120, y: 80, width: 1400, height: 900)
  private let frame2 = CGRect(x: 400, y: 200, width: 1000, height: 700)

  override func setUp() {
    super.setUp()
    StateDirFence.ready()
  }

  /// A directory no other test, and no earlier run, uses.
  private func uniqueDirectory(_ name: String) -> String {
    "/wolf-window-state-test/\(UUID().uuidString)/\(name)"
  }

  /// The saved window state as the launch reads it.
  private func load() -> [Loaded] {
    WindowStateCodec.load(wolf_ffi_app_state_window_state()).map {
      Loaded(
        frame: $0.frame, activeTab: $0.activeTab, minimized: $0.minimized,
        tabs: $0.tabs.map(\.saved))
    }
  }

  /// Commits `live` in `stacking`'s order and flushes, as the terminate path does.
  private func quit(
    _ live: [SessionSeam.LiveWindow], _ stacking: StackingOrder,
    file: StaticString = #filePath, line: UInt = #line
  ) {
    WindowStateCodec.submit(SessionSeam.windowState(live: live, stacking: stacking, orphans: []))
    XCTAssertTrue(
      wolf_ffi_app_state_flush(), "premise: the window state's write must be attempted in time",
      file: file, line: line)
  }

  /// Runs the launch's pure steps over the saved window state, with every directory in
  /// `directories` present, then quits again with the restored windows passed in reverse.
  private func relaunch(directories: Set<String>) {
    let loaded = WindowStateCodec.load(wolf_ffi_app_state_window_state())
    let plan = RestoreSeam.plan(
      windows: loaded.map { RestoreSeam.SavedWindow(tabs: $0.tabs.map(\.saved)) }, home: home,
      isDirectory: { directories.contains($0) })
    var nextId = 100
    var seeds: [StackingOrder.SeedWindow] = []
    var live: [SessionSeam.LiveWindow] = []
    for (index, window) in loaded.enumerated() {
      let positions = plan.filter { $0.groupIndex == index }.map(\.positionInGroup)
      let ids = positions.map { _ -> Int in
        nextId += 1
        return nextId
      }
      let selected = RestoreSeam.selectedPosition(
        active: window.activeTab, materialized: positions
      ).flatMap { positions.firstIndex(of: $0) }
      seeds.append(
        StackingOrder.SeedWindow(
          tabs: ids, activeTab: selected ?? 0, minimized: window.minimized))
      live.append(
        SessionSeam.LiveWindow(
          frame: window.frame ?? .zero,
          tabs: zip(ids, positions).map {
            SessionSeam.LiveTab(id: $0.0, tab: window.tabs[$0.1].saved)
          },
          selected: selected.map { ids[$0] }))
    }
    quit(live.reversed(), StackingOrder(seed: seeds))
  }

  // MARK: - RT1: windows, tabs, the active tab and the zoom

  func testASessionSurvivesAQuitAndRelaunchInStackingOrderWithItsActiveTabAndZoom() {
    let d1 = uniqueDirectory("one")
    let d2 = uniqueDirectory("two")
    let d3 = uniqueDirectory("three")
    let t0 = RestoreSeam.SavedTab(
      cwd: d1, sidebarVisible: true, layout: .leaf(path: "\(d1)/alpha.md", preview: false))
    let zoomed = RestoreSeam.SavedTab(
      cwd: d2, sidebarVisible: false,
      layout: .split(
        orientation: .vertical, weights: [0.3, 0.7],
        children: [
          .leaf(path: "\(d2)/beta.md", preview: false),
          .leaf(path: "\(d2)/beta.md", preview: true, zoomed: true),
        ]))
    let empty = RestoreSeam.SavedTab(
      cwd: "", sidebarVisible: true, layout: .leaf(path: "\(d1)/stale.md", preview: false))
    let t3 = RestoreSeam.SavedTab(
      cwd: d3, sidebarVisible: true,
      layout: .split(
        orientation: .horizontal, weights: [0.6, 0.4],
        children: [
          .leaf(path: "\(d3)/gamma.md", preview: false),
          .leaf(path: d3, preview: false, terminal: true),
        ]))

    let w1 = SessionSeam.LiveWindow(
      frame: frame1,
      tabs: [
        SessionSeam.LiveTab(id: 1, tab: t0), SessionSeam.LiveTab(id: 2, tab: zoomed),
        SessionSeam.LiveTab(id: 3, tab: empty),
      ],
      selected: 2)
    let w2 = SessionSeam.LiveWindow(
      frame: frame2, tabs: [SessionSeam.LiveTab(id: 4, tab: t3)], selected: 4)
    var stacking = StackingOrder()
    for tab in [1, 3, 2, 4] { stacking.focus(tab) }
    quit([w1, w2], stacking)

    let expected = [
      Loaded(frame: frame2, activeTab: 0, minimized: false, tabs: [t3]),
      Loaded(
        frame: frame1, activeTab: 1, minimized: false,
        tabs: [t0, zoomed, RestoreSeam.SavedTab(cwd: "", sidebarVisible: true, layout: nil)]),
    ]
    let first = load()
    XCTAssertEqual(
      first.map(\.frame), [frame2, frame1],
      "W2 holds the tab focused last, so it must load first, with each window's frame; "
        + "[W1, W2] means the commit is not in stacking order, and a missing window means it "
        + "was never written: \(first)")
    XCTAssertEqual(
      first.map(\.activeTab), [0, 1],
      "W1's selected tab is its second, so it must load with active tab 1; 0 means "
        + "active_tab is not written or not read: \(first)")
    XCTAssertEqual(
      first.last?.tabs[safe: 1]?.layout?.zoomedLeafIndex, 1,
      "the zoomed preview is W1's second tab's leaf 1; nil means the zoom is lost, through "
        + "submit writing no zoomed or load marking no leaf: \(first)")
    XCTAssertNil(
      first.last?.tabs[safe: 2]?.layout,
      "an empty tab has no root, whatever layout its cache holds; a layout here means submit "
        + "wrote the empty tab's root: \(first)")
    XCTAssertEqual(
      first, expected,
      "every saved tab must load equal to what was saved, an empty tab with no layout; the "
        + "first differing field names what the round trip loses")

    relaunch(directories: [d1, d2, d3])
    XCTAssertEqual(
      load(), first,
      "a relaunch that changes nothing must write back the window state it read: the same "
        + "stacking order, active tabs and zoom. A different order means the seed does not "
        + "reproduce the saved stacking order")
  }

  // MARK: - RT2: a minimized window

  func testAMinimizedWindowRestoresMinimizedAndBehindTheOthers() {
    let d1 = uniqueDirectory("one")
    let d2 = uniqueDirectory("two")
    let t0 = RestoreSeam.SavedTab(
      cwd: d1, sidebarVisible: true, layout: .leaf(path: "\(d1)/alpha.md", preview: false))
    let t1 = RestoreSeam.SavedTab(
      cwd: d2, sidebarVisible: true, layout: .leaf(path: "\(d2)/beta.md", preview: false))
    let w1 = SessionSeam.LiveWindow(
      frame: frame1, tabs: [SessionSeam.LiveTab(id: 1, tab: t0)], selected: 1)
    let w2 = SessionSeam.LiveWindow(
      frame: frame2, tabs: [SessionSeam.LiveTab(id: 2, tab: t1)], selected: 2)
    var stacking = StackingOrder()
    for tab in [1, 2, 1] { stacking.focus(tab) }
    stacking.minimize([1])
    quit([w1, w2], stacking)

    let first = load()
    XCTAssertEqual(
      first.map(\.frame), [frame2, frame1],
      "W1 was focused last but then minimized, so it must load behind W2: \(first)")
    XCTAssertEqual(
      first.map(\.minimized), [false, true],
      "W1 must load minimized and W2 not; [false, false] means submit never writes the flag "
        + "or load never reads it: \(first)")

    relaunch(directories: [d1, d2])
    let second = load()
    XCTAssertEqual(
      second.map(\.frame), [frame2, frame1],
      "after a relaunch the minimized window must still be behind: \(second)")
    XCTAssertEqual(
      second.map(\.minimized), [false, true],
      "after a relaunch the minimized window must still be minimized; false means the seed "
        + "drops the saved flag: \(second)")
  }

  // MARK: - RT3: the project states a quit writes

  func testTheProjectStatesFollowTheRuleForADirectoryOpenInTwoWindows() {
    let d = uniqueDirectory("shared")
    let e = uniqueDirectory("other")
    let tab: (String, String) -> RestoreSeam.SavedTab = { dir, name in
      RestoreSeam.SavedTab(
        cwd: dir, sidebarVisible: true, layout: .leaf(path: "\(dir)/\(name).md", preview: false))
    }
    let front = SessionSeam.LiveWindow(
      frame: frame1,
      tabs: [
        SessionSeam.LiveTab(id: 1, tab: tab(d, "f1")),
        SessionSeam.LiveTab(id: 2, tab: tab(e, "e1")),
        SessionSeam.LiveTab(id: 3, tab: tab(e, "e2")),
      ],
      selected: 1)
    let back = SessionSeam.LiveWindow(
      frame: frame2,
      tabs: [
        SessionSeam.LiveTab(id: 4, tab: tab(d, "b1")),
        SessionSeam.LiveTab(id: 5, tab: tab(e, "e3")),
      ],
      selected: 4)
    var stacking = StackingOrder()
    for id in [5, 4, 2, 3, 1] { stacking.focus(id) }
    quit([back, front], stacking)

    let shared = wolf_ffi_app_state_project_state(d)
    XCTAssertTrue(shared.found(), "D has open tabs, so the quit must write its project state")
    XCTAssertEqual(
      shared.tab().root().path().toString(), "\(d)/f1.md",
      "rule 1: the frontmost window's active tab is in D, so f1 must be stored; b1 means the "
        + "back window was written first, so it counted as frontmost")
    XCTAssertEqual(
      CGRect(x: shared.x(), y: shared.y(), width: shared.width(), height: shared.height()),
      frame1, "D's project state must carry the frame of the window f1 came from")

    let other = wolf_ffi_app_state_project_state(e)
    XCTAssertTrue(other.found(), "E has open tabs, so the quit must write its project state")
    XCTAssertEqual(
      other.tab().root().path().toString(), "\(e)/e2.md",
      "rule 2: no active tab is in E, so the frontmost window's most recently selected E tab, "
        + "e2, must be stored. e1 means no recency was written and the rule fell back to tab "
        + "order; e3 means the back window counted as frontmost")
  }

  // MARK: - RT4: the per-tab and per-pane extras

  /// S1: every extra of §2 survives a quit and a relaunch: the focus, the sidebar's mode, query
  /// and toggles, the drawer with its share and zoom, an edit pane's cursor and top line, and a
  /// preview's scroll. An empty tab keeps its sidebar and nothing else.
  func testEveryPerTabExtraSurvivesAQuitAndRelaunch() {
    let d1 = uniqueDirectory("extras-one")
    let d2 = uniqueDirectory("extras-two")
    let edited = SavedPaneView(cursor: .init(line: 12, col: 4), topLine: 7)
    let scrolled = SavedPaneView(scroll: 0.25)
    let searching = RestoreSeam.SavedSidebar(
      mode: .search, query: "TODO", caseSensitive: true, useRegex: false, allFiles: true)
    let browsing = RestoreSeam.SavedSidebar(
      mode: .tree, query: "", caseSensitive: false, useRegex: true, allFiles: false)
    let first = RestoreSeam.SavedTab(
      cwd: d1, sidebarVisible: true,
      layout: .split(
        orientation: .vertical, weights: [0.5, 0.3, 0.2],
        children: [
          .leaf(path: "\(d1)/alpha.md", preview: false, view: edited),
          .leaf(path: "\(d1)/alpha.md", preview: true, view: scrolled),
          .leaf(path: d1, preview: false, terminal: true),
        ]),
      focused: .pane(1), sidebar: searching,
      drawer: RestoreSeam.SavedDrawer(open: true, share: 0.4))
    let second = RestoreSeam.SavedTab(
      cwd: d2, sidebarVisible: false, layout: .leaf(path: "\(d2)/beta.md", preview: false),
      focused: .drawer, sidebar: browsing,
      drawer: RestoreSeam.SavedDrawer(open: true, share: 0.5, zoomed: true))
    let empty = RestoreSeam.SavedTab(
      cwd: "", sidebarVisible: true, layout: .leaf(path: "\(d1)/stale.md", preview: false),
      focused: .pane(0), sidebar: searching,
      drawer: RestoreSeam.SavedDrawer(open: true, share: 0.3, zoomed: true))

    var stacking = StackingOrder()
    for tab in [3, 2, 1] { stacking.focus(tab) }
    quit(
      [
        SessionSeam.LiveWindow(
          frame: frame1,
          tabs: [
            SessionSeam.LiveTab(id: 1, tab: first), SessionSeam.LiveTab(id: 2, tab: second),
            SessionSeam.LiveTab(id: 3, tab: empty),
          ],
          selected: 1)
      ], stacking)

    let loaded = load()
    let tabs = loaded.first?.tabs ?? []
    guard tabs.count == 3 else {
      XCTFail("premise: the one window must load with its three tabs: \(loaded)")
      return
    }
    XCTAssertEqual(
      tabs[0].layout?.leafViews, [edited, scrolled, SavedPaneView()],
      "the edit leaf must load its cursor (12, 4) and top line 7, and the preview its scroll"
        + " 0.25. An empty first view means submit skips pane_cursor or load ignores has_cursor;"
        + " an empty second that submit skips pane_scroll or load ignores has_scroll")
    XCTAssertEqual(
      tabs[0].sidebar, searching,
      "the search sidebar must load with its query and toggles; SavedSidebar() means submit"
        + " skips tab_sidebar or load reads none of it")
    XCTAssertEqual(
      tabs[1].sidebar, browsing,
      "the tree sidebar must load with its empty query and its toggles; a nil query or toggle"
        + " means an explicit value loads as unset")
    XCTAssertEqual(
      tabs[0].drawer, RestoreSeam.SavedDrawer(open: true, share: 0.4),
      "the open drawer must load open at 0.4; nil means submit skips tab_drawer or load ignores"
        + " has_drawer")
    XCTAssertEqual(
      tabs[0].focused, .pane(1),
      "the focus on the preview, leaf 1, must load; nil means submit writes the focus as NONE")
    XCTAssertEqual(
      tabs[1].focused, .drawer,
      "a focused drawer must load as .drawer; nil means load maps the DRAWER kind to no focus,"
        + " .pane(0) that submit writes it as a pane")
    XCTAssertEqual(
      tabs[1].drawer, RestoreSeam.SavedDrawer(open: true, share: 0.5, zoomed: true),
      "a zoomed drawer must load zoomed; zoomed: false means submit writes the drawer's zoom as"
        + " no zoom, or load reads the DRAWER zoom kind as none")
    XCTAssertEqual(
      tabs[2], RestoreSeam.SavedTab(cwd: "", sidebarVisible: true, layout: nil, sidebar: searching),
      "an empty tab must load with its sidebar and no layout, focus or drawer, whatever its"
        + " cache held; a focus or drawer here would restore into a tab with no panes")
    XCTAssertEqual(
      tabs, [first, second, tabs[2]],
      "every folder tab must load equal to what was saved; the first differing field names what"
        + " the round trip loses")

    relaunch(directories: [d1, d2])
    XCTAssertEqual(
      load(), loaded,
      "a relaunch that changes nothing must write back every extra it read; a difference means"
        + " the launch's restore steps drop an extra that the first load carried")
  }
}

extension Array {
  /// The element at `index`, or `nil` when it is out of range.
  fileprivate subscript(safe index: Int) -> Element? {
    indices.contains(index) ? self[index] : nil
  }
}
