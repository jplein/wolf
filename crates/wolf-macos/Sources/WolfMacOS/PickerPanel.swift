// The borderless child window that hosts one picker overlay above its main window.

import AppKit

/// A borderless, non-activating floating panel that can become key so its hosted
/// picker's search field types. It never becomes MAIN (it is transient chrome over
/// the parent window). Positioned + ordered by `PickerPanelHost`.
final class PickerPanel: NSPanel {
  /// A borderless panel is not key-eligible by default; override so the hosted
  /// search field's field editor can accept keystrokes.
  override var canBecomeKey: Bool { true }

  /// The panel is transient chrome over the parent window and never becomes main.
  override var canBecomeMain: Bool { false }

  /// Builds an empty, clear, borderless non-activating floating panel with a
  /// shadow. The host assigns `contentView`, becomes the delegate, sizes it via
  /// `PickerPanelSeam`, and orders it in front of the parent.
  init() {
    super.init(
      contentRect: NSRect(x: 0, y: 0, width: 600, height: 400),
      styleMask: [.borderless, .nonactivatingPanel],
      backing: .buffered,
      defer: false)
    isFloatingPanel = true
    becomesKeyOnlyIfNeeded = false
    hidesOnDeactivate = false
    isOpaque = false
    backgroundColor = .clear
    hasShadow = true
  }

  @available(*, unavailable)
  required init?(coder: NSCoder) {
    fatalError("PickerPanel is not constructed from a nib")
  }
}
