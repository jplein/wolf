// Pure line-composition and caret-prefix math for the `ext_cmdline` overlay.

import Foundation

/// The pure display functions backing the macOS command-line overlay. A
/// namespace (`enum` with only static members) mirroring `PaletteSeam`.
public enum CmdlineSeam {
  /// The full line painted by `CmdlineView`: the `prompt` (e.g. a `confirm()`
  /// answer line `"[A], (B): "`), followed by the `firstc` prefix (`:` / `/` /
  /// `?`), followed by the command text. An empty `prompt` yields the same
  /// `firstc + text` as before this seam existed.
  public static func composedLine(prompt: String, firstc: String, text: String) -> String {
    prompt + firstc + text
  }

  /// The prefix whose measured width places the caret: everything up to the
  /// reported byte offset — the `prompt`, the `firstc`, then the `byteCount`-byte
  /// prefix of `text`. The prompt width is included so the caret sits after it.
  public static func caretPrefix(prompt: String, firstc: String, text: String, byteCount: Int)
    -> String
  {
    prompt + firstc + bytePrefix(of: text, byteCount: byteCount)
  }

  /// The UTF-8 prefix of `s` of at most `byteCount` bytes, decoded back to a
  /// `String` (lossily replacing a split multi-byte boundary). Used to place the
  /// caret at the reported byte offset.
  public static func bytePrefix(of s: String, byteCount: Int) -> String {
    if byteCount <= 0 { return "" }
    let bytes = Array(s.utf8)
    if byteCount >= bytes.count { return s }
    return String(decoding: bytes.prefix(byteCount), as: UTF8.self)
  }
}
