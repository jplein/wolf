// The zoom rules of PaneZoom: which pane a toggle zooms, what unzooms, and what a zoom freezes.

import XCTest

@testable import WolfKeyMap

final class PaneZoomTests: XCTestCase {

  // MARK: - Fixtures: two tree panes side by side above a full-width drawer

  private let p1: PaneId = 1
  private let p2: PaneId = 2
  /// The drawer's pane id, kept apart from the tree's ids as the container's are.
  private let drawer: PaneId = 9
  /// A pane id absent from every fixture: a zoomed pane that is gone.
  private let absent: PaneId = 5

  private let r1 = Rect(x: 0, y: 0, w: 480, h: 427)
  private let r2 = Rect(x: 480, y: 0, w: 480, h: 427)
  private let r9 = Rect(x: 0, y: 427, w: 960, h: 213)
  /// The pane area at the live fixture's window size.
  private let bounds = Rect(x: 0, y: 0, w: 960, h: 640)
  /// A larger pane area, as after a window resize while zoomed.
  private let bounds2 = Rect(x: 0, y: 0, w: 1200, h: 760)

  private var rects: [(PaneId, Rect)] { [(p1, r1), (p2, r2), (drawer, r9)] }

  /// Asserts `got` holds exactly `want`, in order, comparing ids and rects apart, since a
  /// tuple is not `Equatable`.
  private func assertRects(
    _ got: [(PaneId, Rect)], _ want: [(PaneId, Rect)], _ message: String,
    file: StaticString = #filePath, line: UInt = #line
  ) {
    XCTAssertEqual(
      got.map { $0.0 }, want.map { $0.0 }, "\(message): ids of \(got)", file: file, line: line)
    XCTAssertEqual(
      got.map { $0.1 }, want.map { $0.1 }, "\(message): rects of \(got)", file: file, line: line)
  }

  // MARK: - K1 to K3: the toggle

  /// K1: an unzoomed toggle zooms the active pane, a tree leaf or the drawer.
  func testToggleZoomsTheActivePaneTheDrawerIncluded() {
    XCTAssertEqual(
      PaneZoom.zoomed(after: .toggle, zoomed: nil, active: p2, panes: [p1, p2]), p2,
      "an unzoomed toggle must zoom the active pane 2. nil means the toggle never zooms; 1 means"
        + " it zooms a pane other than the active one")
    XCTAssertEqual(
      PaneZoom.zoomed(after: .toggle, zoomed: nil, active: drawer, panes: [p1, drawer]), drawer,
      "an unzoomed toggle with the drawer active must zoom the drawer, which counts among the"
        + " view's panes. nil means the drawer cannot be zoomed")
  }

  /// K2: a toggle while zoomed unzooms, whichever pane is active.
  func testToggleAgainUnzooms() {
    XCTAssertNil(
      PaneZoom.zoomed(after: .toggle, zoomed: p2, active: p2, panes: [p1, p2]),
      "a toggle while pane 2 is zoomed and active must unzoom. 2 means Zoom Pane cannot be"
        + " undone by Zoom Pane")
    XCTAssertNil(
      PaneZoom.zoomed(after: .toggle, zoomed: p2, active: p1, panes: [p1, p2]),
      "a toggle while pane 2 is zoomed must unzoom even when the active pane reads 1. 1 means"
        + " the toggle rezooms the active pane instead of unzooming; 2 that it keeps the zoom")
  }

  /// K3: with one pane and no drawer there is nothing to hide, so the toggle does nothing.
  func testToggleWithNothingToHideIsANoOp() {
    XCTAssertNil(
      PaneZoom.zoomed(after: .toggle, zoomed: nil, active: p1, panes: [p1]),
      "a toggle over a lone pane must leave the view unzoomed: 1 means a pane with nothing to"
        + " hide is reported zoomed")
  }

  // MARK: - K4, K5: the events that unzoom

  /// K4: a split and a drawer opening unzoom, since a pane joins the layout.
  func testASplitAndADrawerOpenUnzoom() {
    XCTAssertNil(
      PaneZoom.zoomed(after: .split, zoomed: p2, active: p2, panes: [p1, p2]),
      "a split while pane 2 is zoomed must unzoom before the split, so the new pane shows; 2"
        + " means the split lands hidden behind the zoomed pane")
    XCTAssertNil(
      PaneZoom.zoomed(after: .drawerOpen, zoomed: p2, active: p2, panes: [p1, p2]),
      "a drawer opening while pane 2 is zoomed must unzoom, as a split does; 2 means the drawer"
        + " opens hidden")
  }

  /// K5: a change of panes unzooms only when the zoomed pane is gone.
  func testOnlyClosingTheZoomedPaneUnzooms() {
    XCTAssertNil(
      PaneZoom.zoomed(after: .panesChanged, zoomed: p2, active: p1, panes: [p1, drawer]),
      "the zoomed pane 2 closing must unzoom; 2 means the view stays zoomed on a pane that is"
        + " gone")
    XCTAssertEqual(
      PaneZoom.zoomed(after: .panesChanged, zoomed: p2, active: p2, panes: [p2, drawer]), p2,
      "a hidden pane 1 closing must keep pane 2 zoomed; nil means any close unzooms")
  }

  // MARK: - K6 to K9: what a zoom freezes or redirects

  /// K6: a zoomed view freezes focus moves, the dividers and the drawer edge; an unzoomed one
  /// does not.
  func testAZoomedViewFreezesItsLayout() {
    XCTAssertTrue(
      PaneZoom.freezesLayout(zoomed: p2),
      "a zoomed view must freeze its layout: false lets a focus move or a hidden divider act on"
        + " panes the user cannot see")
    XCTAssertFalse(
      PaneZoom.freezesLayout(zoomed: nil),
      "an unzoomed view must not freeze its layout: true disables every focus move and divider"
        + " drag")
  }

  /// K7: Toggle Terminal acts unzoomed, and while zoomed only when the drawer is the zoomed
  /// pane.
  func testToggleTerminalActsUnzoomedOrOnTheZoomedDrawer() {
    XCTAssertTrue(
      PaneZoom.allowsToggleTerminal(zoomed: nil, drawer: drawer),
      "unzoomed with a drawer, Toggle Terminal must act, closing the drawer")
    XCTAssertTrue(
      PaneZoom.allowsToggleTerminal(zoomed: nil, drawer: nil),
      "unzoomed with no drawer, Toggle Terminal must act, opening one")
    XCTAssertTrue(
      PaneZoom.allowsToggleTerminal(zoomed: drawer, drawer: drawer),
      "with the drawer zoomed, Toggle Terminal must act, closing the zoomed pane; false means a"
        + " zoomed drawer cannot be closed by its own toggle")
    XCTAssertFalse(
      PaneZoom.allowsToggleTerminal(zoomed: p2, drawer: drawer),
      "with tree pane 2 zoomed, Toggle Terminal must do nothing; true closes a drawer the user"
        + " cannot see")
    XCTAssertFalse(
      PaneZoom.allowsToggleTerminal(zoomed: p2, drawer: nil),
      "with tree pane 2 zoomed and no drawer, Toggle Terminal must do nothing; true opens a"
        + " drawer that would unzoom the view")
  }

  /// K8: while zoomed, the other-pane commands take their split path from a tree pane and do
  /// nothing from the drawer.
  func testTheOtherPaneCommandsSplitWhileZoomed() {
    XCTAssertEqual(
      PaneZoom.otherPaneTarget(zoomed: p2, drawer: drawer), .split,
      "a zoomed tree pane beside a drawer is the only visible pane, so the other-pane commands"
        + " must split; nil makes them do nothing, an existing pane binds into a hidden one")
    XCTAssertEqual(
      PaneZoom.otherPaneTarget(zoomed: p2, drawer: nil), .split,
      "a zoomed tree pane with no drawer is the only visible pane, so the other-pane commands"
        + " must split")
    XCTAssertNil(
      PaneZoom.otherPaneTarget(zoomed: drawer, drawer: drawer),
      "a zoomed drawer must leave the other-pane commands doing nothing, as from the drawer"
        + " unzoomed; .split means the drawer is split")
  }

  /// K9: Neovim's current window resolving to a hidden pane is sent back to the zoomed pane.
  func testNeovimOnAHiddenPaneIsSentBack() {
    XCTAssertTrue(
      PaneZoom.refocuses(resolved: p1, zoomed: p2),
      "Neovim on hidden pane 1 while 2 is zoomed must be refocused; false lets the user type"
        + " into a pane they cannot see")
    XCTAssertFalse(
      PaneZoom.refocuses(resolved: p2, zoomed: p2),
      "Neovim on the zoomed pane needs no refocus; true sends focus_win on every wake")
    XCTAssertFalse(
      PaneZoom.refocuses(resolved: nil, zoomed: p2),
      "an unresolved window needs no refocus; true refocuses on a window no pane holds")
    XCTAssertFalse(
      PaneZoom.refocuses(resolved: p1, zoomed: nil),
      "an unzoomed view never refocuses; true pins focus where no zoom asked for it")
  }

  // MARK: - K10: the visible rectangles

  /// K10: a zoomed view shows its pane alone over the whole area, at any area; an unzoomed one,
  /// and one zoomed on a pane that is gone, show every pane unchanged.
  func testAZoomedViewShowsOnlyItsPaneOverTheWholeArea() {
    assertRects(
      PaneZoom.visibleRects(zoomed: p2, rects: rects, bounds: bounds), [(p2, bounds)],
      "zoomed on 2, only pane 2 must show, over the whole area \(bounds). 2 at \(r2) means the"
        + " zoomed pane keeps its own rect; three panes mean the others are not hidden")
    assertRects(
      PaneZoom.visibleRects(zoomed: p2, rects: rects, bounds: bounds2), [(p2, bounds2)],
      "zoomed on 2 over a resized area, pane 2 must follow the area to \(bounds2)")
    assertRects(
      PaneZoom.visibleRects(zoomed: nil, rects: rects, bounds: bounds), rects,
      "unzoomed, every pane must show at its own rect")
    assertRects(
      PaneZoom.visibleRects(zoomed: absent, rects: rects, bounds: bounds), rects,
      "zoomed on pane 5, which is not among the rects, every pane must show rather than none;"
        + " an empty result blanks the view after a missed unzoom")
  }
}
