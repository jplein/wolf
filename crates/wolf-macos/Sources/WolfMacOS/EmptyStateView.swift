// The "no folder open" empty-state overlay.

import AppKit

/// A full-bleed overlay on the editor pane with one centered "Open folder…"
/// button. Fills its superview (the `PaneContainerView`) and toggles via
/// `isHidden`. Deliberately never first responder, so launch keystrokes reach the
/// editor grid rather than the button.
final class EmptyStateView: NSView {
  /// The button's title.
  private static let buttonTitle = "Open folder…"

  /// Invoked when the "Open folder…" button is clicked. The controller wires
  /// this to `presentOpenFolderDialog()`.
  var onOpenFolder: (() -> Void)?

  private let openButton = NSButton()

  /// Builds the overlay (starts hidden; the controller reveals it only for an
  /// empty-state window).
  init() {
    super.init(frame: .zero)
    translatesAutoresizingMaskIntoConstraints = false
    wantsLayer = true
    // Assign the frozen CGColor layer background resolved against the current
    // appearance (re-applied on runtime appearance changes below).
    applyChromeColors()

    openButton.translatesAutoresizingMaskIntoConstraints = false
    openButton.title = Self.buttonTitle
    openButton.bezelStyle = .rounded
    openButton.target = self
    openButton.action = #selector(handleOpenFolder)
    addSubview(openButton)
  }

  @available(*, unavailable)
  required init?(coder: NSCoder) {
    fatalError("EmptyStateView is not constructed from a nib")
  }

  /// Pins the overlay to fill its superview (the editor content view) and centers
  /// the button within it.
  override func viewDidMoveToSuperview() {
    super.viewDidMoveToSuperview()
    guard let superview else { return }
    NSLayoutConstraint.activate([
      leadingAnchor.constraint(equalTo: superview.leadingAnchor),
      trailingAnchor.constraint(equalTo: superview.trailingAnchor),
      topAnchor.constraint(equalTo: superview.topAnchor),
      bottomAnchor.constraint(equalTo: superview.bottomAnchor),
      openButton.centerXAnchor.constraint(equalTo: superview.centerXAnchor),
      openButton.centerYAnchor.constraint(equalTo: superview.centerYAnchor),
    ])
  }

  /// Re-applies the layer background when the effective appearance changes at
  /// runtime (the OS switches dark↔light in `auto` mode, or a forced appearance
  /// propagates from `NSApp.appearance`). Layer CGColors do NOT auto-adapt, so the
  /// background is re-resolved here; the centered button is a stock `NSButton`
  /// whose dynamic colors AppKit re-resolves itself, so no manual redraw is needed.
  override func viewDidChangeEffectiveAppearance() {
    super.viewDidChangeEffectiveAppearance()
    applyChromeColors()
  }

  /// Assigns the overlay's layer background from the dynamic system
  /// `windowBackgroundColor`, resolved against this view's CURRENT effective
  /// appearance. A bare `NSColor.windowBackgroundColor.cgColor` resolves against
  /// whatever appearance is globally current (often stale during an appearance
  /// change), so the conversion runs inside `performAsCurrentDrawingAppearance`.
  private func applyChromeColors() {
    effectiveAppearance.performAsCurrentDrawingAppearance {
      layer?.backgroundColor = NSColor.windowBackgroundColor.cgColor
    }
  }

  /// The button action: forwards to the controller's Open Folder path.
  @objc private func handleOpenFolder() {
    onOpenFolder?()
  }
}
