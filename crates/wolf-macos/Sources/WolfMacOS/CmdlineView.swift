// The native command line (`ext_cmdline`) overlay.

import AppKit
import CoreText
import WolfFFI
import WolfKeyMap

/// The native command-line overlay. Paints one row: the `prompt` (painted inline
/// for `confirm()` / `input()`-style prompts, empty otherwise), then the `firstc`
/// prefix (`:` / `/` / `?`), immediately followed by the command text, with a thin
/// caret at the byte position `pos`. Hidden (via `isHidden`) whenever the command
/// line is not active.
final class CmdlineView: NSView {
  /// Horizontal inset from the view's left edge, matching the editor grid's
  /// per-pane padding so the command line aligns with the grid text.
  private static let horizontalPadding: CGFloat = CGFloat(EditorInset.padding)
  /// Caret width, in points.
  private static let caretWidth: CGFloat = 2

  private var metrics: CellMetrics

  // -- Latest projected state (set by `update`) -----------------------------
  /// The prompt line (e.g. a `confirm()` answer line), painted before `firstc`.
  private var prompt: String = ""
  private var firstc: String = ""
  private var text: String = ""
  private var pos: Int = 0
  private var fgColor: CGColor
  private var bgColor: CGColor

  /// Builds the overlay against the measured `metrics`. Starts hidden (no
  /// command line active yet).
  init(metrics: CellMetrics) {
    self.metrics = metrics
    self.fgColor = WolfColor.fallbackFg
    self.bgColor = WolfColor.fallbackBg
    super.init(frame: .zero)
    wantsLayer = true
    isHidden = true
  }

  @available(*, unavailable)
  required init?(coder: NSCoder) {
    fatalError("CmdlineView is not constructed from a nib")
  }

  /// Top-left origin so text lays out upright in a flipped context.
  override var isFlipped: Bool { true }

  /// Re-measures against `newMetrics` (a per-window font-size change).
  func applyMetrics(_ newMetrics: CellMetrics) {
    metrics = newMetrics
    needsDisplay = true
  }

  /// The preferred height (one text row) for the overlay's frame.
  var preferredHeight: CGFloat { ceil(metrics.cellHeight) }

  /// Projects the latest `FfiCmdline` into the view: stores the prompt / firstc /
  /// text / caret position and the default fg/bg colors, toggles visibility on
  /// `active()`, and requests a repaint. Returns whether the command line is
  /// active (so the container can lay the overlay out or hide it).
  @discardableResult
  func update(_ cmdline: FfiCmdline, defaultFg: CGColor, defaultBg: CGColor) -> Bool {
    let active = cmdline.active()
    firstc = cmdline.firstc().toString()
    prompt = cmdline.prompt().toString()
    text = cmdline.text().toString()
    pos = Int(cmdline.pos())
    fgColor = defaultFg
    bgColor = defaultBg
    isHidden = !active
    needsDisplay = true
    return active
  }

  override func draw(_ dirtyRect: NSRect) {
    guard let ctx = NSGraphicsContext.current?.cgContext else { return }
    ctx.setShouldAntialias(true)
    ctx.setShouldSmoothFonts(false)

    // Background band.
    ctx.setFillColor(bgColor)
    ctx.fill(bounds)

    let font = overlayFont(metrics: metrics)
    let attrs: [NSAttributedString.Key: Any] = [
      .font: font,
      .foregroundColor: NSColor(cgColor: fgColor) ?? .textColor,
    ]

    let line = CmdlineSeam.composedLine(prompt: prompt, firstc: firstc, text: text)
    let baselineY = bounds.minY
    let textOrigin = CGPoint(x: Self.horizontalPadding, y: baselineY)
    (line as NSString).draw(at: textOrigin, withAttributes: attrs)

    // Caret: measure the width of everything up to the byte offset `pos`
    // (plus the prompt and firstc prefixes) and stroke a thin bar there.
    let prefix = CmdlineSeam.caretPrefix(prompt: prompt, firstc: firstc, text: text, byteCount: pos)
    let caretX = Self.horizontalPadding + (prefix as NSString).size(withAttributes: attrs).width
    ctx.setFillColor(fgColor)
    ctx.fill(
      CGRect(x: caretX, y: bounds.minY, width: Self.caretWidth, height: ceil(metrics.cellHeight)))
  }
}

/// Builds an `NSFont` matching the grid's `CellMetrics` (family + size), for the
/// cmdline / completion-popup / toast overlays' `NSString`-based text drawing.
/// Falls back to the monospaced system font when the family cannot be resolved.
func overlayFont(metrics: CellMetrics) -> NSFont {
  let size = CTFontGetSize(metrics.font)
  let family = CTFontCopyFamilyName(metrics.font) as String
  return NSFont(name: family, size: size)
    ?? .monospacedSystemFont(ofSize: size, weight: .regular)
}
