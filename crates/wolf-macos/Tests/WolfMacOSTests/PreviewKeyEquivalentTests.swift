// Menu key equivalents reaching their items while a Markdown preview's web view is first
// responder.

import AppKit
import WebKit
import XCTest

@testable import WolfMacOS

/// Menu key equivalents over a preview whose `WKWebView` is the window's first responder, as a
/// click in the preview leaves it.
///
/// While the web view is first responder, WebKit claims every key equivalent and sends it to
/// the page. The binding's command (`noop:` for an unbound ⌘ chord) comes back to the UI
/// process, which sends it up the web view's responder chain with a sink at the end. Only when
/// the command reaches that sink does WebKit resend the event through `NSApp.sendEvent`, where
/// the main menu sees it. A responder on the chain that swallows the command drops the
/// shortcut. That is a premise read from WebKit's source (`WebViewImpl::performKeyEquivalent`,
/// `executeSavedCommandBySelector` with `WKResponderChainSink`, `doneWithKeyEvent`).
///
/// No WebContent process runs where the gate runs (see `PreviewViewFixture`), so the step in
/// which the page answers the key cannot run. These tests model it by its known outcome: the
/// page does not handle an unbound ⌘ chord, and its binding's command is `noop:`. Everything
/// around that step runs on real objects: the web view, its responder chain, the production
/// app menu and AppKit's menu matching. No window is key offscreen either, so where AppKit
/// would route an event, the tests call the window's and the menu's `performKeyEquivalent`.
///
/// A failure prefixed `premise:` is the environment or the fixture, not the routing. One
/// prefixed `control:` is a chord that fails to reach its item with the container as first
/// responder, so the synthesized event or the recorded menu is wrong and a later failure
/// would say nothing about the web view.
@MainActor
final class PreviewKeyEquivalentTests: XCTestCase {

  /// What the container's `doCommand(by:)` must pass on: the command AppKit's key bindings
  /// give an unbound ⌘ chord, and what WebKit's sink waits for.
  private static let noop = NSSelectorFromString("noop:")

  /// The web view passes a command it does not handle up its responder chain, and the
  /// container passes it on, so it reaches a sink hung at the chain's end as WebKit's
  /// `WKResponderChainSink` is. No page is needed, so this runs without a web process.
  func testAnUnhandledCommandFromThePreviewReachesTheEndOfTheResponderChain() {
    guard let fixture = PreviewViewFixture() else { return }
    defer { fixture.shutdown() }
    let web = fixture.web
    XCTAssertTrue(
      web.nextResponder === fixture.container,
      "premise: the web view's next responder must be the container, the responder under test:"
        + " it is \(String(describing: web.nextResponder))")
    guard web.nextResponder === fixture.container else { return }

    let sent = sendUpTheChain(Self.noop, from: web)
    XCTAssertTrue(
      sent.reachedSink,
      "noop: sent to the web view never reached the end of its responder chain, past"
        + " \(sent.last): a responder between the web view and the sink swallowed it."
        + " WebKit then counts an unbound key equivalent as handled by the page and never"
        + " resends it to the menu. The chain was \(sent.chain)")
  }

  /// With the preview's web view first responder, ⌘W reaches Close and ⌘P reaches Command
  /// Palette….
  ///
  /// WebKit's web-process step is modelled, since no WebContent process runs under the gate:
  /// the page leaves the chord unhandled, so `noop:` goes up the web view's real responder
  /// chain with a sink at its end, and only if the sink receives it is the event resent to
  /// the real menu, as WebKit's resend through `NSApp.sendEvent` reaches it. Whether the
  /// window's `performKeyEquivalent` claims the event is printed, not asserted: without a web
  /// process the value pins WebKit internals the container does not decide.
  func testCommandWAndCommandPReachTheirMenuItemsWithThePreviewFirstResponder() {
    guard let fixture = PreviewViewFixture() else { return }
    defer { fixture.shutdown() }
    guard let menu = RecordedMenu(window: fixture.window) else { return }
    defer { menu.finish() }
    let container = fixture.container
    let window = fixture.window
    guard container.canClosePane, container.paneCount == 2, container.activePaneIsPreview else {
      XCTFail(
        "premise: two panes with the preview active, so Close applies: canClosePane"
          + " \(container.canClosePane), \(container.paneCount) pane(s), preview active"
          + " \(container.activePaneIsPreview)")
      return
    }
    let legs = [(RecordedMenu.cmdW, menu.close), (RecordedMenu.cmdP, menu.palette)]

    guard window.makeFirstResponder(container) else {
      XCTFail("premise: the container must accept first responder")
      return
    }
    for (chord, item) in legs {
      guard let event = menu.event(for: chord) else { return }
      menu.recorder.fired.removeAll()
      let claimed = window.performKeyEquivalent(with: event)
      if !claimed { _ = NSApp.mainMenu?.performKeyEquivalent(with: event) }
      let fired = menu.recorder.fired
      print(
        "KEYROUTE responder=container key=\(chord.name) claimed=\(claimed) chain=n/a"
          + " item=\(fired.last?.title ?? "none")")
      guard !claimed, fired.count == 1, fired[0] === item else {
        XCTFail(
          "control: \(chord.name) with the container as first responder must go unclaimed by"
            + " the window and fire \(item.title) from the menu; claimed \(claimed), fired"
            + " \(fired.map(\.title)). Claimed means a view in the window took it; fired"
            + " nothing means the menu matched no enabled item for the synthesized event")
        return
      }
    }

    let focused = window.makeFirstResponder(fixture.web) && window.firstResponder === fixture.web
    guard focused else {
      XCTFail(
        "premise: the preview's WKWebView must become first responder, as a click makes it:"
          + " first responder is \(String(describing: window.firstResponder))")
      return
    }
    for (chord, item) in legs {
      guard let event = menu.event(for: chord) else { return }
      let claimed = window.performKeyEquivalent(with: event)
      menu.recorder.fired.removeAll()
      let sent = sendUpTheChain(Self.noop, from: fixture.web)
      if sent.reachedSink { _ = NSApp.mainMenu?.performKeyEquivalent(with: event) }
      let fired = menu.recorder.fired
      print(
        "KEYROUTE responder=web key=\(chord.name) claimed=\(claimed)"
          + " chain=\(sent.reachedSink ? "passed" : "swallowed")"
          + " item=\(fired.last?.title ?? "none")")
      XCTAssertTrue(
        fired.count == 1 && fired[0] === item,
        "\(chord.name) with the web view first responder must fire \(item.title) once; it"
          + " fired \(fired.map(\.title)). None, with the chain swallowed, means a responder"
          + " between the web view and the window ate the binding's noop:, so WebKit never"
          + " resends the key to the menu: past \(sent.last), the chain was \(sent.chain)."
          + " None with the chain passed, or another item, means the menu matched wrongly,"
          + " which the control leg should have caught")
    }
  }

  /// Sends `selector` to `responder` as WebKit sends a binding's command, with a sink hung at
  /// the end of the chain for the call, and reports whether the sink received it.
  private func sendUpTheChain(_ selector: Selector, from responder: NSResponder) -> (
    reachedSink: Bool, last: String, chain: String
  ) {
    var last = responder
    while let next = last.nextResponder { last = next }
    let sink = ResponderChainSink()
    last.nextResponder = sink
    responder.doCommand(by: selector)
    last.nextResponder = nil
    return (!sink.received.isEmpty, String(describing: type(of: last)), chain(from: responder))
  }

  /// The responder chain from `responder`, as type names.
  private func chain(from responder: NSResponder) -> String {
    var names: [String] = []
    var next: NSResponder? = responder
    while let current = next {
      names.append(String(describing: type(of: current)))
      next = current.nextResponder
    }
    return names.joined(separator: " -> ")
  }
}

/// A key equivalent: the unshifted key as a menu item spells it, and its modifiers.
private struct Chord {
  let key: String
  let modifiers: NSEvent.ModifierFlags

  /// As `KEYROUTE` lines and messages name it, such as `shift-cmd-n`.
  var name: String {
    var parts: [String] = []
    if modifiers.contains(.control) { parts.append("ctrl") }
    if modifiers.contains(.option) { parts.append("opt") }
    if modifiers.contains(.shift) { parts.append("shift") }
    if modifiers.contains(.command) { parts.append("cmd") }
    return (parts + [key]).joined(separator: "-")
  }
}

/// The production app menu as `NSApp.mainMenu`, every item with a key equivalent retargeted
/// to a recorder, and the key events a keyboard sends for its chords.
///
/// `AppMenu.install` builds the menu over a bare `AppDelegate`, which finds no window
/// controller, so its validation would disable Close. The recorder enables everything instead.
/// Validation is a separate lead, read through `canClosePane`.
@MainActor
private final class RecordedMenu {
  static let cmdW = Chord(key: "w", modifiers: .command)
  static let cmdP = Chord(key: "p", modifiers: .command)

  /// ANSI key codes of the keys the tests' chords use.
  static let keyCodes: [String: UInt16] = ["w": 13, "p": 35]

  private let window: NSWindow
  /// Kept alive: `AppMenu.install` needs a target, and a menu item holds its target weakly.
  private let delegate = AppDelegate()
  let recorder = MenuRecorder()
  let close: NSMenuItem
  let palette: NSMenuItem

  init?(window: NSWindow, file: StaticString = #filePath, line: UInt = #line) {
    self.window = window
    _ = AppMenu.install(target: delegate)
    var built: [NSMenuItem] = []
    func collect(_ menu: NSMenu) {
      for item in menu.items {
        if !item.keyEquivalent.isEmpty { built.append(item) }
        if let submenu = item.submenu { collect(submenu) }
      }
    }
    if let main = NSApp.mainMenu { collect(main) }
    let byTitle = { (title: String) in built.first { $0.title == title } }
    guard let close = byTitle("Close"), let palette = byTitle("Command Palette…") else {
      XCTFail(
        "premise: AppMenu.install must build Close and Command Palette… with key equivalents:"
          + " built \(built.map(\.title))", file: file, line: line)
      NSApp.mainMenu = nil
      return nil
    }
    self.close = close
    self.palette = palette
    for item in built {
      item.target = recorder
      item.action = #selector(MenuRecorder.record(_:))
    }
  }

  /// Removes the menu.
  func finish() {
    NSApp.mainMenu = nil
  }

  /// `chord` as a keyboard delivers it to the window: shifted letters uppercase, and a control
  /// letter as its control character.
  func event(for chord: Chord, file: StaticString = #filePath, line: UInt = #line) -> NSEvent? {
    guard let keyCode = Self.keyCodes[chord.key] else {
      XCTFail("premise: no ANSI key code for \(chord.name)", file: file, line: line)
      return nil
    }
    let ignoring = chord.modifiers.contains(.shift) ? chord.key.uppercased() : chord.key
    var characters = ignoring
    if chord.modifiers.contains(.control), let scalar = chord.key.unicodeScalars.first,
      (0x61...0x7a).contains(scalar.value), let control = Unicode.Scalar(scalar.value & 0x1f)
    {
      characters = String(Character(control))
    }
    let event = NSEvent.keyEvent(
      with: .keyDown, location: .zero, modifierFlags: chord.modifiers,
      timestamp: ProcessInfo.processInfo.systemUptime, windowNumber: window.windowNumber,
      context: nil, characters: characters, charactersIgnoringModifiers: ignoring,
      isARepeat: false, keyCode: keyCode)
    if event == nil {
      XCTFail("premise: could not synthesize \(chord.name)", file: file, line: line)
    }
    return event
  }
}

/// The target every recorded menu item fires: it records the item and enables everything.
private final class MenuRecorder: NSObject, NSMenuItemValidation {
  var fired: [NSMenuItem] = []

  @objc func record(_ sender: Any?) {
    if let item = sender as? NSMenuItem { fired.append(item) }
  }

  func validateMenuItem(_ menuItem: NSMenuItem) -> Bool { true }
}

/// The last responder of a chain, as WebKit's `WKResponderChainSink` is: it records every way
/// a command can arrive and claims it.
private final class ResponderChainSink: NSResponder {
  var received: [String] = []

  override func doCommand(by selector: Selector) {
    received.append("doCommand(by: \(NSStringFromSelector(selector)))")
  }

  override func tryToPerform(_ action: Selector, with object: Any?) -> Bool {
    received.append("tryToPerform(\(NSStringFromSelector(action)))")
    return true
  }

  override func noResponder(for eventSelector: Selector) {
    received.append("noResponder(for: \(NSStringFromSelector(eventSelector)))")
  }
}
