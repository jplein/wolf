// Pure resolution of a terminal cell's packed style into what the pane paints.

import Foundation

/// The cursor shapes of a terminal frame, mirroring `mod terminal_cursor_style` in
/// `crates/wolf-ffi/src/lib.rs`.
public enum TerminalCursorShape: UInt8 {
  case block = 0
  case bar
  case underline
  case hollow
}

/// What one terminal cell paints: packed `0x00RRGGBB` colours after the defaults, inverse
/// and the selection are applied, and the glyph's face and decorations.
public struct TerminalCellPaint: Equatable {
  public let background: UInt32
  public let foreground: UInt32
  /// The foreground's opacity: ``TerminalCellStyle/faintAlpha`` for faint text, else 1.
  public let foregroundAlpha: Double
  /// `false` for invisible text and for a wide character's spacer cells.
  public let drawsGlyph: Bool
  public let style: GlyphStyle
}

/// Resolves the style a terminal frame packs per cell into a ``TerminalCellPaint``.
public enum TerminalCellStyle {
  /// The `FfiTerminalCell.flags()` bits, mirroring `mod terminal_flag`.
  public enum Flag {
    public static let bold: UInt8 = 1 << 0
    public static let faint: UInt8 = 1 << 1
    public static let italic: UInt8 = 1 << 2
    public static let strikethrough: UInt8 = 1 << 3
    public static let inverse: UInt8 = 1 << 4
    public static let invisible: UInt8 = 1 << 5
    public static let blink: UInt8 = 1 << 6
  }

  /// The `FfiTerminalCell.underline()` values, mirroring `mod terminal_underline`.
  public enum Underline {
    public static let none: UInt8 = 0
    public static let single: UInt8 = 1
    public static let double: UInt8 = 2
    public static let curly: UInt8 = 3
    public static let dotted: UInt8 = 4
    public static let dashed: UInt8 = 5
  }

  /// The `FfiTerminalCell.width()` values, mirroring `mod terminal_cell_width`.
  public enum Width {
    public static let narrow: UInt8 = 0
    public static let wide: UInt8 = 1
    public static let spacerTail: UInt8 = 2
    public static let spacerHead: UInt8 = 3
  }

  /// The opacity faint text draws at.
  public static let faintAlpha: Double = 0.5

  /// Resolves one cell. An unset colour takes its default, and the colours swap when
  /// exactly one of `inverse` and `selected` holds, so selecting reverse video shows it
  /// plain. A curly underline is the wave and every other underline the straight line;
  /// blink is ignored, so text draws steady.
  public static func resolve(
    fg: FfiColor, bg: FfiColor, flags: UInt8, underline: UInt8, width: UInt8, selected: Bool,
    defaultFg: FfiColor, defaultBg: FfiColor
  ) -> TerminalCellPaint {
    var foreground = fg.set ? fg.rgb : defaultFg.rgb
    var background = bg.set ? bg.rgb : defaultBg.rgb
    if (flags & Flag.inverse != 0) != selected {
      swap(&foreground, &background)
    }
    let drawsGlyph =
      flags & Flag.invisible == 0 && width != Width.spacerTail && width != Width.spacerHead
    let bold = flags & Flag.bold != 0
    let italic = flags & Flag.italic != 0
    let face: GlyphFace =
      switch (bold, italic) {
      case (true, true): .boldItalic
      case (true, false): .bold
      case (false, true): .italic
      case (false, false): .regular
      }
    let line: GlyphUnderline? =
      switch underline {
      case Underline.none: nil
      case Underline.curly: .wave
      default: .straight
      }
    let decoration = FfiColor(rgb: foreground, set: true)
    return TerminalCellPaint(
      background: background,
      foreground: foreground,
      foregroundAlpha: flags & Flag.faint != 0 ? faintAlpha : 1,
      drawsGlyph: drawsGlyph,
      style: GlyphStyle(
        face: face, underline: line, underlineColor: decoration,
        strikethrough: flags & Flag.strikethrough != 0, strikethroughColor: decoration))
  }

  /// The cursor's fill, the cursor colour when set and else the default foreground, and
  /// the colour of the glyph a block cursor covers, the default background.
  public static func cursorColors(
    cursorColor: FfiColor, defaultFg: FfiColor, defaultBg: FfiColor
  ) -> (fill: UInt32, glyph: UInt32) {
    (cursorColor.set ? cursorColor.rgb : defaultFg.rgb, defaultBg.rgb)
  }

  /// The shape a pane paints its cursor in: the frame's `style` while the pane is focused,
  /// an unknown style as a block, and a hollow block in every other pane whatever the
  /// program set.
  public static func cursorShape(style: UInt8, focused: Bool) -> TerminalCursorShape {
    focused ? (TerminalCursorShape(rawValue: style) ?? .block) : .hollow
  }

  /// How many cells the cursor covers: both cells of a wide character, else one.
  public static func cursorSpan(width: UInt8) -> Int {
    width == Width.wide ? 2 : 1
  }
}
