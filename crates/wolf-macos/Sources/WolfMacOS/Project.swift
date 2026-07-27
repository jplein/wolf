//
//  Project.swift
//  One captive Neovim instance shared by N views, each view bound to one nvim
//  TABPAGE (the project/tabpage-per-view model).
//
//  A `Project` owns the single `WolfCoreHandle` (one process, one shared buffer
//  list) and its per-project wake token; each `WolfWindowController` (a VIEW) is
//  bound to one of the project's tabpages. RENDER and picker wakes go only to the
//  focused view — the project fans them out to its `currentView`; other views hold
//  their last painted frame until refocused. The file-TREE wake is the exception:
//  it fans out to EVERY attached view's sidebar (each view runs its own file-tree
//  traversal and the wake token can't say which view a batch is for). The project
//  stays alive while ANY view
//  remains and shuts its nvim down only when its LAST view closes.
//
//  Ownership (no cycles): the `AppDelegate` coordinator strong-holds `projects`
//  (and `controllers`); each view holds its `project` STRONG (the core must
//  outlive a view teardown while a sibling still binds it); the project holds its
//  views WEAK. The last view's `detach` is the ONLY path that shuts the core down.
//

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

  /// The captive-nvim liveness discriminants, mirroring the FFI `mod liveness`
  /// (which mirrors `wolf_core::Liveness`). Swift cannot import the Rust module, so
  /// these are the hand-kept mirror consumed by `onWake`.
  private enum Liveness {
    static let alive: UInt8 = 0
    static let notResponding: UInt8 = 1
    static let dead: UInt8 = 2
  }

  /// Reentrancy guard for `restartNeovim` (a nested/concurrent restart is a no-op).
  private var restarting = false

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
  /// Reads the captive nvim's LIVENESS first: a `Dead` core fans the crash overlay
  /// to every view and auto-restarts (skipping the stale render); a
  /// `NotResponding` core fans the stall overlay and waits for the user's manual
  /// "Restart Neovim" (button-driven); an `Alive` core hides any overlay and
  /// proceeds with the normal render fan-out.
  func onWake() {
    // Exactly one core wake through `WakeBridge`, independent of view count.
    FrameMetrics.noteWake()

    switch handle.liveness() {
    case Liveness.dead:
      for view in views.allObjects { view.showNvimOverlay(.dead) }
      restartNeovim()
      return
    case Liveness.notResponding:
      for view in views.allObjects { view.showNvimOverlay(.notResponding) }
      return
    default:
      for view in views.allObjects { view.hideNvimOverlay() }
      currentView?.onRenderWake()
    }
  }

  /// Restarts the captive Neovim IN PLACE after a crash/stall and reopens the
  /// Tier-1 file set per view.
  ///
  /// CAPTURES each view's open-file mirror + cwd BEFORE the restart (a respawn
  /// wipes the mirror), then `handle.restart()`s, rebinds every view to a fresh
  /// tabpage (first view → tabpage 1, the rest → a new `:tcd` tabpage), reopens
  /// the captured files (active LAST so it lands focused), restores the current
  /// view, and hides every overlay. Guarded against re-entry.
  func restartNeovim() {
    guard !restarting else { return }
    restarting = true
    defer { restarting = false }

    // Capture the Tier-1 set + cwd per view BEFORE the restart wipes the mirror.
    let snapshot: [(view: WolfWindowController, cwd: String, openFiles: FfiOpenFiles?)] =
      views.allObjects.map { view in
        (view, handle.cwd_for_tabpage(view.tabpageHandle).toString(), view.capturedOpenFiles())
      }

    // Only an ACTUAL restart (Ok(true)) rebinds + reopens. A no-op (`false`) means
    // the nvim self-recovered to Alive before the click (a race) OR the respawn
    // failed (liveness Dead): in both cases we must NOT rebind/reopen — instead
    // refresh the overlay from a FRESH liveness read, mirroring `onWake`'s mapping.
    guard handle.restart() else {
      switch handle.liveness() {
      case Liveness.dead:
        for view in views.allObjects { view.showNvimOverlay(.dead) }
      case Liveness.notResponding:
        for view in views.allObjects { view.showNvimOverlay(.notResponding) }
      default:
        for view in views.allObjects { view.hideNvimOverlay() }
      }
      return
    }

    // Rebind each view to a fresh tabpage and reopen its Tier-1 files.
    for (index, entry) in snapshot.enumerated() {
      let tabpage: Int64 =
        index == 0 ? bindFirstTabpage() : addTabpage(cwd: entry.cwd)
      guard tabpage != 0 else { continue }
      entry.view.rebind(toTabpage: tabpage)
      handle.set_current_tabpage(tabpage)

      // Reopen the captured file-backed buffers; the active buffer LAST so it
      // lands focused in the rebound view.
      if let openFiles = entry.openFiles {
        let active = openFiles.active_path().toString()
        for path in openFiles.files() {
          let p = path.as_str().toString()
          if p != active { handle.open(p) }
        }
        if !active.isEmpty { handle.open(active) }
      }
    }

    // Restore the project's current view (its tabpage becomes current again) and
    // clear every crash/stall overlay.
    if let current = currentView {
      handle.set_current_tabpage(current.tabpageHandle)
    }
    for view in views.allObjects { view.hideNvimOverlay() }

    // Flash a TRANSIENT "Neovim was restarted" banner on every view. This is
    // independent of the liveness overlay (which the fresh nvim's Alive wake clears
    // immediately), so the user sees a visible, self-dismissing confirmation over
    // the now-working editor. Only the SUCCESS path flashes — the no-op/race branch
    // above does not.
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
