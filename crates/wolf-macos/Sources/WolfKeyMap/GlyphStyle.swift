//
//  GlyphStyle.swift
//  Pure, AppKit-/CoreText-free resolution of a cell's packed style-flag bits +
//  its fg / special colors into a paint-ready `GlyphStyle`.
//
//  This is the headless-testable seam (plan §B1) the renderer paints from: it
//  maps `FfiCell.flags()` (bold/italic/underline/undercurl/strikethrough bits,
//  mirroring `mod flag` in `crates/wolf-ffi/src/lib.rs`) plus the cell's
//  foreground and special (underline) colors into a `GlyphStyle` value. It
//  imports ONLY Foundation (the generated `FfiColor` glue rides in the same
//  `WolfKeyMap` module), so it stays AppKit-/CoreText-free and unit-testable
//  without a live font. On-screen font-variant selection + decoration stroking
//  are the renderer's job (`CellMetrics` / `PaneContainerView`).
//

import Foundation

/// Extends the generated (transparent) `FfiColor` struct with component-wise
/// equality so `GlyphStyle` can synthesize `Equatable`. swift-bridge does NOT
/// emit an `Equatable` conformance for transparent structs.
extension FfiColor: Equatable {
    public static func == (lhs: FfiColor, rhs: FfiColor) -> Bool {
        lhs.rgb == rhs.rgb && lhs.set == rhs.set
    }
}

/// The concrete font face a cell's glyph is painted with, derived from its
/// bold/italic style bits.
public enum GlyphFace: Equatable {
    /// The base (upright, non-bold) face.
    case regular
    /// The bold face.
    case bold
    /// The italic (oblique) face.
    case italic
    /// The combined bold + italic face.
    case boldItalic
}

/// A paint-ready resolution of one cell's text attributes: which face to draw
/// the glyph with, plus the underline / strikethrough decorations and the colors
/// they are stroked in. Decorations are stroked MANUALLY by the renderer (in
/// these colors) because `CTLineDraw` ignores AppKit decoration-color attributes.
public struct GlyphStyle: Equatable {
    /// The font face to draw the glyph with.
    public let face: GlyphFace
    /// Whether to stroke an underline (straight; undercurl folds in this pass).
    public let underline: Bool
    /// The color the underline is stroked in (special when set, else fg).
    public let underlineColor: FfiColor
    /// Whether to stroke a strikethrough.
    public let strikethrough: Bool
    /// The color the strikethrough is stroked in (always fg).
    public let strikethroughColor: FfiColor

    /// Memberwise initializer (exposed so the renderer can build a plain
    /// `.regular` style for the cursor glyph).
    public init(
        face: GlyphFace, underline: Bool, underlineColor: FfiColor,
        strikethrough: Bool, strikethroughColor: FfiColor
    ) {
        self.face = face
        self.underline = underline
        self.underlineColor = underlineColor
        self.strikethrough = strikethrough
        self.strikethroughColor = strikethroughColor
    }
}

/// Resolves a cell's packed style flags + colors into a `GlyphStyle`. Pure and
/// deterministic — the single mapping point the renderer and its unit tests
/// share.
public enum GlyphStyleResolver {
    // Style-flag bits, MIRRORING `mod flag` in `crates/wolf-ffi/src/lib.rs` (the
    // single source of truth — keep in lock-step).
    private static let bold: UInt8 = 1 << 0
    private static let italic: UInt8 = 1 << 1
    private static let underline: UInt8 = 1 << 2
    private static let undercurl: UInt8 = 1 << 3
    private static let strikethrough: UInt8 = 1 << 4

    /// Maps a cell's `flags` (from `FfiCell.flags()`) plus its `fg` / `special`
    /// colors into a paint-ready `GlyphStyle`:
    ///   - face = boldItalic if BOTH bold+italic bits, else bold, else italic,
    ///     else regular.
    ///   - underline = underline-bit OR undercurl-bit (undercurl → straight
    ///     underline this pass).
    ///   - underlineColor = special when `special.set`, else fg.
    ///   - strikethrough = strikethrough-bit; strikethroughColor = fg.
    public static func resolve(flags: UInt8, fg: FfiColor, special: FfiColor)
        -> GlyphStyle
    {
        let isBold = flags & bold != 0
        let isItalic = flags & italic != 0
        let face: GlyphFace
        if isBold && isItalic {
            face = .boldItalic
        } else if isBold {
            face = .bold
        } else if isItalic {
            face = .italic
        } else {
            face = .regular
        }

        let hasUnderline = flags & underline != 0 || flags & undercurl != 0
        let underlineColor = special.set ? special : fg
        let hasStrikethrough = flags & strikethrough != 0

        return GlyphStyle(
            face: face,
            underline: hasUnderline,
            underlineColor: underlineColor,
            strikethrough: hasStrikethrough,
            strikethroughColor: fg
        )
    }
}
