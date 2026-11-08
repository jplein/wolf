// The application's run loop entry, called by the one-file WolfMain module.

import AppKit

/// Starts Wolf: the app's whole entry point, kept out of a `main.swift` so the
/// module has no `main` and a test target can link it.
public enum WolfApplication {
  /// Builds the shared application, installs the delegate and runs the event
  /// loop. Returns only when the application terminates.
  public static func run() {
    let app = NSApplication.shared
    let delegate = AppDelegate()
    app.delegate = delegate
    app.setActivationPolicy(.regular)
    // `NSApplication.delegate` is weak; without this an optimized build may release the local.
    withExtendedLifetime(delegate) { app.run() }
  }
}
