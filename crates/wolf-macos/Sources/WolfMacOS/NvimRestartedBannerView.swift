//
//  NvimRestartedBannerView.swift
//  A small, transient, auto-dismissing "Neovim was restarted" banner shown at the
//  top-center of the editor after a successful crash/stall restart.
//
//  DISTINCTION (do not conflate the two overlays):
//    * `NvimStatusOverlayView` is a FULL-BLEED, opaque overlay covering a BROKEN /
//      unusable editor (Neovim crashed `.dead` or stalled `.notResponding`). It is
//      driven by liveness (`Project.onWake` shows/hides it from `handle.liveness()`).
//    * `NvimRestartedBannerView` (this) is a SMALL, non-modal, transient banner
//      shown OVER a WORKING editor to CONFIRM a restart just happened. It is
//      INDEPENDENT of liveness: it is flashed from `Project.restartNeovim`'s success
//      path and auto-dismisses on its own timer, so `onWake`'s Alive branch (which
//      hides the liveness overlay) never clears it.
//
//  Pure AppKit shell (manual smoke only), modeled on `NvimStatusOverlayView` /
//  `EmptyStateView`.
//

import AppKit

/// A small top-center banner that flashes "Neovim was restarted" and auto-hides
/// after [`visibleDuration`]. A subview of the `PaneContainerView`, kept topmost so
/// it composites over the (working) editor; toggles via `isHidden`. Deliberately
/// never first responder.
final class NvimRestartedBannerView: NSView {
  /// The banner message.
  private static let message = "Neovim was restarted"

  /// How long the banner stays visible after a `flash()` before auto-dismissing.
  private static let visibleDuration: TimeInterval = 4.0

  /// Inset of the label from the banner's edges.
  private static let labelPadding: CGFloat = 8

  /// Gap between the top of the superview and the top of the banner.
  private static let topMargin: CGFloat = 12

  private let messageLabel = NSTextField(labelWithString: NvimRestartedBannerView.message)

  /// Monotonic token identifying the LATEST `flash()`. Each `flash()` bumps this
  /// and captures the value; the scheduled hide only fires if the token still
  /// matches — so a newer flash cancels the older pending hide (rapid restarts
  /// coalesce into one banner rather than a premature/double hide).
  private var flashGeneration = 0

  /// Builds the banner (starts hidden; a `flash()` reveals it).
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

    // Pin the label inside the banner with padding; the banner sizes to fit it (no
    // explicit width/height).
    NSLayoutConstraint.activate([
      messageLabel.leadingAnchor.constraint(equalTo: leadingAnchor, constant: Self.labelPadding),
      messageLabel.trailingAnchor.constraint(equalTo: trailingAnchor, constant: -Self.labelPadding),
      messageLabel.topAnchor.constraint(equalTo: topAnchor, constant: Self.labelPadding),
      messageLabel.bottomAnchor.constraint(equalTo: bottomAnchor, constant: -Self.labelPadding),
    ])

    isHidden = true
  }

  @available(*, unavailable)
  required init?(coder: NSCoder) {
    fatalError("NvimRestartedBannerView is not constructed from a nib")
  }

  /// The banner is a PURE PASSTHROUGH: it has no interactive controls, so it must
  /// never intercept a click landing in its small top-center rect while it is
  /// visible (~4s) — the click belongs to the editor pane beneath it. Returning
  /// `nil` for every point routes all hit-testing straight through to the panes.
  override func hitTest(_ point: NSPoint) -> NSView? {
    nil
  }

  /// Pins the banner to the top-center of its superview (the `PaneContainerView`).
  override func viewDidMoveToSuperview() {
    super.viewDidMoveToSuperview()
    guard let superview else { return }
    NSLayoutConstraint.activate([
      centerXAnchor.constraint(equalTo: superview.centerXAnchor),
      topAnchor.constraint(equalTo: superview.topAnchor, constant: Self.topMargin),
    ])
  }

  /// Reveals the banner and schedules its auto-dismiss.
  ///
  /// Bumps [`flashGeneration`] and captures it locally; the delayed hide only runs
  /// if the token is still current, so a newer `flash()` supersedes an older
  /// pending hide (coalescing rapid restarts). `[weak self]` guards teardown.
  func flash() {
    flashGeneration += 1
    let gen = flashGeneration
    isHidden = false
    DispatchQueue.main.asyncAfter(deadline: .now() + Self.visibleDuration) { [weak self] in
      guard let self, self.flashGeneration == gen else { return }
      self.isHidden = true
    }
  }

  /// Re-applies the layer chrome when the effective appearance changes at runtime
  /// (dark↔light). Layer CGColors do NOT auto-adapt, mirroring
  /// `NvimStatusOverlayView`/`EmptyStateView`.
  override func viewDidChangeEffectiveAppearance() {
    super.viewDidChangeEffectiveAppearance()
    applyChromeColors()
  }

  /// Assigns the banner's layer background + border, resolved against this view's
  /// CURRENT effective appearance (a bare `.cgColor` resolves against whatever
  /// appearance is globally current, often stale during a change).
  private func applyChromeColors() {
    effectiveAppearance.performAsCurrentDrawingAppearance {
      layer?.backgroundColor = NSColor.windowBackgroundColor.cgColor
      layer?.borderColor = NSColor.separatorColor.cgColor
      layer?.borderWidth = 1
      layer?.cornerRadius = 8
    }
  }
}
