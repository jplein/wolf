// Requests from the real wolf command over the real socket, routed and opened in a live tab.

import AppKit
import WolfKeyMap
import XCTest

@testable import WolfMacOS

private typealias LiveTab = OpenFolderSeam.LiveTab
private typealias LiveWindow = OpenFolderSeam.LiveWindow

/// The socket half of the harness: a canonical base under `TEST_TMPDIR` entered as the working
/// directory, the process-wide listener bound in its relative state dir, and the staged `wolf`.
///
/// The working directory is entered because no absolute socket path under Bazel's `TMPDIR` fits
/// 103 bytes: the listener and the command both name the state dir relative to the base.
@MainActor
private final class OpenChannel {
  /// The state dir, relative to `base`, that the listener and the command share.
  static let stateDir = "s"
  /// How long a command gets to exit and its request to drain.
  static let timeout: TimeInterval = 15

  let base: String
  private let wolf: String
  private let previous: String

  /// Enters a fresh base and binds the listener in it; `nil`, with the test failed, when the
  /// staged `wolf` does not resolve or the listener does not bind.
  init?(file: StaticString = #filePath, line: UInt = #line) {
    guard let wolf = LiveCore.runfile("WOLF_TEST_WOLF_BIN") else {
      let raw = ProcessInfo.processInfo.environment["WOLF_TEST_WOLF_BIN"] ?? "unset"
      XCTFail(
        "premise: WOLF_TEST_WOLF_BIN (\(raw)) resolves to no staged wolf: the WolfMacOSTests"
          + " target must stage //crates/wolf-core:wolf", file: file, line: line)
      return nil
    }
    guard let base = LiveCore.scratchDirectory(file: file, line: line) else { return nil }
    self.wolf = wolf
    self.base = base
    previous = FileManager.default.currentDirectoryPath
    guard FileManager.default.changeCurrentDirectoryPath(base) else {
      XCTFail("premise: could not enter \(base)", file: file, line: line)
      return nil
    }
    let answer = wolf_ffi_open_listen(Self.stateDir).toString()
    guard answer.isEmpty else {
      XCTFail(
        "premise: wolf_ffi_open_listen(\(Self.stateDir)) in \(base) must bind; it answered"
          + " \(answer)", file: file, line: line)
      FileManager.default.changeCurrentDirectoryPath(previous)
      return nil
    }
  }

  /// Stops the listener and leaves the base.
  func finish() {
    wolf_ffi_open_stop()
    FileManager.default.changeCurrentDirectoryPath(previous)
  }

  /// Runs `wolf args` in the base, with `WOLF_TAB_ID` set to `tabId` only when given, and
  /// returns the requests the listener took; `nil`, with the test failed, when the command
  /// does not exit 0 or nothing drains.
  func send(
    _ args: [String], tabId: String?, file: StaticString = #filePath, line: UInt = #line
  ) -> [FileOpenSeam.Request]? {
    let process = Process()
    process.executableURL = URL(fileURLWithPath: wolf)
    process.arguments = args
    process.currentDirectoryURL = URL(fileURLWithPath: base)
    var env = ProcessInfo.processInfo.environment
    env.removeValue(forKey: "WOLF_TAB_ID")
    env["WOLF_STATE_DIR"] = Self.stateDir
    if let tabId { env["WOLF_TAB_ID"] = tabId }
    process.environment = env
    let stderr = Pipe()
    process.standardError = stderr
    do {
      try process.run()
    } catch {
      XCTFail("premise: could not run \(wolf): \(error)", file: file, line: line)
      return nil
    }
    process.waitUntilExit()
    let message =
      String(data: stderr.fileHandleForReading.readDataToEndOfFile(), encoding: .utf8) ?? ""
    guard process.terminationStatus == 0 else {
      XCTFail(
        "premise: wolf \(args) must exit 0 once Wolf has the request; it exited"
          + " \(process.terminationStatus) saying \(message)", file: file, line: line)
      return nil
    }
    var requests: [FileOpenSeam.Request] = []
    LiveCore.spin(timeout: Self.timeout) {
      for item in wolf_ffi_open_drain() {
        requests.append(FileOpenSeam.Request(item))
      }
      return !requests.isEmpty
    }
    guard !requests.isEmpty else {
      XCTFail(
        "premise: wolf \(args) exited 0, so its request must drain; none did",
        file: file, line: line)
      return nil
    }
    return requests
  }

  /// Starts `wolf args` in the base with `WOLF_TAB_ID` set to `tabId` and leaves it running, as
  /// `wolf --wait` stays until its wait ends; `nil`, with the test failed, when it cannot run.
  func start(
    _ args: [String], tabId: String, file: StaticString = #filePath, line: UInt = #line
  ) -> Process? {
    let process = Process()
    process.executableURL = URL(fileURLWithPath: wolf)
    process.arguments = args
    process.currentDirectoryURL = URL(fileURLWithPath: base)
    var env = ProcessInfo.processInfo.environment
    env["WOLF_STATE_DIR"] = Self.stateDir
    env["WOLF_TAB_ID"] = tabId
    process.environment = env
    process.standardError = Pipe()
    do {
      try process.run()
    } catch {
      XCTFail("premise: could not run \(wolf): \(error)", file: file, line: line)
      return nil
    }
    return process
  }

  /// The one request the listener takes, drained as `AppDelegate` drains: every item taken with
  /// `pop()`, so the request owns it, then put back in arrival order. `nil`, with the test
  /// failed, when not exactly one arrives.
  func drainOwning(file: StaticString = #filePath, line: UInt = #line) -> FileOpenSeam.Request? {
    var requests: [FileOpenSeam.Request] = []
    LiveCore.spin(timeout: Self.timeout) {
      let items = wolf_ffi_open_drain()
      var popped: [FfiOpenItem] = []
      while let item = items.pop() { popped.append(item) }
      requests += popped.reversed().map { FileOpenSeam.Request(owning: $0) }
      return !requests.isEmpty
    }
    guard requests.count == 1 else {
      XCTFail(
        "premise: one running wolf must send one request; \(requests.count) drained",
        file: file, line: line)
      return nil
    }
    return requests[0]
  }
}

/// A live tab to route to: a `TerminalViewFixture` known to the router as tab 7, beside a
/// decoy tab 9 in another window, which is frontmost.
@MainActor
private struct RoutedTab {
  static let id = 7
  static let decoyId = 9

  let fixture: TerminalViewFixture
  /// Tab 7's `WOLF_TAB_ID`, minted by its core.
  let tabId: String
  let directory: String
  let decoyDirectory: String

  var windows: [LiveWindow] {
    [
      LiveWindow(tabs: [LiveTab(id: Self.id, directory: directory)], selected: Self.id),
      LiveWindow(
        tabs: [LiveTab(id: Self.decoyId, directory: decoyDirectory)], selected: Self.decoyId),
    ]
  }

  /// Both tabs' IDs; the decoy's is random, since it has no core.
  let tabIds: [String: Int]

  var stacking: StackingOrder {
    var stacking = StackingOrder()
    stacking.focus(Self.id)
    stacking.focus(Self.decoyId)
    return stacking
  }

  init(fixture: TerminalViewFixture, directory: String, decoyDirectory: String) {
    self.fixture = fixture
    tabId = fixture.handle.assign_tab_id().toString()
    self.directory = directory
    self.decoyDirectory = decoyDirectory
    tabIds = [tabId: Self.id, UUID().uuidString.lowercased(): Self.decoyId]
  }

  func route(_ request: FileOpenSeam.Request) -> FileOpenSeam.Route {
    FileOpenSeam.route(request, windows: windows, tabIds: tabIds, stacking: stacking)
  }
}

/// `specs/cli-and-file-open.md` §4 and §5 end to end on macOS: the real `wolf` binary sends a
/// request over the real socket to the listener `wolf_ffi_open_listen` binds, the test drains
/// it, routes it through `FileOpenSeam.route` with a live tab as tab 7, and for a file sent to
/// that tab calls `PaneContainerView.openFile(_:)` over a real core and Neovim, as
/// `WolfWindowController.openFile` does. AppKit routes no event offscreen and no window is key,
/// so `AppDelegate`'s queue and the controller's raise are a person check.
///
/// Every file lives in a canonical scratch directory, so the path a request drains with is the
/// path the test sent. Panes are told apart by the file their window shows, read with
/// `win_buffer`. The listener queues a request before it replies, so once `wolf` exits 0 its
/// request is there to drain. A failure prefixed `premise:` is a setup step that did not happen.
@MainActor
final class OpenRequestFlowTests: XCTestCase {

  /// How long a settle watches for a late change.
  private let settleSeconds: TimeInterval = 1

  /// Within half a point: the tree's rects round to the backing pixel.
  private let accuracy: CGFloat = 0.5

  // MARK: - E1 to E3: routing what arrives

  /// E1: `wolf DIR` from tab 7's terminal arrives as a directory with tab 7's ID and is Open
  /// Folder with tab 7 current, though the decoy is frontmost.
  func testADirectoryFromATabsTerminalIsOpenFolderFromThatTab() {
    guard let channel = OpenChannel() else { return }
    defer { channel.finish() }
    guard let tab = routedTab() else { return }
    defer { tab.fixture.shutdown() }
    guard let target = LiveCore.scratchDirectory(),
      let requests = channel.send([target], tabId: tab.tabId)
    else { return }

    XCTAssertEqual(requests.count, 1, "one command sends one request: \(requests.count)")
    guard let request = requests.first else { return }
    XCTAssertEqual(
      describe(request.target), "directory(\(target))",
      "an existing directory must arrive as a directory; file(…) means the drained kind decodes"
        + " wrong, so Open Folder never runs")
    XCTAssertEqual(
      request.tabId, tab.tabId, "the command's WOLF_TAB_ID must arrive with the request")
    XCTAssertEqual(
      describe(tab.route(request)), "openFolder(\(target), \(RoutedTab.id))",
      "a directory sent from tab 7's terminal is Open Folder with tab 7 as the current tab;"
        + " current \(RoutedTab.decoyId) means the tab ID was ignored for the frontmost tab")
  }

  /// E2: `wolf F` with no tab ID, `F` in tab 7's directory inside a repository, goes to that
  /// directory, which Open Folder's step then finds open in tab 7.
  func testAFileWithNoTabIdGoesToItsOpenProject() {
    guard let channel = OpenChannel() else { return }
    defer { channel.finish() }
    guard let root = LiveCore.scratchDirectory(), let decoy = LiveCore.scratchDirectory()
    else { return }
    let sub = (root as NSString).appendingPathComponent("app")
    guard makeDirectory((root as NSString).appendingPathComponent(".git")),
      makeDirectory(sub), let file = write("notes.md", in: sub),
      let tab = routedTab(directory: sub, decoy: decoy)
    else { return }
    defer { tab.fixture.shutdown() }
    guard let request = channel.send([file], tabId: nil)?.first else { return }

    let route = tab.route(request)
    XCTAssertEqual(
      describe(route), "openFileInProject(\(file), \(sub), \(RoutedTab.decoyId))",
      "a file with no WOLF_TAB_ID goes to its project, \(sub), the open directory nearest it"
        + " inside the repository \(root), with the frontmost tab current. The root means the"
        + " project lookup was given no open directories")
    guard case .openFileInProject(_, let project, _) = route else { return }
    XCTAssertEqual(
      OpenFolderSeam.step(
        directory: project, current: LiveTab(id: RoutedTab.decoyId, directory: decoy),
        windows: tab.windows, stacking: tab.stacking),
      .focus(tab: RoutedTab.id),
      "Open Folder on the project must focus tab 7, open on it, so the file opens there")
  }

  /// E3: `wolf F` with a well-formed `WOLF_TAB_ID` naming no live tab is resolved as if the
  /// variable were unset.
  func testAFileWithAStaleTabIdFallsBackToResolution() {
    guard let channel = OpenChannel() else { return }
    defer { channel.finish() }
    guard let tab = routedTab(), let file = write("stale.md", in: tab.directory) else { return }
    defer { tab.fixture.shutdown() }
    let stale = UUID().uuidString
    guard let request = channel.send([file], tabId: stale)?.first else { return }

    XCTAssertEqual(
      request.tabId, stale.lowercased(),
      "premise: the stale ID must arrive, lowercase, so the fallback is the router's doing")
    let route = tab.route(request)
    XCTAssertTrue(
      describe(route).hasPrefix("openFileInProject("),
      "a tab ID naming no live tab is handled as unset (§3), so the file goes to its project:"
        + " \(describe(route)). openFile(…) means a stale ID fell back to the current tab")
  }

  // MARK: - E4 to E10: the pane a file lands in

  /// E4: with `vertical[A on X, B on F]` and A active, `F` focuses B, and nothing else moves.
  func testAPaneAlreadyOnTheFileIsFocused() {
    guard let channel = OpenChannel() else { return }
    defer { channel.finish() }
    guard let tab = routedTab(), let x = write("x.md", in: tab.directory),
      let f = write("f.md", in: tab.directory)
    else { return }
    let fixture = tab.fixture
    defer { fixture.shutdown() }
    let handle = fixture.handle
    let container = fixture.container
    let wA = container.activeWin
    guard open(x, in: fixture), let wB = splitVertical(fixture), open(f, in: fixture),
      focus(wA, in: fixture)
    else { return }

    guard deliver(f, to: tab, via: channel) else { return }
    let focused = fixture.wake { handle.active_win() == wB && container.activeWin == wB }
    XCTAssertTrue(
      focused,
      "B (\(wB)) already shows f.md, so it must become Neovim's current window and the view's"
        + " active pane: Neovim \(handle.active_win()), view \(container.activeWin). A (\(wA))"
        + " means .focus sent no focus_win")
    settle(fixture)
    XCTAssertEqual(container.paneCount, 2, "focusing a pane must add none")
    XCTAssertEqual(shows(wA, fixture), x, "A must keep x.md; f.md means F opened in A instead")
    XCTAssertEqual(shows(wB, fixture), f, "B must keep f.md")
  }

  /// E5: rule 1, with `vertical[A, B]` and B active, `F` opens in B and A is unchanged.
  func testTheActiveEditorPaneTakesTheFile() {
    guard let channel = OpenChannel() else { return }
    defer { channel.finish() }
    guard let tab = routedTab(), let x = write("x.md", in: tab.directory),
      let y = write("y.md", in: tab.directory), let f = write("f.md", in: tab.directory)
    else { return }
    let fixture = tab.fixture
    defer { fixture.shutdown() }
    let container = fixture.container
    let wA = container.activeWin
    guard open(x, in: fixture), let wB = splitVertical(fixture), open(y, in: fixture) else {
      return
    }

    guard deliver(f, to: tab, via: channel) else { return }
    let landed = fixture.wake { self.shows(wB, fixture) == f }
    XCTAssertTrue(
      landed,
      "§5.2 rule 1: the active editor pane B (\(wB)) must show f.md; it shows"
        + " \(shows(wB, fixture)), and A (\(wA)) shows \(shows(wA, fixture)). f.md in A means"
        + " .open acted on the first leaf rather than the chosen one")
    settle(fixture)
    XCTAssertEqual(shows(wA, fixture), x, "A must keep x.md")
    XCTAssertEqual(container.paneCount, 2, "opening in a pane must add none")
  }

  /// E6: rule 2, with `vertical[editor E, terminal T]` and T active, `F` opens in E and T keeps
  /// its session.
  func testATerminalPaneIsNeverReplaced() {
    guard let channel = OpenChannel() else { return }
    defer { channel.finish() }
    guard let tab = routedTab(), let x = write("x.md", in: tab.directory),
      let f = write("f.md", in: tab.directory)
    else { return }
    let fixture = tab.fixture
    defer { fixture.shutdown() }
    let handle = fixture.handle
    let container = fixture.container
    let wE = container.activeWin
    guard open(x, in: fixture), let wT = splitVertical(fixture),
      let sT = terminal(in: wT, fixture)
    else { return }

    guard deliver(f, to: tab, via: channel) else { return }
    let landed = fixture.wake { self.shows(wE, fixture) == f }
    XCTAssertTrue(
      landed,
      "§5.2 rule 2: with the terminal pane active, the editor pane E (\(wE)) must show f.md; it"
        + " shows \(shows(wE, fixture)). x.md means .open skipped focus_win and opened in the"
        + " terminal's window")
    settle(fixture)
    XCTAssertEqual(
      fixture.window(showing: sT), wT,
      "the terminal pane T (\(wT)) must keep its session \(sT); the terminal that ran wolf is"
        + " never replaced")
    XCTAssertTrue(
      Array(handle.live_terminals()).contains(sT), "T's session \(sT) must stay live")
    XCTAssertEqual(container.paneCount, 2, "opening in a pane must add none")
  }

  /// E7: rule 3, with `vertical[preview, terminal]` and the terminal active, the leftmost
  /// preview's window shows `F` and is no longer a preview.
  ///
  /// `PreviewViewFixture` previews in the right pane its split adds, so the test turns that pane
  /// into the terminal and previews again in the left, source pane.
  func testAPreviewBesideATerminalBecomesTheEditor() {
    guard let channel = OpenChannel() else { return }
    defer { channel.finish() }
    guard let dir = LiveCore.scratchDirectory(), let decoy = LiveCore.scratchDirectory(),
      let f = write("f.md", in: dir), let preview = PreviewViewFixture()
    else { return }
    let fixture = preview.base
    defer { fixture.shutdown() }
    let tab = RoutedTab(fixture: fixture, directory: dir, decoyDirectory: decoy)
    let handle = fixture.handle
    let container = fixture.container
    let previewWin = preview.previewWin
    let source = handle.windows().map { $0.win() }.first { $0 != previewWin } ?? 0
    guard focus(previewWin, in: fixture), let sT = terminal(in: previewWin, fixture),
      focus(source, in: fixture)
    else { return }

    let scratch = handle.open_preview()
    XCTAssertGreaterThan(
      scratch, 0, "premise: open_preview() must preview \(source), which shows probe.md")
    guard scratch > 0 else { return }
    var previewWins: [Int64] = []
    let previewed = fixture.wake {
      previewWins = handle.previews().map { $0.win() }
      return previewWins == [source] && container.activePaneIsPreview
    }
    XCTAssertTrue(
      previewed,
      "premise: the left pane \(source) must become the only preview and stay active: preview"
        + " windows \(previewWins), preview active \(container.activePaneIsPreview)")
    guard previewed, focus(previewWin, in: fixture) else { return }
    let frames = container.visiblePaneFrames
    guard let previewFrame = frames[source], let terminalFrame = frames[previewWin] else {
      XCTFail("premise: the preview and terminal panes must both show: \(frames)")
      return
    }
    XCTAssertLessThan(
      previewFrame.minX, terminalFrame.minX,
      "premise: the preview \(source) must be left of the terminal \(previewWin); otherwise it"
        + " is not leftmost and rule 3 does not apply")
    guard previewFrame.minX < terminalFrame.minX else { return }

    guard deliver(f, to: tab, via: channel) else { return }
    let converted = fixture.wake {
      previewWins = handle.previews().map { $0.win() }
      return self.shows(source, fixture) == f && !previewWins.contains(source)
    }
    XCTAssertTrue(
      converted,
      "§5.2 rule 3: the preview's window \(source) must become an editor on f.md: it shows"
        + " \(shows(source, fixture)), preview windows \(previewWins). Still a preview means"
        + " .convert did nothing; f.md in \(previewWin) means .convert skipped focus_win")
    settle(fixture)
    XCTAssertEqual(
      fixture.window(showing: sT), previewWin,
      "the terminal pane (\(previewWin)) must keep its session \(sT); another window means :edit"
        + " replaced the terminal that ran wolf")
    XCTAssertEqual(
      container.paneCount, 2,
      "rule 3 converts the preview in place, so no pane is added; 3 means rule 4 split the tab")
  }

  /// E8: rule 4, with only `horizontal[T1, T2]` terminal panes, a new editor pane on `F` is the
  /// root's first child, at the area's left edge and full height, with both terminals right.
  func testOnlyTerminalsSplitTheWholeTabLeft() {
    guard let channel = OpenChannel() else { return }
    defer { channel.finish() }
    guard let tab = routedTab(), let f = write("f.md", in: tab.directory) else { return }
    let fixture = tab.fixture
    defer { fixture.shutdown() }
    let handle = fixture.handle
    let container = fixture.container
    let w1 = container.activeWin
    guard let s1 = terminal(in: w1, fixture) else { return }
    container.splitActivePane(.horizontal)
    var s2: UInt64 = 0
    let split = fixture.wake {
      let active = container.activeWin
      s2 = handle.win_buffer(active).terminal()
      return container.paneCount == 2 && ![0, w1].contains(active) && s2 != 0 && s2 != s1
        && fixture.view(showing: s2) != nil
    }
    XCTAssertTrue(
      split,
      "premise: a horizontal split of the terminal pane must add a second terminal pane:"
        + " \(container.paneCount) pane(s), session \(s2)")
    guard split else { return }
    let w2 = container.activeWin
    let before = container.visiblePaneFrames
    guard let t1 = before[w1], let t2 = before[w2] else {
      XCTFail("premise: both terminal panes must show: \(before)")
      return
    }
    let left = min(t1.minX, t2.minX)
    let top = min(t1.minY, t2.minY)
    let bottom = max(t1.maxY, t2.maxY)

    guard deliver(f, to: tab, via: channel) else { return }
    var newWin: Int64 = 0
    let opened = fixture.wake {
      newWin = handle.windows().map { $0.win() }.first { self.shows($0, fixture) == f } ?? 0
      return container.paneCount == 3 && newWin != 0
        && container.visiblePaneFrames[newWin] != nil
    }
    XCTAssertTrue(
      opened,
      "§5.2 rule 4: with only terminal panes, f.md must open in a new third pane:"
        + " \(container.paneCount) pane(s), window on f.md \(newWin)")
    guard opened else { return }
    settle(fixture)

    let frames = container.visiblePaneFrames
    guard let fNew = frames[newWin], let t1After = frames[w1], let t2After = frames[w2] else {
      XCTFail("every pane must show after the split: \(frames)")
      return
    }
    XCTAssertTrue(
      abs(fNew.minX - left) <= accuracy && abs(fNew.minY - top) <= accuracy
        && abs(fNew.maxY - bottom) <= accuracy,
      "the new pane must sit at the area's left edge \(left), spanning its full height \(top)"
        + " to \(bottom): it is \(fNew). A pane beside one terminal only means the split was"
        + " of the active pane, not of the whole tab")
    XCTAssertTrue(
      t1After.minX >= fNew.maxX - accuracy && t2After.minX >= fNew.maxX - accuracy,
      "both terminals must lie right of the new pane \(fNew): \(t1After), \(t2After)")
    XCTAssertEqual(
      shape(container.paneLayout, container.leafContents()), "v[buffer, h[terminal, terminal]]",
      "the tree must be vertical[new, horizontal[T1, T2]], the new leaf the root's first child")
    let live = Array(handle.live_terminals())
    XCTAssertTrue(
      live.contains(s1) && live.contains(s2), "both sessions must stay live: \(live)")
  }

  /// E9: with A zoomed in `vertical[A, B]`, `F` opens in A, the view unzooms, and both panes
  /// show.
  func testAZoomedTabIsUnzoomed() {
    guard let channel = OpenChannel() else { return }
    defer { channel.finish() }
    guard let tab = routedTab(), let x = write("x.md", in: tab.directory),
      let y = write("y.md", in: tab.directory), let f = write("f.md", in: tab.directory)
    else { return }
    let fixture = tab.fixture
    defer { fixture.shutdown() }
    let container = fixture.container
    let wA = container.activeWin
    guard open(x, in: fixture), let wB = splitVertical(fixture), open(y, in: fixture),
      focus(wA, in: fixture)
    else { return }
    container.toggleZoom()
    guard container.visiblePaneFrames.keys.sorted() == [wA] else {
      XCTFail("premise: toggleZoom() must show A alone: \(container.visiblePaneFrames)")
      return
    }

    guard deliver(f, to: tab, via: channel) else { return }
    let landed = fixture.wake { self.shows(wA, fixture) == f }
    XCTAssertTrue(
      landed, "the zoomed, active editor pane A must take f.md: \(shows(wA, fixture))")
    settle(fixture)
    XCTAssertFalse(
      container.isZoomed,
      "§5.2: a zoomed tab is unzoomed before the file is placed; still zoomed means openFile"
        + " ignored the choice's unzoom")
    XCTAssertEqual(
      Set(container.visiblePaneFrames.keys), [wA, wB],
      "both panes must show once unzoomed: \(container.visiblePaneFrames)")
  }

  /// E10: a file that does not exist opens in the active pane as a buffer for that path, and
  /// nothing is written.
  func testAMissingFileOpensAsANewBuffer() {
    guard let channel = OpenChannel() else { return }
    defer { channel.finish() }
    guard let tab = routedTab() else { return }
    let fixture = tab.fixture
    defer { fixture.shutdown() }
    let container = fixture.container
    let missing = (tab.directory as NSString).appendingPathComponent("not-yet-\(UUID()).md")

    guard deliver(missing, to: tab, via: channel) else { return }
    let landed = fixture.wake { self.shows(container.activeWin, fixture) == missing }
    XCTAssertTrue(
      landed,
      "§5: a missing file must open in the active pane as a buffer named for it; the pane"
        + " shows \(shows(container.activeWin, fixture)), so the path was dropped")
    settle(fixture)
    XCTAssertFalse(
      FileManager.default.fileExists(atPath: missing),
      "opening a missing file must not create it; it is written only when the user saves")
  }

  // MARK: - E11, E12: opens that arrive before the view is ready

  /// E11: a file opened before the first bind, while a rebuild of `vertical[F, B]` with B focused
  /// waits, focuses rebuilt pane F, the editor pane already on it, and B keeps its file.
  func testAnOpenWaitsForThePendingRebuild() {
    guard let dir = LiveCore.scratchDirectory(), let f = write("f.md", in: dir),
      let b = write("b.md", in: dir)
    else { return }
    let saved = RestoreSeam.SavedTab(
      cwd: dir, sidebarVisible: true,
      layout: .split(
        orientation: .vertical, weights: [0.5, 0.5],
        children: [.leaf(path: f, preview: false), .leaf(path: b, preview: false)]),
      focused: .pane(1))
    WindowStateCodec.submit([
      WindowStateCodec.Window(
        frame: CGRect(origin: .zero, size: TerminalViewFixture.windowSize), activeTab: 0,
        minimized: false, tabs: [saved], recency: [0])
    ])
    XCTAssertTrue(wolf_ffi_app_state_flush(), "premise: the window state must flush")
    guard let live = LiveCore.start() else { return }
    defer { live.shutdown() }
    let container = PaneContainerView(
      handle: live.handle, metrics: live.metrics, optionAsAlt: false)
    let renderer = OffscreenRenderer(hosting: container, size: TerminalViewFixture.windowSize)
    defer { withExtendedLifetime(renderer) {} }
    guard
      let rebuild = WindowStateCodec.load(wolf_ffi_app_state_window_state()).last?.tabs.last?
        .rebuild
    else {
      XCTFail("premise: the saved tab must load back with a rebuild")
      return
    }
    container.requestLayoutRebuild(
      rebuild.layout, tab: rebuild.tab, focused: rebuild.focused, drawer: rebuild.drawer)

    container.openFile(f)
    var shown: [String] = []
    var activeIndex: Int?
    var nvimWin: Int64 = 0
    let landed = live.waitFor(timeout: TerminalViewFixture.timeout) {
      container.onRenderWake()
      shown = leafPaths(container)
      let ids = leafIds(container.paneLayout)
      activeIndex = container.activePaneId.flatMap { ids.firstIndex(of: $0) }
      nvimWin = live.handle.active_win()
      return shown == [f, b] && container.activePaneId == ids.first
        && nvimWin == container.activeWin
    }
    XCTAssertTrue(
      landed,
      "§5.2: the open must wait for the rebuild, then focus pane 0, the editor pane already on"
        + " f.md: the panes show \(shown), active leaf \(String(describing: activeIndex)),"
        + " Neovim's window \(nvimWin), the view's \(container.activeWin); want [f.md, b.md]"
        + " with leaf 0 active in both. [f.md, f.md] means the open ran before the rebuild and"
        + " its queued :edit landed in the restored focused pane. [f.md, b.md] with leaf 1"
        + " active means the open ran before the rebuild and the rebuild replaced it")
    XCTAssertEqual(
      container.paneCount, 2,
      "opening a file already shown must not split or close a pane: \(container.paneLayout)")
  }

  /// E12: two files opened before the first bind both open: the first is loaded in Neovim, the
  /// second shows in the active pane.
  func testTwoOpensBeforeTheFirstBindBothOpen() {
    guard let dir = LiveCore.scratchDirectory(), let first = write("first.md", in: dir),
      let second = write("second.md", in: dir)
    else { return }
    guard let live = LiveCore.start() else { return }
    defer { live.shutdown() }
    let handle = live.handle
    let container = PaneContainerView(handle: handle, metrics: live.metrics, optionAsAlt: false)
    let renderer = OffscreenRenderer(hosting: container, size: TerminalViewFixture.windowSize)
    defer { withExtendedLifetime(renderer) {} }

    container.openFile(first)
    container.openFile(second)
    let landed = live.waitFor(timeout: TerminalViewFixture.timeout) {
      container.onRenderWake()
      let win = container.activeWin
      return win != 0 && handle.win_buffer(win).abs_path().toString() == second
    }
    XCTAssertTrue(
      landed,
      "the second file must show in the active pane once the view binds: it shows"
        + " \(handle.win_buffer(container.activeWin).abs_path().toString())")
    guard landed else { return }
    let probe = (dir as NSString).appendingPathComponent("listed-\(UUID().uuidString).txt")
    typeEx(
      "lua local n=0 for _,b in ipairs(vim.api.nvim_list_bufs()) do"
        + " if vim.api.nvim_buf_get_name(b)=='\(first)' and vim.bo[b].buflisted then n=1 end"
        + " end vim.fn.writefile({tostring(n)}, '\(probe)')", into: handle)
    var listed = ""
    live.waitFor(timeout: TerminalViewFixture.timeout) {
      listed = ((try? String(contentsOfFile: probe, encoding: .utf8)) ?? "")
        .trimmingCharacters(in: .whitespacesAndNewlines)
      return !listed.isEmpty
    }
    XCTAssertEqual(
      listed, "1",
      "the first file must be loaded and listed in Neovim too; 0 means the queue kept only the"
        + " last open, and an empty probe that Neovim never ran it")
  }

  // MARK: - M1: a waited file

  /// M1: a real `wolf --wait F` from tab 7's terminal, drained with `pop()` into
  /// `Request(owning:)`, is opened with its wait by `openFile(_:wait:)` in the active pane of a
  /// vertical split, and `:q` there, closing the buffer's last window, ends the command with `0`.
  func testAWaitedFileEndsItsCommandWhenItsPaneCloses() {
    guard let channel = OpenChannel() else { return }
    defer { channel.finish() }
    guard let tab = routedTab(), let x = write("x.md", in: tab.directory),
      let f = write("waited.md", in: tab.directory)
    else { return }
    let fixture = tab.fixture
    defer { fixture.shutdown() }
    let handle = fixture.handle
    let container = fixture.container
    guard open(x, in: fixture), let wB = splitVertical(fixture) else { return }

    guard let wolf = channel.start(["--wait", f], tabId: tab.tabId) else { return }
    defer { if wolf.isRunning { wolf.terminate() } }
    guard let request = channel.drainOwning() else { return }
    guard case .openFile(let opened, let target) = tab.route(request), target == RoutedTab.id,
      opened == f
    else {
      XCTFail(
        "premise: \(f) sent with tab 7's ID must route to openFile(\(f), 7): it routes"
          + " \(describe(tab.route(request)))")
      return
    }
    container.openFile(opened, wait: request.wait)
    let landed = fixture.wake { self.shows(wB, fixture) == f && handle.active_win() == wB }
    XCTAssertTrue(
      landed,
      "premise: the waited file must open in the active pane B (\(wB)), which Neovim must have"
        + " current: B shows \(shows(wB, fixture)), Neovim's window is \(handle.active_win())")
    guard landed else { return }

    typeEx("q", into: handle)
    let exited = fixture.wake { !wolf.isRunning }
    XCTAssertTrue(
      exited,
      "wolf --wait must exit once :q closes the last window on \(f); it is still running, so"
        + " nothing ended its wait")
    guard exited else { return }
    let stderr = (wolf.standardError as? Pipe).flatMap {
      String(data: $0.fileHandleForReading.readDataToEndOfFile(), encoding: .utf8)
    }
    XCTAssertEqual(
      wolf.terminationStatus, 0,
      "wolf --wait must exit 0 when its buffer closes; 1 means the wait was dropped on its way"
        + " to the core: Request(owning:) kept no item, openFile never called wait_on, or"
        + " WolfCoreHandle.wait_on dropped it. stderr: \(stderr ?? "")")
  }

  // MARK: - Helpers

  /// A `TerminalViewFixture` routed as tab 7 on `directory`, a fresh scratch directory when not
  /// given, beside the decoy; `nil`, with the test failed, when a step does not happen.
  private func routedTab(
    directory: String? = nil, decoy: String? = nil, file: StaticString = #filePath,
    line: UInt = #line
  ) -> RoutedTab? {
    guard let dir = directory ?? LiveCore.scratchDirectory(file: file, line: line),
      let decoyDir = decoy ?? LiveCore.scratchDirectory(file: file, line: line),
      let fixture = TerminalViewFixture(file: file, line: line)
    else { return nil }
    return RoutedTab(fixture: fixture, directory: dir, decoyDirectory: decoyDir)
  }

  /// Sends `path` from tab 7's terminal, checks it routes to tab 7, and opens it there as
  /// `WolfWindowController.openFile` does; `false`, with the test failed, when it does not.
  private func deliver(
    _ path: String, to tab: RoutedTab, via channel: OpenChannel, file: StaticString = #filePath,
    line: UInt = #line
  ) -> Bool {
    guard let requests = channel.send([path], tabId: tab.tabId, file: file, line: line) else {
      return false
    }
    let route: FileOpenSeam.Route? = requests.count == 1 ? tab.route(requests[0]) : nil
    guard case .openFile(let opened, let target)? = route, target == RoutedTab.id,
      opened == path
    else {
      XCTFail(
        "premise: \(path) sent with tab 7's ID must route to openFile(\(path), 7): routes"
          + " \(requests.map { describe(tab.route($0)) })", file: file, line: line)
      return false
    }
    tab.fixture.container.openFile(opened)
    return true
  }

  /// What window `win` shows, by absolute path.
  private func shows(_ win: Int64, _ fixture: TerminalViewFixture) -> String {
    fixture.handle.win_buffer(win).abs_path().toString()
  }

  /// Opens `path` in the active pane and waits until it shows there.
  private func open(
    _ path: String, in fixture: TerminalViewFixture, file: StaticString = #filePath,
    line: UInt = #line
  ) -> Bool {
    fixture.handle.open(path)
    let opened = fixture.wake { self.shows(fixture.container.activeWin, fixture) == path }
    XCTAssertTrue(opened, "premise: \(path) must open in the active pane", file: file, line: line)
    return opened
  }

  /// Splits the active pane vertically and returns the new, active pane's window.
  private func splitVertical(
    _ fixture: TerminalViewFixture, file: StaticString = #filePath, line: UInt = #line
  ) -> Int64? {
    let container = fixture.container
    let before = container.paneCount
    let origin = container.activeWin
    container.splitActivePane(.vertical)
    let split = fixture.wake {
      container.paneCount == before + 1 && ![0, origin].contains(container.activeWin)
    }
    XCTAssertTrue(
      split, "premise: a vertical split must add an active pane: \(container.paneCount)",
      file: file, line: line)
    return split ? container.activeWin : nil
  }

  /// Focuses `win` in Neovim and waits for the view to follow.
  private func focus(
    _ win: Int64, in fixture: TerminalViewFixture, file: StaticString = #filePath,
    line: UInt = #line
  ) -> Bool {
    fixture.handle.focus_win(win)
    let focused = fixture.wake {
      fixture.container.activeWin == win && fixture.handle.active_win() == win
    }
    XCTAssertTrue(focused, "premise: focus_win(\(win)) must take effect", file: file, line: line)
    return focused
  }

  /// Starts `/bin/sh` in Neovim's current window, which must be `win`, and returns its session.
  private func terminal(
    in win: Int64, _ fixture: TerminalViewFixture, file: StaticString = #filePath,
    line: UInt = #line
  ) -> UInt64? {
    let session = fixture.live.openTerminal("exec /bin/sh")
    let shown =
      session != 0
      && fixture.wake {
        fixture.window(showing: session) == win && fixture.view(showing: session) != nil
      }
    XCTAssertTrue(
      shown, "premise: \(win) must show a /bin/sh session: session \(session)", file: file,
      line: line)
    return shown ? session : nil
  }

  /// Runs render wakes for `settleSeconds`.
  private func settle(_ fixture: TerminalViewFixture) {
    fixture.live.waitFor(timeout: settleSeconds) {
      fixture.container.onRenderWake()
      return false
    }
  }

  /// Writes a short file `name` in `dir` and returns its path; `nil`, with the test failed,
  /// when it cannot.
  private func write(
    _ name: String, in dir: String, file: StaticString = #filePath, line: UInt = #line
  ) -> String? {
    let path = (dir as NSString).appendingPathComponent(name)
    do {
      try "# \(name)\n".write(toFile: path, atomically: true, encoding: .utf8)
      return path
    } catch {
      XCTFail("premise: could not write \(path): \(error)", file: file, line: line)
      return nil
    }
  }

  private func makeDirectory(
    _ path: String, file: StaticString = #filePath, line: UInt = #line
  ) -> Bool {
    do {
      try FileManager.default.createDirectory(atPath: path, withIntermediateDirectories: true)
      return true
    } catch {
      XCTFail("premise: could not create \(path): \(error)", file: file, line: line)
      return false
    }
  }

  /// The file each tree leaf shows, in depth-first order; `""` for a leaf showing no file.
  private func leafPaths(_ container: PaneContainerView) -> [String] {
    let contents = container.leafContents()
    return leafIds(container.paneLayout).map { id in
      if case .buffer(let path) = contents[id] { return path }
      return ""
    }
  }

  private func leafIds(_ layout: PaneLayout) -> [PaneId] {
    switch layout {
    case .leaf(let id): return [id]
    case .split(_, let children, _): return children.flatMap(leafIds)
    }
  }

  /// The layout written as `v[…]` and `h[…]` of each leaf's kind.
  private func shape(_ layout: PaneLayout, _ contents: [PaneId: SessionSeam.LeafContent]) -> String
  {
    switch layout {
    case .leaf(let id):
      switch contents[id] {
      case .buffer?: return "buffer"
      case .terminal?: return "terminal"
      case .preview?: return "preview"
      default: return "unreported"
      }
    case .split(let orientation, let children, _):
      let kind = orientation == .vertical ? "v" : "h"
      return kind + "[" + children.map { shape($0, contents) }.joined(separator: ", ") + "]"
    }
  }

  /// Types `:command` and Return into Neovim.
  private func typeEx(_ command: String, into handle: WolfCoreHandle) {
    for scalar in (":" + command).unicodeScalars {
      handle.send_key(
        FfiKeyEvent(
          kind: 0, ch: scalar.value, fnum: 0, ctrl: false, shift: false, alt: false, meta: false))
    }
    handle.send_key(
      FfiKeyEvent(kind: 1, ch: 0, fnum: 0, ctrl: false, shift: false, alt: false, meta: false))
  }

  private func describe(_ target: FileOpenSeam.Target) -> String {
    switch target {
    case .activate: return "activate"
    case .directory(let path): return "directory(\(path))"
    case .file(let path): return "file(\(path))"
    }
  }

  private func describe(_ route: FileOpenSeam.Route) -> String {
    let label = { (tab: Int?) in tab.map { String($0) } ?? "nil" }
    switch route {
    case .activate(let current):
      return "activate(\(label(current)))"
    case .openFolder(let directory, let current):
      return "openFolder(\(directory), \(label(current)))"
    case .openFile(let file, let tab):
      return "openFile(\(file), \(tab))"
    case .openFileInProject(let file, let project, let current):
      return "openFileInProject(\(file), \(project), \(label(current)))"
    }
  }
}
