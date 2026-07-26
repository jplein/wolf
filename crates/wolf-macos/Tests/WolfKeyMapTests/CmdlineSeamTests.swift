//
//  CmdlineSeamTests.swift
//  Pure, headless unit tests for the `WolfKeyMap.CmdlineSeam` namespace.
//
//  The cmdline-render analog of `PaletteSeamTests.swift`: it exercises the pure,
//  display-free seams behind the macOS cmdline band with scalar inputs only — NO
//  FfiCmdline (which can't be built headlessly), NO NSView, NO AppKit. Three
//  contracts are under test:
//
//    * CmdlineSeam.composedLine(prompt:firstc:text:) -> String
//      — the string painted in the cmdline band, `prompt + firstc + text`. An
//      empty `prompt` collapses to today's `firstc + text`, so normal `:` / `/`
//      command entry is unchanged; a non-empty `prompt` (e.g. a `confirm()`
//      dialog's `[A], (B): ` answer line) is rendered inline. This pins the
//      regression fix: with `firstc` and `text` both empty the line is still the
//      prompt, not blank.
//
//    * CmdlineSeam.caretPrefix(prompt:firstc:text:byteCount:) -> String
//      — the substring left of the caret whose width positions the caret:
//      `prompt + firstc + bytePrefix(of: text, byteCount:)`. Including the prompt
//      width keeps the caret after the prompt. Defined in terms of `bytePrefix`,
//      so the same tested byte-slicing backs the caret math.
//
//    * CmdlineSeam.bytePrefix(of:byteCount:) -> String
//      — the UTF-8 byte-prefix of a string (relocated verbatim from the old
//      `CmdlineView.bytePrefix`): `byteCount <= 0` -> "", `byteCount >=` the
//      UTF-8 length -> the whole string, else the leading `byteCount` UTF-8 bytes
//      decoded back to a String (lossy on a split multi-byte boundary).
//
//  NOTE to coder: this references the pure `WolfKeyMap.CmdlineSeam` enum the coder
//  will add in `Sources/WolfKeyMap/CmdlineSeam.swift`. The signatures asserted
//  below are the contract the implementation must satisfy.
//

import XCTest

@testable import WolfKeyMap

final class CmdlineSeamTests: XCTestCase {

  // MARK: - composedLine

  /// One `composedLine` table row: (description, prompt, firstc, text, expected).
  private struct ComposedCase {
    let desc: String
    let prompt: String
    let firstc: String
    let text: String
    let expected: String
  }

  func testComposedLineTable() {
    let cases: [ComposedCase] = [
      // --- The dialog regression case -----------------------------------
      ComposedCase(
        // A `confirm()` answer line arrives with an empty firstc/text and a
        // non-empty prompt; the composed line must be the prompt, not blank.
        desc: "prompt only (confirm dialog), empty firstc/text => prompt",
        prompt: "[A], (B): ", firstc: "", text: "", expected: "[A], (B): "),
      // --- Normal command entry, unchanged ------------------------------
      ComposedCase(
        desc: "normal ':' command (empty prompt) => firstc + text",
        prompt: "", firstc: ":", text: "wq", expected: ":wq"),
      ComposedCase(
        desc: "normal '/' search (empty prompt) => firstc + text",
        prompt: "", firstc: "/", text: "foo", expected: "/foo"),
      // --- Prompt with entered text -------------------------------------
      ComposedCase(
        desc: "prompt + text => prompt + firstc + text",
        prompt: "Name: ", firstc: "", text: "bar", expected: "Name: bar"),
    ]

    for c in cases {
      let got = CmdlineSeam.composedLine(
        prompt: c.prompt, firstc: c.firstc, text: c.text)
      XCTAssertEqual(got, c.expected, "case: \(c.desc)")
    }
  }

  // MARK: - caretPrefix

  /// One `caretPrefix` table row: (description, prompt, firstc, text, byteCount,
  /// expected).
  private struct CaretCase {
    let desc: String
    let prompt: String
    let firstc: String
    let text: String
    let byteCount: Int
    let expected: String
  }

  func testCaretPrefixTable() {
    let cases: [CaretCase] = [
      // --- Prompt present, caret inside text ----------------------------
      CaretCase(
        // Prompt width is included so the caret sits after the prompt, at
        // byte offset 2 within "Bob" => "Bo".
        desc: "prompt + caret inside text => prompt + firstc + bytePrefix",
        prompt: "[A], (B): ", firstc: "", text: "Bob", byteCount: 2,
        expected: "[A], (B): Bo"),
      // --- Normal command entry, unchanged ------------------------------
      CaretCase(
        desc: "normal ':' command, caret after 1 byte => ':w'",
        prompt: "", firstc: ":", text: "wq", byteCount: 1, expected: ":w"),
      // --- Caret at the very start of text, with a prompt ---------------
      CaretCase(
        // byteCount 0 => empty text prefix, so the caret prefix is just the
        // prompt (caret sits immediately after the prompt).
        desc: "caret at start of text with prompt => prompt only",
        prompt: "P: ", firstc: "", text: "xyz", byteCount: 0, expected: "P: "),
    ]

    for c in cases {
      let got = CmdlineSeam.caretPrefix(
        prompt: c.prompt, firstc: c.firstc, text: c.text, byteCount: c.byteCount)
      XCTAssertEqual(got, c.expected, "case: \(c.desc)")
    }
  }

  // MARK: - bytePrefix

  /// One `bytePrefix` table row: (description, string, byteCount, expected).
  private struct BytePrefixCase {
    let desc: String
    let string: String
    let byteCount: Int
    let expected: String
  }

  func testBytePrefixTable() {
    // Named constant to satisfy `mnd`: the caret is past the whole string.
    let pastLengthByteCount = 5

    let cases: [BytePrefixCase] = [
      // --- Non-positive byte counts collapse to empty -------------------
      BytePrefixCase(
        desc: "byteCount 0 => empty",
        string: "abc", byteCount: 0, expected: ""),
      BytePrefixCase(
        desc: "negative byteCount => empty",
        string: "abc", byteCount: -1, expected: ""),
      // --- byteCount at/beyond the UTF-8 length returns the whole string -
      BytePrefixCase(
        desc: "byteCount beyond utf8 length => whole string",
        string: "ab", byteCount: pastLengthByteCount, expected: "ab"),
      // --- ASCII mid-string slice ---------------------------------------
      BytePrefixCase(
        desc: "ASCII mid slice: 'hello' first 3 bytes => 'hel'",
        string: "hello", byteCount: 3, expected: "hel"),
      // --- Multi-byte clean boundary ------------------------------------
      BytePrefixCase(
        // Each CJK character is 3 UTF-8 bytes, so 3 bytes lands exactly on
        // the boundary after the first character.
        desc: "multi-byte clean boundary: '日本' first 3 bytes => '日'",
        string: "日本", byteCount: 3, expected: "日"),
    ]

    for c in cases {
      let got = CmdlineSeam.bytePrefix(of: c.string, byteCount: c.byteCount)
      XCTAssertEqual(got, c.expected, "case: \(c.desc)")
    }
  }
}
