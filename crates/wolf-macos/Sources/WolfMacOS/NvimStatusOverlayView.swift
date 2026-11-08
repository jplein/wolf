// The full-bleed overlay shown while the captive Neovim is dead or not responding.

import AppKit

/// The overlay's mode, mirroring the non-`Alive` `wolf_core::Liveness` states.
enum NvimStatusMode {
  /// The captive nvim process died. `retrying` is whether the project may still
  /// make an automatic restart attempt (`LivenessSeam.Overlay.crashed`).
  case dead(retrying: Bool)
  /// The captive nvim stalled (the user drives a manual restart via the button).
  case notResponding
}

/// A full-bleed opaque overlay on the editor pane with a centered status label
/// and a "Restart Neovim" button. Fills its superview (the `PaneContainerView`)
/// and toggles via `isHidden`. Deliberately never first responder.
final class NvimStatusOverlayView: NSView {
  /// The restart button's title.
  private static let buttonTitle = "Restart Neovim"

  /// The status message shown for a crashed captive nvim while an automatic
  /// restart is still to come (`.dead(retrying: true)`).
  private static let deadMessage = "Neovim crashed — restarting…"

  /// The status message shown for a crashed captive nvim once the automatic
  /// attempt is spent and only the button is left (`.dead(retrying: false)`).
  /// wolf-gnome's `CRASHED_FAILED_MESSAGE`, verbatim.
  private static let deadRetryFailedMessage = "Neovim crashed — automatic restart failed"

  /// The status message shown for a stalled (`.notResponding`) captive nvim.
  private static let notRespondingMessage = "Neovim stopped responding"

  /// Invoked when the "Restart Neovim" button is clicked. The controller wires
  /// this to `project.restartNeovim()`.
  var onRestart: (() -> Void)?

  private let messageLabel = NSTextField(labelWithString: "")
  private let restartButton = NSButton()

  /// Builds the overlay (starts hidden; a wake reveals it via `show(mode:)`).
  init() {
    super.init(frame: .zero)
    translatesAutoresizingMaskIntoConstraints = false
    wantsLayer = true
    applyChromeColors()

    messageLabel.translatesAutoresizingMaskIntoConstraints = false
    messageLabel.alignment = .center
    messageLabel.font = NSFont.systemFont(ofSize: NSFont.systemFontSize)
    messageLabel.textColor = .labelColor
    addSubview(messageLabel)

    restartButton.translatesAutoresizingMaskIntoConstraints = false
    restartButton.title = Self.buttonTitle
    restartButton.bezelStyle = .rounded
    restartButton.target = self
    restartButton.action = #selector(handleRestart)
    addSubview(restartButton)

    isHidden = true
  }

  @available(*, unavailable)
  required init?(coder: NSCoder) {
    fatalError("NvimStatusOverlayView is not constructed from a nib")
  }

  /// Pins the overlay to fill its superview and stacks the label above the button.
  override func viewDidMoveToSuperview() {
    super.viewDidMoveToSuperview()
    guard let superview else { return }
    NSLayoutConstraint.activate([
      leadingAnchor.constraint(equalTo: superview.leadingAnchor),
      trailingAnchor.constraint(equalTo: superview.trailingAnchor),
      topAnchor.constraint(equalTo: superview.topAnchor),
      bottomAnchor.constraint(equalTo: superview.bottomAnchor),
      messageLabel.centerXAnchor.constraint(equalTo: superview.centerXAnchor),
      messageLabel.centerYAnchor.constraint(equalTo: superview.centerYAnchor, constant: -20),
      restartButton.centerXAnchor.constraint(equalTo: superview.centerXAnchor),
      restartButton.topAnchor.constraint(equalTo: messageLabel.bottomAnchor, constant: 16),
    ])
  }

  /// Reveals the overlay for `mode`, updating the status message. The restart
  /// button is always present (for `.dead(retrying: true)` it is a fallback while
  /// the project auto-restarts; otherwise it is the primary action).
  func show(mode: NvimStatusMode) {
    switch mode {
    case .dead(retrying: true):
      messageLabel.stringValue = Self.deadMessage
    case .dead(retrying: false):
      messageLabel.stringValue = Self.deadRetryFailedMessage
    case .notResponding:
      messageLabel.stringValue = Self.notRespondingMessage
    }
    isHidden = false
  }

  /// Hides the overlay (liveness returned to `Alive`).
  func hide() {
    isHidden = true
  }

  /// Re-applies the layer background when the effective appearance changes at
  /// runtime (dark↔light). Layer CGColors do NOT auto-adapt, mirroring
  /// `EmptyStateView`.
  override func viewDidChangeEffectiveAppearance() {
    super.viewDidChangeEffectiveAppearance()
    applyChromeColors()
  }

  /// Assigns the overlay's opaque layer background from the dynamic system
  /// `windowBackgroundColor`, resolved against this view's CURRENT effective
  /// appearance (mirrors `EmptyStateView.applyChromeColors`).
  private func applyChromeColors() {
    effectiveAppearance.performAsCurrentDrawingAppearance {
      layer?.backgroundColor = NSColor.windowBackgroundColor.cgColor
    }
  }

  /// The button action: forwards to the project's restart path.
  @objc private func handleRestart() {
    onRestart?()
  }
}
