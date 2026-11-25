// Pure placement of a preview's web view in its pane, and the root style that caps its page.

import Foundation

/// Where a Markdown preview's web view sits in its pane, and the inline style on the page's
/// root element `<html>` that caps and centres the content inside it.
///
/// The web view spans its pane below the statusbar strip, so the page's scrollbar sits at the
/// pane's edge and the margins scroll as the page. The column cap is the page's: a `max-width`
/// with auto side margins on the root, never the view's frame. The style goes on `<html>`,
/// never `<body>`, because the preview stylesheet owns the body's box.
public enum PreviewWidthSeam {
  /// A preview web view's frame in its container, and how far it stops short of its pane.
  public struct Placement: Equatable {
    /// The web view's frame, in the container's flipped coordinates.
    public let frame: Rect
    /// How far the frame stops short of the pane on each side, both sides alike.
    public let sideInset: Double
    /// How far the frame stops short of the pane's bottom.
    public let bottomInset: Double

    public init(frame: Rect, sideInset: Double, bottomInset: Double) {
      self.frame = frame
      self.sideInset = sideInset
      self.bottomInset = bottomInset
    }
  }

  /// The web view's placement in `pane`, within the pane area `area`.
  ///
  /// The frame starts below the `statusbarHeight` strip. A side or the bottom that borders an
  /// interior divider stops `dividerBand` short of it, so the container's divider line and its
  /// drag stay uncovered. The side inset applies to both sides whenever either borders a
  /// divider, so the page centres its content where an editor centres its grid. The top is
  /// never inset: the strip covers the band there.
  public static func placement(
    pane: Rect, area: Rect, statusbarHeight: Double, dividerBand: Double
  ) -> Placement {
    let sideBorders = pane.x > area.x + 0.5 || pane.x + pane.w < area.x + area.w - 0.5
    let bottomBorders = pane.y + pane.h < area.y + area.h - 0.5
    let side = sideBorders ? dividerBand : 0
    let bottom = bottomBorders ? dividerBand : 0
    return Placement(
      frame: Rect(
        x: pane.x + side, y: pane.y + statusbarHeight, w: max(0, pane.w - 2 * side),
        h: max(0, pane.h - statusbarHeight - bottom)),
      sideInset: side, bottomInset: bottom)
  }

  /// The inline style of the page's root element, as `name:value` declarations joined by `;`.
  ///
  /// It pads the page by `padding`, less the placement's insets, so the content box sits where
  /// an editor's grid rect does. With a `maxCols` cap and a positive `cellWidth`, the cases
  /// `PaneGeometry` caps, it adds a `max-width` of the capped columns plus the side padding,
  /// centred by auto side margins; `nil` is a full-width source.
  public static func rootStyle(
    maxCols: Int?, cellWidth: Double, padding: Double, placement: Placement
  ) -> String {
    let side = max(0, padding - placement.sideInset)
    let bottom = max(0, padding - placement.bottomInset)
    var declarations = [
      "box-sizing:border-box",
      "padding-top:\(length(max(0, padding)))",
      "padding-right:\(length(side))",
      "padding-bottom:\(length(bottom))",
      "padding-left:\(length(side))",
    ]
    if let maxCols, maxCols > 0, cellWidth > 0 {
      declarations.append("max-width:\(length(Double(maxCols) * cellWidth + 2 * side))")
      declarations.append("margin-left:auto")
      declarations.append("margin-right:auto")
    }
    return declarations.joined(separator: ";")
  }

  /// `html` with ` style="<rootStyle>"` inserted directly after its first `<html`, which must
  /// precede `<head>`; `&` and `"` are escaped.
  ///
  /// Everything from `<head>` on is untouched, so the head stays byte-identical across renders.
  /// `html` comes back unchanged when `rootStyle` is empty or no `<html` precedes `<head>`.
  public static func styledDocument(_ html: String, rootStyle: String) -> String {
    guard !rootStyle.isEmpty, let head = html.range(of: "<head>") else { return html }
    var search = html.startIndex..<head.lowerBound
    while let open = html.range(of: "<html", range: search) {
      let next = html[open.upperBound]
      if next == ">" || next.isWhitespace {
        let value = rootStyle.replacingOccurrences(of: "&", with: "&amp;")
          .replacingOccurrences(of: "\"", with: "&quot;")
        var out = html
        out.insert(contentsOf: " style=\"\(value)\"", at: open.upperBound)
        return out
      }
      search = open.upperBound..<head.lowerBound
    }
    return html
  }

  /// A script that sets the page's root element's `style` attribute to exactly `rootStyle`.
  public static func applyRootStyleJS(_ rootStyle: String) -> String {
    "document.documentElement.setAttribute(\"style\",\(PreviewLinkSeam.jsonEscape(rootStyle)));"
  }

  /// A CSS pixel length: a whole number bare, any other in Swift's shortest round-trip form,
  /// which never takes the locale's decimal separator.
  private static func length(_ value: Double) -> String {
    guard value.isFinite, abs(value) >= 1e-4 else { return "0px" }
    if value == value.rounded(), abs(value) < 1e15 { return "\(Int(value))px" }
    return "\(value)px"
  }
}
