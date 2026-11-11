// One captive Neovim shared by N views, each bound to one nvim tabpage.

import AppKit
import Foundation
import WolfFFI
import WolfKeyMap

/// One captive Neovim instance shared by multiple views (one per nvim tabpage).
/// Main-thread-only (like `WolfWindowController` / `WindowRegistry`).
final class Project {
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

  /// The single shared core handle for this project's captive Neovim.
  let handle: WolfCoreHandle

  /// The core's per-PROJECT wake-routing token (minted per `wolf_ffi_start`).
  /// The front-end registers ONE process-global wake callback per channel and
  /// looks this token up in `WindowRegistry` to reach the owning project, which
  /// fans RENDER and picker wakes out to its `currentView` while fanning the
  /// file-TREE wake out to ALL of its attached views' sidebars.
  let wakeToken: UInt64

  /// The view whose tabpage is currently the project's current tabpage (the one
  /// nvim renders). Wakes route ONLY here; other views hold their last paint.
  private(set) weak var currentView: WolfWindowController?

  /// The project's live views (weak — the coordinator's `controllers` array owns
  /// their lifetime; a view removes itself via `detach` at teardown).
  private let views = NSHashTable<WolfWindowController>.weakObjects()

  /// The coordinator, asked to drop this project when its last view closes. Weak
  /// so the coordinator's `projects` array owns the lifetime.
  weak var coordinator: AppDelegate?

  private init(handle: WolfCoreHandle, wakeToken: UInt64, coordinator: AppDelegate?) {
    self.handle = handle
    self.wakeToken = wakeToken
    self.coordinator = coordinator
  }

  /// Starts a FRESH captive Neovim (a new project) at `launchCwd` and registers
  /// it in `WindowRegistry` under its wake token. The ONLY caller of
  /// `wolf_ffi_start`. Returns `nil` when the core fails to start (the caller
  /// then decides whether to present a fatal alert).
  static func start(launchCwd: String, coordinator: AppDelegate) -> Project? {
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
    let project = Project(handle: handle, wakeToken: token, coordinator: coordinator)
    WindowRegistry.shared.register(project, token: token)
    return project
  }

  /// Binds the FIRST view to the project's implicit tabpage 1 (a fresh nvim owns
  /// one tabpage), returning its stable HANDLE. No `tabnew`.
  func bindFirstTabpage() -> Int64 {
    handle.current_tabpage()
  }

  /// Creates a new tabpage (a joining/restored view), optionally rooted at `cwd`
  /// via `:tcd` (empty = no `:tcd`), returning its stable HANDLE. `0` when the
  /// core is gone (the caller drops the would-be view).
  func addTabpage(cwd: String) -> Int64 {
    handle.create_tabpage(cwd)
  }

  /// Adds `view` to the project's weak view list.
  func attach(_ view: WolfWindowController) {
    views.add(view)
  }

  /// Removes `view`. Reassigns `currentView` to a surviving view when the removed
  /// one was current; when NO views remain this was the last view, so the project
  /// shuts its core down (the ONLY core-shutdown path).
  func detach(_ view: WolfWindowController) {
    views.remove(view)
    if currentView === view {
      currentView = views.allObjects.first
    }
    if views.allObjects.isEmpty {
      shutdown()
    }
  }

  /// Makes `view`'s tabpage the project's current tabpage (so nvim renders it).
  /// A no-op when `view` is already current. Drives every tabpage switch.
  func setCurrent(_ view: WolfWindowController) {
    guard currentView !== view else { return }
    currentView = view
    handle.set_current_tabpage(view.tabpageHandle)
  }

  /// Pushes the resolved appearance to this project's nvim (`:set background=`).
  /// `dark: true` selects dark, `false` light. Called by `AppDelegate` at launch,
  /// live in `auto` mode when the OS toggles, and after each new project starts.
  func setBackground(dark: Bool) {
    handle.set_background(dark)
  }

  /// Forwards the OS app focus state to this project's captive Neovim
  /// (`nvim_ui_set_focus`). `gained: true` (app activated) fires `FocusGained`,
  /// which the init.lua `WolfAutoRead` augroup rides to `:checktime` and reload an
  /// externally-changed unmodified buffer (the Dock-reactivation reload path);
  /// `false` (app resigned) fires `FocusLost`. Both edges are reported so each
  /// refocus is a genuine transition. Called by `AppDelegate`'s
  /// activate/resign fan-out.
  func setUiFocus(gained: Bool) {
    handle.set_ui_focus(gained)
  }

  /// Closes `view`'s tabpage (a non-last view close). Skips the RPC when `view`
  /// is the LAST view — the last view's `detach` shuts the whole core down
  /// instead (closing the last tabpage would be a no-op E784 anyway).
  func closeTabpage(for view: WolfWindowController) {
    guard views.allObjects.count > 1 else { return }
    handle.close_tabpage(view.tabpageHandle)
  }

  /// Fans a render wake out to the current view (others hold their last paint).
  ///
  /// Reads the captive nvim's LIVENESS and acts on `LivenessSeam`'s decision: it
  /// fans the overlay to every view, renders the current view only for an `Alive`
  /// core, schedules at most one deferred automatic restart for a `Dead` one (see
  /// `autoRestartDelay`), and drains the current view's queued commands in every
  /// state. The drain runs last because a drained close can detach the view and
  /// shut the core down.
  func onWake() {
    // Exactly one core wake through `WakeBridge`, independent of view count.
    FrameMetrics.noteWake()

    let liveness = currentLiveness()
    let decision = LivenessSeam.decision(
      liveness: liveness, autoRestartAttempts: autoRestartAttempts)
    if liveness == .alive {
      autoRestartAttempts = 0
    }
    applyOverlay(decision.overlay)
    if decision.render {
      currentView?.onRenderWake()
    }
    if decision.persist {
      coordinator?.scheduleStateSnapshot()
    }
    if decision.autoRestart, !restarting, !restartScheduled {
      scheduleAutoRestart()
    }
    if decision.drain {
      currentView?.drainAndDispatchCommands()
    }
  }

  /// The captive nvim's liveness. An unknown discriminant reads as `.alive`.
  private func currentLiveness() -> NvimLiveness {
    NvimLiveness(rawValue: handle.liveness()) ?? .alive
  }

  /// Fans `overlay` out to every view.
  private func applyOverlay(_ overlay: LivenessSeam.Overlay) {
    for view in views.allObjects {
      switch overlay {
      case .hidden:
        view.hideNvimOverlay()
      case .crashed(let retrying):
        view.showNvimOverlay(.dead(retrying: retrying))
      case .stalled:
        view.showNvimOverlay(.notResponding)
      }
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
  /// Tier-1 file set per view.
  ///
  /// CAPTURES each view's open-file mirror + cwd BEFORE the restart (a respawn
  /// replaces the mirror), then `handle.restart()`s and runs each view's
  /// `LivenessSeam.restoreSteps(for:)`: the first view rebinds to the fresh
  /// tabpage 1 and is re-rooted at its cwd, the rest to a new `:tcd` tabpage, and
  /// the captured files reopen with the active one LAST so it lands focused. Then
  /// it restores the current view and hides every overlay. Guarded against
  /// re-entry.
  ///
  /// When no restart happened, it applies only the overlay of a fresh decision,
  /// never its `autoRestart`: scheduling is `onWake`'s alone, and scheduling here
  /// would bypass the cap.
  private func respawnNeovim() {
    guard !restarting else { return }
    restarting = true
    defer { restarting = false }

    // Capture the Tier-1 set + cwd per view BEFORE the restart replaces the mirror.
    let attached = views.allObjects
    let terminals = attached.map { $0.capturedTerminals() }
    let captured: [LivenessSeam.CapturedView] = attached.map { view in
      let openFiles = view.capturedOpenFiles()
      return LivenessSeam.CapturedView(
        cwd: handle.cwd_for_tabpage(view.tabpageHandle).toString(),
        files: openFiles?.files().map { $0.as_str().toString() } ?? [],
        activePath: openFiles?.active_path().toString() ?? "")
    }

    // Only an actual restart rebinds and reopens. `false` means nvim self-recovered first or the
    // respawn failed, so refresh only the overlay from a fresh liveness read.
    guard handle.restart() else {
      applyOverlay(
        LivenessSeam.decision(
          liveness: currentLiveness(), autoRestartAttempts: autoRestartAttempts
        ).overlay)
      return
    }
    autoRestartAttempts = 0

    // The pump applies these in order, so a chdir or an open lands in the tabpage made current.
    for (view, steps) in zip(attached, LivenessSeam.restoreSteps(for: captured)) {
      viewSteps: for step in steps {
        let tabpage: Int64
        switch step {
        case .bindFirstTabpage:
          tabpage = bindFirstTabpage()
        case .newTabpage(let cwd):
          tabpage = addTabpage(cwd: cwd)
        case .chdir(let path):
          handle.chdir(path)
          continue
        case .open(let path):
          handle.open(path)
          continue
        }
        // A tabpage that could not be made skips the rest of this view's steps.
        guard tabpage != 0 else { break viewSteps }
        view.rebind(toTabpage: tabpage)
        handle.set_current_tabpage(tabpage)
      }
    }

    // Sessions outlive Neovim; each view puts its own back once its first pane binds.
    let live = Set(Array(handle.live_terminals()))
    for (view, captured) in zip(attached, terminals) {
      view.requestTerminalRestore(
        LivenessSeam.terminalRestore(
          splits: captured.splits, drawerOpen: captured.drawerOpen, live: live))
    }

    // Restore the project's current view (its tabpage becomes current again) and
    // clear every crash/stall overlay.
    if let current = currentView {
      handle.set_current_tabpage(current.tabpageHandle)
    }
    for view in views.allObjects { view.hideNvimOverlay() }

    // The banner is independent of the liveness overlay, which the fresh nvim's Alive wake clears
    // at once. Only the success path flashes it.
    for view in views.allObjects { view.flashRestartNotice() }
  }

  /// Fans a file-search wake out to the current view's picker (session-local, so
  /// a stray non-current wake is harmless).
  func onPickerWake() {
    currentView?.picker?.scheduleRefresh()
  }

  /// Fans a file-tree wake out to EVERY attached view's sidebar (not just the
  /// current view). Unlike `onWake`/`onPickerWake` — which stay currentView-only
  /// by design (nvim renders only the current tabpage; pickers are transient on
  /// the key window) — each view runs its OWN independent `start_tree_search()`
  /// traversal, and the per-project wake token carries no view discriminator, so
  /// the front-end cannot tell which view the streaming batch belongs to. Fanning
  /// to all views is cheap and idempotent: each `FileTreeView.refresh()` is
  /// version-gated on its own `search.version()` and coalesced via
  /// `refreshCoalescer`, so a view with no new data does no real work.
  func onTreeWake() {
    for view in views.allObjects {
      view.tree?.scheduleRefresh()
    }
  }

  /// Fans a content-search wake out to EVERY attached view's search panel, for the
  /// same reason as `onTreeWake`: each view runs its OWN independent
  /// `start_search()` session, the per-project wake token carries no view
  /// discriminator, and each `SearchView.scheduleRefresh()` is version-gated +
  /// coalesced, so an idle panel does no real work.
  func onSearchWake() {
    for view in views.allObjects {
      view.search?.scheduleRefresh()
    }
  }

  /// Shuts the captive Neovim down (flushing the shared state store), unregisters
  /// the wake token, and drops this project from the coordinator. Called by
  /// `detach` when the last view closes (and as a terminate-time backstop).
  func shutdown() {
    handle.shutdown()
    WindowRegistry.shared.unregister(token: wakeToken)
    coordinator?.removeProject(self)
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
