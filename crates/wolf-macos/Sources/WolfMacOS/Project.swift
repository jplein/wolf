//
//  Project.swift
//  One captive Neovim instance shared by N views, each view bound to one nvim
//  TABPAGE (the project/tabpage-per-view model).
//
//  A `Project` owns the single `WolfCoreHandle` (one process, one shared buffer
//  list) and its per-project wake token; each `WolfWindowController` (a VIEW) is
//  bound to one of the project's tabpages. Only the focused view live-updates —
//  the project fans every wake out to its `currentView`; other views hold their
//  last painted frame until refocused. The project stays alive while ANY view
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

  /// The single shared core handle for this project's captive Neovim.
  let handle: WolfCoreHandle

  /// The core's per-PROJECT wake-routing token (minted per `wolf_ffi_start`).
  /// The front-end registers ONE process-global wake callback per channel and
  /// looks this token up in `WindowRegistry` to reach the owning project, which
  /// fans the wake out to its `currentView`.
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
  func onWake() {
    // Exactly one core wake through `WakeBridge`, independent of view count.
    FrameMetrics.noteWake()
    currentView?.onRenderWake()
  }

  /// Fans a file-search wake out to the current view's picker (session-local, so
  /// a stray non-current wake is harmless).
  func onPickerWake() {
    currentView?.picker?.scheduleRefresh()
  }

  /// Fans a file-tree wake out to the current view's sidebar (session-local).
  func onTreeWake() {
    currentView?.tree?.scheduleRefresh()
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
