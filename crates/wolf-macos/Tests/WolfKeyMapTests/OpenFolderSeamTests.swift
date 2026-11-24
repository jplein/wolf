// See crates/wolf-macos/Tests/WolfKeyMapTests/README.md

import CoreGraphics
import Foundation
import XCTest

@testable import WolfKeyMap

private typealias LiveTab = OpenFolderSeam.LiveTab
private typealias LiveWindow = OpenFolderSeam.LiveWindow

/// `OpenFolderSeam` decides `specs/application-state.md` §6's step for a directory `D`: focus the
/// tab §4's rule picks when one is open on `D`, else fill an empty current tab, else open a new
/// window. It also canonicalizes `D`, picks the frame a fill applies, and picks whether the folder
/// tab shows its sidebar.
///
/// A `LiveWindow`'s `selected` is the selected tab's id, and `.focus(tab:)` answers a tab id.
/// Tab ids here are never equal to a tab index, so an answer that confuses the two names no tab.
/// Every live window list is passed out of stacking order, so only the stacking can put it right.
///
/// The lookup runs through wolf-ffi's builder scratch, never the store, so this file needs no
/// `StateDirFence`.
final class OpenFolderSeamTests: XCTestCase {

  private let dirD = "/wolf-open-folder-test/project"
  private let dirE = "/wolf-open-folder-test/elsewhere"
  private let dirC = "/wolf-open-folder-test/other"

  /// The stacking that focuses `tabs` in order, so the last is frontmost.
  private func stacking(focusing tabs: [Int]) -> StackingOrder {
    var stacking = StackingOrder()
    for tab in tabs { stacking.focus(tab) }
    return stacking
  }

  // MARK: - M1: a background tab of a back window is focused

  func testATabOnTheDirectoryInABackWindowIsFocused() {
    let current = LiveTab(id: 11, directory: dirE)
    let front = LiveWindow(tabs: [current], selected: 11)
    let back = LiveWindow(
      tabs: [LiveTab(id: 21, directory: dirC), LiveTab(id: 22, directory: dirD)], selected: 21)
    let step = OpenFolderSeam.step(
      directory: dirD, current: current, windows: [back, front],
      stacking: stacking(focusing: [22, 21, 11]))
    XCTAssertEqual(
      step, .focus(tab: 22),
      "D is open only in tab 22, a background tab of the back window, so §6 step 1 must focus "
        + "it; .newWindow means the lookup found no tab on D, .focus(tab: 1) means it answered "
        + "the tab index rather than its id")
  }

  func testWithNoCurrentTabAnOpenTabIsStillFocused() {
    let window = LiveWindow(
      tabs: [LiveTab(id: 21, directory: dirC), LiveTab(id: 22, directory: dirD)], selected: 21)
    XCTAssertEqual(
      OpenFolderSeam.step(
        directory: dirD, current: nil, windows: [window], stacking: stacking(focusing: [22, 21])),
      .focus(tab: 22),
      "with no key Wolf window, a tab open on D must still be focused; .newWindow means the "
        + "missing current tab short-circuits the lookup")
  }

  // MARK: - M2: the frontmost window whose selected tab is on D wins

  func testTheFrontmostWindowWhoseSelectedTabIsOnTheDirectoryWins() {
    let current = LiveTab(id: 51, directory: dirE)
    let behind = LiveWindow(tabs: [LiveTab(id: 31, directory: dirD)], selected: 31)
    let ahead = LiveWindow(tabs: [LiveTab(id: 41, directory: dirD)], selected: 41)
    let invoking = LiveWindow(tabs: [current], selected: 51)
    let step = OpenFolderSeam.step(
      directory: dirD, current: current, windows: [behind, ahead, invoking],
      stacking: stacking(focusing: [31, 41, 51]))
    XCTAssertEqual(
      step, .focus(tab: 41),
      "both D windows have their selected tab on D, and tab 41's window is ahead of tab 31's in "
        + "the stacking, so §4 rule 1 picks 41; .focus(tab: 31) means the lookup's windows were "
        + "built in input order rather than stacking order")
  }

  // MARK: - M3: of two background tabs, the more recently selected wins

  func testTheMoreRecentOfTwoBackgroundTabsOnTheDirectoryWins() {
    let current = LiveTab(id: 61, directory: dirE)
    let window = LiveWindow(
      tabs: [current, LiveTab(id: 62, directory: dirD), LiveTab(id: 63, directory: dirD)],
      selected: 61)
    let step = OpenFolderSeam.step(
      directory: dirD, current: current, windows: [window],
      stacking: stacking(focusing: [62, 63, 61]))
    XCTAssertEqual(
      step, .focus(tab: 63),
      "no selected tab is on D, so §4 rule 2 picks the D tab selected most recently, 63, the "
        + "later in tab order; .focus(tab: 62) means no recency reached the lookup and tab "
        + "order decided")
  }

  // MARK: - M4, M5: no tab on D

  func testAnEmptyCurrentTabIsFilledWhenNoTabIsOnTheDirectory() {
    let current = LiveTab(id: 71, directory: "")
    let other = LiveWindow(tabs: [LiveTab(id: 81, directory: dirE)], selected: 81)
    let step = OpenFolderSeam.step(
      directory: dirD, current: current,
      windows: [other, LiveWindow(tabs: [current], selected: 71)],
      stacking: stacking(focusing: [81, 71]))
    XCTAssertEqual(
      step, .fill,
      "no tab is on D and the current tab is empty, so §6 step 2 must fill it; .newWindow means "
        + "the empty current tab was not considered, .focus means the empty tab matched D")
  }

  func testACurrentTabWithADirectoryOpensANewWindow() {
    let current = LiveTab(id: 91, directory: dirE)
    let other = LiveWindow(tabs: [LiveTab(id: 101, directory: "")], selected: 101)
    let step = OpenFolderSeam.step(
      directory: dirD, current: current,
      windows: [other, LiveWindow(tabs: [current], selected: 91)],
      stacking: stacking(focusing: [101, 91]))
    XCTAssertEqual(
      step, .newWindow,
      "no tab is on D and the current tab has a directory, so §6 step 3 must open a new window "
        + "even though another window holds an empty tab; .fill means a tab with a directory "
        + "would be replaced, which §6 forbids")
  }

  func testWithNoCurrentTabAndNoTabOnTheDirectoryANewWindowOpens() {
    let other = LiveWindow(tabs: [LiveTab(id: 111, directory: "")], selected: 111)
    XCTAssertEqual(
      OpenFolderSeam.step(
        directory: dirD, current: nil, windows: [other], stacking: stacking(focusing: [111])),
      .newWindow,
      "with no key Wolf window there is no current tab to fill, so a new window must open; "
        + ".fill means a non-key window's empty tab was taken for the current one")
  }

  // MARK: - M6: canonical directory

  func testCanonicalDirectoryResolvesASymlink() throws {
    let env = ProcessInfo.processInfo.environment
    let base = env["TEST_TMPDIR"].flatMap { $0.isEmpty ? nil : $0 } ?? NSTemporaryDirectory()
    let scratch = (base as NSString).appendingPathComponent("wolf-open-folder-\(UUID().uuidString)")
    let target = (scratch as NSString).appendingPathComponent("target")
    let link = (scratch as NSString).appendingPathComponent("link")
    try FileManager.default.createDirectory(atPath: target, withIntermediateDirectories: true)
    try FileManager.default.createSymbolicLink(atPath: link, withDestinationPath: target)
    defer { try? FileManager.default.removeItem(atPath: scratch) }

    let viaLink = OpenFolderSeam.canonicalDirectory(link)
    XCTAssertEqual(
      viaLink, OpenFolderSeam.canonicalDirectory(target),
      "a symlink and its target name one directory, so both must canonicalize alike; a "
        + "difference means the symlink was not resolved")
    XCTAssertNotEqual(
      viaLink, link,
      "the link's canonical form must not be the link itself; equal means the path came back "
        + "as given")
  }

  func testCanonicalDirectoryKeepsThePrivatePrefixCoreUses() {
    XCTAssertEqual(
      OpenFolderSeam.canonicalDirectory("/tmp"), "/private/tmp",
      "macOS links /tmp to /private/tmp, and core's apply_cwd canonicalizes to /private/tmp, "
        + "so tab directories and project-state keys use that form; \"/tmp\" means the path "
        + "came back as given or Foundation's resolvingSymlinksInPath stripped /private")
  }

  func testCanonicalDirectoryFallsBackToAMissingPathAsGiven() {
    let missing = "/wolf-open-folder-test/\(UUID().uuidString)/missing"
    XCTAssertEqual(
      OpenFolderSeam.canonicalDirectory(missing), missing,
      "a path that does not resolve must come back as given")
  }

  // MARK: - M7: the frame a fill applies

  func testAFillAppliesTheStoredFrameOnlyToALoneTabOnScreen() {
    let screen = CGRect(x: 0, y: 0, width: 1920, height: 1080)
    let stored = CGRect(x: 120, y: 80, width: 1400, height: 900)
    XCTAssertEqual(
      OpenFolderSeam.fillFrame(stored: stored, windowTabs: 1, screenVisibleFrames: [screen]),
      stored,
      "a fill of its window's only tab applies the stored frame when it is on a screen; nil "
        + "means the frame was dropped")
    XCTAssertNil(
      OpenFolderSeam.fillFrame(stored: stored, windowTabs: 2, screenVisibleFrames: [screen]),
      "a fill of one of two tabs keeps the window's frame; a frame means windowTabs is ignored")
    XCTAssertNil(
      OpenFolderSeam.fillFrame(
        stored: CGRect(x: 5000, y: 5000, width: 800, height: 600), windowTabs: 1,
        screenVisibleFrames: [screen]),
      "a stored frame on no screen is discarded, as at launch; a frame means it skipped "
        + "StateSeam.restoredWindowFrame")
    XCTAssertNil(
      OpenFolderSeam.fillFrame(stored: nil, windowTabs: 1, screenVisibleFrames: [screen]),
      "with no stored frame there is nothing to apply")
  }

  // MARK: - The live directory a folder tab counts as being on

  func testAFolderTabWithNoReportedCwdIsOnItsLaunchDirectory() {
    XCTAssertEqual(
      OpenFolderSeam.liveDirectory(hasFolder: true, reportedCwd: "", launchCwd: dirD), dirD,
      "a folder tab whose Neovim has not reported its cwd yet is on its launch directory; \"\" "
        + "means two files of one new project opened together open the project twice")
    XCTAssertEqual(
      OpenFolderSeam.liveDirectory(hasFolder: true, reportedCwd: dirE, launchCwd: dirD), dirE,
      "a reported cwd wins over the launch directory; the launch directory means a `:cd` is "
        + "ignored")
    XCTAssertEqual(
      OpenFolderSeam.liveDirectory(hasFolder: false, reportedCwd: dirE, launchCwd: dirD), "",
      "an empty tab has no directory, whatever its Neovim reports; a path means an empty tab "
        + "counts as open on it")
  }

  // MARK: - OS1-OS3: the sidebar of a folder opened into New Tab's empty tab

  func testWithNoProjectStateAFillTakesTheCapturedSidebar() {
    XCTAssertFalse(
      OpenFolderSeam.sidebarVisible(stored: nil, newTab: false),
      "New Tab captured a hidden sidebar and the folder has no project state, so §6 hides it; "
        + "true means the captured value was ignored for the default")
  }

  func testAStoredSidebarWinsOverTheCapturedOne() {
    XCTAssertFalse(
      OpenFolderSeam.sidebarVisible(stored: false, newTab: true),
      "the folder's project state hides the sidebar and New Tab captured it shown, so the "
        + "stored setting wins; true means the captured value was applied over it")
  }

  func testWithNothingCapturedAFolderTabShowsTheSidebar() {
    XCTAssertTrue(
      OpenFolderSeam.sidebarVisible(stored: nil, newTab: nil),
      "a folder opened into an empty tab New Tab did not make, with no project state, shows "
        + "the sidebar by default; false means a missing capture was read as hidden")
  }
}
