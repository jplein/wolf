// One tab's captive Neovim: its core, its wakes, and its crash restart.

import AppKit
import Foundation
import WolfFFI
import WolfKeyMap

/// What a `TabCore` asks of the tab it serves. `WolfWindowController` conforms; a live test
/// stands a recording view in for it, since a window controller cannot be built offscreen.
protocol TabCoreView: AnyObject {
  /// Paints the tab from the core's latest frame.
  func onRenderWake()
  /// Drains and dispatches the core's queued user commands.
  func drainAndDispatchCommands()
  /// Refreshes the tab's file picker on a file-search wake.
  func onPickerWake()
  /// Refreshes the tab's file-tree sidebar on a tree wake.
  func onTreeWake()
  /// Refreshes the tab's sidebar search panel on a content-search wake.
  func onSearchWake()
  /// Shows the crash/stall overlay for `mode`.
  func showNvimOverlay(_ mode: NvimStatusMode)
  /// Hides the crash/stall overlay.
  func hideNvimOverlay()
  /// Flashes the transient "Neovim was restarted" banner.
  func flashRestartNotice()
  /// The tab's terminal sessions and whether its drawer is open, captured before a restart.
  func capturedTerminals() -> (splits: [UInt64], drawerOpen: Bool)
  /// Resets the tab's panes to one fresh pane after a successful restart.
  func resetAfterRestart()
  /// Hands a restart's terminal restore to the tab, which runs it once its first pane binds.
  func requestTerminalRestore(_ steps: [LivenessSeam.TerminalRestoreStep])
}

/// One tab's captive Neovim. Every tab owns one, started when the tab is created or restored
/// and shut down when it closes, so a crash, a stall or a `:cd` reaches that tab alone.
/// Main-thread-only (like `WolfWindowController` / `WindowRegistry`).
final class TabCore {
  /// Initial grid width in columns (the core's launch UI size).
  private static let initialCols: UInt16 = 80
  /// Initial grid height in rows.
  private static let initialRows: UInt16 = 24

  /// How long an automatic restart waits after the `Dead` wake that scheduled it.
  ///
  /// `handle.restart()` blocks the main thread for the whole respawn, so the crash
  /// overlay must be drawn before it runs. A timer that is not yet due makes the
  /// run loop go through its before-waiting pass, where AppKit displays and Core
  /// Animation commits; an immediate main-queue block does not guarantee that.
  /// wolf-gnome measured the same thing: an idle callback never presented the
  /// overlay, and a 50 ms timer did (`RESTART_DEFER` in
  /// `crates/wolf-gnome/src/shell.rs`). The latency is cosmetic and paid at most
  /// once per crash.
  private static let autoRestartDelay: TimeInterval = 0.05

  /// Reentrancy guard for `respawnNeovim` (a nested/concurrent restart is a no-op).
  private var restarting = false

  /// Automatic restart attempts since the core was last seen `Alive`, capped by
  /// `LivenessSeam.maxAutoRestartAttempts`. Reset by an `Alive` reading, a
  /// successful respawn and the button; incremented when an attempt is scheduled.
  private var autoRestartAttempts = 0

  /// Whether an automatic restart is scheduled but not yet run, so a burst of
  /// `Dead` wakes schedules exactly one attempt.
  private var restartScheduled = false

  /// The core handle of this tab's captive Neovim.
  let handle: WolfCoreHandle

  /// The core's wake-routing token (minted per `wolf_ffi_start`). The front-end
  /// registers ONE process-global wake callback per channel, serving every tab's
  /// core, and looks this token up in `WindowRegistry` to reach this tab.
  let wakeToken: UInt64

  /// The directory the core was started in, where a restart respawns it.
  let launchCwd: String

  /// The tab's ID (`specs/cli-and-file-open.md` §3), which every terminal session the core
  /// starts sees as `WOLF_TAB_ID`. Assigned once at `start`, before any view can start a
  /// session, and kept by a crash restart, since the tab is the same tab.
  let wolfTabId: String

  /// The tab this core serves. Weak: the tab's controller holds the core strongly.
  weak var view: TabCoreView?

  /// The coordinator, asked to schedule state commits. Weak so the coordinator owns
  /// the lifetime of the controllers that own the cores.
  weak var coordinator: AppDelegate?

  private init(
    handle: WolfCoreHandle, wakeToken: UInt64, launchCwd: String, wolfTabId: String,
    coordinator: AppDelegate?
  ) {
    self.handle = handle
    self.wakeToken = wakeToken
    self.launchCwd = launchCwd
    self.wolfTabId = wolfTabId
    self.coordinator = coordinator
  }

  /// Starts a FRESH captive Neovim at `launchCwd` and registers it in
  /// `WindowRegistry` under its wake token. The ONLY caller of `wolf_ffi_start`.
  /// Returns `nil` when the core fails to start (the caller then decides whether
  /// to present a fatal alert).
  static func start(launchCwd: String, coordinator: AppDelegate?) -> TabCore? {
    // Point wolf-core at the bundled Neovim unless a developer has already
    // exported `WOLF_NVIM_BIN` (overwrite=0 so the dev override wins).
    if let bin = Self.nvimBinPath() {
      setenv("WOLF_NVIM_BIN", bin, 0)
    }
    // Point wolf-core's content search at the bundled ripgrep unless a developer
    // has already exported `WOLF_RG_BIN` (overwrite=0 so the dev override wins).
    if let rg = Self.rgBinPath() {
      setenv("WOLF_RG_BIN", rg, 0)
    }
    // Give terminal panes the bundled `xterm-ghostty` entry unless a developer has
    // exported `WOLF_TERMINFO_DIR`, which `wolf_ffi_start` reads.
    if let terminfo = Self.terminfoDirPath() {
      setenv("WOLF_TERMINFO_DIR", terminfo, 0)
    }
    guard
      let handle = wolf_ffi_start(
        Self.initLuaPath(), launchCwd, Self.initialCols, Self.initialRows)
    else {
      return nil
    }
    let token = handle.wake_token()
    let tabCore = TabCore(
      handle: handle, wakeToken: token, launchCwd: launchCwd,
      wolfTabId: handle.assign_tab_id().toString(), coordinator: coordinator)
    WindowRegistry.shared.register(tabCore, token: token)
    return tabCore
  }

  /// Pushes the resolved appearance to this tab's nvim (`:set background=`).
  /// `dark: true` selects dark, `false` light. Called by `AppDelegate` when the
  /// core starts and live in `auto` mode when the OS toggles.
  func setBackground(dark: Bool) {
    handle.set_background(dark)
  }

  /// Reports this tab's focus to its Neovim (`nvim_ui_set_focus`). `gained: true`
  /// fires `FocusGained`, which the init.lua `WolfAutoRead` augroup rides to
  /// `:checktime`, so a file another tab saved reloads here. Called by
  /// `AppDelegate.refreshTabFocus()` with each `TabFocusSeam` edge.
  func setUiFocus(gained: Bool) {
    handle.set_ui_focus(gained)
  }

  /// Handles a render wake for this tab.
  ///
  /// Reads the captive nvim's LIVENESS and acts on `LivenessSeam`'s decision: it
  /// shows the overlay on this tab, renders only for an `Alive` core, schedules at
  /// most one deferred automatic restart for a `Dead` one (see `autoRestartDelay`),
  /// and drains the tab's queued commands in every state. The drain runs last
  /// because a drained close can tear the tab down and shut the core down.
  func onWake() {
    FrameMetrics.noteWake()

    let liveness = currentLiveness()
    let decision = LivenessSeam.decision(
      liveness: liveness, autoRestartAttempts: autoRestartAttempts)
    if liveness == .alive {
      autoRestartAttempts = 0
    }
    applyOverlay(decision.overlay)
    if decision.render {
      view?.onRenderWake()
    }
    if decision.persist {
      coordinator?.scheduleStateSnapshot()
    }
    if decision.autoRestart, !restarting, !restartScheduled {
      scheduleAutoRestart()
    }
    if decision.drain {
      view?.drainAndDispatchCommands()
    }
  }

  /// Forwards a file-search wake to this tab's picker.
  func onPickerWake() {
    view?.onPickerWake()
  }

  /// Forwards a file-tree wake to this tab's sidebar.
  func onTreeWake() {
    view?.onTreeWake()
  }

  /// Forwards a content-search wake to this tab's search panel.
  func onSearchWake() {
    view?.onSearchWake()
  }

  /// The captive nvim's liveness. An unknown discriminant reads as `.alive`.
  private func currentLiveness() -> NvimLiveness {
    NvimLiveness(rawValue: handle.liveness()) ?? .alive
  }

  /// Shows `overlay` on this tab.
  private func applyOverlay(_ overlay: LivenessSeam.Overlay) {
    guard let view else { return }
    switch overlay {
    case .hidden:
      view.hideNvimOverlay()
    case .crashed(let retrying):
      view.showNvimOverlay(.dead(retrying: retrying))
    case .stalled:
      view.showNvimOverlay(.notResponding)
    }
  }

  /// Schedules ONE automatic respawn `autoRestartDelay` from now, counting the
  /// attempt at scheduling time, as wolf-gnome's `schedule_restart` does.
  private func scheduleAutoRestart() {
    restartScheduled = true
    autoRestartAttempts += 1
    DispatchQueue.main.asyncAfter(deadline: .now() + Self.autoRestartDelay) { [weak self] in
      guard let self else { return }
      self.restartScheduled = false
      self.respawnNeovim()
    }
  }

  /// Restarts the captive Neovim IN PLACE: the "Restart Neovim" button's entry.
  /// An explicit restart resets the automatic attempt count, so the cap never
  /// refuses the user.
  func restartNeovim() {
    autoRestartAttempts = 0
    respawnNeovim()
  }

  /// Restarts the captive Neovim IN PLACE after a crash/stall and reopens the
  /// tab's Tier-1 file set.
  ///
  /// CAPTURES the open-file mirror, cwd and terminals BEFORE the restart (a respawn
  /// replaces the mirror); these are snapshot reads a dead core still answers. Then
  /// it `handle.restart()`s, resets the tab to one fresh pane and runs
  /// `LivenessSeam.restoreSteps(for:launchCwd:)`, which re-roots the tab and reopens
  /// its files with the active one LAST so it lands focused. Guarded against re-entry.
  ///
  /// When no restart happened, it applies only the overlay of a fresh decision,
  /// never its `autoRestart`: scheduling is `onWake`'s alone, and scheduling here
  /// would bypass the cap.
  private func respawnNeovim() {
    guard !restarting else { return }
    restarting = true
    defer { restarting = false }

    let openFiles = handle.open_files_for_tabpage(handle.active_tabpage())
    let captured = LivenessSeam.CapturedView(
      cwd: handle.cwd().toString(),
      files: openFiles.files().map { $0.as_str().toString() },
      activePath: openFiles.active_path().toString())
    let terminals = view?.capturedTerminals() ?? (splits: [], drawerOpen: false)

    // Only an actual restart reopens. `false` means nvim self-recovered first or the respawn
    // failed, so refresh only the overlay from a fresh liveness read.
    guard handle.restart() else {
      applyOverlay(
        LivenessSeam.decision(
          liveness: currentLiveness(), autoRestartAttempts: autoRestartAttempts
        ).overlay)
      return
    }
    autoRestartAttempts = 0

    view?.resetAfterRestart()
    // The pump applies these in order, so the opens resolve against the re-rooted cwd.
    for step in LivenessSeam.restoreSteps(for: captured, launchCwd: launchCwd) {
      switch step {
      case .chdir(let path):
        handle.chdir(path)
      case .open(let path):
        handle.open(path)
      }
    }

    // Sessions outlive Neovim; the tab puts its own back once its first pane binds.
    let live = Set(Array(handle.live_terminals()))
    view?.requestTerminalRestore(
      LivenessSeam.terminalRestore(
        splits: terminals.splits, drawerOpen: terminals.drawerOpen, live: live))

    // The banner is independent of the liveness overlay, which the fresh nvim's Alive wake clears
    // at once. Only the success path flashes it.
    view?.hideNvimOverlay()
    view?.flashRestartNotice()
  }

  /// Shuts the captive Neovim down and unregisters the wake token. Called by the
  /// tab's teardown.
  func shutdown() {
    handle.shutdown()
    WindowRegistry.shared.unregister(token: wakeToken)
  }

  // MARK: - Bundled-Neovim resolution (the sole `wolf_ffi_start` call site)

  /// Resolves the bundled Neovim binary at `Contents/Resources/nvim/bin/nvim`,
  /// or `nil` when absent/not executable (so a `WOLF_NVIM_BIN` export wins).
  private static func nvimBinPath() -> String? {
    guard let resourcePath = Bundle.main.resourcePath else { return nil }
    let candidate = resourcePath + "/nvim/bin/nvim"
    guard FileManager.default.isExecutableFile(atPath: candidate) else {
      return nil
    }
    return candidate
  }

  /// Resolves the bundled ripgrep binary at `Contents/Resources/rg/rg`, or `nil`
  /// when absent/not executable (so a `WOLF_RG_BIN` export wins, and an
  /// unbundled dev build degrades gracefully to no content search).
  static func rgBinPath() -> String? {
    guard let resourcePath = Bundle.main.resourcePath else { return nil }
    let candidate = resourcePath + "/rg/rg"
    guard FileManager.default.isExecutableFile(atPath: candidate) else {
      return nil
    }
    return candidate
  }

  /// Resolves the bundled terminfo database at `Contents/Resources/terminfo`, or
  /// `nil` when absent (an unbundled dev build, whose panes fall back to
  /// `xterm-256color`).
  static func terminfoDirPath() -> String? {
    guard let resourcePath = Bundle.main.resourcePath else { return nil }
    let candidate = resourcePath + "/terminfo"
    var isDirectory: ObjCBool = false
    guard FileManager.default.fileExists(atPath: candidate, isDirectory: &isDirectory),
      isDirectory.boolValue
    else {
      return nil
    }
    return candidate
  }

  /// Resolves the bundled `runtime/nvim/init.lua`; a `WOLF_INIT_LUA` override is
  /// honored first (for development).
  private static func initLuaPath() -> String {
    if let override = ProcessInfo.processInfo.environment["WOLF_INIT_LUA"],
      !override.isEmpty
    {
      return override
    }
    if let resource = Bundle.main.path(forResource: "init", ofType: "lua") {
      return resource
    }
    return "runtime/nvim/init.lua"
  }
}
