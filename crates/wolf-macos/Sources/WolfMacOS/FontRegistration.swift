// Registers the bundled font files with CoreText at process scope.

import CoreText
import Foundation

/// Registers the vendored fonts shipped at `Contents/Resources/fonts/*.ttf` so
/// CoreText resolves their families — "iA Writer Mono S" (the default grid font),
/// "JetBrains Mono", and `symbolsFamily`, the face for Private Use Area icons,
/// which is never a configured family: `CellMetrics` cascades it behind every grid
/// face. Registration is PROCESS-scoped (not installed system-wide) and
/// LOG-NOT-FAIL: a missing fonts dir (developer/`swift run` paths) or an
/// already-registered face is logged and skipped, never fatal.
enum FontRegistration {
  /// The family of the bundled icon face, "Symbols Nerd Font Mono", which
  /// `CellMetrics` puts in every grid face's cascade list, behind the face itself
  /// and ahead of CoreText's system fallback.
  static let symbolsFamily = "Symbols Nerd Font Mono"

  /// Registers every `.ttf` under the bundle's `fonts/` resource directory with
  /// CoreText for this process. A no-op (with a log line) when the directory is
  /// absent or holds no TTFs.
  static func registerBundledFonts() {
    guard let resourcePath = Bundle.main.resourcePath else {
      NSLog("wolf: no bundle resourcePath; skipping bundled font registration")
      return
    }
    registerFonts(in: URL(fileURLWithPath: resourcePath).appendingPathComponent("fonts"))
  }

  /// Registers every `.ttf` directly under `fontsDir` with CoreText for this
  /// process, under the same log-not-fail rule as `registerBundledFonts()`. The
  /// test binary calls it on the font trees Bazel stages.
  static func registerFonts(in fontsDir: URL) {
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
