//
//  PreviewLinkSeamTests.swift
//  Pure, headless unit tests for the AppKit-free `WolfKeyMap.PreviewLinkSeam`.
//
//  These exercise the pure link-classification and JS-escaping the coder adds in
//  `Sources/WolfKeyMap/PreviewLinkSeam.swift` (plan §A). Like `PreviewSeamTests`,
//  they import ONLY Foundation-level test machinery (no AppKit, no WKWebView, no
//  live Neovim) and stay in the headless `WolfKeyMap` module. The nav-delegate
//  routing, `NSWorkspace.open`, and `evaluateJavaScript` are AppKit glue
//  (compile + manual-smoke only); ALL branching lives here and is unit-tested.
//
//  ## CONTRACT UNDER TEST (the API the coder must honor)
//
//    public enum PreviewLinkSeam {
//        public enum Action: Equatable {
//            case openInBrowser(URL)
//            case scrollToFragment(String)
//            case previewFile(path: String, fragment: String?)
//        }
//        static func classify(url: URL, sourcePath: String) -> Action
//        static func scrollIntoViewJS(fragmentId: String) -> String
//    }
//
//  classify rules (plan §A):
//    * non-`file` scheme (http/https/mailto/...) -> `.openInBrowser(url)`.
//    * `file` scheme, SAME path as source + fragment -> `.scrollToFragment(frag)`.
//    * `file` scheme, SAME path + no fragment -> `.previewFile(url.path, nil)`.
//    * `file` scheme, DIFFERENT path -> `.previewFile(url.path, frag?)`.
//    * path comparison is via `standardizedFileURL.path`, so `/a/./x.md`
//      normalizes to `/a/x.md`.
//

import XCTest

@testable import WolfKeyMap

final class PreviewLinkSeamTests: XCTestCase {

  // MARK: - external http(s) links open in the browser

  func testExternalHttpOpensBrowser() {
    let url = URL(string: "https://example.com/page")!
    XCTAssertEqual(
      PreviewLinkSeam.classify(url: url, sourcePath: "/a/x.md"),
      .openInBrowser(url),
      "an http(s) link must open in the default browser, not the preview webview")
  }

  // MARK: - mailto links open in the browser (default handler)

  func testMailtoOpensBrowser() {
    let url = URL(string: "mailto:someone@example.com")!
    XCTAssertEqual(
      PreviewLinkSeam.classify(url: url, sourcePath: "/a/x.md"),
      .openInBrowser(url),
      "a mailto link is a non-file scheme and must be handed to the OS default handler")
  }

  // MARK: - a same-file fragment scrolls (never previews)

  func testSameFileFragmentScrolls() {
    let url = URL(string: "file:///a/x.md#sec")!
    XCTAssertEqual(
      PreviewLinkSeam.classify(url: url, sourcePath: "/a/x.md"),
      .scrollToFragment("sec"),
      "a #anchor into the SAME file scrolls to that fragment")
  }

  // MARK: - a cross-file link WITH a fragment previews the file + carries the fragment

  func testCrossFileWithFragmentPreviews() {
    let url = URL(string: "file:///a/y.md#sec")!
    XCTAssertEqual(
      PreviewLinkSeam.classify(url: url, sourcePath: "/a/x.md"),
      .previewFile(path: "/a/y.md", fragment: "sec"),
      "a link to a DIFFERENT file with a #anchor previews that file and keeps the fragment")
  }

  // MARK: - a cross-file link WITHOUT a fragment previews the file (nil fragment)

  func testCrossFileNoFragmentPreviews() {
    let url = URL(string: "file:///a/y.md")!
    XCTAssertEqual(
      PreviewLinkSeam.classify(url: url, sourcePath: "/a/x.md"),
      .previewFile(path: "/a/y.md", fragment: nil),
      "a link to a DIFFERENT file with no anchor previews that file with a nil fragment")
  }

  // MARK: - a same-file link with NO fragment re-previews the same file (nil fragment)

  func testSameFileNoFragment() {
    let url = URL(string: "file:///a/x.md")!
    XCTAssertEqual(
      PreviewLinkSeam.classify(url: url, sourcePath: "/a/x.md"),
      .previewFile(path: "/a/x.md", fragment: nil),
      "a same-file link with no fragment previews the file itself (nil fragment)")
  }

  // MARK: - path normalization: /a/./x.md and /a/x.md are the SAME file

  func testPathNormalization() {
    let url = URL(string: "file:///a/x.md#sec")!
    XCTAssertEqual(
      PreviewLinkSeam.classify(url: url, sourcePath: "/a/./x.md"),
      .scrollToFragment("sec"),
      "the source path is standardized (/a/./x.md == /a/x.md), so this is a same-file scroll")
  }

  // MARK: - scrollIntoViewJS JSON-escapes the fragment id

  func testScrollIntoViewJSEscapes() {
    let js = PreviewLinkSeam.scrollIntoViewJS(fragmentId: "a\"b")
    XCTAssertTrue(
      js.contains("\\\""),
      "a double-quote in the fragment id must be JSON-escaped to \\\" in the emitted JS: \(js)")
    XCTAssertTrue(
      js.contains("scrollIntoView"),
      "the emitted JS must call scrollIntoView: \(js)")
  }
}
