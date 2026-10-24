// See crates/wolf-macos/Tests/WolfKeyMapTests/README.md#keymaptestsswift

import XCTest

@testable import WolfKeyMap

// MARK: - FfiKeyEvent.kind discriminants
// Must match `kind: u8` of the Rust `FfiKeyEvent`; `key_event_from_ffi` is the source of truth.
private enum Kind {
  static let char: UInt8 = 0
  static let enter: UInt8 = 1
  static let esc: UInt8 = 2
  static let tab: UInt8 = 3
  static let backspace: UInt8 = 4
  static let delete: UInt8 = 5
  static let left: UInt8 = 6
  static let right: UInt8 = 7
  static let up: UInt8 = 8
  static let down: UInt8 = 9
  static let home: UInt8 = 10
  static let end: UInt8 = 11
  static let pageUp: UInt8 = 12
  static let pageDown: UInt8 = 13
  static let function: UInt8 = 14
}

// MARK: - macOS virtual key codes (kVK_*)
// Backspace is `kVK_Delete` (0x33) and forward-delete `kVK_ForwardDelete` (0x75): do not swap.
private enum VK {
  static let returnKey: UInt16 = 0x24  // kVK_Return
  static let tab: UInt16 = 0x30  // kVK_Tab
  static let backspace: UInt16 = 0x33  // kVK_Delete (the Backspace key)
  static let escape: UInt16 = 0x35  // kVK_Escape
  static let forwardDelete: UInt16 = 0x75  // kVK_ForwardDelete
  static let home: UInt16 = 0x73  // kVK_Home
  static let end: UInt16 = 0x77  // kVK_End
  static let pageUp: UInt16 = 0x74  // kVK_PageUp
  static let pageDown: UInt16 = 0x79  // kVK_PageDown
  static let leftArrow: UInt16 = 0x7B  // kVK_LeftArrow
  static let rightArrow: UInt16 = 0x7C  // kVK_RightArrow
  static let downArrow: UInt16 = 0x7D  // kVK_DownArrow
  static let upArrow: UInt16 = 0x7E  // kVK_UpArrow
  static let f1: UInt16 = 0x7A  // kVK_F1
  static let f5: UInt16 = 0x60  // kVK_F5
  static let shiftLeft: UInt16 = 0x38  // kVK_Shift (bare modifier)
  static let ansiA: UInt16 = 0x00  // kVK_ANSI_A
  static let ansiS: UInt16 = 0x01  // kVK_ANSI_S
  static let ansiW: UInt16 = 0x0D  // kVK_ANSI_W
  static let ansiX: UInt16 = 0x07  // kVK_ANSI_X
  static let ansi1: UInt16 = 0x12  // kVK_ANSI_1
}

// MARK: - NSEvent function-key unicode scalars
// Hardcoded to stay AppKit-free; fidelity only, since `keyCode` decides every named key.
private enum FnChar {
  static let upArrow = "\u{F700}"  // NSUpArrowFunctionKey
  static let downArrow = "\u{F701}"  // NSDownArrowFunctionKey
  static let leftArrow = "\u{F702}"  // NSLeftArrowFunctionKey
  static let rightArrow = "\u{F703}"  // NSRightArrowFunctionKey
  static let f1 = "\u{F704}"  // NSF1FunctionKey
  static let f5 = "\u{F708}"  // NSF5FunctionKey
  static let home = "\u{F729}"  // NSHomeFunctionKey
  static let end = "\u{F72B}"  // NSEndFunctionKey
  static let pageUp = "\u{F72C}"  // NSPageUpFunctionKey
  static let pageDown = "\u{F72D}"  // NSPageDownFunctionKey
  static let forwardDelete = "\u{F728}"  // NSDeleteFunctionKey (forward delete)
  // Control characters that the named keys deliver in `characters`:
  static let carriageReturn = "\u{0D}"  // Return
  static let tab = "\u{09}"  // Tab
  static let escape = "\u{1B}"  // Escape
  static let backspace = "\u{7F}"  // Backspace key (DEL char on macOS)
}

/// F-key numbers asserted below (named to avoid magic numbers).
private let kF1Num: UInt8 = 1
private let kF5Num: UInt8 = 5

final class KeyMapTests: XCTestCase {

  // MARK: helpers

  /// The Unicode scalar value of the first character of `s` (for `ch` asserts).
  private func scalar(_ s: String) -> UInt32 {
    guard let first = s.unicodeScalars.first else {
      XCTFail("expected a non-empty string to take a scalar from")
      return 0
    }
    return first.value
  }

  /// Asserts `got` is non-nil and matches every expected field, with a clear
  /// message. Field-by-field so this works whether or not the generated
  /// `FfiKeyEvent` is `Equatable`.
  private func assertKey(
    _ got: FfiKeyEvent?,
    kind: UInt8,
    ch: UInt32 = 0,
    fnum: UInt8 = 0,
    ctrl: Bool = false,
    shift: Bool = false,
    alt: Bool = false,
    meta: Bool = false,
    _ desc: String,
    file: StaticString = #filePath,
    line: UInt = #line
  ) {
    guard let ev = got else {
      XCTFail("case \(desc): expected a key event, got nil", file: file, line: line)
      return
    }
    XCTAssertEqual(ev.kind, kind, "case \(desc): kind", file: file, line: line)
    // `ch` is only meaningful for Char; for named keys it should be 0.
    XCTAssertEqual(ev.ch, ch, "case \(desc): ch", file: file, line: line)
    XCTAssertEqual(ev.fnum, fnum, "case \(desc): fnum", file: file, line: line)
    XCTAssertEqual(ev.ctrl, ctrl, "case \(desc): ctrl", file: file, line: line)
    XCTAssertEqual(ev.shift, shift, "case \(desc): shift", file: file, line: line)
    XCTAssertEqual(ev.alt, alt, "case \(desc): alt", file: file, line: line)
    XCTAssertEqual(ev.meta, meta, "case \(desc): meta", file: file, line: line)
  }

  // MARK: plain + shifted printable chars (the Shift contract)

  func testPlainLowercaseChar() {
    // 'a' with no modifiers -> Char('a'), all flags false.
    let got = WolfKeyMap.translate(
      keyCode: VK.ansiA,
      characters: "a",
      charactersIgnoringModifiers: "a",
      ctrl: false, shift: false, alt: false, meta: false
    )
    assertKey(got, kind: Kind.char, ch: scalar("a"), "plain 'a'")
  }

  func testShiftedUppercaseCharDeliversGlyphWithShiftFalse() {
    // Shift+'a': `characters` and `charactersIgnoringModifiers` both carry 'A' (the latter ignores
    // Cmd and Option, not Shift). The contract: glyph 'A', shift flag false.
    let got = WolfKeyMap.translate(
      keyCode: VK.ansiA,
      characters: "A",
      charactersIgnoringModifiers: "A",
      ctrl: false, shift: true, alt: false, meta: false
    )
    assertKey(
      got, kind: Kind.char, ch: scalar("A"), shift: false,
      "Shift+'a' -> 'A' shift=false")
  }

  func testShiftedSymbolDeliversGlyphWithShiftFalse() {
    // Shift+'1' on a US layout yields '!'. The shifted glyph is delivered in
    // `characters`; shift flag stays FALSE for the Char.
    let got = WolfKeyMap.translate(
      keyCode: VK.ansi1,
      characters: "!",
      charactersIgnoringModifiers: "!",
      ctrl: false, shift: true, alt: false, meta: false
    )
    assertKey(
      got, kind: Kind.char, ch: scalar("!"), shift: false,
      "Shift+'1' -> '!' shift=false")
  }

  // MARK: named keys (shift PRESERVED here)

  func testNamedKeys() {
    // (description, keyCode, characters, expectedKind, expectedFnum)
    let cases: [(desc: String, code: UInt16, chars: String, kind: UInt8, fnum: UInt8)] = [
      ("Return", VK.returnKey, FnChar.carriageReturn, Kind.enter, 0),
      ("Escape", VK.escape, FnChar.escape, Kind.esc, 0),
      ("Tab", VK.tab, FnChar.tab, Kind.tab, 0),
      ("Backspace", VK.backspace, FnChar.backspace, Kind.backspace, 0),
      ("Delete(forward)", VK.forwardDelete, FnChar.forwardDelete, Kind.delete, 0),
      ("Up", VK.upArrow, FnChar.upArrow, Kind.up, 0),
      ("Down", VK.downArrow, FnChar.downArrow, Kind.down, 0),
      ("Left", VK.leftArrow, FnChar.leftArrow, Kind.left, 0),
      ("Right", VK.rightArrow, FnChar.rightArrow, Kind.right, 0),
      ("Home", VK.home, FnChar.home, Kind.home, 0),
      ("End", VK.end, FnChar.end, Kind.end, 0),
      ("PageUp", VK.pageUp, FnChar.pageUp, Kind.pageUp, 0),
      ("PageDown", VK.pageDown, FnChar.pageDown, Kind.pageDown, 0),
      ("F1", VK.f1, FnChar.f1, Kind.function, kF1Num),
      ("F5", VK.f5, FnChar.f5, Kind.function, kF5Num),
    ]

    for c in cases {
      let got = WolfKeyMap.translate(
        keyCode: c.code,
        characters: c.chars,
        charactersIgnoringModifiers: c.chars,
        ctrl: false, shift: false, alt: false, meta: false
      )
      assertKey(got, kind: c.kind, fnum: c.fnum, c.desc)
    }
  }

  func testShiftOnNamedKeyIsPreserved() {
    // Shift+Tab keeps the real shift flag (named key -> nvim <S-Tab>).
    let tab = WolfKeyMap.translate(
      keyCode: VK.tab,
      characters: FnChar.tab,
      charactersIgnoringModifiers: FnChar.tab,
      ctrl: false, shift: true, alt: false, meta: false
    )
    assertKey(tab, kind: Kind.tab, shift: true, "Shift+Tab keeps shift=true")

    // Shift+Left likewise (named arrow -> <S-Left>).
    let left = WolfKeyMap.translate(
      keyCode: VK.leftArrow,
      characters: FnChar.leftArrow,
      charactersIgnoringModifiers: FnChar.leftArrow,
      ctrl: false, shift: true, alt: false, meta: false
    )
    assertKey(left, kind: Kind.left, shift: true, "Shift+Left keeps shift=true")
  }

  // MARK: modifier combos on printable chars

  func testCtrlComboUsesCharsIgnoringModifiers() {
    // Ctrl+w: `characters` carries U+0017 but `charactersIgnoringModifiers` the bare 'w'; translate
    // must pick 'w' with ctrl=true, never the raw control char.
    let got = WolfKeyMap.translate(
      keyCode: VK.ansiW,
      characters: "\u{17}",
      charactersIgnoringModifiers: "w",
      ctrl: true, shift: false, alt: false, meta: false
    )
    assertKey(
      got, kind: Kind.char, ch: scalar("w"), ctrl: true,
      "Ctrl+w -> 'w' ctrl=true")
  }

  func testAltComboUsesCharsIgnoringModifiers() {
    // Option+x: NSEvent puts a layout-dependent glyph (e.g. '≈') in
    // `characters`; translate must use 'x' from charactersIgnoringModifiers.
    let got = WolfKeyMap.translate(
      keyCode: VK.ansiX,
      characters: "\u{2248}",  // '≈', Option+x on a US layout
      charactersIgnoringModifiers: "x",
      ctrl: false, shift: false, alt: true, meta: false
    )
    assertKey(
      got, kind: Kind.char, ch: scalar("x"), alt: true,
      "Alt+x -> 'x' alt=true")
  }

  func testMetaComboUsesCharsIgnoringModifiers() {
    // Cmd+s: meta/Command held. translate uses charactersIgnoringModifiers
    // and sets meta=true (maps to nvim <D-s>).
    let got = WolfKeyMap.translate(
      keyCode: VK.ansiS,
      characters: "s",
      charactersIgnoringModifiers: "s",
      ctrl: false, shift: false, alt: false, meta: true
    )
    assertKey(
      got, kind: Kind.char, ch: scalar("s"), meta: true,
      "Cmd+s -> 's' meta=true")
  }

  func testCtrlComboLowercasesUppercaseInput() {
    // Ctrl+Shift+w: `charactersIgnoringModifiers` may carry 'W'; translate still delivers lowercase
    // 'w' with ctrl=true and shift=false (shift never rides a Char).
    let got = WolfKeyMap.translate(
      keyCode: VK.ansiW,
      characters: "\u{17}",
      charactersIgnoringModifiers: "W",
      ctrl: true, shift: true, alt: false, meta: false
    )
    assertKey(
      got, kind: Kind.char, ch: scalar("w"), ctrl: true, shift: false,
      "Ctrl+Shift+w -> 'w' ctrl=true shift=false")
  }

  // MARK: nil cases

  func testBareModifierKeyIsNil() {
    // Pressing Shift alone: no printable character is produced. translate must
    // return nil (there is nothing to send to nvim).
    let got = WolfKeyMap.translate(
      keyCode: VK.shiftLeft,
      characters: "",
      charactersIgnoringModifiers: "",
      ctrl: false, shift: true, alt: false, meta: false
    )
    XCTAssertNil(got, "bare Shift modifier should translate to nil")
  }

  func testUnmodifiedControlCharIsNil() {
    // A control scalar with no modifiers and no named keyCode must not pass as a Char -> nil.
    // U+0001 stands in for any control scalar that is not a named key.
    let got = WolfKeyMap.translate(
      keyCode: 0xFFFF,  // deliberately not any named/printable keyCode
      characters: "\u{01}",
      charactersIgnoringModifiers: "\u{01}",
      ctrl: false, shift: false, alt: false, meta: false
    )
    XCTAssertNil(got, "an unmodified control char should translate to nil")
  }
}
