//
//  FontRegistration.swift
//  Registers the bundled font TTFs with CoreText (process scope). The app bundles
//  two families in Contents/Resources/fonts/: JetBrains Mono and iA Writer Mono S
//  (the default grid font).
//

import CoreText
import Foundation

/// Registers the vendored fonts shipped at `Contents/Resources/fonts/*.ttf` so
/// CoreText resolves their families — both bundled families, "iA Writer Mono S"
/// (the default grid font) and "JetBrains Mono". Registration is PROCESS-scoped
/// (not installed system-wide) and
/// LOG-NOT-FAIL: a missing fonts dir (developer/`swift run` paths) or an
/// already-registered face is logged and skipped, never fatal.
enum FontRegistration {
  /// Registers every `.ttf` under the bundle's `fonts/` resource directory with
  /// CoreText for this process. A no-op (with a log line) when the directory is
  /// absent or holds no TTFs.
  static func registerBundledFonts() {
    guard let resourcePath = Bundle.main.resourcePath else {
      NSLog("wolf: no bundle resourcePath; skipping bundled font registration")
      return
    }
    let fontsDir = URL(fileURLWithPath: resourcePath).appendingPathComponent("fonts")

    let manager = FileManager.default
    var isDir: ObjCBool = false
    guard manager.fileExists(atPath: fontsDir.path, isDirectory: &isDir), isDir.boolValue
    else {
      NSLog("wolf: bundled fonts dir absent at \(fontsDir.path); skipping registration")
      return
    }

    let ttfURLs: [URL]
    do {
      ttfURLs = try manager.contentsOfDirectory(
        at: fontsDir, includingPropertiesForKeys: nil
      )
      .filter { $0.pathExtension.lowercased() == "ttf" }
    } catch {
      NSLog("wolf: could not enumerate \(fontsDir.path): \(error); skipping registration")
      return
    }

    guard !ttfURLs.isEmpty else {
      NSLog("wolf: no .ttf files under \(fontsDir.path); skipping registration")
      return
    }

    CTFontManagerRegisterFontURLs(ttfURLs as CFArray, .process, true) { errors, _ in
      let cfErrors = errors as? [CFError] ?? []
      if !cfErrors.isEmpty {
        // Log-not-fail: a face already registered (e.g. after a relaunch)
        // surfaces here and is non-fatal.
        NSLog("wolf: some bundled fonts failed to register: \(cfErrors)")
      }
      return true
    }
  }
}
