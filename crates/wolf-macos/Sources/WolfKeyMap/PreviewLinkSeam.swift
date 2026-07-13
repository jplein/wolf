//
//  PreviewLinkSeam.swift
//  Pure, AppKit-free Markdown-preview link-handling seam.
//
//  The sibling of `PreviewSeam` / `CommandSeam`: a pure, display-free decision
//  exercised with scalar inputs only — NO AppKit, NO WKWebView, NO live Neovim. It
//  classifies a link the user clicked in the preview web view (a resolved URL plus
//  the previewed source file's path) into one of three actions, and emits the JS
//  used to scroll a fragment into view. ALL link-routing branching lives here so it
//  is headlessly unit-testable; the `PreviewNavigationDelegate` glue that calls
//  `NSWorkspace.open` / `evaluateJavaScript` stays thin (compile + manual-smoke).
//
//  Imports ONLY Foundation, so it lives in the Foundation-only `WolfKeyMap` module
//  alongside `PreviewSeam.swift` / `CommandSeam.swift`.
//

import Foundation

/// Pure Markdown-preview link classification + scroll-JS synthesis.
public enum PreviewLinkSeam {
  /// What to do with a clicked preview link.
  public enum Action: Equatable {
    /// A non-`file` scheme (http/https/mailto/tel/ftp/…): hand it to the OS
    /// default handler (the browser / mail client), NOT the preview web view.
    case openInBrowser(URL)
    /// A same-file `#anchor`: scroll the current preview to `fragment`.
    case scrollToFragment(String)
    /// A `file://` link to preview (`path` is ABSOLUTE): repoint the pane at
    /// `path`; when `fragment` is non-nil, scroll to it after the load.
    case previewFile(path: String, fragment: String?)
  }

  /// Classifies `url` (WebKit's resolved link target) against the previewed
  /// `sourcePath` (the abs path of the file currently shown in the pane).
  ///
  /// Rules (plan §A):
  ///  - a scheme other than `file` (and non-nil) — `http`/`https`/`mailto`/`tel`/
  ///    `ftp`/… — → `.openInBrowser(url)`.
  ///  - `file` scheme: `frag` is `url.fragment` (Foundation returns it
  ///    percent-decoded; an empty fragment is treated as none). The link's path is
  ///    compared to `sourcePath` via `standardizedFileURL.path` (so `/a/./x.md`
  ///    normalizes to `/a/x.md`):
  ///     - same file + fragment → `.scrollToFragment(frag)`,
  ///     - same file + no fragment → `.previewFile(path: url.path, fragment: nil)`,
  ///     - different file → `.previewFile(path: url.path, fragment: frag)`.
  ///  - a nil scheme (defensive): treat a path-bearing URL as a file to preview,
  ///    else `.openInBrowser(url)`.
  public static func classify(url: URL, sourcePath: String) -> Action {
    guard let scheme = url.scheme, !scheme.isEmpty else {
      // Defensive: no scheme. Preview if it names a path, else hand off.
      if !url.path.isEmpty {
        return .previewFile(path: url.path, fragment: normalizedFragment(url.fragment))
      }
      return .openInBrowser(url)
    }

    if scheme != "file" {
      return .openInBrowser(url)
    }

    let frag = normalizedFragment(url.fragment)
    let sourceStandardized = URL(fileURLWithPath: sourcePath).standardizedFileURL.path
    let linkStandardized = url.standardizedFileURL.path
    let sameFile = sourceStandardized == linkStandardized

    if sameFile {
      if let frag {
        return .scrollToFragment(frag)
      }
      return .previewFile(path: url.path, fragment: nil)
    }
    return .previewFile(path: url.path, fragment: frag)
  }

  /// A self-invoking JS snippet that finds `#<fragmentId>` and scrolls it into
  /// view (top-aligned, honoring the preview CSS `scroll-margin-top`). `fragmentId`
  /// is JSON-escaped with a hand-written escaper (Foundation-only; no
  /// `JSONEncoder`) covering the characters that would break a JS string literal or
  /// let markup escape into the surrounding document.
  public static func scrollIntoViewJS(fragmentId: String) -> String {
    let escaped = jsonEscape(fragmentId)
    return
      "(function(){var e=document.getElementById(\(escaped));if(e){e.scrollIntoView(true);}})();"
  }

  /// A percent-decoded, empty-collapsed fragment: `nil` when `raw` is nil or the
  /// empty string, else the fragment itself.
  private static func normalizedFragment(_ raw: String?) -> String? {
    guard let raw, !raw.isEmpty else { return nil }
    return raw
  }

  /// JSON-escapes `s` into a double-quoted JS/JSON string literal, covering `\`,
  /// `"`, newline, carriage return, tab, `<`, and `/` (the last two so an embedded
  /// `</script>` or `<` can never break out of the JS context). Any other control
  /// character below U+0020 is emitted as a `\u00XX` escape.
  private static func jsonEscape(_ s: String) -> String {
    var out = "\""
    for scalar in s.unicodeScalars {
      switch scalar {
      case "\\":
        out += "\\\\"
      case "\"":
        out += "\\\""
      case "\n":
        out += "\\n"
      case "\r":
        out += "\\r"
      case "\t":
        out += "\\t"
      case "<":
        out += "\\u003C"
      case "/":
        out += "\\/"
      default:
        if scalar.value < 0x20 {
          out += String(format: "\\u%04X", scalar.value)
        } else {
          out.unicodeScalars.append(scalar)
        }
      }
    }
    out += "\""
    return out
  }
}
