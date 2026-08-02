//
//  KeyCompositionTests.swift
//  Pure, headless unit tests for the AppKit-free `KeyComposition` seam (the macOS
//  option-as-option feature).
//
//  These are the sibling of `KeyMapTests` / `AppearanceSeamTests`: a pure,
//  display-free check exercised with scalar inputs only — NO AppKit, NO Carbon, NO
//  live Neovim. The Carbon/UCKeyTranslate composition glue (`KeyComposer`) is not
//  headlessly testable and lives in the WolfMacOS module; the DECISION logic it
//  brackets is extracted here into `KeyComposition` so it CAN be tested.
//
//  CONTRACTS UNDER TEST (plan "Seam boundary"):
//    * `KeyComposition.routesThroughComposer(keyCode:ctrl:meta:optionAsAlt:) -> Bool`
//      is true IFF `!optionAsAlt && !ctrl && !meta && !isNamedKey(keyCode)`. In
//      option-as-option mode EVERY printable-candidate key routes through the
//      composer (so the committing keystroke of a dead-key pair composes); when
//      option-as-alt is on, or any of ctrl/meta is held, or the key is a NAMED key
//      (Return, arrows, F-keys), it does NOT — the existing `translate` path owns it.
//    * `KeyComposition.charEvents(for:) -> [FfiKeyEvent]` emits ONE modifier-free
//      Char event per Unicode scalar of the composed string, filtering C0/DEL
//      controls, with kind=Char(0) and ctrl/shift/alt/meta all false.
//
//  NOTE to coder: this references the `KeyComposition` seam (new,
//  Foundation-only) and `WolfKeyMap.isNamedKey(keyCode:)` (a public wrapper over
//  the private `namedKey`), both of which the coder adds. The signatures and field
//  names asserted below are the contract the implementation must satisfy.
//

import XCTest

@testable import WolfKeyMap

// MARK: - FfiKeyEvent.kind discriminant (mirrors the Rust `FfiKeyEvent` kind
// scheme; `Char` is 0). Matches `KeyMapTests`' `Kind.char`.
private let kKindChar: UInt8 = 0

// MARK: - macOS virtual key codes (kVK_*) used below.
private enum VK {
  static let ansiU: UInt16 = 0x20  // kVK_ANSI_U — a plain printable letter
  static let returnKey: UInt16 = 0x24  // kVK_Return — a NAMED key
  static let leftArrow: UInt16 = 0x7B  // kVK_LeftArrow — a NAMED key
  static let f1: UInt16 = 0x7A  // kVK_F1 — a NAMED key
}

// MARK: - composed scalars asserted below.
private let kUUmlaut: UInt32 = 0x00FC  // "ü" (LATIN SMALL LETTER U WITH DIAERESIS)
private let kEAcute: UInt32 = 0x00E9  // "é" (LATIN SMALL LETTER E WITH ACUTE)

final class KeyCompositionTests: XCTestCase {

  // MARK: routesThroughComposer

  func testRoutesThroughComposerWhenPlainAndOptionAsOption() {
    // The one true case: option-as-option (optionAsAlt=false), no ctrl/meta, and a
    // plain (non-named) printable keyCode.
    XCTAssertTrue(
      KeyComposition.routesThroughComposer(
        keyCode: VK.ansiU, ctrl: false, meta: false, optionAsAlt: false),
      "a plain printable key in option-as-option mode must route through the composer")
  }

  func testDoesNotRouteWhenOptionAsAlt() {
    // Legacy option-as-alt: never compose (so `<M-…>` Alt mappings still work).
    XCTAssertFalse(
      KeyComposition.routesThroughComposer(
        keyCode: VK.ansiU, ctrl: false, meta: false, optionAsAlt: true),
      "option-as-alt mode must NOT route through the composer")
  }

  func testDoesNotRouteWhenCtrlHeld() {
    // Ctrl combos (e.g. Ctrl+u, Ctrl+Option+x) go to `translate`, not the composer.
    XCTAssertFalse(
      KeyComposition.routesThroughComposer(
        keyCode: VK.ansiU, ctrl: true, meta: false, optionAsAlt: false),
      "a ctrl-held key must NOT route through the composer")
  }

  func testDoesNotRouteWhenMetaHeld() {
    // Command combos (e.g. Cmd+s) go to `translate`, not the composer.
    XCTAssertFalse(
      KeyComposition.routesThroughComposer(
        keyCode: VK.ansiU, ctrl: false, meta: true, optionAsAlt: false),
      "a meta-held key must NOT route through the composer")
  }

  func testDoesNotRouteForNamedKeys() {
    // Named keys (Return, arrows, F-keys) are recognized from keyCode and owned by
    // `translate` (so Option+Left stays `<M-Left>`), never composed.
    let named: [(desc: String, code: UInt16)] = [
      ("Return", VK.returnKey),
      ("LeftArrow", VK.leftArrow),
      ("F1", VK.f1),
    ]
    for c in named {
      XCTAssertFalse(
        KeyComposition.routesThroughComposer(
          keyCode: c.code, ctrl: false, meta: false, optionAsAlt: false),
        "named key \(c.desc) must NOT route through the composer even in option-as-option mode")
      // Cross-check the underlying predicate the routing gate relies on.
      XCTAssertTrue(
        WolfKeyMap.isNamedKey(keyCode: c.code),
        "\(c.desc) must be recognized as a named key")
    }
    // And the plain letter must NOT be a named key.
    XCTAssertFalse(
      WolfKeyMap.isNamedKey(keyCode: VK.ansiU),
      "a plain printable letter ('u') must NOT be a named key")
  }

  // MARK: charEvents

  /// Asserts `ev` is a modifier-free Char event carrying `ch`.
  private func assertCharEvent(
    _ ev: FfiKeyEvent,
    ch: UInt32,
    _ desc: String,
    file: StaticString = #filePath,
    line: UInt = #line
  ) {
    XCTAssertEqual(ev.kind, kKindChar, "\(desc): kind must be Char", file: file, line: line)
    XCTAssertEqual(ev.ch, ch, "\(desc): ch", file: file, line: line)
    XCTAssertEqual(ev.ctrl, false, "\(desc): ctrl must be false", file: file, line: line)
    XCTAssertEqual(ev.shift, false, "\(desc): shift must be false", file: file, line: line)
    XCTAssertEqual(ev.alt, false, "\(desc): alt must be false", file: file, line: line)
    XCTAssertEqual(ev.meta, false, "\(desc): meta must be false", file: file, line: line)
  }

  func testCharEventsSingleScalar() {
    // "ü" -> exactly one modifier-free Char event with ch == 0x00FC.
    let events = KeyComposition.charEvents(for: "ü")
    XCTAssertEqual(events.count, 1, "\"ü\" must produce exactly one event")
    guard let ev = events.first else {
      XCTFail("expected one event for \"ü\"")
      return
    }
    assertCharEvent(ev, ch: kUUmlaut, "\"ü\"")
  }

  func testCharEventsTwoScalars() {
    // A two-scalar string -> two events, one per scalar, in order, all mods false.
    let events = KeyComposition.charEvents(for: "üé")
    XCTAssertEqual(events.count, 2, "\"üé\" must produce exactly two events")
    guard events.count == 2 else { return }
    assertCharEvent(events[0], ch: kUUmlaut, "\"üé\"[0]")
    assertCharEvent(events[1], ch: kEAcute, "\"üé\"[1]")
  }

  func testCharEventsControlOnlyIsEmpty() {
    // A control-only string (C0 / DEL) produces no events (controls are filtered).
    XCTAssertTrue(
      KeyComposition.charEvents(for: "\u{1B}").isEmpty,
      "a control-only string (ESC) must produce no events")
    XCTAssertTrue(
      KeyComposition.charEvents(for: "\u{7F}").isEmpty,
      "a control-only string (DEL) must produce no events")
  }

  func testCharEventsEmptyStringIsEmpty() {
    // An empty composed string (dead key still pending) produces no events.
    XCTAssertTrue(
      KeyComposition.charEvents(for: "").isEmpty,
      "an empty composed string must produce no events")
  }
}
