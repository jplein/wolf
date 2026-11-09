// Pure, AppKit-free crash/stall recovery decisions, ported from wolf-gnome's liveness_ui.

import Foundation

/// The captive Neovim's health as `WolfCoreHandle.liveness()` reports it. The raw
/// values are the discriminants of wolf-ffi's `mod liveness` (which mirrors
/// `wolf_core::Liveness`); Swift cannot import that module, so this is the hand-kept
/// mirror. An unknown discriminant decodes to `nil`.
public enum NvimLiveness: UInt8 {
  /// The captive Neovim answers its RPCs.
  case alive = 0
  /// A synchronous RPC elapsed its budget: Neovim is wedged but may recover.
  case notResponding = 1
  /// The captive Neovim process died.
  case dead = 2
}

/// Maps a liveness reading and the automatic restart count to what the front-end
/// does with a wake. A namespace (`enum` with only static members).
public enum LivenessSeam {
  /// How many automatic restart attempts are made per crash before only the
  /// "Restart Neovim" button is left.
  ///
  /// The cap exists only to stop a retry loop against a respawn that keeps
  /// failing: a failed respawn re-marks the core `Dead` and wakes, which would
  /// otherwise schedule another attempt forever. It does not limit the user,
  /// because the button resets the count.
  public static let maxAutoRestartAttempts: Int = 1

  /// What the full-bleed crash/stall overlay shows.
  public enum Overlay: Equatable {
    /// No overlay: the editor is usable and paints normally.
    case hidden
    /// The captive Neovim died. `retrying` means an automatic attempt is still
    /// permitted by the cap, not that one is running: the count is incremented
    /// when an attempt is scheduled, so a `Dead` reading taken while that attempt
    /// is pending reports `retrying: false`.
    case crashed(retrying: Bool)
    /// The captive Neovim is wedged.
    case stalled
  }

  /// Everything one liveness reading decides.
  public struct Decision: Equatable {
    /// What the overlay shows.
    public let overlay: Overlay
    /// Whether to schedule an automatic restart.
    public let autoRestart: Bool
    /// Whether the current view runs its render wake. The opaque overlay hides
    /// a frozen frame, so repainting it is waste.
    public let render: Bool
    /// Whether to drain the core's queued user commands: true in every state.
    /// See `crates/wolf-macos/Sources/WolfKeyMap/README.md#livenessseamswift`.
    public let drain: Bool
    /// Whether the wake schedules a state commit. Only an alive core's maps are
    /// current; a stalled or dead core's would write stale or empty tabs.
    public let persist: Bool

    /// Builds a decision from its five fields.
    public init(overlay: Overlay, autoRestart: Bool, render: Bool, drain: Bool, persist: Bool) {
      self.overlay = overlay
      self.autoRestart = autoRestart
      self.render = render
      self.drain = drain
      self.persist = persist
    }
  }

  /// Decides what a wake does, given `liveness` and the number of automatic
  /// restart attempts made since the core was last seen `Alive`.
  ///
  /// | liveness | attempts | overlay | autoRestart | render | drain | persist |
  /// | --- | --- | --- | --- | --- | --- | --- |
  /// | `alive` | any | `hidden` | false | true | true | true |
  /// | `notResponding` | any | `stalled` | false | false | true | false |
  /// | `dead` | < max | `crashed(retrying: true)` | true | false | true | false |
  /// | `dead` | ≥ max | `crashed(retrying: false)` | false | false | true | false |
  ///
  /// The `switch` has no `default`, so a new `NvimLiveness` case fails to compile
  /// until it is classified. `notResponding` never auto-restarts: a restart
  /// force-kills Neovim, and anything not yet in a swap file is lost, so that is
  /// the user's call alone. `alive` and `notResponding` ignore the count, so a
  /// spent budget never stops a live editor painting.
  public static func decision(liveness: NvimLiveness, autoRestartAttempts: Int) -> Decision {
    let mayAutoRestart = autoRestartAttempts < maxAutoRestartAttempts
    switch liveness {
    case .alive:
      return Decision(
        overlay: .hidden, autoRestart: false, render: true, drain: true, persist: true)
    case .notResponding:
      return Decision(
        overlay: .stalled, autoRestart: false, render: false, drain: true, persist: false)
    case .dead:
      return Decision(
        overlay: .crashed(retrying: mayAutoRestart), autoRestart: mayAutoRestart,
        render: false, drain: true, persist: false)
    }
  }
}
