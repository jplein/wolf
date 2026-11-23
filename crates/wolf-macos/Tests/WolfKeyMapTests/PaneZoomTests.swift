// PaneZoom's rules: what zooms, what unzooms, a focus move included, and what a zoom freezes.

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

  /// A file opened from outside Wolf unzooms, whichever pane is zoomed and active.
  func testOpeningAFileUnzooms() {
    let cases: [(zoomed: PaneId, active: PaneId, panes: [PaneId])] = [
      (p2, p2, [p1, p2]), (p2, p1, [p1, p2]), (drawer, drawer, [p1, p2, drawer]),
    ]
    for (zoomed, active, panes) in cases {
      XCTAssertNil(
        PaneZoom.zoomed(after: .openFile, zoomed: zoomed, active: active, panes: panes),
        "opening a file with \(zoomed) zoomed and \(active) active must unzoom, so the pane it "
          + "lands in shows; \(zoomed) means .openFile keeps a zoom whose pane is still there, "
          + "as .panesChanged does")
    }
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

  // MARK: - K6 to K9: what a zoom freezes, redirects or gives up

  /// K6: a zoomed view freezes the tree dividers and the drawer edge; an unzoomed one does not.
  /// Focus moves are not frozen: K9 and K11 unzoom on them.
  func testAZoomedViewFreezesItsLayout() {
    XCTAssertTrue(
      PaneZoom.freezesLayout(zoomed: p2),
      "a zoomed view must freeze its dividers and drawer edge: false lets a drag act on panes"
        + " the user cannot see")
    XCTAssertFalse(
      PaneZoom.freezesLayout(zoomed: nil),
      "an unzoomed view must not freeze its layout: true disables every divider and edge drag")
  }

  /// K7: Toggle Terminal unzooms, whether the drawer is closed or open and hidden, and keeps the
  /// zoom only when the drawer is the zoomed pane, whose close then unzooms.
  func testToggleTerminalUnzoomsUnlessTheDrawerIsZoomed() {
    XCTAssertNil(
      PaneZoom.zoomed(
        after: .toggleTerminal(drawer: nil), zoomed: p2, active: p2, panes: [p1, p2]),
      "with tree pane 2 zoomed and the drawer closed, Toggle Terminal must unzoom before the"
        + " drawer opens; 2 means the drawer opens behind the zoomed pane")
    XCTAssertNil(
      PaneZoom.zoomed(
        after: .toggleTerminal(drawer: drawer), zoomed: p2, active: p2,
        panes: [p1, p2, drawer]),
      "with tree pane 2 zoomed and the drawer open and hidden, Toggle Terminal must unzoom"
        + " before the drawer closes; 2 means the user closes a drawer they cannot see")
    XCTAssertEqual(
      PaneZoom.zoomed(
        after: .toggleTerminal(drawer: drawer), zoomed: drawer, active: drawer,
        panes: [p1, p2, drawer]),
      drawer,
      "with the drawer zoomed, Toggle Terminal must keep the zoom until the drawer's close"
        + " unzooms it; nil flashes the hidden panes for a round trip before the close")
    XCTAssertNil(
      PaneZoom.zoomed(
        after: .toggleTerminal(drawer: drawer), zoomed: nil, active: p1,
        panes: [p1, p2, drawer]),
      "unzoomed, Toggle Terminal must leave the view unzoomed; 1 means it zooms the active pane")
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

  /// K9: a focus move to another pane unzooms, Neovim's own move to a hidden pane included; a
  /// move with no target, or onto the zoomed pane, keeps the zoom.
  func testAFocusMoveToAnotherPaneUnzooms() {
    let panes = [p1, p2, drawer]
    XCTAssertNil(
      PaneZoom.zoomed(after: .focusMove(target: p1), zoomed: p2, active: p2, panes: panes),
      "a focus move to pane 1 while 2 is zoomed must unzoom, so the pane focused shows; 2 means"
        + " the user types into a pane they cannot see")
    XCTAssertEqual(
      PaneZoom.zoomed(after: .focusMove(target: nil), zoomed: p2, active: p2, panes: panes), p2,
      "a focus move with no target must keep pane 2 zoomed; nil means a chord at the edge"
        + " unzooms though focus stays where it is")
    XCTAssertEqual(
      PaneZoom.zoomed(after: .focusMove(target: p2), zoomed: p2, active: p2, panes: panes), p2,
      "Neovim reporting the zoomed pane itself must keep the zoom; nil means every wake that"
        + " reads the current window unzooms")
    XCTAssertNil(
      PaneZoom.zoomed(after: .focusMove(target: p1), zoomed: nil, active: p2, panes: panes),
      "an unzoomed focus move must leave the view unzoomed; 1 or 2 means a focus move zooms")
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

  // MARK: - K11 to K13: a focus move while zoomed, through the seams applyFocus composes

  // A plus shape above a full-width drawer: T over [L | M | R] over B, then D.
  private let pT: PaneId = 11
  private let pL: PaneId = 12
  private let pM: PaneId = 13
  private let pR: PaneId = 14
  private let pB: PaneId = 15
  /// The live fixture's width, and the editor height above its drawer.
  private let plusWidth: Double = 960
  private let plusEditorHeight: Double = 427

  /// `horizontal[T, vertical[L, M, R], B]`, built as the view builds its tree.
  private func plusTree() -> PaneTree {
    let tree = PaneTree(root: pT)
    tree.split(pT, pL, .horizontal)
    tree.split(pL, pB, .horizontal)
    tree.split(pL, pM, .vertical)
    tree.split(pM, pR, .vertical)
    return tree
  }

  /// A focus move from `from` while `zoomed` is zoomed, composed as `applyFocus` composes it:
  /// the origin, its target in the unzoomed layout, and the zoom after the move.
  private func zoomedMove(
    zoomed: PaneId, from: PaneId?, _ dir: FocusDir, lastEditorPane: PaneId? = nil
  ) -> (target: PaneId?, zoomedAfter: PaneId?) {
    let tree = plusTree()
    let origin = PaneZoom.focusOrigin(zoomed: zoomed, from: from)
    let target = origin.flatMap {
      DrawerFocusSeam.focusTarget(
        from: $0, dir: dir, tree: tree, drawer: drawer, lastEditorPane: lastEditorPane,
        width: plusWidth, editorHeight: plusEditorHeight)
    }
    let after = PaneZoom.zoomed(
      after: .focusMove(target: target), zoomed: zoomed, active: zoomed,
      panes: tree.leaves() + [drawer])
    return (target, after)
  }

  /// K11: from the zoomed pane, a move resolves in the unzoomed layout and unzooms, in every
  /// direction and across both drawer bridges.
  func testAFocusMoveWithATargetUnzoomsInEachDirection() {
    XCTAssertEqual(
      plusTree().leaves(), [pT, pL, pM, pR, pB],
      "premise: the plus fixture's leaves must read T, L, M, R, B, or the cases below test"
        + " another shape")
    let cases: [(FocusDir, PaneId, String)] = [
      (.up, pT, "up"), (.down, pB, "down"), (.left, pL, "left"), (.right, pR, "right"),
    ]
    for (dir, want, name) in cases {
      let move = zoomedMove(zoomed: pM, from: pM, dir)
      XCTAssertEqual(
        move.target, want,
        "zoomed on M, \(name) must resolve to its neighbour \(want) in the unzoomed layout; nil"
          + " means the move is resolved in the zoomed view, which has no other pane")
      XCTAssertNil(
        move.zoomedAfter, "zoomed on M, a move \(name) to \(want) must unzoom, so it shows")
    }

    let down = zoomedMove(zoomed: pB, from: pB, .down)
    XCTAssertEqual(
      down.target, drawer,
      "zoomed on B, the bottom pane, down must bridge into the drawer; nil means the bridge is"
        + " lost when the target is resolved")
    XCTAssertNil(down.zoomedAfter, "zoomed on B, a move down into the drawer must unzoom")

    let up = zoomedMove(zoomed: drawer, from: drawer, .up, lastEditorPane: pM)
    XCTAssertEqual(
      up.target, pM,
      "zoomed on the drawer, up must reach the last editor pane M; nil means a drawer origin"
        + " resolves nothing, \(pB) that the MRU bridge falls back to the bottom-most pane")
    XCTAssertNil(up.zoomedAfter, "zoomed on the drawer, a move up to M must unzoom")
  }

  /// K12: a move with no target in the unzoomed layout keeps the zoom, at the tree's edges and
  /// sideways from the drawer.
  func testAFocusMoveWithNoTargetKeepsTheZoom() {
    let cases: [(PaneId, FocusDir, String)] = [
      (pL, .left, "L moving left"), (pT, .up, "T moving up"),
      (drawer, .left, "the drawer moving left"),
    ]
    for (zoomed, dir, name) in cases {
      let move = zoomedMove(zoomed: zoomed, from: zoomed, dir, lastEditorPane: pM)
      XCTAssertNil(
        move.target,
        "zoomed on \(name) has no target; \(String(describing: move.target)) means a direction"
          + " other than down bridges into the drawer, or the drawer resolves more than up")
      XCTAssertEqual(
        move.zoomedAfter, zoomed,
        "zoomed on \(name), a move with no target must keep the zoom; nil unzooms though focus"
          + " stays where it is")
    }
  }

  /// K13: a zoomed move starts from the zoomed pane, whatever pane the caller resolved; an
  /// unzoomed one starts from that pane.
  func testAZoomedFocusMoveStartsFromTheZoomedPane() {
    XCTAssertEqual(
      PaneZoom.focusOrigin(zoomed: pM, from: pL), pM,
      "zoomed on M, a move must start from M even when Neovim's window reads L; L resolves the"
        + " move from a hidden pane")
    XCTAssertEqual(
      PaneZoom.focusOrigin(zoomed: nil, from: pL), pL,
      "unzoomed, a move must start from the pane it was made in")
    XCTAssertEqual(
      zoomedMove(zoomed: pM, from: pL, .right).target, pR,
      "zoomed on M with Neovim's window on L, right must reach M's neighbour R; M means the move"
        + " started from L")
  }

  // MARK: - K14: the restore's re-zoom

  /// K14: a restore re-zooms its pane only when that pane is restored and the view has another
  /// pane for it to hide.
  func testARestoreRezoomsOnlyARestoredPaneWithSomethingToHide() {
    XCTAssertEqual(
      PaneZoom.zoomed(after: .restore(p2), zoomed: nil, active: p1, panes: [p1, p2]), p2,
      "a restore of pane 2 over [1, 2] must zoom 2; nil means the saved zoom is never applied,"
        + " 1 that the active pane is zoomed instead")
    XCTAssertNil(
      PaneZoom.zoomed(after: .restore(absent), zoomed: nil, active: p1, panes: [p1, p2]),
      "a restore of pane 5, which was not restored, must leave the tab unzoomed; 5 zooms a"
        + " pane that is not there, 1 the wrong one")
    XCTAssertNil(
      PaneZoom.zoomed(after: .restore(p1), zoomed: nil, active: p1, panes: [p1]),
      "a restore of a lone pane must leave it unzoomed: it has nothing to hide")
  }
}
