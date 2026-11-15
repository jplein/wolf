// Checks that Preview and Edit in Another Pane reuse the existing other pane rather than splitting.

import AppKit
import WolfKeyMap
import XCTest

@testable import WolfMacOS

/// Each test repeats what `WolfWindowController.previewInOtherPane` and `editInOtherPane`
/// do — `otherPaneTarget(prefer:)`, then the bridge call with that target, then a wake —
/// over a `TerminalViewFixture`, because AppKit routes no event to an offscreen window.
/// No test waits on a page: the preview's web view is never asked to load anything here.
@MainActor
final class OtherPaneFlowTests: XCTestCase {

  /// The state after the first press: two panes, the editor showing `a.md` and the
  /// preview of `a.md` beside it.
  private struct Opened {
    let editorWin: Int64
    let previewWin: Int64
    let bPath: String
  }

  /// Every open preview as `(window, source basename)`. `previews()` yields borrowed refs
  /// valid only in the loop body, so the scalars are copied out.
  private func previews(_ handle: WolfCoreHandle) -> [(win: Int64, source: String)] {
    var out: [(win: Int64, source: String)] = []
    for preview in handle.previews() {
      out.append((preview.win(), preview.source_name().toString()))
    }
    return out
  }

  /// The basename of the file window `win` shows, or "" for none.
  private func shownFile(_ handle: WolfCoreHandle, _ win: Int64) -> String {
    (handle.win_buffer(win).abs_path().toString() as NSString).lastPathComponent
  }

  /// Whether the container's pane tree holds as many panes as Neovim has windows, so a
  /// pane count read now is not one wake behind a split.
  private func settled(_ fixture: TerminalViewFixture) -> Bool {
    fixture.container.paneCount == fixture.handle.windows().map { $0.win() }.count
  }

  /// Writes `a.md` and `b.md` into Neovim's working directory, edits `a.md`, and presses
  /// Preview in Another Pane from the lone pane. `nil`, with the test failed, when a
  /// premise does not hold.
  private func openFirstPreview(_ fixture: TerminalViewFixture) -> Opened? {
    let handle = fixture.handle
    let container = fixture.container
    var cwd = ""
    guard
      fixture.live.waitFor(
        timeout: TerminalViewFixture.timeout,
        {
          cwd = handle.cwd().toString()
          return !cwd.isEmpty
        })
    else {
      XCTFail("premise: Neovim never reported its working directory")
      return nil
    }
    let aPath = (cwd as NSString).appendingPathComponent("a.md")
    let bPath = (cwd as NSString).appendingPathComponent("b.md")
    do {
      try "# First\n\nThe first document.\n".write(
        toFile: aPath, atomically: true, encoding: .utf8)
      try "# Second\n\nThe second document.\n".write(
        toFile: bPath, atomically: true, encoding: .utf8)
    } catch {
      XCTFail("premise: could not write the fixtures in \(cwd): \(error)")
      return nil
    }

    // Waits for a.md itself: the startup buffer is the markdown-named Untitled.md.
    fixture.typeEx("edit \(aPath)")
    guard fixture.wake(until: { shownFile(handle, handle.active_win()) == "a.md" }) else {
      XCTFail(
        "premise: :edit \(aPath) never showed a.md in the active window, which shows"
          + " \(shownFile(handle, handle.active_win())). The startup buffer is the markdown-named"
          + " Untitled.md, so a Markdown buffer being active proves nothing")
      return nil
    }

    let target = container.otherPaneTarget(prefer: .right)
    guard target == 0 else {
      XCTFail(
        "premise: from a lone pane the Preview target must be 0, a split; it is"
          + " \(String(describing: target)). nil means the container has no active tree leaf")
      return nil
    }
    _ = handle.preview_in_other(0)
    container.onRenderWake()

    var opened = previews(handle)
    guard
      fixture.wake(until: {
        opened = previews(handle)
        return container.paneCount == 2 && opened.count == 1 && settled(fixture)
      })
    else {
      XCTFail(
        "premise: the first press must split into two panes with one preview:"
          + " \(container.paneCount) pane(s), previews \(opened)")
      return nil
    }
    let previewWin = opened[0].win
    // otherPaneTarget resolves from the container's active pane, which follows Neovim on a wake.
    guard
      fixture.wake(until: {
        shownFile(handle, handle.active_win()) == "a.md"
          && container.activeWin == handle.active_win()
      })
    else {
      XCTFail(
        "premise: after the first press focus must stay on the source a.md beside the preview:"
          + " active window \(handle.active_win()) shows \(shownFile(handle, handle.active_win())),"
          + " the container's active window is \(container.activeWin), preview window"
          + " \(previewWin)")
      return nil
    }
    let editorWin = handle.active_win()
    guard previewWin != editorWin else {
      XCTFail(
        "premise: after the first press focus must stay on the source a.md beside the preview:"
          + " the active window \(editorWin) is the preview window \(previewWin)")
      return nil
    }
    return Opened(editorWin: editorWin, previewWin: previewWin, bPath: bPath)
  }

  /// T6: with the editor and a preview side by side, previewing a second file puts its
  /// preview into the existing preview pane, keeps two panes, and leaves the editor focused.
  func testPreviewInAnotherPaneReusesThePreviewPane() {
    guard let fixture = TerminalViewFixture() else { return }
    defer { fixture.shutdown() }
    let handle = fixture.handle
    let container = fixture.container
    guard let opened = openFirstPreview(fixture) else { return }

    fixture.typeEx("edit \(opened.bPath)")
    guard
      fixture.wake(until: {
        shownFile(handle, opened.editorWin) == "b.md" && container.activeWin == opened.editorWin
      })
    else {
      XCTFail(
        "premise: :edit b.md must land in the editor window \(opened.editorWin) with the"
          + " container's active pane on it; it shows \(shownFile(handle, opened.editorWin)),"
          + " the active window is \(handle.active_win()) and the container's is"
          + " \(container.activeWin)")
      return
    }

    let target = container.otherPaneTarget(prefer: .right)
    XCTAssertEqual(
      target, opened.previewWin,
      "the second Preview's target must be the existing preview window \(opened.previewWin);"
        + " 0 asks core to split a third pane, nil makes the press a no-op, and the editor's own"
        + " window \(opened.editorWin) would replace the source")
    guard let target else { return }
    _ = handle.preview_in_other(target)
    container.onRenderWake()

    var after = previews(handle)
    fixture.wake(until: {
      after = previews(handle)
      return after.contains { $0.source == "b.md" } && settled(fixture)
    })
    XCTAssertTrue(
      after.count == 1 && after.first?.win == opened.previewWin && after.first?.source == "b.md",
      "the only preview must be b.md in the existing window \(opened.previewWin); previews are"
        + " \(after). A second entry means core split a new pane for b.md; no b.md entry means"
        + " the press did nothing")
    XCTAssertEqual(
      container.paneCount, 2,
      "the window must keep two panes; three means the press split instead of reusing")
    XCTAssertEqual(
      handle.active_win(), opened.editorWin,
      "Preview in Another Pane must leave the editor \(opened.editorWin) focused; the preview"
        + " window \(opened.previewWin) focused means focus followed the preview")
  }

  /// T7: with the editor beside a preview of a different file, Edit in Another Pane from
  /// the preview puts that file's source into the existing editor pane, keeps two panes,
  /// and focuses the editor.
  func testEditInAnotherPaneReusesTheEditorPane() {
    guard let fixture = TerminalViewFixture() else { return }
    defer { fixture.shutdown() }
    let handle = fixture.handle
    let container = fixture.container
    guard let opened = openFirstPreview(fixture) else { return }

    fixture.neovimChord("l")
    guard
      fixture.wake(until: {
        handle.active_win() == opened.previewWin && container.activeWin == opened.previewWin
      })
    else {
      XCTFail(
        "premise: <C-w>l must focus the preview window \(opened.previewWin); Neovim's active"
          + " window is \(handle.active_win()), the container's \(container.activeWin)")
      return
    }

    // The route a picker takes for a preview pane: repoint the preview at b.md.
    _ = handle.preview_file(opened.previewWin, opened.bPath)
    var repointed = previews(handle)
    guard
      fixture.wake(until: {
        repointed = previews(handle)
        return repointed.count == 1 && repointed[0].win == opened.previewWin
          && repointed[0].source == "b.md"
      })
    else {
      XCTFail(
        "premise: preview_file must repoint the preview window \(opened.previewWin) at b.md;"
          + " previews are \(repointed)")
      return
    }

    let target = container.otherPaneTarget(prefer: .left)
    XCTAssertEqual(
      target, opened.editorWin,
      "the Edit target must be the existing editor window \(opened.editorWin); 0 asks core to"
        + " split a third pane, nil makes the press a no-op, and the preview's own window"
        + " \(opened.previewWin) would replace the preview")
    guard let target else { return }
    _ = handle.edit_in_other(target)
    container.onRenderWake()

    fixture.wake(until: {
      shownFile(handle, handle.active_win()) == "b.md" && settled(fixture)
    })
    XCTAssertEqual(
      handle.active_win(), opened.editorWin,
      "Edit in Another Pane must focus the editor \(opened.editorWin), which received the"
        + " source; the active window is \(handle.active_win()), showing"
        + " \(shownFile(handle, handle.active_win()))")
    XCTAssertEqual(
      shownFile(handle, opened.editorWin), "b.md",
      "the editor window \(opened.editorWin) must show b.md's source; a.md means the source went"
        + " elsewhere, into a split or over the preview")
    XCTAssertEqual(
      container.paneCount, 2,
      "the window must keep two panes; three means the press split instead of reusing")
    let after = previews(handle)
    XCTAssertTrue(
      after.count == 1 && after.first?.win == opened.previewWin && after.first?.source == "b.md",
      "the preview window \(opened.previewWin) must still preview b.md; previews are \(after)."
        + " Another window means the split moved the preview; none means the source replaced it")
  }
}
