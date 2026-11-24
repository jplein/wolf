import WolfFFI
public func wolf_ffi_start<GenericIntoRustString: IntoRustString>(_ init_lua: GenericIntoRustString, _ cwd: GenericIntoRustString, _ cols: UInt16, _ rows: UInt16) -> Optional<WolfCoreHandle> {
    { let val = __swift_bridge__$wolf_ffi_start({ let rustString = init_lua.intoRustString(); rustString.isOwned = false; return rustString.ptr }(), { let rustString = cwd.intoRustString(); rustString.isOwned = false; return rustString.ptr }(), cols, rows); if val != nil { return WolfCoreHandle(ptr: val!) } else { return nil } }()
}
public func wolf_ffi_ancestors_to_reveal<GenericIntoRustString: IntoRustString>(_ rel_path: GenericIntoRustString) -> RustVec<RustString> {
    RustVec(ptr: __swift_bridge__$wolf_ffi_ancestors_to_reveal({ let rustString = rel_path.intoRustString(); rustString.isOwned = false; return rustString.ptr }()))
}
public func wolf_ffi_load_config() -> FfiConfig {
    FfiConfig(ptr: __swift_bridge__$wolf_ffi_load_config())
}
public func wolf_ffi_font_license_notices() -> RustString {
    RustString(ptr: __swift_bridge__$wolf_ffi_font_license_notices())
}
public func wolf_ffi_library_license_notices() -> RustString {
    RustString(ptr: __swift_bridge__$wolf_ffi_library_license_notices())
}
public func wolf_ffi_app_state_begin() {
    __swift_bridge__$wolf_ffi_app_state_begin()
}
public func wolf_ffi_app_state_add_window(_ has_position: Bool, _ x: Double, _ y: Double, _ width: Double, _ height: Double, _ active_tab: UInt32) {
    __swift_bridge__$wolf_ffi_app_state_add_window(has_position, x, y, width, height, active_tab)
}
public func wolf_ffi_app_state_window_minimized(_ minimized: Bool) {
    __swift_bridge__$wolf_ffi_app_state_window_minimized(minimized)
}
public func wolf_ffi_app_state_add_tab<GenericIntoRustString: IntoRustString>(_ directory: GenericIntoRustString, _ sidebar_visible: Bool, _ focused_kind: UInt8, _ focused_index: UInt32, _ zoomed_kind: UInt8, _ zoomed_index: UInt32) {
    __swift_bridge__$wolf_ffi_app_state_add_tab({ let rustString = directory.intoRustString(); rustString.isOwned = false; return rustString.ptr }(), sidebar_visible, focused_kind, focused_index, zoomed_kind, zoomed_index)
}
public func wolf_ffi_app_state_tab_sidebar<GenericIntoRustString: IntoRustString>(_ mode: UInt8, _ query: GenericIntoRustString, _ case_sensitive: Bool, _ use_regex: Bool, _ all_files: Bool) {
    __swift_bridge__$wolf_ffi_app_state_tab_sidebar(mode, { let rustString = query.intoRustString(); rustString.isOwned = false; return rustString.ptr }(), case_sensitive, use_regex, all_files)
}
public func wolf_ffi_app_state_tab_drawer(_ open: Bool, _ share: Double) {
    __swift_bridge__$wolf_ffi_app_state_tab_drawer(open, share)
}
public func wolf_ffi_app_state_push_pane<GenericIntoRustString: IntoRustString>(_ kind: UInt8, _ path: GenericIntoRustString, _ weight: Double) {
    __swift_bridge__$wolf_ffi_app_state_push_pane(kind, { let rustString = path.intoRustString(); rustString.isOwned = false; return rustString.ptr }(), weight)
}
public func wolf_ffi_app_state_pane_cursor(_ line: UInt32, _ col: UInt32, _ top_line: UInt32) {
    __swift_bridge__$wolf_ffi_app_state_pane_cursor(line, col, top_line)
}
public func wolf_ffi_app_state_pane_scroll(_ scroll: Double) {
    __swift_bridge__$wolf_ffi_app_state_pane_scroll(scroll)
}
public func wolf_ffi_app_state_push_layout(_ orientation: UInt8, _ child_count: UInt32, _ weight: Double) {
    __swift_bridge__$wolf_ffi_app_state_push_layout(orientation, child_count, weight)
}
public func wolf_ffi_app_state_commit_root() {
    __swift_bridge__$wolf_ffi_app_state_commit_root()
}
public func wolf_ffi_app_state_push_recency(_ tab: UInt32) {
    __swift_bridge__$wolf_ffi_app_state_push_recency(tab)
}
public func wolf_ffi_app_state_commit() {
    __swift_bridge__$wolf_ffi_app_state_commit()
}
public func wolf_ffi_app_state_flush() -> Bool {
    __swift_bridge__$wolf_ffi_app_state_flush()
}
public func wolf_ffi_app_state_window_state() -> FfiAppState {
    FfiAppState(ptr: __swift_bridge__$wolf_ffi_app_state_window_state())
}
public func wolf_ffi_app_state_project_state<GenericIntoRustString: IntoRustString>(_ directory: GenericIntoRustString) -> FfiProjectState {
    FfiProjectState(ptr: __swift_bridge__$wolf_ffi_app_state_project_state({ let rustString = directory.intoRustString(); rustString.isOwned = false; return rustString.ptr }()))
}
public func wolf_ffi_app_state_project_tab<GenericIntoRustString: IntoRustString>(_ directory: GenericIntoRustString) -> FfiProjectTab {
    FfiProjectTab(ptr: __swift_bridge__$wolf_ffi_app_state_project_tab({ let rustString = directory.intoRustString(); rustString.isOwned = false; return rustString.ptr }()))
}
public func wolf_ffi_app_state_choose_pane<GenericIntoRustString: IntoRustString>(_ file: GenericIntoRustString) -> FfiPaneChoice {
    FfiPaneChoice(ptr: __swift_bridge__$wolf_ffi_app_state_choose_pane({ let rustString = file.intoRustString(); rustString.isOwned = false; return rustString.ptr }()))
}
public func wolf_ffi_app_state_project_directory<GenericIntoRustString: IntoRustString>(_ file: GenericIntoRustString) -> RustString {
    RustString(ptr: __swift_bridge__$wolf_ffi_app_state_project_directory({ let rustString = file.intoRustString(); rustString.isOwned = false; return rustString.ptr }()))
}
public func wolf_ffi_open_listen<GenericIntoRustString: IntoRustString>(_ state_dir: GenericIntoRustString) -> RustString {
    RustString(ptr: __swift_bridge__$wolf_ffi_open_listen({ let rustString = state_dir.intoRustString(); rustString.isOwned = false; return rustString.ptr }()))
}
public func wolf_ffi_open_stop() {
    __swift_bridge__$wolf_ffi_open_stop()
}
public func wolf_ffi_open_drain() -> RustVec<FfiOpenItem> {
    RustVec(ptr: __swift_bridge__$wolf_ffi_open_drain())
}
public func wolf_ffi_open_item<GenericIntoRustString: IntoRustString>(_ path: GenericIntoRustString) -> FfiOpenItem {
    FfiOpenItem(ptr: __swift_bridge__$wolf_ffi_open_item({ let rustString = path.intoRustString(); rustString.isOwned = false; return rustString.ptr }()))
}
public func wolf_ffi_command_count() -> UInt8 {
    __swift_bridge__$wolf_ffi_command_count()
}
public func wolf_ffi_command_kind<GenericIntoRustString: IntoRustString>(_ id: GenericIntoRustString) -> UInt8 {
    __swift_bridge__$wolf_ffi_command_kind({ let rustString = id.intoRustString(); rustString.isOwned = false; return rustString.ptr }())
}
public struct FfiColor {
    public var rgb: UInt32
    public var set: Bool

    public init(rgb: UInt32,set: Bool) {
        self.rgb = rgb
        self.set = set
    }

    @inline(__always)
    func intoFfiRepr() -> __swift_bridge__$FfiColor {
        { let val = self; return __swift_bridge__$FfiColor(rgb: val.rgb, set: val.set); }()
    }
}
extension __swift_bridge__$FfiColor {
    @inline(__always)
    func intoSwiftRepr() -> FfiColor {
        { let val = self; return FfiColor(rgb: val.rgb, set: val.set); }()
    }
}
extension __swift_bridge__$Option$FfiColor {
    @inline(__always)
    func intoSwiftRepr() -> Optional<FfiColor> {
        if self.is_some {
            return self.val.intoSwiftRepr()
        } else {
            return nil
        }
    }

    @inline(__always)
    static func fromSwiftRepr(_ val: Optional<FfiColor>) -> __swift_bridge__$Option$FfiColor {
        if let v = val {
            return __swift_bridge__$Option$FfiColor(is_some: true, val: v.intoFfiRepr())
        } else {
            return __swift_bridge__$Option$FfiColor(is_some: false, val: __swift_bridge__$FfiColor())
        }
    }
}
public struct FfiKeyEvent {
    public var kind: UInt8
    public var ch: UInt32
    public var fnum: UInt8
    public var ctrl: Bool
    public var shift: Bool
    public var alt: Bool
    public var meta: Bool

    public init(kind: UInt8,ch: UInt32,fnum: UInt8,ctrl: Bool,shift: Bool,alt: Bool,meta: Bool) {
        self.kind = kind
        self.ch = ch
        self.fnum = fnum
        self.ctrl = ctrl
        self.shift = shift
        self.alt = alt
        self.meta = meta
    }

    @inline(__always)
    func intoFfiRepr() -> __swift_bridge__$FfiKeyEvent {
        { let val = self; return __swift_bridge__$FfiKeyEvent(kind: val.kind, ch: val.ch, fnum: val.fnum, ctrl: val.ctrl, shift: val.shift, alt: val.alt, meta: val.meta); }()
    }
}
extension __swift_bridge__$FfiKeyEvent {
    @inline(__always)
    func intoSwiftRepr() -> FfiKeyEvent {
        { let val = self; return FfiKeyEvent(kind: val.kind, ch: val.ch, fnum: val.fnum, ctrl: val.ctrl, shift: val.shift, alt: val.alt, meta: val.meta); }()
    }
}
extension __swift_bridge__$Option$FfiKeyEvent {
    @inline(__always)
    func intoSwiftRepr() -> Optional<FfiKeyEvent> {
        if self.is_some {
            return self.val.intoSwiftRepr()
        } else {
            return nil
        }
    }

    @inline(__always)
    static func fromSwiftRepr(_ val: Optional<FfiKeyEvent>) -> __swift_bridge__$Option$FfiKeyEvent {
        if let v = val {
            return __swift_bridge__$Option$FfiKeyEvent(is_some: true, val: v.intoFfiRepr())
        } else {
            return __swift_bridge__$Option$FfiKeyEvent(is_some: false, val: __swift_bridge__$FfiKeyEvent())
        }
    }
}
public struct FfiMouseEvent {
    public var button: UInt8
    public var action: UInt8
    public var grid: UInt64
    public var row: UInt32
    public var col: UInt32
    public var ctrl: Bool
    public var shift: Bool
    public var alt: Bool

    public init(button: UInt8,action: UInt8,grid: UInt64,row: UInt32,col: UInt32,ctrl: Bool,shift: Bool,alt: Bool) {
        self.button = button
        self.action = action
        self.grid = grid
        self.row = row
        self.col = col
        self.ctrl = ctrl
        self.shift = shift
        self.alt = alt
    }

    @inline(__always)
    func intoFfiRepr() -> __swift_bridge__$FfiMouseEvent {
        { let val = self; return __swift_bridge__$FfiMouseEvent(button: val.button, action: val.action, grid: val.grid, row: val.row, col: val.col, ctrl: val.ctrl, shift: val.shift, alt: val.alt); }()
    }
}
extension __swift_bridge__$FfiMouseEvent {
    @inline(__always)
    func intoSwiftRepr() -> FfiMouseEvent {
        { let val = self; return FfiMouseEvent(button: val.button, action: val.action, grid: val.grid, row: val.row, col: val.col, ctrl: val.ctrl, shift: val.shift, alt: val.alt); }()
    }
}
extension __swift_bridge__$Option$FfiMouseEvent {
    @inline(__always)
    func intoSwiftRepr() -> Optional<FfiMouseEvent> {
        if self.is_some {
            return self.val.intoSwiftRepr()
        } else {
            return nil
        }
    }

    @inline(__always)
    static func fromSwiftRepr(_ val: Optional<FfiMouseEvent>) -> __swift_bridge__$Option$FfiMouseEvent {
        if let v = val {
            return __swift_bridge__$Option$FfiMouseEvent(is_some: true, val: v.intoFfiRepr())
        } else {
            return __swift_bridge__$Option$FfiMouseEvent(is_some: false, val: __swift_bridge__$FfiMouseEvent())
        }
    }
}
public struct FfiScrollEvent {
    public var direction: UInt8
    public var grid: UInt64
    public var row: UInt32
    public var col: UInt32

    public init(direction: UInt8,grid: UInt64,row: UInt32,col: UInt32) {
        self.direction = direction
        self.grid = grid
        self.row = row
        self.col = col
    }

    @inline(__always)
    func intoFfiRepr() -> __swift_bridge__$FfiScrollEvent {
        { let val = self; return __swift_bridge__$FfiScrollEvent(direction: val.direction, grid: val.grid, row: val.row, col: val.col); }()
    }
}
extension __swift_bridge__$FfiScrollEvent {
    @inline(__always)
    func intoSwiftRepr() -> FfiScrollEvent {
        { let val = self; return FfiScrollEvent(direction: val.direction, grid: val.grid, row: val.row, col: val.col); }()
    }
}
extension __swift_bridge__$Option$FfiScrollEvent {
    @inline(__always)
    func intoSwiftRepr() -> Optional<FfiScrollEvent> {
        if self.is_some {
            return self.val.intoSwiftRepr()
        } else {
            return nil
        }
    }

    @inline(__always)
    static func fromSwiftRepr(_ val: Optional<FfiScrollEvent>) -> __swift_bridge__$Option$FfiScrollEvent {
        if let v = val {
            return __swift_bridge__$Option$FfiScrollEvent(is_some: true, val: v.intoFfiRepr())
        } else {
            return __swift_bridge__$Option$FfiScrollEvent(is_some: false, val: __swift_bridge__$FfiScrollEvent())
        }
    }
}

public class WolfCoreHandle: WolfCoreHandleRefMut {
    var isOwned: Bool = true

    public override init(ptr: UnsafeMutableRawPointer) {
        super.init(ptr: ptr)
    }

    deinit {
        if isOwned {
            __swift_bridge__$WolfCoreHandle$_free(ptr)
        }
    }
}
public class WolfCoreHandleRefMut: WolfCoreHandleRef {
    public override init(ptr: UnsafeMutableRawPointer) {
        super.init(ptr: ptr)
    }
}
public class WolfCoreHandleRef {
    var ptr: UnsafeMutableRawPointer

    public init(ptr: UnsafeMutableRawPointer) {
        self.ptr = ptr
    }
}
extension WolfCoreHandleRef {
    public func wake_token() -> UInt64 {
        __swift_bridge__$WolfCoreHandle$wake_token(ptr)
    }

    public func read_frame() -> FfiFrame {
        FfiFrame(ptr: __swift_bridge__$WolfCoreHandle$read_frame(ptr))
    }

    public func read_frame_for_grid(_ grid: UInt64) -> FfiFrame {
        FfiFrame(ptr: __swift_bridge__$WolfCoreHandle$read_frame_for_grid(ptr, grid))
    }

    public func send_key(_ ev: FfiKeyEvent) {
        __swift_bridge__$WolfCoreHandle$send_key(ptr, ev.intoFfiRepr())
    }

    public func send_mouse(_ ev: FfiMouseEvent) {
        __swift_bridge__$WolfCoreHandle$send_mouse(ptr, ev.intoFfiRepr())
    }

    public func send_scroll(_ ev: FfiScrollEvent) {
        __swift_bridge__$WolfCoreHandle$send_scroll(ptr, ev.intoFfiRepr())
    }

    public func resize(_ cols: UInt16, _ rows: UInt16) {
        __swift_bridge__$WolfCoreHandle$resize(ptr, cols, rows)
    }

    public func resize_grid(_ grid: UInt64, _ cols: UInt16, _ rows: UInt16) {
        __swift_bridge__$WolfCoreHandle$resize_grid(ptr, grid, cols, rows)
    }

    public func drain_intents() -> RustVec<FfiLayoutIntent> {
        RustVec(ptr: __swift_bridge__$WolfCoreHandle$drain_intents(ptr))
    }

    public func drain_commands() -> RustVec<FfiCommand> {
        RustVec(ptr: __swift_bridge__$WolfCoreHandle$drain_commands(ptr))
    }

    public func windows() -> RustVec<FfiWinInfo> {
        RustVec(ptr: __swift_bridge__$WolfCoreHandle$windows(ptr))
    }

    public func grid_for_win(_ win: Int64) -> UInt64 {
        __swift_bridge__$WolfCoreHandle$grid_for_win(ptr, win)
    }

    public func active_win() -> Int64 {
        __swift_bridge__$WolfCoreHandle$active_win(ptr)
    }

    public func create_tabpage<GenericIntoRustString: IntoRustString>(_ cwd: GenericIntoRustString) -> Int64 {
        __swift_bridge__$WolfCoreHandle$create_tabpage(ptr, { let rustString = cwd.intoRustString(); rustString.isOwned = false; return rustString.ptr }())
    }

    public func rebuild_tab(_ tab: FfiAppTabRef) -> RustVec<Int64> {
        RustVec(ptr: __swift_bridge__$WolfCoreHandle$rebuild_tab(ptr, tab.ptr))
    }

    public func current_tabpage() -> Int64 {
        __swift_bridge__$WolfCoreHandle$current_tabpage(ptr)
    }

    public func active_tabpage() -> Int64 {
        __swift_bridge__$WolfCoreHandle$active_tabpage(ptr)
    }

    public func cwd_for_tabpage(_ tab: Int64) -> RustString {
        RustString(ptr: __swift_bridge__$WolfCoreHandle$cwd_for_tabpage(ptr, tab))
    }

    public func set_current_tabpage(_ tab: Int64) {
        __swift_bridge__$WolfCoreHandle$set_current_tabpage(ptr, tab)
    }

    public func close_tabpage(_ tab: Int64) {
        __swift_bridge__$WolfCoreHandle$close_tabpage(ptr, tab)
    }

    public func focus_win(_ win: Int64) {
        __swift_bridge__$WolfCoreHandle$focus_win(ptr, win)
    }

    public func wait_on(_ item: FfiOpenItemRef) {
        __swift_bridge__$WolfCoreHandle$wait_on(ptr, item.ptr)
    }

    public func liveness() -> UInt8 {
        __swift_bridge__$WolfCoreHandle$liveness(ptr)
    }

    public func restart() -> Bool {
        __swift_bridge__$WolfCoreHandle$restart(ptr)
    }

    public func open_files_for_tabpage(_ tab: Int64) -> FfiOpenFiles {
        FfiOpenFiles(ptr: __swift_bridge__$WolfCoreHandle$open_files_for_tabpage(ptr, tab))
    }

    public func set_ui_focus(_ gained: Bool) {
        __swift_bridge__$WolfCoreHandle$set_ui_focus(ptr, gained)
    }

    public func shutdown() {
        __swift_bridge__$WolfCoreHandle$shutdown(ptr)
    }

    public func start_file_search() -> FfiFileSearch {
        FfiFileSearch(ptr: __swift_bridge__$WolfCoreHandle$start_file_search(ptr))
    }

    public func start_tree_search() -> FfiFileSearch {
        FfiFileSearch(ptr: __swift_bridge__$WolfCoreHandle$start_tree_search(ptr))
    }

    public func start_search<GenericIntoRustString: IntoRustString>(_ query: GenericIntoRustString, _ use_regex: Bool, _ case_sensitive: Bool, _ all_files: Bool) -> FfiSearch {
        FfiSearch(ptr: __swift_bridge__$WolfCoreHandle$start_search(ptr, { let rustString = query.intoRustString(); rustString.isOwned = false; return rustString.ptr }(), use_regex, case_sensitive, all_files))
    }

    public func ignorecase() -> Bool {
        __swift_bridge__$WolfCoreHandle$ignorecase(ptr)
    }

    public func open_at<GenericIntoRustString: IntoRustString>(_ path: GenericIntoRustString, _ line: UInt64, _ col: UInt64) {
        __swift_bridge__$WolfCoreHandle$open_at(ptr, { let rustString = path.intoRustString(); rustString.isOwned = false; return rustString.ptr }(), line, col)
    }

    public func command_palette() -> FfiCommandPalette {
        FfiCommandPalette(ptr: __swift_bridge__$WolfCoreHandle$command_palette(ptr))
    }

    public func current_file() -> RustString {
        RustString(ptr: __swift_bridge__$WolfCoreHandle$current_file(ptr))
    }

    public func cwd() -> RustString {
        RustString(ptr: __swift_bridge__$WolfCoreHandle$cwd(ptr))
    }

    public func recent_dirs() -> RustVec<RustString> {
        RustVec(ptr: __swift_bridge__$WolfCoreHandle$recent_dirs(ptr))
    }

    public func record_recent_dir<GenericIntoRustString: IntoRustString>(_ path: GenericIntoRustString) {
        __swift_bridge__$WolfCoreHandle$record_recent_dir(ptr, { let rustString = path.intoRustString(); rustString.isOwned = false; return rustString.ptr }())
    }

    public func theme_titlebar_bg() -> FfiColor {
        __swift_bridge__$WolfCoreHandle$theme_titlebar_bg(ptr).intoSwiftRepr()
    }

    public func theme_titlebar_fg() -> FfiColor {
        __swift_bridge__$WolfCoreHandle$theme_titlebar_fg(ptr).intoSwiftRepr()
    }

    public func pane_status_for_grid(_ grid: UInt64) -> FfiPaneStatus {
        FfiPaneStatus(ptr: __swift_bridge__$WolfCoreHandle$pane_status_for_grid(ptr, grid))
    }

    public func win_buffer(_ win: Int64) -> FfiWinBuffer {
        FfiWinBuffer(ptr: __swift_bridge__$WolfCoreHandle$win_buffer(ptr, win))
    }

    public func window_cursor(_ win: Int64) -> FfiWindowCursor {
        FfiWindowCursor(ptr: __swift_bridge__$WolfCoreHandle$window_cursor(ptr, win))
    }

    public func open<GenericIntoRustString: IntoRustString>(_ path: GenericIntoRustString) {
        __swift_bridge__$WolfCoreHandle$open(ptr, { let rustString = path.intoRustString(); rustString.isOwned = false; return rustString.ptr }())
    }

    public func chdir<GenericIntoRustString: IntoRustString>(_ path: GenericIntoRustString) {
        __swift_bridge__$WolfCoreHandle$chdir(ptr, { let rustString = path.intoRustString(); rustString.isOwned = false; return rustString.ptr }())
    }

    public func set_background(_ dark: Bool) {
        __swift_bridge__$WolfCoreHandle$set_background(ptr, dark)
    }

    public func close_pane() {
        __swift_bridge__$WolfCoreHandle$close_pane(ptr)
    }

    public func split_horizontal() {
        __swift_bridge__$WolfCoreHandle$split_horizontal(ptr)
    }

    public func split_vertical() {
        __swift_bridge__$WolfCoreHandle$split_vertical(ptr)
    }

    public func new_buffer() {
        __swift_bridge__$WolfCoreHandle$new_buffer(ptr)
    }

    public func toggle_terminal() {
        __swift_bridge__$WolfCoreHandle$toggle_terminal(ptr)
    }

    public func toggle_full_width() {
        __swift_bridge__$WolfCoreHandle$toggle_full_width(ptr)
    }

    public func is_full_width_active() -> Bool {
        __swift_bridge__$WolfCoreHandle$is_full_width_active(ptr)
    }

    public func full_width_for_win(_ win: Int64) -> Bool {
        __swift_bridge__$WolfCoreHandle$full_width_for_win(ptr, win)
    }

    public func copy_selection() -> RustString {
        RustString(ptr: __swift_bridge__$WolfCoreHandle$copy_selection(ptr))
    }

    public func paste<GenericIntoRustString: IntoRustString>(_ text: GenericIntoRustString) {
        __swift_bridge__$WolfCoreHandle$paste(ptr, { let rustString = text.intoRustString(); rustString.isOwned = false; return rustString.ptr }())
    }

    public func buffer_picker() -> FfiBufferPicker {
        FfiBufferPicker(ptr: __swift_bridge__$WolfCoreHandle$buffer_picker(ptr))
    }

    public func switch_buffer(_ bufnr: Int64) {
        __swift_bridge__$WolfCoreHandle$switch_buffer(ptr, bufnr)
    }

    public func is_visual_mode() -> Bool {
        __swift_bridge__$WolfCoreHandle$is_visual_mode(ptr)
    }

    public func is_markdown_active() -> Bool {
        __swift_bridge__$WolfCoreHandle$is_markdown_active(ptr)
    }

    public func is_preview_active() -> Bool {
        __swift_bridge__$WolfCoreHandle$is_preview_active(ptr)
    }

    public func edit_source() -> Int64 {
        __swift_bridge__$WolfCoreHandle$edit_source(ptr)
    }

    public func open_preview() -> Int64 {
        __swift_bridge__$WolfCoreHandle$open_preview(ptr)
    }

    public func render_preview(_ source_bufnr: Int64) -> RustString {
        RustString(ptr: __swift_bridge__$WolfCoreHandle$render_preview(ptr, source_bufnr))
    }

    public func open_in_browser() -> RustString {
        RustString(ptr: __swift_bridge__$WolfCoreHandle$open_in_browser(ptr))
    }

    public func preview_file<GenericIntoRustString: IntoRustString>(_ win: Int64, _ path: GenericIntoRustString) -> Int64 {
        __swift_bridge__$WolfCoreHandle$preview_file(ptr, win, { let rustString = path.intoRustString(); rustString.isOwned = false; return rustString.ptr }())
    }

    public func preview_buffer(_ win: Int64, _ bufnr: Int64) -> Int64 {
        __swift_bridge__$WolfCoreHandle$preview_buffer(ptr, win, bufnr)
    }

    public func preview_in_other(_ target_win: Int64) -> Int64 {
        __swift_bridge__$WolfCoreHandle$preview_in_other(ptr, target_win)
    }

    public func edit_in_other(_ target_win: Int64) -> Int64 {
        __swift_bridge__$WolfCoreHandle$edit_in_other(ptr, target_win)
    }

    public func open_split_left<GenericIntoRustString: IntoRustString>(_ path: GenericIntoRustString) -> Int64 {
        __swift_bridge__$WolfCoreHandle$open_split_left(ptr, { let rustString = path.intoRustString(); rustString.isOwned = false; return rustString.ptr }())
    }

    public func assign_tab_id() -> RustString {
        RustString(ptr: __swift_bridge__$WolfCoreHandle$assign_tab_id(ptr))
    }

    public func previews() -> RustVec<FfiPreview> {
        RustVec(ptr: __swift_bridge__$WolfCoreHandle$previews(ptr))
    }

    public func history_can_back(_ win: Int64) -> Bool {
        __swift_bridge__$WolfCoreHandle$history_can_back(ptr, win)
    }

    public func history_can_forward(_ win: Int64) -> Bool {
        __swift_bridge__$WolfCoreHandle$history_can_forward(ptr, win)
    }

    public func history_back(_ win: Int64) -> Bool {
        __swift_bridge__$WolfCoreHandle$history_back(ptr, win)
    }

    public func history_forward(_ win: Int64) -> Bool {
        __swift_bridge__$WolfCoreHandle$history_forward(ptr, win)
    }

    public func history_jump(_ win: Int64, _ index: UInt32) -> Bool {
        __swift_bridge__$WolfCoreHandle$history_jump(ptr, win, index)
    }

    public func history_entries(_ win: Int64) -> RustVec<FfiHistoryEntry> {
        RustVec(ptr: __swift_bridge__$WolfCoreHandle$history_entries(ptr, win))
    }

    public func cmdline() -> FfiCmdline {
        FfiCmdline(ptr: __swift_bridge__$WolfCoreHandle$cmdline(ptr))
    }

    public func pum() -> FfiPum {
        FfiPum(ptr: __swift_bridge__$WolfCoreHandle$pum(ptr))
    }

    public func toast() -> FfiToast {
        FfiToast(ptr: __swift_bridge__$WolfCoreHandle$toast(ptr))
    }

    public func open_terminal<GenericIntoRustString: IntoRustString>(_ win: Int64, _ target: UInt8, _ command: GenericIntoRustString) -> UInt64 {
        __swift_bridge__$WolfCoreHandle$open_terminal(ptr, win, target, { let rustString = command.intoRustString(); rustString.isOwned = false; return rustString.ptr }())
    }

    public func bind_terminal(_ id: UInt64, _ win: Int64, _ target: UInt8) -> Bool {
        __swift_bridge__$WolfCoreHandle$bind_terminal(ptr, id, win, target)
    }

    public func queue_bind_terminal(_ id: UInt64, _ win: Int64, _ target: UInt8) -> Bool {
        __swift_bridge__$WolfCoreHandle$queue_bind_terminal(ptr, id, win, target)
    }

    public func terminal_cwd(_ id: UInt64) -> RustString {
        RustString(ptr: __swift_bridge__$WolfCoreHandle$terminal_cwd(ptr, id))
    }

    public func terminal_frame(_ id: UInt64) -> FfiTerminalFrame {
        FfiTerminalFrame(ptr: __swift_bridge__$WolfCoreHandle$terminal_frame(ptr, id))
    }

    public func terminal_key(_ id: UInt64, _ ev: FfiKeyEvent) {
        __swift_bridge__$WolfCoreHandle$terminal_key(ptr, id, ev.intoFfiRepr())
    }

    public func terminal_text<GenericIntoRustString: IntoRustString>(_ id: UInt64, _ text: GenericIntoRustString) {
        __swift_bridge__$WolfCoreHandle$terminal_text(ptr, id, { let rustString = text.intoRustString(); rustString.isOwned = false; return rustString.ptr }())
    }

    public func terminal_paste<GenericIntoRustString: IntoRustString>(_ id: UInt64, _ text: GenericIntoRustString) {
        __swift_bridge__$WolfCoreHandle$terminal_paste(ptr, id, { let rustString = text.intoRustString(); rustString.isOwned = false; return rustString.ptr }())
    }

    public func terminal_mouse(_ id: UInt64, _ kind: UInt8, _ button: UInt8, _ x: Double, _ y: Double, _ mods: UInt8) {
        __swift_bridge__$WolfCoreHandle$terminal_mouse(ptr, id, kind, button, x, y, mods)
    }

    public func terminal_scroll(_ id: UInt64, _ direction: UInt8, _ lines: UInt16, _ x: Double, _ y: Double, _ mods: UInt8) {
        __swift_bridge__$WolfCoreHandle$terminal_scroll(ptr, id, direction, lines, x, y, mods)
    }

    public func terminal_focus(_ id: UInt64, _ gained: Bool) {
        __swift_bridge__$WolfCoreHandle$terminal_focus(ptr, id, gained)
    }

    public func resize_terminal(_ id: UInt64, _ cols: UInt16, _ rows: UInt16, _ cell_w_px: UInt32, _ cell_h_px: UInt32) {
        __swift_bridge__$WolfCoreHandle$resize_terminal(ptr, id, cols, rows, cell_w_px, cell_h_px)
    }

    public func close_terminal(_ id: UInt64) {
        __swift_bridge__$WolfCoreHandle$close_terminal(ptr, id)
    }

    public func live_terminals() -> RustVec<UInt64> {
        RustVec(ptr: __swift_bridge__$WolfCoreHandle$live_terminals(ptr))
    }

    public func terminal_selection_text(_ id: UInt64) -> RustString {
        RustString(ptr: __swift_bridge__$WolfCoreHandle$terminal_selection_text(ptr, id))
    }
}
extension WolfCoreHandle: Vectorizable {
    public static func vecOfSelfNew() -> UnsafeMutableRawPointer {
        __swift_bridge__$Vec_WolfCoreHandle$new()
    }

    public static func vecOfSelfFree(vecPtr: UnsafeMutableRawPointer) {
        __swift_bridge__$Vec_WolfCoreHandle$drop(vecPtr)
    }

    public static func vecOfSelfPush(vecPtr: UnsafeMutableRawPointer, value: WolfCoreHandle) {
        __swift_bridge__$Vec_WolfCoreHandle$push(vecPtr, {value.isOwned = false; return value.ptr;}())
    }

    public static func vecOfSelfPop(vecPtr: UnsafeMutableRawPointer) -> Optional<Self> {
        let pointer = __swift_bridge__$Vec_WolfCoreHandle$pop(vecPtr)
        if pointer == nil {
            return nil
        } else {
            return (WolfCoreHandle(ptr: pointer!) as! Self)
        }
    }

    public static func vecOfSelfGet(vecPtr: UnsafeMutableRawPointer, index: UInt) -> Optional<WolfCoreHandleRef> {
        let pointer = __swift_bridge__$Vec_WolfCoreHandle$get(vecPtr, index)
        if pointer == nil {
            return nil
        } else {
            return WolfCoreHandleRef(ptr: pointer!)
        }
    }

    public static func vecOfSelfGetMut(vecPtr: UnsafeMutableRawPointer, index: UInt) -> Optional<WolfCoreHandleRefMut> {
        let pointer = __swift_bridge__$Vec_WolfCoreHandle$get_mut(vecPtr, index)
        if pointer == nil {
            return nil
        } else {
            return WolfCoreHandleRefMut(ptr: pointer!)
        }
    }

    public static func vecOfSelfAsPtr(vecPtr: UnsafeMutableRawPointer) -> UnsafePointer<WolfCoreHandleRef> {
        UnsafePointer<WolfCoreHandleRef>(OpaquePointer(__swift_bridge__$Vec_WolfCoreHandle$as_ptr(vecPtr)))
    }

    public static func vecOfSelfLen(vecPtr: UnsafeMutableRawPointer) -> UInt {
        __swift_bridge__$Vec_WolfCoreHandle$len(vecPtr)
    }
}


public class FfiTerminalFrame: FfiTerminalFrameRefMut {
    var isOwned: Bool = true

    public override init(ptr: UnsafeMutableRawPointer) {
        super.init(ptr: ptr)
    }

    deinit {
        if isOwned {
            __swift_bridge__$FfiTerminalFrame$_free(ptr)
        }
    }
}
public class FfiTerminalFrameRefMut: FfiTerminalFrameRef {
    public override init(ptr: UnsafeMutableRawPointer) {
        super.init(ptr: ptr)
    }
}
public class FfiTerminalFrameRef {
    var ptr: UnsafeMutableRawPointer

    public init(ptr: UnsafeMutableRawPointer) {
        self.ptr = ptr
    }
}
extension FfiTerminalFrameRef {
    public func version() -> UInt64 {
        __swift_bridge__$FfiTerminalFrame$version(ptr)
    }

    public func cols() -> UInt16 {
        __swift_bridge__$FfiTerminalFrame$cols(ptr)
    }

    public func rows() -> UInt16 {
        __swift_bridge__$FfiTerminalFrame$rows(ptr)
    }

    public func row_version(_ row: UInt16) -> UInt64 {
        __swift_bridge__$FfiTerminalFrame$row_version(ptr, row)
    }

    public func row_cells(_ row: UInt16) -> RustVec<FfiTerminalCell> {
        RustVec(ptr: __swift_bridge__$FfiTerminalFrame$row_cells(ptr, row))
    }

    public func row_has_selection(_ row: UInt16) -> Bool {
        __swift_bridge__$FfiTerminalFrame$row_has_selection(ptr, row)
    }

    public func row_selection_start(_ row: UInt16) -> UInt16 {
        __swift_bridge__$FfiTerminalFrame$row_selection_start(ptr, row)
    }

    public func row_selection_end(_ row: UInt16) -> UInt16 {
        __swift_bridge__$FfiTerminalFrame$row_selection_end(ptr, row)
    }

    public func has_cursor() -> Bool {
        __swift_bridge__$FfiTerminalFrame$has_cursor(ptr)
    }

    public func cursor_col() -> UInt16 {
        __swift_bridge__$FfiTerminalFrame$cursor_col(ptr)
    }

    public func cursor_row() -> UInt16 {
        __swift_bridge__$FfiTerminalFrame$cursor_row(ptr)
    }

    public func cursor_visible() -> Bool {
        __swift_bridge__$FfiTerminalFrame$cursor_visible(ptr)
    }

    public func cursor_blinking() -> Bool {
        __swift_bridge__$FfiTerminalFrame$cursor_blinking(ptr)
    }

    public func cursor_style() -> UInt8 {
        __swift_bridge__$FfiTerminalFrame$cursor_style(ptr)
    }

    public func default_fg() -> FfiColor {
        __swift_bridge__$FfiTerminalFrame$default_fg(ptr).intoSwiftRepr()
    }

    public func default_bg() -> FfiColor {
        __swift_bridge__$FfiTerminalFrame$default_bg(ptr).intoSwiftRepr()
    }

    public func cursor_color() -> FfiColor {
        __swift_bridge__$FfiTerminalFrame$cursor_color(ptr).intoSwiftRepr()
    }

    public func scrollbar_total() -> UInt64 {
        __swift_bridge__$FfiTerminalFrame$scrollbar_total(ptr)
    }

    public func scrollbar_offset() -> UInt64 {
        __swift_bridge__$FfiTerminalFrame$scrollbar_offset(ptr)
    }

    public func scrollbar_len() -> UInt64 {
        __swift_bridge__$FfiTerminalFrame$scrollbar_len(ptr)
    }

    public func mouse_tracking() -> Bool {
        __swift_bridge__$FfiTerminalFrame$mouse_tracking(ptr)
    }

    public func title() -> RustString {
        RustString(ptr: __swift_bridge__$FfiTerminalFrame$title(ptr))
    }

    public func pwd() -> RustString {
        RustString(ptr: __swift_bridge__$FfiTerminalFrame$pwd(ptr))
    }

    public func shell() -> RustString {
        RustString(ptr: __swift_bridge__$FfiTerminalFrame$shell(ptr))
    }

    public func has_exited() -> Bool {
        __swift_bridge__$FfiTerminalFrame$has_exited(ptr)
    }

    public func exit_code() -> Int32 {
        __swift_bridge__$FfiTerminalFrame$exit_code(ptr)
    }
}
extension FfiTerminalFrame: Vectorizable {
    public static func vecOfSelfNew() -> UnsafeMutableRawPointer {
        __swift_bridge__$Vec_FfiTerminalFrame$new()
    }

    public static func vecOfSelfFree(vecPtr: UnsafeMutableRawPointer) {
        __swift_bridge__$Vec_FfiTerminalFrame$drop(vecPtr)
    }

    public static func vecOfSelfPush(vecPtr: UnsafeMutableRawPointer, value: FfiTerminalFrame) {
        __swift_bridge__$Vec_FfiTerminalFrame$push(vecPtr, {value.isOwned = false; return value.ptr;}())
    }

    public static func vecOfSelfPop(vecPtr: UnsafeMutableRawPointer) -> Optional<Self> {
        let pointer = __swift_bridge__$Vec_FfiTerminalFrame$pop(vecPtr)
        if pointer == nil {
            return nil
        } else {
            return (FfiTerminalFrame(ptr: pointer!) as! Self)
        }
    }

    public static func vecOfSelfGet(vecPtr: UnsafeMutableRawPointer, index: UInt) -> Optional<FfiTerminalFrameRef> {
        let pointer = __swift_bridge__$Vec_FfiTerminalFrame$get(vecPtr, index)
        if pointer == nil {
            return nil
        } else {
            return FfiTerminalFrameRef(ptr: pointer!)
        }
    }

    public static func vecOfSelfGetMut(vecPtr: UnsafeMutableRawPointer, index: UInt) -> Optional<FfiTerminalFrameRefMut> {
        let pointer = __swift_bridge__$Vec_FfiTerminalFrame$get_mut(vecPtr, index)
        if pointer == nil {
            return nil
        } else {
            return FfiTerminalFrameRefMut(ptr: pointer!)
        }
    }

    public static func vecOfSelfAsPtr(vecPtr: UnsafeMutableRawPointer) -> UnsafePointer<FfiTerminalFrameRef> {
        UnsafePointer<FfiTerminalFrameRef>(OpaquePointer(__swift_bridge__$Vec_FfiTerminalFrame$as_ptr(vecPtr)))
    }

    public static func vecOfSelfLen(vecPtr: UnsafeMutableRawPointer) -> UInt {
        __swift_bridge__$Vec_FfiTerminalFrame$len(vecPtr)
    }
}


public class FfiTerminalCell: FfiTerminalCellRefMut {
    var isOwned: Bool = true

    public override init(ptr: UnsafeMutableRawPointer) {
        super.init(ptr: ptr)
    }

    deinit {
        if isOwned {
            __swift_bridge__$FfiTerminalCell$_free(ptr)
        }
    }
}
public class FfiTerminalCellRefMut: FfiTerminalCellRef {
    public override init(ptr: UnsafeMutableRawPointer) {
        super.init(ptr: ptr)
    }
}
public class FfiTerminalCellRef {
    var ptr: UnsafeMutableRawPointer

    public init(ptr: UnsafeMutableRawPointer) {
        self.ptr = ptr
    }
}
extension FfiTerminalCellRef {
    public func text() -> RustString {
        RustString(ptr: __swift_bridge__$FfiTerminalCell$text(ptr))
    }

    public func fg() -> FfiColor {
        __swift_bridge__$FfiTerminalCell$fg(ptr).intoSwiftRepr()
    }

    public func bg() -> FfiColor {
        __swift_bridge__$FfiTerminalCell$bg(ptr).intoSwiftRepr()
    }

    public func flags() -> UInt8 {
        __swift_bridge__$FfiTerminalCell$flags(ptr)
    }

    public func underline() -> UInt8 {
        __swift_bridge__$FfiTerminalCell$underline(ptr)
    }

    public func width() -> UInt8 {
        __swift_bridge__$FfiTerminalCell$width(ptr)
    }
}
extension FfiTerminalCell: Vectorizable {
    public static func vecOfSelfNew() -> UnsafeMutableRawPointer {
        __swift_bridge__$Vec_FfiTerminalCell$new()
    }

    public static func vecOfSelfFree(vecPtr: UnsafeMutableRawPointer) {
        __swift_bridge__$Vec_FfiTerminalCell$drop(vecPtr)
    }

    public static func vecOfSelfPush(vecPtr: UnsafeMutableRawPointer, value: FfiTerminalCell) {
        __swift_bridge__$Vec_FfiTerminalCell$push(vecPtr, {value.isOwned = false; return value.ptr;}())
    }

    public static func vecOfSelfPop(vecPtr: UnsafeMutableRawPointer) -> Optional<Self> {
        let pointer = __swift_bridge__$Vec_FfiTerminalCell$pop(vecPtr)
        if pointer == nil {
            return nil
        } else {
            return (FfiTerminalCell(ptr: pointer!) as! Self)
        }
    }

    public static func vecOfSelfGet(vecPtr: UnsafeMutableRawPointer, index: UInt) -> Optional<FfiTerminalCellRef> {
        let pointer = __swift_bridge__$Vec_FfiTerminalCell$get(vecPtr, index)
        if pointer == nil {
            return nil
        } else {
            return FfiTerminalCellRef(ptr: pointer!)
        }
    }

    public static func vecOfSelfGetMut(vecPtr: UnsafeMutableRawPointer, index: UInt) -> Optional<FfiTerminalCellRefMut> {
        let pointer = __swift_bridge__$Vec_FfiTerminalCell$get_mut(vecPtr, index)
        if pointer == nil {
            return nil
        } else {
            return FfiTerminalCellRefMut(ptr: pointer!)
        }
    }

    public static func vecOfSelfAsPtr(vecPtr: UnsafeMutableRawPointer) -> UnsafePointer<FfiTerminalCellRef> {
        UnsafePointer<FfiTerminalCellRef>(OpaquePointer(__swift_bridge__$Vec_FfiTerminalCell$as_ptr(vecPtr)))
    }

    public static func vecOfSelfLen(vecPtr: UnsafeMutableRawPointer) -> UInt {
        __swift_bridge__$Vec_FfiTerminalCell$len(vecPtr)
    }
}


public class FfiOpenFiles: FfiOpenFilesRefMut {
    var isOwned: Bool = true

    public override init(ptr: UnsafeMutableRawPointer) {
        super.init(ptr: ptr)
    }

    deinit {
        if isOwned {
            __swift_bridge__$FfiOpenFiles$_free(ptr)
        }
    }
}
public class FfiOpenFilesRefMut: FfiOpenFilesRef {
    public override init(ptr: UnsafeMutableRawPointer) {
        super.init(ptr: ptr)
    }
}
public class FfiOpenFilesRef {
    var ptr: UnsafeMutableRawPointer

    public init(ptr: UnsafeMutableRawPointer) {
        self.ptr = ptr
    }
}
extension FfiOpenFilesRef {
    public func active_path() -> RustString {
        RustString(ptr: __swift_bridge__$FfiOpenFiles$active_path(ptr))
    }

    public func files() -> RustVec<RustString> {
        RustVec(ptr: __swift_bridge__$FfiOpenFiles$files(ptr))
    }
}
extension FfiOpenFiles: Vectorizable {
    public static func vecOfSelfNew() -> UnsafeMutableRawPointer {
        __swift_bridge__$Vec_FfiOpenFiles$new()
    }

    public static func vecOfSelfFree(vecPtr: UnsafeMutableRawPointer) {
        __swift_bridge__$Vec_FfiOpenFiles$drop(vecPtr)
    }

    public static func vecOfSelfPush(vecPtr: UnsafeMutableRawPointer, value: FfiOpenFiles) {
        __swift_bridge__$Vec_FfiOpenFiles$push(vecPtr, {value.isOwned = false; return value.ptr;}())
    }

    public static func vecOfSelfPop(vecPtr: UnsafeMutableRawPointer) -> Optional<Self> {
        let pointer = __swift_bridge__$Vec_FfiOpenFiles$pop(vecPtr)
        if pointer == nil {
            return nil
        } else {
            return (FfiOpenFiles(ptr: pointer!) as! Self)
        }
    }

    public static func vecOfSelfGet(vecPtr: UnsafeMutableRawPointer, index: UInt) -> Optional<FfiOpenFilesRef> {
        let pointer = __swift_bridge__$Vec_FfiOpenFiles$get(vecPtr, index)
        if pointer == nil {
            return nil
        } else {
            return FfiOpenFilesRef(ptr: pointer!)
        }
    }

    public static func vecOfSelfGetMut(vecPtr: UnsafeMutableRawPointer, index: UInt) -> Optional<FfiOpenFilesRefMut> {
        let pointer = __swift_bridge__$Vec_FfiOpenFiles$get_mut(vecPtr, index)
        if pointer == nil {
            return nil
        } else {
            return FfiOpenFilesRefMut(ptr: pointer!)
        }
    }

    public static func vecOfSelfAsPtr(vecPtr: UnsafeMutableRawPointer) -> UnsafePointer<FfiOpenFilesRef> {
        UnsafePointer<FfiOpenFilesRef>(OpaquePointer(__swift_bridge__$Vec_FfiOpenFiles$as_ptr(vecPtr)))
    }

    public static func vecOfSelfLen(vecPtr: UnsafeMutableRawPointer) -> UInt {
        __swift_bridge__$Vec_FfiOpenFiles$len(vecPtr)
    }
}


public class FfiPreview: FfiPreviewRefMut {
    var isOwned: Bool = true

    public override init(ptr: UnsafeMutableRawPointer) {
        super.init(ptr: ptr)
    }

    deinit {
        if isOwned {
            __swift_bridge__$FfiPreview$_free(ptr)
        }
    }
}
public class FfiPreviewRefMut: FfiPreviewRef {
    public override init(ptr: UnsafeMutableRawPointer) {
        super.init(ptr: ptr)
    }
}
public class FfiPreviewRef {
    var ptr: UnsafeMutableRawPointer

    public init(ptr: UnsafeMutableRawPointer) {
        self.ptr = ptr
    }
}
extension FfiPreviewRef {
    public func win() -> Int64 {
        __swift_bridge__$FfiPreview$win(ptr)
    }

    public func source_bufnr() -> Int64 {
        __swift_bridge__$FfiPreview$source_bufnr(ptr)
    }

    public func source_name() -> RustString {
        RustString(ptr: __swift_bridge__$FfiPreview$source_name(ptr))
    }

    public func source_path() -> RustString {
        RustString(ptr: __swift_bridge__$FfiPreview$source_path(ptr))
    }

    public func source_display_path() -> RustString {
        RustString(ptr: __swift_bridge__$FfiPreview$source_display_path(ptr))
    }

    public func source_tick() -> UInt64 {
        __swift_bridge__$FfiPreview$source_tick(ptr)
    }

    public func source_full_width() -> Bool {
        __swift_bridge__$FfiPreview$source_full_width(ptr)
    }

    public func source_cursor_line() -> UInt32 {
        __swift_bridge__$FfiPreview$source_cursor_line(ptr)
    }

    public func source_cursor_col() -> UInt32 {
        __swift_bridge__$FfiPreview$source_cursor_col(ptr)
    }
}
extension FfiPreview: Vectorizable {
    public static func vecOfSelfNew() -> UnsafeMutableRawPointer {
        __swift_bridge__$Vec_FfiPreview$new()
    }

    public static func vecOfSelfFree(vecPtr: UnsafeMutableRawPointer) {
        __swift_bridge__$Vec_FfiPreview$drop(vecPtr)
    }

    public static func vecOfSelfPush(vecPtr: UnsafeMutableRawPointer, value: FfiPreview) {
        __swift_bridge__$Vec_FfiPreview$push(vecPtr, {value.isOwned = false; return value.ptr;}())
    }

    public static func vecOfSelfPop(vecPtr: UnsafeMutableRawPointer) -> Optional<Self> {
        let pointer = __swift_bridge__$Vec_FfiPreview$pop(vecPtr)
        if pointer == nil {
            return nil
        } else {
            return (FfiPreview(ptr: pointer!) as! Self)
        }
    }

    public static func vecOfSelfGet(vecPtr: UnsafeMutableRawPointer, index: UInt) -> Optional<FfiPreviewRef> {
        let pointer = __swift_bridge__$Vec_FfiPreview$get(vecPtr, index)
        if pointer == nil {
            return nil
        } else {
            return FfiPreviewRef(ptr: pointer!)
        }
    }

    public static func vecOfSelfGetMut(vecPtr: UnsafeMutableRawPointer, index: UInt) -> Optional<FfiPreviewRefMut> {
        let pointer = __swift_bridge__$Vec_FfiPreview$get_mut(vecPtr, index)
        if pointer == nil {
            return nil
        } else {
            return FfiPreviewRefMut(ptr: pointer!)
        }
    }

    public static func vecOfSelfAsPtr(vecPtr: UnsafeMutableRawPointer) -> UnsafePointer<FfiPreviewRef> {
        UnsafePointer<FfiPreviewRef>(OpaquePointer(__swift_bridge__$Vec_FfiPreview$as_ptr(vecPtr)))
    }

    public static func vecOfSelfLen(vecPtr: UnsafeMutableRawPointer) -> UInt {
        __swift_bridge__$Vec_FfiPreview$len(vecPtr)
    }
}


public class FfiHistoryEntry: FfiHistoryEntryRefMut {
    var isOwned: Bool = true

    public override init(ptr: UnsafeMutableRawPointer) {
        super.init(ptr: ptr)
    }

    deinit {
        if isOwned {
            __swift_bridge__$FfiHistoryEntry$_free(ptr)
        }
    }
}
public class FfiHistoryEntryRefMut: FfiHistoryEntryRef {
    public override init(ptr: UnsafeMutableRawPointer) {
        super.init(ptr: ptr)
    }
}
public class FfiHistoryEntryRef {
    var ptr: UnsafeMutableRawPointer

    public init(ptr: UnsafeMutableRawPointer) {
        self.ptr = ptr
    }
}
extension FfiHistoryEntryRef {
    public func display() -> RustString {
        RustString(ptr: __swift_bridge__$FfiHistoryEntry$display(ptr))
    }

    public func index() -> UInt32 {
        __swift_bridge__$FfiHistoryEntry$index(ptr)
    }

    public func is_current() -> Bool {
        __swift_bridge__$FfiHistoryEntry$is_current(ptr)
    }
}
extension FfiHistoryEntry: Vectorizable {
    public static func vecOfSelfNew() -> UnsafeMutableRawPointer {
        __swift_bridge__$Vec_FfiHistoryEntry$new()
    }

    public static func vecOfSelfFree(vecPtr: UnsafeMutableRawPointer) {
        __swift_bridge__$Vec_FfiHistoryEntry$drop(vecPtr)
    }

    public static func vecOfSelfPush(vecPtr: UnsafeMutableRawPointer, value: FfiHistoryEntry) {
        __swift_bridge__$Vec_FfiHistoryEntry$push(vecPtr, {value.isOwned = false; return value.ptr;}())
    }

    public static func vecOfSelfPop(vecPtr: UnsafeMutableRawPointer) -> Optional<Self> {
        let pointer = __swift_bridge__$Vec_FfiHistoryEntry$pop(vecPtr)
        if pointer == nil {
            return nil
        } else {
            return (FfiHistoryEntry(ptr: pointer!) as! Self)
        }
    }

    public static func vecOfSelfGet(vecPtr: UnsafeMutableRawPointer, index: UInt) -> Optional<FfiHistoryEntryRef> {
        let pointer = __swift_bridge__$Vec_FfiHistoryEntry$get(vecPtr, index)
        if pointer == nil {
            return nil
        } else {
            return FfiHistoryEntryRef(ptr: pointer!)
        }
    }

    public static func vecOfSelfGetMut(vecPtr: UnsafeMutableRawPointer, index: UInt) -> Optional<FfiHistoryEntryRefMut> {
        let pointer = __swift_bridge__$Vec_FfiHistoryEntry$get_mut(vecPtr, index)
        if pointer == nil {
            return nil
        } else {
            return FfiHistoryEntryRefMut(ptr: pointer!)
        }
    }

    public static func vecOfSelfAsPtr(vecPtr: UnsafeMutableRawPointer) -> UnsafePointer<FfiHistoryEntryRef> {
        UnsafePointer<FfiHistoryEntryRef>(OpaquePointer(__swift_bridge__$Vec_FfiHistoryEntry$as_ptr(vecPtr)))
    }

    public static func vecOfSelfLen(vecPtr: UnsafeMutableRawPointer) -> UInt {
        __swift_bridge__$Vec_FfiHistoryEntry$len(vecPtr)
    }
}


public class FfiFileSearch: FfiFileSearchRefMut {
    var isOwned: Bool = true

    public override init(ptr: UnsafeMutableRawPointer) {
        super.init(ptr: ptr)
    }

    deinit {
        if isOwned {
            __swift_bridge__$FfiFileSearch$_free(ptr)
        }
    }
}
public class FfiFileSearchRefMut: FfiFileSearchRef {
    public override init(ptr: UnsafeMutableRawPointer) {
        super.init(ptr: ptr)
    }
}
public class FfiFileSearchRef {
    var ptr: UnsafeMutableRawPointer

    public init(ptr: UnsafeMutableRawPointer) {
        self.ptr = ptr
    }
}
extension FfiFileSearchRef {
    public func ranked<GenericIntoRustString: IntoRustString>(_ query: GenericIntoRustString, _ max: UInt) -> RustVec<FfiFileEntry> {
        RustVec(ptr: __swift_bridge__$FfiFileSearch$ranked(ptr, { let rustString = query.intoRustString(); rustString.isOwned = false; return rustString.ptr }(), max))
    }

    public func children_of<GenericIntoRustString: IntoRustString>(_ dir: GenericIntoRustString) -> RustVec<FfiTreeNode> {
        RustVec(ptr: __swift_bridge__$FfiFileSearch$children_of(ptr, { let rustString = dir.intoRustString(); rustString.isOwned = false; return rustString.ptr }()))
    }

    public func rel_path_for<GenericIntoRustString: IntoRustString>(_ abs: GenericIntoRustString) -> RustString {
        RustString(ptr: __swift_bridge__$FfiFileSearch$rel_path_for(ptr, { let rustString = abs.intoRustString(); rustString.isOwned = false; return rustString.ptr }()))
    }

    public func version() -> UInt64 {
        __swift_bridge__$FfiFileSearch$version(ptr)
    }
}
extension FfiFileSearch: Vectorizable {
    public static func vecOfSelfNew() -> UnsafeMutableRawPointer {
        __swift_bridge__$Vec_FfiFileSearch$new()
    }

    public static func vecOfSelfFree(vecPtr: UnsafeMutableRawPointer) {
        __swift_bridge__$Vec_FfiFileSearch$drop(vecPtr)
    }

    public static func vecOfSelfPush(vecPtr: UnsafeMutableRawPointer, value: FfiFileSearch) {
        __swift_bridge__$Vec_FfiFileSearch$push(vecPtr, {value.isOwned = false; return value.ptr;}())
    }

    public static func vecOfSelfPop(vecPtr: UnsafeMutableRawPointer) -> Optional<Self> {
        let pointer = __swift_bridge__$Vec_FfiFileSearch$pop(vecPtr)
        if pointer == nil {
            return nil
        } else {
            return (FfiFileSearch(ptr: pointer!) as! Self)
        }
    }

    public static func vecOfSelfGet(vecPtr: UnsafeMutableRawPointer, index: UInt) -> Optional<FfiFileSearchRef> {
        let pointer = __swift_bridge__$Vec_FfiFileSearch$get(vecPtr, index)
        if pointer == nil {
            return nil
        } else {
            return FfiFileSearchRef(ptr: pointer!)
        }
    }

    public static func vecOfSelfGetMut(vecPtr: UnsafeMutableRawPointer, index: UInt) -> Optional<FfiFileSearchRefMut> {
        let pointer = __swift_bridge__$Vec_FfiFileSearch$get_mut(vecPtr, index)
        if pointer == nil {
            return nil
        } else {
            return FfiFileSearchRefMut(ptr: pointer!)
        }
    }

    public static func vecOfSelfAsPtr(vecPtr: UnsafeMutableRawPointer) -> UnsafePointer<FfiFileSearchRef> {
        UnsafePointer<FfiFileSearchRef>(OpaquePointer(__swift_bridge__$Vec_FfiFileSearch$as_ptr(vecPtr)))
    }

    public static func vecOfSelfLen(vecPtr: UnsafeMutableRawPointer) -> UInt {
        __swift_bridge__$Vec_FfiFileSearch$len(vecPtr)
    }
}


public class FfiFileEntry: FfiFileEntryRefMut {
    var isOwned: Bool = true

    public override init(ptr: UnsafeMutableRawPointer) {
        super.init(ptr: ptr)
    }

    deinit {
        if isOwned {
            __swift_bridge__$FfiFileEntry$_free(ptr)
        }
    }
}
public class FfiFileEntryRefMut: FfiFileEntryRef {
    public override init(ptr: UnsafeMutableRawPointer) {
        super.init(ptr: ptr)
    }
}
public class FfiFileEntryRef {
    var ptr: UnsafeMutableRawPointer

    public init(ptr: UnsafeMutableRawPointer) {
        self.ptr = ptr
    }
}
extension FfiFileEntryRef {
    public func display() -> RustString {
        RustString(ptr: __swift_bridge__$FfiFileEntry$display(ptr))
    }

    public func path() -> RustString {
        RustString(ptr: __swift_bridge__$FfiFileEntry$path(ptr))
    }
}
extension FfiFileEntry: Vectorizable {
    public static func vecOfSelfNew() -> UnsafeMutableRawPointer {
        __swift_bridge__$Vec_FfiFileEntry$new()
    }

    public static func vecOfSelfFree(vecPtr: UnsafeMutableRawPointer) {
        __swift_bridge__$Vec_FfiFileEntry$drop(vecPtr)
    }

    public static func vecOfSelfPush(vecPtr: UnsafeMutableRawPointer, value: FfiFileEntry) {
        __swift_bridge__$Vec_FfiFileEntry$push(vecPtr, {value.isOwned = false; return value.ptr;}())
    }

    public static func vecOfSelfPop(vecPtr: UnsafeMutableRawPointer) -> Optional<Self> {
        let pointer = __swift_bridge__$Vec_FfiFileEntry$pop(vecPtr)
        if pointer == nil {
            return nil
        } else {
            return (FfiFileEntry(ptr: pointer!) as! Self)
        }
    }

    public static func vecOfSelfGet(vecPtr: UnsafeMutableRawPointer, index: UInt) -> Optional<FfiFileEntryRef> {
        let pointer = __swift_bridge__$Vec_FfiFileEntry$get(vecPtr, index)
        if pointer == nil {
            return nil
        } else {
            return FfiFileEntryRef(ptr: pointer!)
        }
    }

    public static func vecOfSelfGetMut(vecPtr: UnsafeMutableRawPointer, index: UInt) -> Optional<FfiFileEntryRefMut> {
        let pointer = __swift_bridge__$Vec_FfiFileEntry$get_mut(vecPtr, index)
        if pointer == nil {
            return nil
        } else {
            return FfiFileEntryRefMut(ptr: pointer!)
        }
    }

    public static func vecOfSelfAsPtr(vecPtr: UnsafeMutableRawPointer) -> UnsafePointer<FfiFileEntryRef> {
        UnsafePointer<FfiFileEntryRef>(OpaquePointer(__swift_bridge__$Vec_FfiFileEntry$as_ptr(vecPtr)))
    }

    public static func vecOfSelfLen(vecPtr: UnsafeMutableRawPointer) -> UInt {
        __swift_bridge__$Vec_FfiFileEntry$len(vecPtr)
    }
}


public class FfiTreeNode: FfiTreeNodeRefMut {
    var isOwned: Bool = true

    public override init(ptr: UnsafeMutableRawPointer) {
        super.init(ptr: ptr)
    }

    deinit {
        if isOwned {
            __swift_bridge__$FfiTreeNode$_free(ptr)
        }
    }
}
public class FfiTreeNodeRefMut: FfiTreeNodeRef {
    public override init(ptr: UnsafeMutableRawPointer) {
        super.init(ptr: ptr)
    }
}
public class FfiTreeNodeRef {
    var ptr: UnsafeMutableRawPointer

    public init(ptr: UnsafeMutableRawPointer) {
        self.ptr = ptr
    }
}
extension FfiTreeNodeRef {
    public func name() -> RustString {
        RustString(ptr: __swift_bridge__$FfiTreeNode$name(ptr))
    }

    public func rel_path() -> RustString {
        RustString(ptr: __swift_bridge__$FfiTreeNode$rel_path(ptr))
    }

    public func abs_path() -> RustString {
        RustString(ptr: __swift_bridge__$FfiTreeNode$abs_path(ptr))
    }

    public func is_dir() -> Bool {
        __swift_bridge__$FfiTreeNode$is_dir(ptr)
    }
}
extension FfiTreeNode: Vectorizable {
    public static func vecOfSelfNew() -> UnsafeMutableRawPointer {
        __swift_bridge__$Vec_FfiTreeNode$new()
    }

    public static func vecOfSelfFree(vecPtr: UnsafeMutableRawPointer) {
        __swift_bridge__$Vec_FfiTreeNode$drop(vecPtr)
    }

    public static func vecOfSelfPush(vecPtr: UnsafeMutableRawPointer, value: FfiTreeNode) {
        __swift_bridge__$Vec_FfiTreeNode$push(vecPtr, {value.isOwned = false; return value.ptr;}())
    }

    public static func vecOfSelfPop(vecPtr: UnsafeMutableRawPointer) -> Optional<Self> {
        let pointer = __swift_bridge__$Vec_FfiTreeNode$pop(vecPtr)
        if pointer == nil {
            return nil
        } else {
            return (FfiTreeNode(ptr: pointer!) as! Self)
        }
    }

    public static func vecOfSelfGet(vecPtr: UnsafeMutableRawPointer, index: UInt) -> Optional<FfiTreeNodeRef> {
        let pointer = __swift_bridge__$Vec_FfiTreeNode$get(vecPtr, index)
        if pointer == nil {
            return nil
        } else {
            return FfiTreeNodeRef(ptr: pointer!)
        }
    }

    public static func vecOfSelfGetMut(vecPtr: UnsafeMutableRawPointer, index: UInt) -> Optional<FfiTreeNodeRefMut> {
        let pointer = __swift_bridge__$Vec_FfiTreeNode$get_mut(vecPtr, index)
        if pointer == nil {
            return nil
        } else {
            return FfiTreeNodeRefMut(ptr: pointer!)
        }
    }

    public static func vecOfSelfAsPtr(vecPtr: UnsafeMutableRawPointer) -> UnsafePointer<FfiTreeNodeRef> {
        UnsafePointer<FfiTreeNodeRef>(OpaquePointer(__swift_bridge__$Vec_FfiTreeNode$as_ptr(vecPtr)))
    }

    public static func vecOfSelfLen(vecPtr: UnsafeMutableRawPointer) -> UInt {
        __swift_bridge__$Vec_FfiTreeNode$len(vecPtr)
    }
}


public class FfiSearch: FfiSearchRefMut {
    var isOwned: Bool = true

    public override init(ptr: UnsafeMutableRawPointer) {
        super.init(ptr: ptr)
    }

    deinit {
        if isOwned {
            __swift_bridge__$FfiSearch$_free(ptr)
        }
    }
}
public class FfiSearchRefMut: FfiSearchRef {
    public override init(ptr: UnsafeMutableRawPointer) {
        super.init(ptr: ptr)
    }
}
public class FfiSearchRef {
    var ptr: UnsafeMutableRawPointer

    public init(ptr: UnsafeMutableRawPointer) {
        self.ptr = ptr
    }
}
extension FfiSearchRef {
    public func version() -> UInt64 {
        __swift_bridge__$FfiSearch$version(ptr)
    }

    public func files(_ budget: UInt32) -> RustVec<FfiSearchFile> {
        RustVec(ptr: __swift_bridge__$FfiSearch$files(ptr, budget))
    }
}
extension FfiSearch: Vectorizable {
    public static func vecOfSelfNew() -> UnsafeMutableRawPointer {
        __swift_bridge__$Vec_FfiSearch$new()
    }

    public static func vecOfSelfFree(vecPtr: UnsafeMutableRawPointer) {
        __swift_bridge__$Vec_FfiSearch$drop(vecPtr)
    }

    public static func vecOfSelfPush(vecPtr: UnsafeMutableRawPointer, value: FfiSearch) {
        __swift_bridge__$Vec_FfiSearch$push(vecPtr, {value.isOwned = false; return value.ptr;}())
    }

    public static func vecOfSelfPop(vecPtr: UnsafeMutableRawPointer) -> Optional<Self> {
        let pointer = __swift_bridge__$Vec_FfiSearch$pop(vecPtr)
        if pointer == nil {
            return nil
        } else {
            return (FfiSearch(ptr: pointer!) as! Self)
        }
    }

    public static func vecOfSelfGet(vecPtr: UnsafeMutableRawPointer, index: UInt) -> Optional<FfiSearchRef> {
        let pointer = __swift_bridge__$Vec_FfiSearch$get(vecPtr, index)
        if pointer == nil {
            return nil
        } else {
            return FfiSearchRef(ptr: pointer!)
        }
    }

    public static func vecOfSelfGetMut(vecPtr: UnsafeMutableRawPointer, index: UInt) -> Optional<FfiSearchRefMut> {
        let pointer = __swift_bridge__$Vec_FfiSearch$get_mut(vecPtr, index)
        if pointer == nil {
            return nil
        } else {
            return FfiSearchRefMut(ptr: pointer!)
        }
    }

    public static func vecOfSelfAsPtr(vecPtr: UnsafeMutableRawPointer) -> UnsafePointer<FfiSearchRef> {
        UnsafePointer<FfiSearchRef>(OpaquePointer(__swift_bridge__$Vec_FfiSearch$as_ptr(vecPtr)))
    }

    public static func vecOfSelfLen(vecPtr: UnsafeMutableRawPointer) -> UInt {
        __swift_bridge__$Vec_FfiSearch$len(vecPtr)
    }
}


public class FfiSearchFile: FfiSearchFileRefMut {
    var isOwned: Bool = true

    public override init(ptr: UnsafeMutableRawPointer) {
        super.init(ptr: ptr)
    }

    deinit {
        if isOwned {
            __swift_bridge__$FfiSearchFile$_free(ptr)
        }
    }
}
public class FfiSearchFileRefMut: FfiSearchFileRef {
    public override init(ptr: UnsafeMutableRawPointer) {
        super.init(ptr: ptr)
    }
}
public class FfiSearchFileRef {
    var ptr: UnsafeMutableRawPointer

    public init(ptr: UnsafeMutableRawPointer) {
        self.ptr = ptr
    }
}
extension FfiSearchFileRef {
    public func label() -> RustString {
        RustString(ptr: __swift_bridge__$FfiSearchFile$label(ptr))
    }

    public func rel_path() -> RustString {
        RustString(ptr: __swift_bridge__$FfiSearchFile$rel_path(ptr))
    }

    public func abs_path() -> RustString {
        RustString(ptr: __swift_bridge__$FfiSearchFile$abs_path(ptr))
    }

    public func matches() -> RustVec<FfiSearchMatch> {
        RustVec(ptr: __swift_bridge__$FfiSearchFile$matches(ptr))
    }
}
extension FfiSearchFile: Vectorizable {
    public static func vecOfSelfNew() -> UnsafeMutableRawPointer {
        __swift_bridge__$Vec_FfiSearchFile$new()
    }

    public static func vecOfSelfFree(vecPtr: UnsafeMutableRawPointer) {
        __swift_bridge__$Vec_FfiSearchFile$drop(vecPtr)
    }

    public static func vecOfSelfPush(vecPtr: UnsafeMutableRawPointer, value: FfiSearchFile) {
        __swift_bridge__$Vec_FfiSearchFile$push(vecPtr, {value.isOwned = false; return value.ptr;}())
    }

    public static func vecOfSelfPop(vecPtr: UnsafeMutableRawPointer) -> Optional<Self> {
        let pointer = __swift_bridge__$Vec_FfiSearchFile$pop(vecPtr)
        if pointer == nil {
            return nil
        } else {
            return (FfiSearchFile(ptr: pointer!) as! Self)
        }
    }

    public static func vecOfSelfGet(vecPtr: UnsafeMutableRawPointer, index: UInt) -> Optional<FfiSearchFileRef> {
        let pointer = __swift_bridge__$Vec_FfiSearchFile$get(vecPtr, index)
        if pointer == nil {
            return nil
        } else {
            return FfiSearchFileRef(ptr: pointer!)
        }
    }

    public static func vecOfSelfGetMut(vecPtr: UnsafeMutableRawPointer, index: UInt) -> Optional<FfiSearchFileRefMut> {
        let pointer = __swift_bridge__$Vec_FfiSearchFile$get_mut(vecPtr, index)
        if pointer == nil {
            return nil
        } else {
            return FfiSearchFileRefMut(ptr: pointer!)
        }
    }

    public static func vecOfSelfAsPtr(vecPtr: UnsafeMutableRawPointer) -> UnsafePointer<FfiSearchFileRef> {
        UnsafePointer<FfiSearchFileRef>(OpaquePointer(__swift_bridge__$Vec_FfiSearchFile$as_ptr(vecPtr)))
    }

    public static func vecOfSelfLen(vecPtr: UnsafeMutableRawPointer) -> UInt {
        __swift_bridge__$Vec_FfiSearchFile$len(vecPtr)
    }
}


public class FfiSearchMatch: FfiSearchMatchRefMut {
    var isOwned: Bool = true

    public override init(ptr: UnsafeMutableRawPointer) {
        super.init(ptr: ptr)
    }

    deinit {
        if isOwned {
            __swift_bridge__$FfiSearchMatch$_free(ptr)
        }
    }
}
public class FfiSearchMatchRefMut: FfiSearchMatchRef {
    public override init(ptr: UnsafeMutableRawPointer) {
        super.init(ptr: ptr)
    }
}
public class FfiSearchMatchRef {
    var ptr: UnsafeMutableRawPointer

    public init(ptr: UnsafeMutableRawPointer) {
        self.ptr = ptr
    }
}
extension FfiSearchMatchRef {
    public func line_number() -> UInt64 {
        __swift_bridge__$FfiSearchMatch$line_number(ptr)
    }

    public func column() -> UInt32 {
        __swift_bridge__$FfiSearchMatch$column(ptr)
    }

    public func display_text() -> RustString {
        RustString(ptr: __swift_bridge__$FfiSearchMatch$display_text(ptr))
    }

    public func truncated_left() -> Bool {
        __swift_bridge__$FfiSearchMatch$truncated_left(ptr)
    }

    public func truncated_right() -> Bool {
        __swift_bridge__$FfiSearchMatch$truncated_right(ptr)
    }

    public func range_starts() -> RustVec<UInt32> {
        RustVec(ptr: __swift_bridge__$FfiSearchMatch$range_starts(ptr))
    }

    public func range_ends() -> RustVec<UInt32> {
        RustVec(ptr: __swift_bridge__$FfiSearchMatch$range_ends(ptr))
    }
}
extension FfiSearchMatch: Vectorizable {
    public static func vecOfSelfNew() -> UnsafeMutableRawPointer {
        __swift_bridge__$Vec_FfiSearchMatch$new()
    }

    public static func vecOfSelfFree(vecPtr: UnsafeMutableRawPointer) {
        __swift_bridge__$Vec_FfiSearchMatch$drop(vecPtr)
    }

    public static func vecOfSelfPush(vecPtr: UnsafeMutableRawPointer, value: FfiSearchMatch) {
        __swift_bridge__$Vec_FfiSearchMatch$push(vecPtr, {value.isOwned = false; return value.ptr;}())
    }

    public static func vecOfSelfPop(vecPtr: UnsafeMutableRawPointer) -> Optional<Self> {
        let pointer = __swift_bridge__$Vec_FfiSearchMatch$pop(vecPtr)
        if pointer == nil {
            return nil
        } else {
            return (FfiSearchMatch(ptr: pointer!) as! Self)
        }
    }

    public static func vecOfSelfGet(vecPtr: UnsafeMutableRawPointer, index: UInt) -> Optional<FfiSearchMatchRef> {
        let pointer = __swift_bridge__$Vec_FfiSearchMatch$get(vecPtr, index)
        if pointer == nil {
            return nil
        } else {
            return FfiSearchMatchRef(ptr: pointer!)
        }
    }

    public static func vecOfSelfGetMut(vecPtr: UnsafeMutableRawPointer, index: UInt) -> Optional<FfiSearchMatchRefMut> {
        let pointer = __swift_bridge__$Vec_FfiSearchMatch$get_mut(vecPtr, index)
        if pointer == nil {
            return nil
        } else {
            return FfiSearchMatchRefMut(ptr: pointer!)
        }
    }

    public static func vecOfSelfAsPtr(vecPtr: UnsafeMutableRawPointer) -> UnsafePointer<FfiSearchMatchRef> {
        UnsafePointer<FfiSearchMatchRef>(OpaquePointer(__swift_bridge__$Vec_FfiSearchMatch$as_ptr(vecPtr)))
    }

    public static func vecOfSelfLen(vecPtr: UnsafeMutableRawPointer) -> UInt {
        __swift_bridge__$Vec_FfiSearchMatch$len(vecPtr)
    }
}


public class FfiConfig: FfiConfigRefMut {
    var isOwned: Bool = true

    public override init(ptr: UnsafeMutableRawPointer) {
        super.init(ptr: ptr)
    }

    deinit {
        if isOwned {
            __swift_bridge__$FfiConfig$_free(ptr)
        }
    }
}
public class FfiConfigRefMut: FfiConfigRef {
    public override init(ptr: UnsafeMutableRawPointer) {
        super.init(ptr: ptr)
    }
}
public class FfiConfigRef {
    var ptr: UnsafeMutableRawPointer

    public init(ptr: UnsafeMutableRawPointer) {
        self.ptr = ptr
    }
}
extension FfiConfigRef {
    public func font_family() -> RustString {
        RustString(ptr: __swift_bridge__$FfiConfig$font_family(ptr))
    }

    public func font_size() -> Double {
        __swift_bridge__$FfiConfig$font_size(ptr)
    }

    public func line_height() -> Double {
        __swift_bridge__$FfiConfig$line_height(ptr)
    }

    public func appearance() -> UInt8 {
        __swift_bridge__$FfiConfig$appearance(ptr)
    }

    public func macos_option_as_alt() -> Bool {
        __swift_bridge__$FfiConfig$macos_option_as_alt(ptr)
    }
}
extension FfiConfig: Vectorizable {
    public static func vecOfSelfNew() -> UnsafeMutableRawPointer {
        __swift_bridge__$Vec_FfiConfig$new()
    }

    public static func vecOfSelfFree(vecPtr: UnsafeMutableRawPointer) {
        __swift_bridge__$Vec_FfiConfig$drop(vecPtr)
    }

    public static func vecOfSelfPush(vecPtr: UnsafeMutableRawPointer, value: FfiConfig) {
        __swift_bridge__$Vec_FfiConfig$push(vecPtr, {value.isOwned = false; return value.ptr;}())
    }

    public static func vecOfSelfPop(vecPtr: UnsafeMutableRawPointer) -> Optional<Self> {
        let pointer = __swift_bridge__$Vec_FfiConfig$pop(vecPtr)
        if pointer == nil {
            return nil
        } else {
            return (FfiConfig(ptr: pointer!) as! Self)
        }
    }

    public static func vecOfSelfGet(vecPtr: UnsafeMutableRawPointer, index: UInt) -> Optional<FfiConfigRef> {
        let pointer = __swift_bridge__$Vec_FfiConfig$get(vecPtr, index)
        if pointer == nil {
            return nil
        } else {
            return FfiConfigRef(ptr: pointer!)
        }
    }

    public static func vecOfSelfGetMut(vecPtr: UnsafeMutableRawPointer, index: UInt) -> Optional<FfiConfigRefMut> {
        let pointer = __swift_bridge__$Vec_FfiConfig$get_mut(vecPtr, index)
        if pointer == nil {
            return nil
        } else {
            return FfiConfigRefMut(ptr: pointer!)
        }
    }

    public static func vecOfSelfAsPtr(vecPtr: UnsafeMutableRawPointer) -> UnsafePointer<FfiConfigRef> {
        UnsafePointer<FfiConfigRef>(OpaquePointer(__swift_bridge__$Vec_FfiConfig$as_ptr(vecPtr)))
    }

    public static func vecOfSelfLen(vecPtr: UnsafeMutableRawPointer) -> UInt {
        __swift_bridge__$Vec_FfiConfig$len(vecPtr)
    }
}


public class FfiAppState: FfiAppStateRefMut {
    var isOwned: Bool = true

    public override init(ptr: UnsafeMutableRawPointer) {
        super.init(ptr: ptr)
    }

    deinit {
        if isOwned {
            __swift_bridge__$FfiAppState$_free(ptr)
        }
    }
}
public class FfiAppStateRefMut: FfiAppStateRef {
    public override init(ptr: UnsafeMutableRawPointer) {
        super.init(ptr: ptr)
    }
}
public class FfiAppStateRef {
    var ptr: UnsafeMutableRawPointer

    public init(ptr: UnsafeMutableRawPointer) {
        self.ptr = ptr
    }
}
extension FfiAppStateRef {
    public func windows() -> RustVec<FfiAppWindow> {
        RustVec(ptr: __swift_bridge__$FfiAppState$windows(ptr))
    }
}
extension FfiAppState: Vectorizable {
    public static func vecOfSelfNew() -> UnsafeMutableRawPointer {
        __swift_bridge__$Vec_FfiAppState$new()
    }

    public static func vecOfSelfFree(vecPtr: UnsafeMutableRawPointer) {
        __swift_bridge__$Vec_FfiAppState$drop(vecPtr)
    }

    public static func vecOfSelfPush(vecPtr: UnsafeMutableRawPointer, value: FfiAppState) {
        __swift_bridge__$Vec_FfiAppState$push(vecPtr, {value.isOwned = false; return value.ptr;}())
    }

    public static func vecOfSelfPop(vecPtr: UnsafeMutableRawPointer) -> Optional<Self> {
        let pointer = __swift_bridge__$Vec_FfiAppState$pop(vecPtr)
        if pointer == nil {
            return nil
        } else {
            return (FfiAppState(ptr: pointer!) as! Self)
        }
    }

    public static func vecOfSelfGet(vecPtr: UnsafeMutableRawPointer, index: UInt) -> Optional<FfiAppStateRef> {
        let pointer = __swift_bridge__$Vec_FfiAppState$get(vecPtr, index)
        if pointer == nil {
            return nil
        } else {
            return FfiAppStateRef(ptr: pointer!)
        }
    }

    public static func vecOfSelfGetMut(vecPtr: UnsafeMutableRawPointer, index: UInt) -> Optional<FfiAppStateRefMut> {
        let pointer = __swift_bridge__$Vec_FfiAppState$get_mut(vecPtr, index)
        if pointer == nil {
            return nil
        } else {
            return FfiAppStateRefMut(ptr: pointer!)
        }
    }

    public static func vecOfSelfAsPtr(vecPtr: UnsafeMutableRawPointer) -> UnsafePointer<FfiAppStateRef> {
        UnsafePointer<FfiAppStateRef>(OpaquePointer(__swift_bridge__$Vec_FfiAppState$as_ptr(vecPtr)))
    }

    public static func vecOfSelfLen(vecPtr: UnsafeMutableRawPointer) -> UInt {
        __swift_bridge__$Vec_FfiAppState$len(vecPtr)
    }
}


public class FfiAppWindow: FfiAppWindowRefMut {
    var isOwned: Bool = true

    public override init(ptr: UnsafeMutableRawPointer) {
        super.init(ptr: ptr)
    }

    deinit {
        if isOwned {
            __swift_bridge__$FfiAppWindow$_free(ptr)
        }
    }
}
public class FfiAppWindowRefMut: FfiAppWindowRef {
    public override init(ptr: UnsafeMutableRawPointer) {
        super.init(ptr: ptr)
    }
}
public class FfiAppWindowRef {
    var ptr: UnsafeMutableRawPointer

    public init(ptr: UnsafeMutableRawPointer) {
        self.ptr = ptr
    }
}
extension FfiAppWindowRef {
    public func has_position() -> Bool {
        __swift_bridge__$FfiAppWindow$has_position(ptr)
    }

    public func x() -> Double {
        __swift_bridge__$FfiAppWindow$x(ptr)
    }

    public func y() -> Double {
        __swift_bridge__$FfiAppWindow$y(ptr)
    }

    public func width() -> Double {
        __swift_bridge__$FfiAppWindow$width(ptr)
    }

    public func height() -> Double {
        __swift_bridge__$FfiAppWindow$height(ptr)
    }

    public func active_tab() -> UInt32 {
        __swift_bridge__$FfiAppWindow$active_tab(ptr)
    }

    public func minimized() -> Bool {
        __swift_bridge__$FfiAppWindow$minimized(ptr)
    }

    public func tabs() -> RustVec<FfiAppTab> {
        RustVec(ptr: __swift_bridge__$FfiAppWindow$tabs(ptr))
    }
}
extension FfiAppWindow: Vectorizable {
    public static func vecOfSelfNew() -> UnsafeMutableRawPointer {
        __swift_bridge__$Vec_FfiAppWindow$new()
    }

    public static func vecOfSelfFree(vecPtr: UnsafeMutableRawPointer) {
        __swift_bridge__$Vec_FfiAppWindow$drop(vecPtr)
    }

    public static func vecOfSelfPush(vecPtr: UnsafeMutableRawPointer, value: FfiAppWindow) {
        __swift_bridge__$Vec_FfiAppWindow$push(vecPtr, {value.isOwned = false; return value.ptr;}())
    }

    public static func vecOfSelfPop(vecPtr: UnsafeMutableRawPointer) -> Optional<Self> {
        let pointer = __swift_bridge__$Vec_FfiAppWindow$pop(vecPtr)
        if pointer == nil {
            return nil
        } else {
            return (FfiAppWindow(ptr: pointer!) as! Self)
        }
    }

    public static func vecOfSelfGet(vecPtr: UnsafeMutableRawPointer, index: UInt) -> Optional<FfiAppWindowRef> {
        let pointer = __swift_bridge__$Vec_FfiAppWindow$get(vecPtr, index)
        if pointer == nil {
            return nil
        } else {
            return FfiAppWindowRef(ptr: pointer!)
        }
    }

    public static func vecOfSelfGetMut(vecPtr: UnsafeMutableRawPointer, index: UInt) -> Optional<FfiAppWindowRefMut> {
        let pointer = __swift_bridge__$Vec_FfiAppWindow$get_mut(vecPtr, index)
        if pointer == nil {
            return nil
        } else {
            return FfiAppWindowRefMut(ptr: pointer!)
        }
    }

    public static func vecOfSelfAsPtr(vecPtr: UnsafeMutableRawPointer) -> UnsafePointer<FfiAppWindowRef> {
        UnsafePointer<FfiAppWindowRef>(OpaquePointer(__swift_bridge__$Vec_FfiAppWindow$as_ptr(vecPtr)))
    }

    public static func vecOfSelfLen(vecPtr: UnsafeMutableRawPointer) -> UInt {
        __swift_bridge__$Vec_FfiAppWindow$len(vecPtr)
    }
}


public class FfiAppTab: FfiAppTabRefMut {
    var isOwned: Bool = true

    public override init(ptr: UnsafeMutableRawPointer) {
        super.init(ptr: ptr)
    }

    deinit {
        if isOwned {
            __swift_bridge__$FfiAppTab$_free(ptr)
        }
    }
}
public class FfiAppTabRefMut: FfiAppTabRef {
    public override init(ptr: UnsafeMutableRawPointer) {
        super.init(ptr: ptr)
    }
}
public class FfiAppTabRef {
    var ptr: UnsafeMutableRawPointer

    public init(ptr: UnsafeMutableRawPointer) {
        self.ptr = ptr
    }
}
extension FfiAppTabRef {
    public func directory() -> RustString {
        RustString(ptr: __swift_bridge__$FfiAppTab$directory(ptr))
    }

    public func has_root() -> Bool {
        __swift_bridge__$FfiAppTab$has_root(ptr)
    }

    public func root() -> FfiAppNode {
        FfiAppNode(ptr: __swift_bridge__$FfiAppTab$root(ptr))
    }

    public func focused_kind() -> UInt8 {
        __swift_bridge__$FfiAppTab$focused_kind(ptr)
    }

    public func focused_index() -> UInt32 {
        __swift_bridge__$FfiAppTab$focused_index(ptr)
    }

    public func zoomed_kind() -> UInt8 {
        __swift_bridge__$FfiAppTab$zoomed_kind(ptr)
    }

    public func zoomed_index() -> UInt32 {
        __swift_bridge__$FfiAppTab$zoomed_index(ptr)
    }

    public func sidebar_visible() -> Bool {
        __swift_bridge__$FfiAppTab$sidebar_visible(ptr)
    }

    public func sidebar_mode() -> UInt8 {
        __swift_bridge__$FfiAppTab$sidebar_mode(ptr)
    }

    public func has_query() -> Bool {
        __swift_bridge__$FfiAppTab$has_query(ptr)
    }

    public func query() -> RustString {
        RustString(ptr: __swift_bridge__$FfiAppTab$query(ptr))
    }

    public func case_sensitive() -> UInt8 {
        __swift_bridge__$FfiAppTab$case_sensitive(ptr)
    }

    public func use_regex() -> UInt8 {
        __swift_bridge__$FfiAppTab$use_regex(ptr)
    }

    public func all_files() -> UInt8 {
        __swift_bridge__$FfiAppTab$all_files(ptr)
    }

    public func has_drawer() -> Bool {
        __swift_bridge__$FfiAppTab$has_drawer(ptr)
    }

    public func drawer_open() -> Bool {
        __swift_bridge__$FfiAppTab$drawer_open(ptr)
    }

    public func drawer_share() -> Double {
        __swift_bridge__$FfiAppTab$drawer_share(ptr)
    }

    public func restored() -> FfiAppTab {
        FfiAppTab(ptr: __swift_bridge__$FfiAppTab$restored(ptr))
    }
}
extension FfiAppTab: Vectorizable {
    public static func vecOfSelfNew() -> UnsafeMutableRawPointer {
        __swift_bridge__$Vec_FfiAppTab$new()
    }

    public static func vecOfSelfFree(vecPtr: UnsafeMutableRawPointer) {
        __swift_bridge__$Vec_FfiAppTab$drop(vecPtr)
    }

    public static func vecOfSelfPush(vecPtr: UnsafeMutableRawPointer, value: FfiAppTab) {
        __swift_bridge__$Vec_FfiAppTab$push(vecPtr, {value.isOwned = false; return value.ptr;}())
    }

    public static func vecOfSelfPop(vecPtr: UnsafeMutableRawPointer) -> Optional<Self> {
        let pointer = __swift_bridge__$Vec_FfiAppTab$pop(vecPtr)
        if pointer == nil {
            return nil
        } else {
            return (FfiAppTab(ptr: pointer!) as! Self)
        }
    }

    public static func vecOfSelfGet(vecPtr: UnsafeMutableRawPointer, index: UInt) -> Optional<FfiAppTabRef> {
        let pointer = __swift_bridge__$Vec_FfiAppTab$get(vecPtr, index)
        if pointer == nil {
            return nil
        } else {
            return FfiAppTabRef(ptr: pointer!)
        }
    }

    public static func vecOfSelfGetMut(vecPtr: UnsafeMutableRawPointer, index: UInt) -> Optional<FfiAppTabRefMut> {
        let pointer = __swift_bridge__$Vec_FfiAppTab$get_mut(vecPtr, index)
        if pointer == nil {
            return nil
        } else {
            return FfiAppTabRefMut(ptr: pointer!)
        }
    }

    public static func vecOfSelfAsPtr(vecPtr: UnsafeMutableRawPointer) -> UnsafePointer<FfiAppTabRef> {
        UnsafePointer<FfiAppTabRef>(OpaquePointer(__swift_bridge__$Vec_FfiAppTab$as_ptr(vecPtr)))
    }

    public static func vecOfSelfLen(vecPtr: UnsafeMutableRawPointer) -> UInt {
        __swift_bridge__$Vec_FfiAppTab$len(vecPtr)
    }
}


public class FfiAppNode: FfiAppNodeRefMut {
    var isOwned: Bool = true

    public override init(ptr: UnsafeMutableRawPointer) {
        super.init(ptr: ptr)
    }

    deinit {
        if isOwned {
            __swift_bridge__$FfiAppNode$_free(ptr)
        }
    }
}
public class FfiAppNodeRefMut: FfiAppNodeRef {
    public override init(ptr: UnsafeMutableRawPointer) {
        super.init(ptr: ptr)
    }
}
public class FfiAppNodeRef {
    var ptr: UnsafeMutableRawPointer

    public init(ptr: UnsafeMutableRawPointer) {
        self.ptr = ptr
    }
}
extension FfiAppNodeRef {
    public func is_pane() -> Bool {
        __swift_bridge__$FfiAppNode$is_pane(ptr)
    }

    public func kind() -> UInt8 {
        __swift_bridge__$FfiAppNode$kind(ptr)
    }

    public func path() -> RustString {
        RustString(ptr: __swift_bridge__$FfiAppNode$path(ptr))
    }

    public func has_cursor() -> Bool {
        __swift_bridge__$FfiAppNode$has_cursor(ptr)
    }

    public func cursor_line() -> UInt32 {
        __swift_bridge__$FfiAppNode$cursor_line(ptr)
    }

    public func cursor_col() -> UInt32 {
        __swift_bridge__$FfiAppNode$cursor_col(ptr)
    }

    public func has_top_line() -> Bool {
        __swift_bridge__$FfiAppNode$has_top_line(ptr)
    }

    public func top_line() -> UInt32 {
        __swift_bridge__$FfiAppNode$top_line(ptr)
    }

    public func has_scroll() -> Bool {
        __swift_bridge__$FfiAppNode$has_scroll(ptr)
    }

    public func scroll() -> Double {
        __swift_bridge__$FfiAppNode$scroll(ptr)
    }

    public func orientation() -> UInt8 {
        __swift_bridge__$FfiAppNode$orientation(ptr)
    }

    public func child_count() -> UInt32 {
        __swift_bridge__$FfiAppNode$child_count(ptr)
    }

    public func weight(_ index: UInt32) -> Double {
        __swift_bridge__$FfiAppNode$weight(ptr, index)
    }

    public func child(_ index: UInt32) -> FfiAppNode {
        FfiAppNode(ptr: __swift_bridge__$FfiAppNode$child(ptr, index))
    }
}
extension FfiAppNode: Vectorizable {
    public static func vecOfSelfNew() -> UnsafeMutableRawPointer {
        __swift_bridge__$Vec_FfiAppNode$new()
    }

    public static func vecOfSelfFree(vecPtr: UnsafeMutableRawPointer) {
        __swift_bridge__$Vec_FfiAppNode$drop(vecPtr)
    }

    public static func vecOfSelfPush(vecPtr: UnsafeMutableRawPointer, value: FfiAppNode) {
        __swift_bridge__$Vec_FfiAppNode$push(vecPtr, {value.isOwned = false; return value.ptr;}())
    }

    public static func vecOfSelfPop(vecPtr: UnsafeMutableRawPointer) -> Optional<Self> {
        let pointer = __swift_bridge__$Vec_FfiAppNode$pop(vecPtr)
        if pointer == nil {
            return nil
        } else {
            return (FfiAppNode(ptr: pointer!) as! Self)
        }
    }

    public static func vecOfSelfGet(vecPtr: UnsafeMutableRawPointer, index: UInt) -> Optional<FfiAppNodeRef> {
        let pointer = __swift_bridge__$Vec_FfiAppNode$get(vecPtr, index)
        if pointer == nil {
            return nil
        } else {
            return FfiAppNodeRef(ptr: pointer!)
        }
    }

    public static func vecOfSelfGetMut(vecPtr: UnsafeMutableRawPointer, index: UInt) -> Optional<FfiAppNodeRefMut> {
        let pointer = __swift_bridge__$Vec_FfiAppNode$get_mut(vecPtr, index)
        if pointer == nil {
            return nil
        } else {
            return FfiAppNodeRefMut(ptr: pointer!)
        }
    }

    public static func vecOfSelfAsPtr(vecPtr: UnsafeMutableRawPointer) -> UnsafePointer<FfiAppNodeRef> {
        UnsafePointer<FfiAppNodeRef>(OpaquePointer(__swift_bridge__$Vec_FfiAppNode$as_ptr(vecPtr)))
    }

    public static func vecOfSelfLen(vecPtr: UnsafeMutableRawPointer) -> UInt {
        __swift_bridge__$Vec_FfiAppNode$len(vecPtr)
    }
}


public class FfiProjectState: FfiProjectStateRefMut {
    var isOwned: Bool = true

    public override init(ptr: UnsafeMutableRawPointer) {
        super.init(ptr: ptr)
    }

    deinit {
        if isOwned {
            __swift_bridge__$FfiProjectState$_free(ptr)
        }
    }
}
public class FfiProjectStateRefMut: FfiProjectStateRef {
    public override init(ptr: UnsafeMutableRawPointer) {
        super.init(ptr: ptr)
    }
}
public class FfiProjectStateRef {
    var ptr: UnsafeMutableRawPointer

    public init(ptr: UnsafeMutableRawPointer) {
        self.ptr = ptr
    }
}
extension FfiProjectStateRef {
    public func found() -> Bool {
        __swift_bridge__$FfiProjectState$found(ptr)
    }

    public func has_position() -> Bool {
        __swift_bridge__$FfiProjectState$has_position(ptr)
    }

    public func x() -> Double {
        __swift_bridge__$FfiProjectState$x(ptr)
    }

    public func y() -> Double {
        __swift_bridge__$FfiProjectState$y(ptr)
    }

    public func width() -> Double {
        __swift_bridge__$FfiProjectState$width(ptr)
    }

    public func height() -> Double {
        __swift_bridge__$FfiProjectState$height(ptr)
    }

    public func tab() -> FfiAppTab {
        FfiAppTab(ptr: __swift_bridge__$FfiProjectState$tab(ptr))
    }
}
extension FfiProjectState: Vectorizable {
    public static func vecOfSelfNew() -> UnsafeMutableRawPointer {
        __swift_bridge__$Vec_FfiProjectState$new()
    }

    public static func vecOfSelfFree(vecPtr: UnsafeMutableRawPointer) {
        __swift_bridge__$Vec_FfiProjectState$drop(vecPtr)
    }

    public static func vecOfSelfPush(vecPtr: UnsafeMutableRawPointer, value: FfiProjectState) {
        __swift_bridge__$Vec_FfiProjectState$push(vecPtr, {value.isOwned = false; return value.ptr;}())
    }

    public static func vecOfSelfPop(vecPtr: UnsafeMutableRawPointer) -> Optional<Self> {
        let pointer = __swift_bridge__$Vec_FfiProjectState$pop(vecPtr)
        if pointer == nil {
            return nil
        } else {
            return (FfiProjectState(ptr: pointer!) as! Self)
        }
    }

    public static func vecOfSelfGet(vecPtr: UnsafeMutableRawPointer, index: UInt) -> Optional<FfiProjectStateRef> {
        let pointer = __swift_bridge__$Vec_FfiProjectState$get(vecPtr, index)
        if pointer == nil {
            return nil
        } else {
            return FfiProjectStateRef(ptr: pointer!)
        }
    }

    public static func vecOfSelfGetMut(vecPtr: UnsafeMutableRawPointer, index: UInt) -> Optional<FfiProjectStateRefMut> {
        let pointer = __swift_bridge__$Vec_FfiProjectState$get_mut(vecPtr, index)
        if pointer == nil {
            return nil
        } else {
            return FfiProjectStateRefMut(ptr: pointer!)
        }
    }

    public static func vecOfSelfAsPtr(vecPtr: UnsafeMutableRawPointer) -> UnsafePointer<FfiProjectStateRef> {
        UnsafePointer<FfiProjectStateRef>(OpaquePointer(__swift_bridge__$Vec_FfiProjectState$as_ptr(vecPtr)))
    }

    public static func vecOfSelfLen(vecPtr: UnsafeMutableRawPointer) -> UInt {
        __swift_bridge__$Vec_FfiProjectState$len(vecPtr)
    }
}


public class FfiProjectTab: FfiProjectTabRefMut {
    var isOwned: Bool = true

    public override init(ptr: UnsafeMutableRawPointer) {
        super.init(ptr: ptr)
    }

    deinit {
        if isOwned {
            __swift_bridge__$FfiProjectTab$_free(ptr)
        }
    }
}
public class FfiProjectTabRefMut: FfiProjectTabRef {
    public override init(ptr: UnsafeMutableRawPointer) {
        super.init(ptr: ptr)
    }
}
public class FfiProjectTabRef {
    var ptr: UnsafeMutableRawPointer

    public init(ptr: UnsafeMutableRawPointer) {
        self.ptr = ptr
    }
}
extension FfiProjectTabRef {
    public func found() -> Bool {
        __swift_bridge__$FfiProjectTab$found(ptr)
    }

    public func window() -> UInt32 {
        __swift_bridge__$FfiProjectTab$window(ptr)
    }

    public func tab() -> UInt32 {
        __swift_bridge__$FfiProjectTab$tab(ptr)
    }
}
extension FfiProjectTab: Vectorizable {
    public static func vecOfSelfNew() -> UnsafeMutableRawPointer {
        __swift_bridge__$Vec_FfiProjectTab$new()
    }

    public static func vecOfSelfFree(vecPtr: UnsafeMutableRawPointer) {
        __swift_bridge__$Vec_FfiProjectTab$drop(vecPtr)
    }

    public static func vecOfSelfPush(vecPtr: UnsafeMutableRawPointer, value: FfiProjectTab) {
        __swift_bridge__$Vec_FfiProjectTab$push(vecPtr, {value.isOwned = false; return value.ptr;}())
    }

    public static func vecOfSelfPop(vecPtr: UnsafeMutableRawPointer) -> Optional<Self> {
        let pointer = __swift_bridge__$Vec_FfiProjectTab$pop(vecPtr)
        if pointer == nil {
            return nil
        } else {
            return (FfiProjectTab(ptr: pointer!) as! Self)
        }
    }

    public static func vecOfSelfGet(vecPtr: UnsafeMutableRawPointer, index: UInt) -> Optional<FfiProjectTabRef> {
        let pointer = __swift_bridge__$Vec_FfiProjectTab$get(vecPtr, index)
        if pointer == nil {
            return nil
        } else {
            return FfiProjectTabRef(ptr: pointer!)
        }
    }

    public static func vecOfSelfGetMut(vecPtr: UnsafeMutableRawPointer, index: UInt) -> Optional<FfiProjectTabRefMut> {
        let pointer = __swift_bridge__$Vec_FfiProjectTab$get_mut(vecPtr, index)
        if pointer == nil {
            return nil
        } else {
            return FfiProjectTabRefMut(ptr: pointer!)
        }
    }

    public static func vecOfSelfAsPtr(vecPtr: UnsafeMutableRawPointer) -> UnsafePointer<FfiProjectTabRef> {
        UnsafePointer<FfiProjectTabRef>(OpaquePointer(__swift_bridge__$Vec_FfiProjectTab$as_ptr(vecPtr)))
    }

    public static func vecOfSelfLen(vecPtr: UnsafeMutableRawPointer) -> UInt {
        __swift_bridge__$Vec_FfiProjectTab$len(vecPtr)
    }
}


public class FfiPaneChoice: FfiPaneChoiceRefMut {
    var isOwned: Bool = true

    public override init(ptr: UnsafeMutableRawPointer) {
        super.init(ptr: ptr)
    }

    deinit {
        if isOwned {
            __swift_bridge__$FfiPaneChoice$_free(ptr)
        }
    }
}
public class FfiPaneChoiceRefMut: FfiPaneChoiceRef {
    public override init(ptr: UnsafeMutableRawPointer) {
        super.init(ptr: ptr)
    }
}
public class FfiPaneChoiceRef {
    var ptr: UnsafeMutableRawPointer

    public init(ptr: UnsafeMutableRawPointer) {
        self.ptr = ptr
    }
}
extension FfiPaneChoiceRef {
    public func unzoom() -> Bool {
        __swift_bridge__$FfiPaneChoice$unzoom(ptr)
    }

    public func placement() -> UInt8 {
        __swift_bridge__$FfiPaneChoice$placement(ptr)
    }

    public func pane() -> UInt32 {
        __swift_bridge__$FfiPaneChoice$pane(ptr)
    }
}
extension FfiPaneChoice: Vectorizable {
    public static func vecOfSelfNew() -> UnsafeMutableRawPointer {
        __swift_bridge__$Vec_FfiPaneChoice$new()
    }

    public static func vecOfSelfFree(vecPtr: UnsafeMutableRawPointer) {
        __swift_bridge__$Vec_FfiPaneChoice$drop(vecPtr)
    }

    public static func vecOfSelfPush(vecPtr: UnsafeMutableRawPointer, value: FfiPaneChoice) {
        __swift_bridge__$Vec_FfiPaneChoice$push(vecPtr, {value.isOwned = false; return value.ptr;}())
    }

    public static func vecOfSelfPop(vecPtr: UnsafeMutableRawPointer) -> Optional<Self> {
        let pointer = __swift_bridge__$Vec_FfiPaneChoice$pop(vecPtr)
        if pointer == nil {
            return nil
        } else {
            return (FfiPaneChoice(ptr: pointer!) as! Self)
        }
    }

    public static func vecOfSelfGet(vecPtr: UnsafeMutableRawPointer, index: UInt) -> Optional<FfiPaneChoiceRef> {
        let pointer = __swift_bridge__$Vec_FfiPaneChoice$get(vecPtr, index)
        if pointer == nil {
            return nil
        } else {
            return FfiPaneChoiceRef(ptr: pointer!)
        }
    }

    public static func vecOfSelfGetMut(vecPtr: UnsafeMutableRawPointer, index: UInt) -> Optional<FfiPaneChoiceRefMut> {
        let pointer = __swift_bridge__$Vec_FfiPaneChoice$get_mut(vecPtr, index)
        if pointer == nil {
            return nil
        } else {
            return FfiPaneChoiceRefMut(ptr: pointer!)
        }
    }

    public static func vecOfSelfAsPtr(vecPtr: UnsafeMutableRawPointer) -> UnsafePointer<FfiPaneChoiceRef> {
        UnsafePointer<FfiPaneChoiceRef>(OpaquePointer(__swift_bridge__$Vec_FfiPaneChoice$as_ptr(vecPtr)))
    }

    public static func vecOfSelfLen(vecPtr: UnsafeMutableRawPointer) -> UInt {
        __swift_bridge__$Vec_FfiPaneChoice$len(vecPtr)
    }
}


public class FfiOpenItem: FfiOpenItemRefMut {
    var isOwned: Bool = true

    public override init(ptr: UnsafeMutableRawPointer) {
        super.init(ptr: ptr)
    }

    deinit {
        if isOwned {
            __swift_bridge__$FfiOpenItem$_free(ptr)
        }
    }
}
public class FfiOpenItemRefMut: FfiOpenItemRef {
    public override init(ptr: UnsafeMutableRawPointer) {
        super.init(ptr: ptr)
    }
}
public class FfiOpenItemRef {
    var ptr: UnsafeMutableRawPointer

    public init(ptr: UnsafeMutableRawPointer) {
        self.ptr = ptr
    }
}
extension FfiOpenItemRef {
    public func kind() -> UInt8 {
        __swift_bridge__$FfiOpenItem$kind(ptr)
    }

    public func path() -> RustString {
        RustString(ptr: __swift_bridge__$FfiOpenItem$path(ptr))
    }

    public func tab_id() -> RustString {
        RustString(ptr: __swift_bridge__$FfiOpenItem$tab_id(ptr))
    }

    public func waits() -> Bool {
        __swift_bridge__$FfiOpenItem$waits(ptr)
    }
}
extension FfiOpenItem: Vectorizable {
    public static func vecOfSelfNew() -> UnsafeMutableRawPointer {
        __swift_bridge__$Vec_FfiOpenItem$new()
    }

    public static func vecOfSelfFree(vecPtr: UnsafeMutableRawPointer) {
        __swift_bridge__$Vec_FfiOpenItem$drop(vecPtr)
    }

    public static func vecOfSelfPush(vecPtr: UnsafeMutableRawPointer, value: FfiOpenItem) {
        __swift_bridge__$Vec_FfiOpenItem$push(vecPtr, {value.isOwned = false; return value.ptr;}())
    }

    public static func vecOfSelfPop(vecPtr: UnsafeMutableRawPointer) -> Optional<Self> {
        let pointer = __swift_bridge__$Vec_FfiOpenItem$pop(vecPtr)
        if pointer == nil {
            return nil
        } else {
            return (FfiOpenItem(ptr: pointer!) as! Self)
        }
    }

    public static func vecOfSelfGet(vecPtr: UnsafeMutableRawPointer, index: UInt) -> Optional<FfiOpenItemRef> {
        let pointer = __swift_bridge__$Vec_FfiOpenItem$get(vecPtr, index)
        if pointer == nil {
            return nil
        } else {
            return FfiOpenItemRef(ptr: pointer!)
        }
    }

    public static func vecOfSelfGetMut(vecPtr: UnsafeMutableRawPointer, index: UInt) -> Optional<FfiOpenItemRefMut> {
        let pointer = __swift_bridge__$Vec_FfiOpenItem$get_mut(vecPtr, index)
        if pointer == nil {
            return nil
        } else {
            return FfiOpenItemRefMut(ptr: pointer!)
        }
    }

    public static func vecOfSelfAsPtr(vecPtr: UnsafeMutableRawPointer) -> UnsafePointer<FfiOpenItemRef> {
        UnsafePointer<FfiOpenItemRef>(OpaquePointer(__swift_bridge__$Vec_FfiOpenItem$as_ptr(vecPtr)))
    }

    public static func vecOfSelfLen(vecPtr: UnsafeMutableRawPointer) -> UInt {
        __swift_bridge__$Vec_FfiOpenItem$len(vecPtr)
    }
}


public class FfiFrame: FfiFrameRefMut {
    var isOwned: Bool = true

    public override init(ptr: UnsafeMutableRawPointer) {
        super.init(ptr: ptr)
    }

    deinit {
        if isOwned {
            __swift_bridge__$FfiFrame$_free(ptr)
        }
    }
}
public class FfiFrameRefMut: FfiFrameRef {
    public override init(ptr: UnsafeMutableRawPointer) {
        super.init(ptr: ptr)
    }
}
public class FfiFrameRef {
    var ptr: UnsafeMutableRawPointer

    public init(ptr: UnsafeMutableRawPointer) {
        self.ptr = ptr
    }
}
extension FfiFrameRef {
    public func cols() -> UInt16 {
        __swift_bridge__$FfiFrame$cols(ptr)
    }

    public func rows() -> UInt16 {
        __swift_bridge__$FfiFrame$rows(ptr)
    }

    public func cursor_row() -> UInt16 {
        __swift_bridge__$FfiFrame$cursor_row(ptr)
    }

    public func cursor_col() -> UInt16 {
        __swift_bridge__$FfiFrame$cursor_col(ptr)
    }

    public func cursor_shape() -> UInt8 {
        __swift_bridge__$FfiFrame$cursor_shape(ptr)
    }

    public func cursor_cell_percentage() -> UInt32 {
        __swift_bridge__$FfiFrame$cursor_cell_percentage(ptr)
    }

    public func cursor_visible() -> Bool {
        __swift_bridge__$FfiFrame$cursor_visible(ptr)
    }

    public func grid_id() -> UInt64 {
        __swift_bridge__$FfiFrame$grid_id(ptr)
    }

    public func cursor_grid() -> UInt64 {
        __swift_bridge__$FfiFrame$cursor_grid(ptr)
    }

    public func default_fg() -> FfiColor {
        __swift_bridge__$FfiFrame$default_fg(ptr).intoSwiftRepr()
    }

    public func default_bg() -> FfiColor {
        __swift_bridge__$FfiFrame$default_bg(ptr).intoSwiftRepr()
    }

    public func cursor_fill() -> FfiColor {
        __swift_bridge__$FfiFrame$cursor_fill(ptr).intoSwiftRepr()
    }

    public func cursor_glyph() -> FfiColor {
        __swift_bridge__$FfiFrame$cursor_glyph(ptr).intoSwiftRepr()
    }

    public func cells() -> RustVec<FfiCell> {
        RustVec(ptr: __swift_bridge__$FfiFrame$cells(ptr))
    }

    public func topline() -> UInt64 {
        __swift_bridge__$FfiFrame$topline(ptr)
    }

    public func botline() -> UInt64 {
        __swift_bridge__$FfiFrame$botline(ptr)
    }

    public func curline() -> UInt64 {
        __swift_bridge__$FfiFrame$curline(ptr)
    }

    public func linecount() -> UInt64 {
        __swift_bridge__$FfiFrame$linecount(ptr)
    }
}
extension FfiFrame: Vectorizable {
    public static func vecOfSelfNew() -> UnsafeMutableRawPointer {
        __swift_bridge__$Vec_FfiFrame$new()
    }

    public static func vecOfSelfFree(vecPtr: UnsafeMutableRawPointer) {
        __swift_bridge__$Vec_FfiFrame$drop(vecPtr)
    }

    public static func vecOfSelfPush(vecPtr: UnsafeMutableRawPointer, value: FfiFrame) {
        __swift_bridge__$Vec_FfiFrame$push(vecPtr, {value.isOwned = false; return value.ptr;}())
    }

    public static func vecOfSelfPop(vecPtr: UnsafeMutableRawPointer) -> Optional<Self> {
        let pointer = __swift_bridge__$Vec_FfiFrame$pop(vecPtr)
        if pointer == nil {
            return nil
        } else {
            return (FfiFrame(ptr: pointer!) as! Self)
        }
    }

    public static func vecOfSelfGet(vecPtr: UnsafeMutableRawPointer, index: UInt) -> Optional<FfiFrameRef> {
        let pointer = __swift_bridge__$Vec_FfiFrame$get(vecPtr, index)
        if pointer == nil {
            return nil
        } else {
            return FfiFrameRef(ptr: pointer!)
        }
    }

    public static func vecOfSelfGetMut(vecPtr: UnsafeMutableRawPointer, index: UInt) -> Optional<FfiFrameRefMut> {
        let pointer = __swift_bridge__$Vec_FfiFrame$get_mut(vecPtr, index)
        if pointer == nil {
            return nil
        } else {
            return FfiFrameRefMut(ptr: pointer!)
        }
    }

    public static func vecOfSelfAsPtr(vecPtr: UnsafeMutableRawPointer) -> UnsafePointer<FfiFrameRef> {
        UnsafePointer<FfiFrameRef>(OpaquePointer(__swift_bridge__$Vec_FfiFrame$as_ptr(vecPtr)))
    }

    public static func vecOfSelfLen(vecPtr: UnsafeMutableRawPointer) -> UInt {
        __swift_bridge__$Vec_FfiFrame$len(vecPtr)
    }
}


public class FfiCell: FfiCellRefMut {
    var isOwned: Bool = true

    public override init(ptr: UnsafeMutableRawPointer) {
        super.init(ptr: ptr)
    }

    deinit {
        if isOwned {
            __swift_bridge__$FfiCell$_free(ptr)
        }
    }
}
public class FfiCellRefMut: FfiCellRef {
    public override init(ptr: UnsafeMutableRawPointer) {
        super.init(ptr: ptr)
    }
}
public class FfiCellRef {
    var ptr: UnsafeMutableRawPointer

    public init(ptr: UnsafeMutableRawPointer) {
        self.ptr = ptr
    }
}
extension FfiCellRef {
    public func text() -> RustString {
        RustString(ptr: __swift_bridge__$FfiCell$text(ptr))
    }

    public func fg() -> FfiColor {
        __swift_bridge__$FfiCell$fg(ptr).intoSwiftRepr()
    }

    public func bg() -> FfiColor {
        __swift_bridge__$FfiCell$bg(ptr).intoSwiftRepr()
    }

    public func special() -> FfiColor {
        __swift_bridge__$FfiCell$special(ptr).intoSwiftRepr()
    }

    public func flags() -> UInt8 {
        __swift_bridge__$FfiCell$flags(ptr)
    }
}
extension FfiCell: Vectorizable {
    public static func vecOfSelfNew() -> UnsafeMutableRawPointer {
        __swift_bridge__$Vec_FfiCell$new()
    }

    public static func vecOfSelfFree(vecPtr: UnsafeMutableRawPointer) {
        __swift_bridge__$Vec_FfiCell$drop(vecPtr)
    }

    public static func vecOfSelfPush(vecPtr: UnsafeMutableRawPointer, value: FfiCell) {
        __swift_bridge__$Vec_FfiCell$push(vecPtr, {value.isOwned = false; return value.ptr;}())
    }

    public static func vecOfSelfPop(vecPtr: UnsafeMutableRawPointer) -> Optional<Self> {
        let pointer = __swift_bridge__$Vec_FfiCell$pop(vecPtr)
        if pointer == nil {
            return nil
        } else {
            return (FfiCell(ptr: pointer!) as! Self)
        }
    }

    public static func vecOfSelfGet(vecPtr: UnsafeMutableRawPointer, index: UInt) -> Optional<FfiCellRef> {
        let pointer = __swift_bridge__$Vec_FfiCell$get(vecPtr, index)
        if pointer == nil {
            return nil
        } else {
            return FfiCellRef(ptr: pointer!)
        }
    }

    public static func vecOfSelfGetMut(vecPtr: UnsafeMutableRawPointer, index: UInt) -> Optional<FfiCellRefMut> {
        let pointer = __swift_bridge__$Vec_FfiCell$get_mut(vecPtr, index)
        if pointer == nil {
            return nil
        } else {
            return FfiCellRefMut(ptr: pointer!)
        }
    }

    public static func vecOfSelfAsPtr(vecPtr: UnsafeMutableRawPointer) -> UnsafePointer<FfiCellRef> {
        UnsafePointer<FfiCellRef>(OpaquePointer(__swift_bridge__$Vec_FfiCell$as_ptr(vecPtr)))
    }

    public static func vecOfSelfLen(vecPtr: UnsafeMutableRawPointer) -> UInt {
        __swift_bridge__$Vec_FfiCell$len(vecPtr)
    }
}


public class FfiLayoutIntent: FfiLayoutIntentRefMut {
    var isOwned: Bool = true

    public override init(ptr: UnsafeMutableRawPointer) {
        super.init(ptr: ptr)
    }

    deinit {
        if isOwned {
            __swift_bridge__$FfiLayoutIntent$_free(ptr)
        }
    }
}
public class FfiLayoutIntentRefMut: FfiLayoutIntentRef {
    public override init(ptr: UnsafeMutableRawPointer) {
        super.init(ptr: ptr)
    }
}
public class FfiLayoutIntentRef {
    var ptr: UnsafeMutableRawPointer

    public init(ptr: UnsafeMutableRawPointer) {
        self.ptr = ptr
    }
}
extension FfiLayoutIntentRef {
    public func kind() -> UInt8 {
        __swift_bridge__$FfiLayoutIntent$kind(ptr)
    }

    public func orientation() -> UInt8 {
        __swift_bridge__$FfiLayoutIntent$orientation(ptr)
    }

    public func dir() -> UInt8 {
        __swift_bridge__$FfiLayoutIntent$dir(ptr)
    }

    public func from() -> Int64 {
        __swift_bridge__$FfiLayoutIntent$from(ptr)
    }

    public func win() -> Int64 {
        __swift_bridge__$FfiLayoutIntent$win(ptr)
    }
}
extension FfiLayoutIntent: Vectorizable {
    public static func vecOfSelfNew() -> UnsafeMutableRawPointer {
        __swift_bridge__$Vec_FfiLayoutIntent$new()
    }

    public static func vecOfSelfFree(vecPtr: UnsafeMutableRawPointer) {
        __swift_bridge__$Vec_FfiLayoutIntent$drop(vecPtr)
    }

    public static func vecOfSelfPush(vecPtr: UnsafeMutableRawPointer, value: FfiLayoutIntent) {
        __swift_bridge__$Vec_FfiLayoutIntent$push(vecPtr, {value.isOwned = false; return value.ptr;}())
    }

    public static func vecOfSelfPop(vecPtr: UnsafeMutableRawPointer) -> Optional<Self> {
        let pointer = __swift_bridge__$Vec_FfiLayoutIntent$pop(vecPtr)
        if pointer == nil {
            return nil
        } else {
            return (FfiLayoutIntent(ptr: pointer!) as! Self)
        }
    }

    public static func vecOfSelfGet(vecPtr: UnsafeMutableRawPointer, index: UInt) -> Optional<FfiLayoutIntentRef> {
        let pointer = __swift_bridge__$Vec_FfiLayoutIntent$get(vecPtr, index)
        if pointer == nil {
            return nil
        } else {
            return FfiLayoutIntentRef(ptr: pointer!)
        }
    }

    public static func vecOfSelfGetMut(vecPtr: UnsafeMutableRawPointer, index: UInt) -> Optional<FfiLayoutIntentRefMut> {
        let pointer = __swift_bridge__$Vec_FfiLayoutIntent$get_mut(vecPtr, index)
        if pointer == nil {
            return nil
        } else {
            return FfiLayoutIntentRefMut(ptr: pointer!)
        }
    }

    public static func vecOfSelfAsPtr(vecPtr: UnsafeMutableRawPointer) -> UnsafePointer<FfiLayoutIntentRef> {
        UnsafePointer<FfiLayoutIntentRef>(OpaquePointer(__swift_bridge__$Vec_FfiLayoutIntent$as_ptr(vecPtr)))
    }

    public static func vecOfSelfLen(vecPtr: UnsafeMutableRawPointer) -> UInt {
        __swift_bridge__$Vec_FfiLayoutIntent$len(vecPtr)
    }
}


public class FfiCommand: FfiCommandRefMut {
    var isOwned: Bool = true

    public override init(ptr: UnsafeMutableRawPointer) {
        super.init(ptr: ptr)
    }

    deinit {
        if isOwned {
            __swift_bridge__$FfiCommand$_free(ptr)
        }
    }
}
public class FfiCommandRefMut: FfiCommandRef {
    public override init(ptr: UnsafeMutableRawPointer) {
        super.init(ptr: ptr)
    }
}
public class FfiCommandRef {
    var ptr: UnsafeMutableRawPointer

    public init(ptr: UnsafeMutableRawPointer) {
        self.ptr = ptr
    }
}
extension FfiCommandRef {
    public func kind() -> UInt8 {
        __swift_bridge__$FfiCommand$kind(ptr)
    }
}
extension FfiCommand: Vectorizable {
    public static func vecOfSelfNew() -> UnsafeMutableRawPointer {
        __swift_bridge__$Vec_FfiCommand$new()
    }

    public static func vecOfSelfFree(vecPtr: UnsafeMutableRawPointer) {
        __swift_bridge__$Vec_FfiCommand$drop(vecPtr)
    }

    public static func vecOfSelfPush(vecPtr: UnsafeMutableRawPointer, value: FfiCommand) {
        __swift_bridge__$Vec_FfiCommand$push(vecPtr, {value.isOwned = false; return value.ptr;}())
    }

    public static func vecOfSelfPop(vecPtr: UnsafeMutableRawPointer) -> Optional<Self> {
        let pointer = __swift_bridge__$Vec_FfiCommand$pop(vecPtr)
        if pointer == nil {
            return nil
        } else {
            return (FfiCommand(ptr: pointer!) as! Self)
        }
    }

    public static func vecOfSelfGet(vecPtr: UnsafeMutableRawPointer, index: UInt) -> Optional<FfiCommandRef> {
        let pointer = __swift_bridge__$Vec_FfiCommand$get(vecPtr, index)
        if pointer == nil {
            return nil
        } else {
            return FfiCommandRef(ptr: pointer!)
        }
    }

    public static func vecOfSelfGetMut(vecPtr: UnsafeMutableRawPointer, index: UInt) -> Optional<FfiCommandRefMut> {
        let pointer = __swift_bridge__$Vec_FfiCommand$get_mut(vecPtr, index)
        if pointer == nil {
            return nil
        } else {
            return FfiCommandRefMut(ptr: pointer!)
        }
    }

    public static func vecOfSelfAsPtr(vecPtr: UnsafeMutableRawPointer) -> UnsafePointer<FfiCommandRef> {
        UnsafePointer<FfiCommandRef>(OpaquePointer(__swift_bridge__$Vec_FfiCommand$as_ptr(vecPtr)))
    }

    public static func vecOfSelfLen(vecPtr: UnsafeMutableRawPointer) -> UInt {
        __swift_bridge__$Vec_FfiCommand$len(vecPtr)
    }
}


public class FfiCommandPalette: FfiCommandPaletteRefMut {
    var isOwned: Bool = true

    public override init(ptr: UnsafeMutableRawPointer) {
        super.init(ptr: ptr)
    }

    deinit {
        if isOwned {
            __swift_bridge__$FfiCommandPalette$_free(ptr)
        }
    }
}
public class FfiCommandPaletteRefMut: FfiCommandPaletteRef {
    public override init(ptr: UnsafeMutableRawPointer) {
        super.init(ptr: ptr)
    }
}
public class FfiCommandPaletteRef {
    var ptr: UnsafeMutableRawPointer

    public init(ptr: UnsafeMutableRawPointer) {
        self.ptr = ptr
    }
}
extension FfiCommandPaletteRef {
    public func add_window<GenericIntoRustString: IntoRustString>(_ index: UInt16, _ cwd: GenericIntoRustString) {
        __swift_bridge__$FfiCommandPalette$add_window(ptr, index, { let rustString = cwd.intoRustString(); rustString.isOwned = false; return rustString.ptr }())
    }

    public func ranked<GenericIntoRustString: IntoRustString>(_ query: GenericIntoRustString, _ max: UInt) -> RustVec<FfiCommandRow> {
        RustVec(ptr: __swift_bridge__$FfiCommandPalette$ranked(ptr, { let rustString = query.intoRustString(); rustString.isOwned = false; return rustString.ptr }(), max))
    }

    public func record<GenericIntoRustString: IntoRustString>(_ key: GenericIntoRustString) {
        __swift_bridge__$FfiCommandPalette$record(ptr, { let rustString = key.intoRustString(); rustString.isOwned = false; return rustString.ptr }())
    }
}
extension FfiCommandPalette: Vectorizable {
    public static func vecOfSelfNew() -> UnsafeMutableRawPointer {
        __swift_bridge__$Vec_FfiCommandPalette$new()
    }

    public static func vecOfSelfFree(vecPtr: UnsafeMutableRawPointer) {
        __swift_bridge__$Vec_FfiCommandPalette$drop(vecPtr)
    }

    public static func vecOfSelfPush(vecPtr: UnsafeMutableRawPointer, value: FfiCommandPalette) {
        __swift_bridge__$Vec_FfiCommandPalette$push(vecPtr, {value.isOwned = false; return value.ptr;}())
    }

    public static func vecOfSelfPop(vecPtr: UnsafeMutableRawPointer) -> Optional<Self> {
        let pointer = __swift_bridge__$Vec_FfiCommandPalette$pop(vecPtr)
        if pointer == nil {
            return nil
        } else {
            return (FfiCommandPalette(ptr: pointer!) as! Self)
        }
    }

    public static func vecOfSelfGet(vecPtr: UnsafeMutableRawPointer, index: UInt) -> Optional<FfiCommandPaletteRef> {
        let pointer = __swift_bridge__$Vec_FfiCommandPalette$get(vecPtr, index)
        if pointer == nil {
            return nil
        } else {
            return FfiCommandPaletteRef(ptr: pointer!)
        }
    }

    public static func vecOfSelfGetMut(vecPtr: UnsafeMutableRawPointer, index: UInt) -> Optional<FfiCommandPaletteRefMut> {
        let pointer = __swift_bridge__$Vec_FfiCommandPalette$get_mut(vecPtr, index)
        if pointer == nil {
            return nil
        } else {
            return FfiCommandPaletteRefMut(ptr: pointer!)
        }
    }

    public static func vecOfSelfAsPtr(vecPtr: UnsafeMutableRawPointer) -> UnsafePointer<FfiCommandPaletteRef> {
        UnsafePointer<FfiCommandPaletteRef>(OpaquePointer(__swift_bridge__$Vec_FfiCommandPalette$as_ptr(vecPtr)))
    }

    public static func vecOfSelfLen(vecPtr: UnsafeMutableRawPointer) -> UInt {
        __swift_bridge__$Vec_FfiCommandPalette$len(vecPtr)
    }
}


public class FfiCommandRow: FfiCommandRowRefMut {
    var isOwned: Bool = true

    public override init(ptr: UnsafeMutableRawPointer) {
        super.init(ptr: ptr)
    }

    deinit {
        if isOwned {
            __swift_bridge__$FfiCommandRow$_free(ptr)
        }
    }
}
public class FfiCommandRowRefMut: FfiCommandRowRef {
    public override init(ptr: UnsafeMutableRawPointer) {
        super.init(ptr: ptr)
    }
}
public class FfiCommandRowRef {
    var ptr: UnsafeMutableRawPointer

    public init(ptr: UnsafeMutableRawPointer) {
        self.ptr = ptr
    }
}
extension FfiCommandRowRef {
    public func display() -> RustString {
        RustString(ptr: __swift_bridge__$FfiCommandRow$display(ptr))
    }

    public func row_kind() -> UInt8 {
        __swift_bridge__$FfiCommandRow$row_kind(ptr)
    }

    public func command_kind() -> UInt8 {
        __swift_bridge__$FfiCommandRow$command_kind(ptr)
    }

    public func window_index() -> UInt16 {
        __swift_bridge__$FfiCommandRow$window_index(ptr)
    }

    public func path() -> RustString {
        RustString(ptr: __swift_bridge__$FfiCommandRow$path(ptr))
    }

    public func mru_key() -> RustString {
        RustString(ptr: __swift_bridge__$FfiCommandRow$mru_key(ptr))
    }
}
extension FfiCommandRow: Vectorizable {
    public static func vecOfSelfNew() -> UnsafeMutableRawPointer {
        __swift_bridge__$Vec_FfiCommandRow$new()
    }

    public static func vecOfSelfFree(vecPtr: UnsafeMutableRawPointer) {
        __swift_bridge__$Vec_FfiCommandRow$drop(vecPtr)
    }

    public static func vecOfSelfPush(vecPtr: UnsafeMutableRawPointer, value: FfiCommandRow) {
        __swift_bridge__$Vec_FfiCommandRow$push(vecPtr, {value.isOwned = false; return value.ptr;}())
    }

    public static func vecOfSelfPop(vecPtr: UnsafeMutableRawPointer) -> Optional<Self> {
        let pointer = __swift_bridge__$Vec_FfiCommandRow$pop(vecPtr)
        if pointer == nil {
            return nil
        } else {
            return (FfiCommandRow(ptr: pointer!) as! Self)
        }
    }

    public static func vecOfSelfGet(vecPtr: UnsafeMutableRawPointer, index: UInt) -> Optional<FfiCommandRowRef> {
        let pointer = __swift_bridge__$Vec_FfiCommandRow$get(vecPtr, index)
        if pointer == nil {
            return nil
        } else {
            return FfiCommandRowRef(ptr: pointer!)
        }
    }

    public static func vecOfSelfGetMut(vecPtr: UnsafeMutableRawPointer, index: UInt) -> Optional<FfiCommandRowRefMut> {
        let pointer = __swift_bridge__$Vec_FfiCommandRow$get_mut(vecPtr, index)
        if pointer == nil {
            return nil
        } else {
            return FfiCommandRowRefMut(ptr: pointer!)
        }
    }

    public static func vecOfSelfAsPtr(vecPtr: UnsafeMutableRawPointer) -> UnsafePointer<FfiCommandRowRef> {
        UnsafePointer<FfiCommandRowRef>(OpaquePointer(__swift_bridge__$Vec_FfiCommandRow$as_ptr(vecPtr)))
    }

    public static func vecOfSelfLen(vecPtr: UnsafeMutableRawPointer) -> UInt {
        __swift_bridge__$Vec_FfiCommandRow$len(vecPtr)
    }
}


public class FfiBufferPicker: FfiBufferPickerRefMut {
    var isOwned: Bool = true

    public override init(ptr: UnsafeMutableRawPointer) {
        super.init(ptr: ptr)
    }

    deinit {
        if isOwned {
            __swift_bridge__$FfiBufferPicker$_free(ptr)
        }
    }
}
public class FfiBufferPickerRefMut: FfiBufferPickerRef {
    public override init(ptr: UnsafeMutableRawPointer) {
        super.init(ptr: ptr)
    }
}
public class FfiBufferPickerRef {
    var ptr: UnsafeMutableRawPointer

    public init(ptr: UnsafeMutableRawPointer) {
        self.ptr = ptr
    }
}
extension FfiBufferPickerRef {
    public func ranked<GenericIntoRustString: IntoRustString>(_ query: GenericIntoRustString, _ max: UInt) -> RustVec<FfiBufferRow> {
        RustVec(ptr: __swift_bridge__$FfiBufferPicker$ranked(ptr, { let rustString = query.intoRustString(); rustString.isOwned = false; return rustString.ptr }(), max))
    }
}
extension FfiBufferPicker: Vectorizable {
    public static func vecOfSelfNew() -> UnsafeMutableRawPointer {
        __swift_bridge__$Vec_FfiBufferPicker$new()
    }

    public static func vecOfSelfFree(vecPtr: UnsafeMutableRawPointer) {
        __swift_bridge__$Vec_FfiBufferPicker$drop(vecPtr)
    }

    public static func vecOfSelfPush(vecPtr: UnsafeMutableRawPointer, value: FfiBufferPicker) {
        __swift_bridge__$Vec_FfiBufferPicker$push(vecPtr, {value.isOwned = false; return value.ptr;}())
    }

    public static func vecOfSelfPop(vecPtr: UnsafeMutableRawPointer) -> Optional<Self> {
        let pointer = __swift_bridge__$Vec_FfiBufferPicker$pop(vecPtr)
        if pointer == nil {
            return nil
        } else {
            return (FfiBufferPicker(ptr: pointer!) as! Self)
        }
    }

    public static func vecOfSelfGet(vecPtr: UnsafeMutableRawPointer, index: UInt) -> Optional<FfiBufferPickerRef> {
        let pointer = __swift_bridge__$Vec_FfiBufferPicker$get(vecPtr, index)
        if pointer == nil {
            return nil
        } else {
            return FfiBufferPickerRef(ptr: pointer!)
        }
    }

    public static func vecOfSelfGetMut(vecPtr: UnsafeMutableRawPointer, index: UInt) -> Optional<FfiBufferPickerRefMut> {
        let pointer = __swift_bridge__$Vec_FfiBufferPicker$get_mut(vecPtr, index)
        if pointer == nil {
            return nil
        } else {
            return FfiBufferPickerRefMut(ptr: pointer!)
        }
    }

    public static func vecOfSelfAsPtr(vecPtr: UnsafeMutableRawPointer) -> UnsafePointer<FfiBufferPickerRef> {
        UnsafePointer<FfiBufferPickerRef>(OpaquePointer(__swift_bridge__$Vec_FfiBufferPicker$as_ptr(vecPtr)))
    }

    public static func vecOfSelfLen(vecPtr: UnsafeMutableRawPointer) -> UInt {
        __swift_bridge__$Vec_FfiBufferPicker$len(vecPtr)
    }
}


public class FfiBufferRow: FfiBufferRowRefMut {
    var isOwned: Bool = true

    public override init(ptr: UnsafeMutableRawPointer) {
        super.init(ptr: ptr)
    }

    deinit {
        if isOwned {
            __swift_bridge__$FfiBufferRow$_free(ptr)
        }
    }
}
public class FfiBufferRowRefMut: FfiBufferRowRef {
    public override init(ptr: UnsafeMutableRawPointer) {
        super.init(ptr: ptr)
    }
}
public class FfiBufferRowRef {
    var ptr: UnsafeMutableRawPointer

    public init(ptr: UnsafeMutableRawPointer) {
        self.ptr = ptr
    }
}
extension FfiBufferRowRef {
    public func bufnr() -> Int64 {
        __swift_bridge__$FfiBufferRow$bufnr(ptr)
    }

    public func display() -> RustString {
        RustString(ptr: __swift_bridge__$FfiBufferRow$display(ptr))
    }

    public func modified() -> Bool {
        __swift_bridge__$FfiBufferRow$modified(ptr)
    }
}
extension FfiBufferRow: Vectorizable {
    public static func vecOfSelfNew() -> UnsafeMutableRawPointer {
        __swift_bridge__$Vec_FfiBufferRow$new()
    }

    public static func vecOfSelfFree(vecPtr: UnsafeMutableRawPointer) {
        __swift_bridge__$Vec_FfiBufferRow$drop(vecPtr)
    }

    public static func vecOfSelfPush(vecPtr: UnsafeMutableRawPointer, value: FfiBufferRow) {
        __swift_bridge__$Vec_FfiBufferRow$push(vecPtr, {value.isOwned = false; return value.ptr;}())
    }

    public static func vecOfSelfPop(vecPtr: UnsafeMutableRawPointer) -> Optional<Self> {
        let pointer = __swift_bridge__$Vec_FfiBufferRow$pop(vecPtr)
        if pointer == nil {
            return nil
        } else {
            return (FfiBufferRow(ptr: pointer!) as! Self)
        }
    }

    public static func vecOfSelfGet(vecPtr: UnsafeMutableRawPointer, index: UInt) -> Optional<FfiBufferRowRef> {
        let pointer = __swift_bridge__$Vec_FfiBufferRow$get(vecPtr, index)
        if pointer == nil {
            return nil
        } else {
            return FfiBufferRowRef(ptr: pointer!)
        }
    }

    public static func vecOfSelfGetMut(vecPtr: UnsafeMutableRawPointer, index: UInt) -> Optional<FfiBufferRowRefMut> {
        let pointer = __swift_bridge__$Vec_FfiBufferRow$get_mut(vecPtr, index)
        if pointer == nil {
            return nil
        } else {
            return FfiBufferRowRefMut(ptr: pointer!)
        }
    }

    public static func vecOfSelfAsPtr(vecPtr: UnsafeMutableRawPointer) -> UnsafePointer<FfiBufferRowRef> {
        UnsafePointer<FfiBufferRowRef>(OpaquePointer(__swift_bridge__$Vec_FfiBufferRow$as_ptr(vecPtr)))
    }

    public static func vecOfSelfLen(vecPtr: UnsafeMutableRawPointer) -> UInt {
        __swift_bridge__$Vec_FfiBufferRow$len(vecPtr)
    }
}


public class FfiWinInfo: FfiWinInfoRefMut {
    var isOwned: Bool = true

    public override init(ptr: UnsafeMutableRawPointer) {
        super.init(ptr: ptr)
    }

    deinit {
        if isOwned {
            __swift_bridge__$FfiWinInfo$_free(ptr)
        }
    }
}
public class FfiWinInfoRefMut: FfiWinInfoRef {
    public override init(ptr: UnsafeMutableRawPointer) {
        super.init(ptr: ptr)
    }
}
public class FfiWinInfoRef {
    var ptr: UnsafeMutableRawPointer

    public init(ptr: UnsafeMutableRawPointer) {
        self.ptr = ptr
    }
}
extension FfiWinInfoRef {
    public func grid() -> UInt64 {
        __swift_bridge__$FfiWinInfo$grid(ptr)
    }

    public func win() -> Int64 {
        __swift_bridge__$FfiWinInfo$win(ptr)
    }

    public func hidden() -> Bool {
        __swift_bridge__$FfiWinInfo$hidden(ptr)
    }
}
extension FfiWinInfo: Vectorizable {
    public static func vecOfSelfNew() -> UnsafeMutableRawPointer {
        __swift_bridge__$Vec_FfiWinInfo$new()
    }

    public static func vecOfSelfFree(vecPtr: UnsafeMutableRawPointer) {
        __swift_bridge__$Vec_FfiWinInfo$drop(vecPtr)
    }

    public static func vecOfSelfPush(vecPtr: UnsafeMutableRawPointer, value: FfiWinInfo) {
        __swift_bridge__$Vec_FfiWinInfo$push(vecPtr, {value.isOwned = false; return value.ptr;}())
    }

    public static func vecOfSelfPop(vecPtr: UnsafeMutableRawPointer) -> Optional<Self> {
        let pointer = __swift_bridge__$Vec_FfiWinInfo$pop(vecPtr)
        if pointer == nil {
            return nil
        } else {
            return (FfiWinInfo(ptr: pointer!) as! Self)
        }
    }

    public static func vecOfSelfGet(vecPtr: UnsafeMutableRawPointer, index: UInt) -> Optional<FfiWinInfoRef> {
        let pointer = __swift_bridge__$Vec_FfiWinInfo$get(vecPtr, index)
        if pointer == nil {
            return nil
        } else {
            return FfiWinInfoRef(ptr: pointer!)
        }
    }

    public static func vecOfSelfGetMut(vecPtr: UnsafeMutableRawPointer, index: UInt) -> Optional<FfiWinInfoRefMut> {
        let pointer = __swift_bridge__$Vec_FfiWinInfo$get_mut(vecPtr, index)
        if pointer == nil {
            return nil
        } else {
            return FfiWinInfoRefMut(ptr: pointer!)
        }
    }

    public static func vecOfSelfAsPtr(vecPtr: UnsafeMutableRawPointer) -> UnsafePointer<FfiWinInfoRef> {
        UnsafePointer<FfiWinInfoRef>(OpaquePointer(__swift_bridge__$Vec_FfiWinInfo$as_ptr(vecPtr)))
    }

    public static func vecOfSelfLen(vecPtr: UnsafeMutableRawPointer) -> UInt {
        __swift_bridge__$Vec_FfiWinInfo$len(vecPtr)
    }
}


public class FfiPaneStatus: FfiPaneStatusRefMut {
    var isOwned: Bool = true

    public override init(ptr: UnsafeMutableRawPointer) {
        super.init(ptr: ptr)
    }

    deinit {
        if isOwned {
            __swift_bridge__$FfiPaneStatus$_free(ptr)
        }
    }
}
public class FfiPaneStatusRefMut: FfiPaneStatusRef {
    public override init(ptr: UnsafeMutableRawPointer) {
        super.init(ptr: ptr)
    }
}
public class FfiPaneStatusRef {
    var ptr: UnsafeMutableRawPointer

    public init(ptr: UnsafeMutableRawPointer) {
        self.ptr = ptr
    }
}
extension FfiPaneStatusRef {
    public func filename() -> RustString {
        RustString(ptr: __swift_bridge__$FfiPaneStatus$filename(ptr))
    }

    public func display_path() -> RustString {
        RustString(ptr: __swift_bridge__$FfiPaneStatus$display_path(ptr))
    }

    public func abs_path() -> RustString {
        RustString(ptr: __swift_bridge__$FfiPaneStatus$abs_path(ptr))
    }
}
extension FfiPaneStatus: Vectorizable {
    public static func vecOfSelfNew() -> UnsafeMutableRawPointer {
        __swift_bridge__$Vec_FfiPaneStatus$new()
    }

    public static func vecOfSelfFree(vecPtr: UnsafeMutableRawPointer) {
        __swift_bridge__$Vec_FfiPaneStatus$drop(vecPtr)
    }

    public static func vecOfSelfPush(vecPtr: UnsafeMutableRawPointer, value: FfiPaneStatus) {
        __swift_bridge__$Vec_FfiPaneStatus$push(vecPtr, {value.isOwned = false; return value.ptr;}())
    }

    public static func vecOfSelfPop(vecPtr: UnsafeMutableRawPointer) -> Optional<Self> {
        let pointer = __swift_bridge__$Vec_FfiPaneStatus$pop(vecPtr)
        if pointer == nil {
            return nil
        } else {
            return (FfiPaneStatus(ptr: pointer!) as! Self)
        }
    }

    public static func vecOfSelfGet(vecPtr: UnsafeMutableRawPointer, index: UInt) -> Optional<FfiPaneStatusRef> {
        let pointer = __swift_bridge__$Vec_FfiPaneStatus$get(vecPtr, index)
        if pointer == nil {
            return nil
        } else {
            return FfiPaneStatusRef(ptr: pointer!)
        }
    }

    public static func vecOfSelfGetMut(vecPtr: UnsafeMutableRawPointer, index: UInt) -> Optional<FfiPaneStatusRefMut> {
        let pointer = __swift_bridge__$Vec_FfiPaneStatus$get_mut(vecPtr, index)
        if pointer == nil {
            return nil
        } else {
            return FfiPaneStatusRefMut(ptr: pointer!)
        }
    }

    public static func vecOfSelfAsPtr(vecPtr: UnsafeMutableRawPointer) -> UnsafePointer<FfiPaneStatusRef> {
        UnsafePointer<FfiPaneStatusRef>(OpaquePointer(__swift_bridge__$Vec_FfiPaneStatus$as_ptr(vecPtr)))
    }

    public static func vecOfSelfLen(vecPtr: UnsafeMutableRawPointer) -> UInt {
        __swift_bridge__$Vec_FfiPaneStatus$len(vecPtr)
    }
}


public class FfiWinBuffer: FfiWinBufferRefMut {
    var isOwned: Bool = true

    public override init(ptr: UnsafeMutableRawPointer) {
        super.init(ptr: ptr)
    }

    deinit {
        if isOwned {
            __swift_bridge__$FfiWinBuffer$_free(ptr)
        }
    }
}
public class FfiWinBufferRefMut: FfiWinBufferRef {
    public override init(ptr: UnsafeMutableRawPointer) {
        super.init(ptr: ptr)
    }
}
public class FfiWinBufferRef {
    var ptr: UnsafeMutableRawPointer

    public init(ptr: UnsafeMutableRawPointer) {
        self.ptr = ptr
    }
}
extension FfiWinBufferRef {
    public func reported() -> Bool {
        __swift_bridge__$FfiWinBuffer$reported(ptr)
    }

    public func abs_path() -> RustString {
        RustString(ptr: __swift_bridge__$FfiWinBuffer$abs_path(ptr))
    }

    public func terminal() -> UInt64 {
        __swift_bridge__$FfiWinBuffer$terminal(ptr)
    }
}
extension FfiWinBuffer: Vectorizable {
    public static func vecOfSelfNew() -> UnsafeMutableRawPointer {
        __swift_bridge__$Vec_FfiWinBuffer$new()
    }

    public static func vecOfSelfFree(vecPtr: UnsafeMutableRawPointer) {
        __swift_bridge__$Vec_FfiWinBuffer$drop(vecPtr)
    }

    public static func vecOfSelfPush(vecPtr: UnsafeMutableRawPointer, value: FfiWinBuffer) {
        __swift_bridge__$Vec_FfiWinBuffer$push(vecPtr, {value.isOwned = false; return value.ptr;}())
    }

    public static func vecOfSelfPop(vecPtr: UnsafeMutableRawPointer) -> Optional<Self> {
        let pointer = __swift_bridge__$Vec_FfiWinBuffer$pop(vecPtr)
        if pointer == nil {
            return nil
        } else {
            return (FfiWinBuffer(ptr: pointer!) as! Self)
        }
    }

    public static func vecOfSelfGet(vecPtr: UnsafeMutableRawPointer, index: UInt) -> Optional<FfiWinBufferRef> {
        let pointer = __swift_bridge__$Vec_FfiWinBuffer$get(vecPtr, index)
        if pointer == nil {
            return nil
        } else {
            return FfiWinBufferRef(ptr: pointer!)
        }
    }

    public static func vecOfSelfGetMut(vecPtr: UnsafeMutableRawPointer, index: UInt) -> Optional<FfiWinBufferRefMut> {
        let pointer = __swift_bridge__$Vec_FfiWinBuffer$get_mut(vecPtr, index)
        if pointer == nil {
            return nil
        } else {
            return FfiWinBufferRefMut(ptr: pointer!)
        }
    }

    public static func vecOfSelfAsPtr(vecPtr: UnsafeMutableRawPointer) -> UnsafePointer<FfiWinBufferRef> {
        UnsafePointer<FfiWinBufferRef>(OpaquePointer(__swift_bridge__$Vec_FfiWinBuffer$as_ptr(vecPtr)))
    }

    public static func vecOfSelfLen(vecPtr: UnsafeMutableRawPointer) -> UInt {
        __swift_bridge__$Vec_FfiWinBuffer$len(vecPtr)
    }
}


public class FfiWindowCursor: FfiWindowCursorRefMut {
    var isOwned: Bool = true

    public override init(ptr: UnsafeMutableRawPointer) {
        super.init(ptr: ptr)
    }

    deinit {
        if isOwned {
            __swift_bridge__$FfiWindowCursor$_free(ptr)
        }
    }
}
public class FfiWindowCursorRefMut: FfiWindowCursorRef {
    public override init(ptr: UnsafeMutableRawPointer) {
        super.init(ptr: ptr)
    }
}
public class FfiWindowCursorRef {
    var ptr: UnsafeMutableRawPointer

    public init(ptr: UnsafeMutableRawPointer) {
        self.ptr = ptr
    }
}
extension FfiWindowCursorRef {
    public func found() -> Bool {
        __swift_bridge__$FfiWindowCursor$found(ptr)
    }

    public func line() -> UInt32 {
        __swift_bridge__$FfiWindowCursor$line(ptr)
    }

    public func col() -> UInt32 {
        __swift_bridge__$FfiWindowCursor$col(ptr)
    }

    public func top_line() -> UInt32 {
        __swift_bridge__$FfiWindowCursor$top_line(ptr)
    }
}
extension FfiWindowCursor: Vectorizable {
    public static func vecOfSelfNew() -> UnsafeMutableRawPointer {
        __swift_bridge__$Vec_FfiWindowCursor$new()
    }

    public static func vecOfSelfFree(vecPtr: UnsafeMutableRawPointer) {
        __swift_bridge__$Vec_FfiWindowCursor$drop(vecPtr)
    }

    public static func vecOfSelfPush(vecPtr: UnsafeMutableRawPointer, value: FfiWindowCursor) {
        __swift_bridge__$Vec_FfiWindowCursor$push(vecPtr, {value.isOwned = false; return value.ptr;}())
    }

    public static func vecOfSelfPop(vecPtr: UnsafeMutableRawPointer) -> Optional<Self> {
        let pointer = __swift_bridge__$Vec_FfiWindowCursor$pop(vecPtr)
        if pointer == nil {
            return nil
        } else {
            return (FfiWindowCursor(ptr: pointer!) as! Self)
        }
    }

    public static func vecOfSelfGet(vecPtr: UnsafeMutableRawPointer, index: UInt) -> Optional<FfiWindowCursorRef> {
        let pointer = __swift_bridge__$Vec_FfiWindowCursor$get(vecPtr, index)
        if pointer == nil {
            return nil
        } else {
            return FfiWindowCursorRef(ptr: pointer!)
        }
    }

    public static func vecOfSelfGetMut(vecPtr: UnsafeMutableRawPointer, index: UInt) -> Optional<FfiWindowCursorRefMut> {
        let pointer = __swift_bridge__$Vec_FfiWindowCursor$get_mut(vecPtr, index)
        if pointer == nil {
            return nil
        } else {
            return FfiWindowCursorRefMut(ptr: pointer!)
        }
    }

    public static func vecOfSelfAsPtr(vecPtr: UnsafeMutableRawPointer) -> UnsafePointer<FfiWindowCursorRef> {
        UnsafePointer<FfiWindowCursorRef>(OpaquePointer(__swift_bridge__$Vec_FfiWindowCursor$as_ptr(vecPtr)))
    }

    public static func vecOfSelfLen(vecPtr: UnsafeMutableRawPointer) -> UInt {
        __swift_bridge__$Vec_FfiWindowCursor$len(vecPtr)
    }
}


public class FfiCmdline: FfiCmdlineRefMut {
    var isOwned: Bool = true

    public override init(ptr: UnsafeMutableRawPointer) {
        super.init(ptr: ptr)
    }

    deinit {
        if isOwned {
            __swift_bridge__$FfiCmdline$_free(ptr)
        }
    }
}
public class FfiCmdlineRefMut: FfiCmdlineRef {
    public override init(ptr: UnsafeMutableRawPointer) {
        super.init(ptr: ptr)
    }
}
public class FfiCmdlineRef {
    var ptr: UnsafeMutableRawPointer

    public init(ptr: UnsafeMutableRawPointer) {
        self.ptr = ptr
    }
}
extension FfiCmdlineRef {
    public func active() -> Bool {
        __swift_bridge__$FfiCmdline$active(ptr)
    }

    public func firstc() -> RustString {
        RustString(ptr: __swift_bridge__$FfiCmdline$firstc(ptr))
    }

    public func prompt() -> RustString {
        RustString(ptr: __swift_bridge__$FfiCmdline$prompt(ptr))
    }

    public func text() -> RustString {
        RustString(ptr: __swift_bridge__$FfiCmdline$text(ptr))
    }

    public func pos() -> UInt32 {
        __swift_bridge__$FfiCmdline$pos(ptr)
    }

    public func indent() -> UInt32 {
        __swift_bridge__$FfiCmdline$indent(ptr)
    }

    public func level() -> UInt32 {
        __swift_bridge__$FfiCmdline$level(ptr)
    }
}
extension FfiCmdline: Vectorizable {
    public static func vecOfSelfNew() -> UnsafeMutableRawPointer {
        __swift_bridge__$Vec_FfiCmdline$new()
    }

    public static func vecOfSelfFree(vecPtr: UnsafeMutableRawPointer) {
        __swift_bridge__$Vec_FfiCmdline$drop(vecPtr)
    }

    public static func vecOfSelfPush(vecPtr: UnsafeMutableRawPointer, value: FfiCmdline) {
        __swift_bridge__$Vec_FfiCmdline$push(vecPtr, {value.isOwned = false; return value.ptr;}())
    }

    public static func vecOfSelfPop(vecPtr: UnsafeMutableRawPointer) -> Optional<Self> {
        let pointer = __swift_bridge__$Vec_FfiCmdline$pop(vecPtr)
        if pointer == nil {
            return nil
        } else {
            return (FfiCmdline(ptr: pointer!) as! Self)
        }
    }

    public static func vecOfSelfGet(vecPtr: UnsafeMutableRawPointer, index: UInt) -> Optional<FfiCmdlineRef> {
        let pointer = __swift_bridge__$Vec_FfiCmdline$get(vecPtr, index)
        if pointer == nil {
            return nil
        } else {
            return FfiCmdlineRef(ptr: pointer!)
        }
    }

    public static func vecOfSelfGetMut(vecPtr: UnsafeMutableRawPointer, index: UInt) -> Optional<FfiCmdlineRefMut> {
        let pointer = __swift_bridge__$Vec_FfiCmdline$get_mut(vecPtr, index)
        if pointer == nil {
            return nil
        } else {
            return FfiCmdlineRefMut(ptr: pointer!)
        }
    }

    public static func vecOfSelfAsPtr(vecPtr: UnsafeMutableRawPointer) -> UnsafePointer<FfiCmdlineRef> {
        UnsafePointer<FfiCmdlineRef>(OpaquePointer(__swift_bridge__$Vec_FfiCmdline$as_ptr(vecPtr)))
    }

    public static func vecOfSelfLen(vecPtr: UnsafeMutableRawPointer) -> UInt {
        __swift_bridge__$Vec_FfiCmdline$len(vecPtr)
    }
}


public class FfiPum: FfiPumRefMut {
    var isOwned: Bool = true

    public override init(ptr: UnsafeMutableRawPointer) {
        super.init(ptr: ptr)
    }

    deinit {
        if isOwned {
            __swift_bridge__$FfiPum$_free(ptr)
        }
    }
}
public class FfiPumRefMut: FfiPumRef {
    public override init(ptr: UnsafeMutableRawPointer) {
        super.init(ptr: ptr)
    }
}
public class FfiPumRef {
    var ptr: UnsafeMutableRawPointer

    public init(ptr: UnsafeMutableRawPointer) {
        self.ptr = ptr
    }
}
extension FfiPumRef {
    public func visible() -> Bool {
        __swift_bridge__$FfiPum$visible(ptr)
    }

    public func selected() -> Int64 {
        __swift_bridge__$FfiPum$selected(ptr)
    }

    public func anchor_grid() -> Int64 {
        __swift_bridge__$FfiPum$anchor_grid(ptr)
    }

    public func anchor_row() -> Int64 {
        __swift_bridge__$FfiPum$anchor_row(ptr)
    }

    public func anchor_col() -> Int64 {
        __swift_bridge__$FfiPum$anchor_col(ptr)
    }

    public func items() -> RustVec<FfiPumItem> {
        RustVec(ptr: __swift_bridge__$FfiPum$items(ptr))
    }
}
extension FfiPum: Vectorizable {
    public static func vecOfSelfNew() -> UnsafeMutableRawPointer {
        __swift_bridge__$Vec_FfiPum$new()
    }

    public static func vecOfSelfFree(vecPtr: UnsafeMutableRawPointer) {
        __swift_bridge__$Vec_FfiPum$drop(vecPtr)
    }

    public static func vecOfSelfPush(vecPtr: UnsafeMutableRawPointer, value: FfiPum) {
        __swift_bridge__$Vec_FfiPum$push(vecPtr, {value.isOwned = false; return value.ptr;}())
    }

    public static func vecOfSelfPop(vecPtr: UnsafeMutableRawPointer) -> Optional<Self> {
        let pointer = __swift_bridge__$Vec_FfiPum$pop(vecPtr)
        if pointer == nil {
            return nil
        } else {
            return (FfiPum(ptr: pointer!) as! Self)
        }
    }

    public static func vecOfSelfGet(vecPtr: UnsafeMutableRawPointer, index: UInt) -> Optional<FfiPumRef> {
        let pointer = __swift_bridge__$Vec_FfiPum$get(vecPtr, index)
        if pointer == nil {
            return nil
        } else {
            return FfiPumRef(ptr: pointer!)
        }
    }

    public static func vecOfSelfGetMut(vecPtr: UnsafeMutableRawPointer, index: UInt) -> Optional<FfiPumRefMut> {
        let pointer = __swift_bridge__$Vec_FfiPum$get_mut(vecPtr, index)
        if pointer == nil {
            return nil
        } else {
            return FfiPumRefMut(ptr: pointer!)
        }
    }

    public static func vecOfSelfAsPtr(vecPtr: UnsafeMutableRawPointer) -> UnsafePointer<FfiPumRef> {
        UnsafePointer<FfiPumRef>(OpaquePointer(__swift_bridge__$Vec_FfiPum$as_ptr(vecPtr)))
    }

    public static func vecOfSelfLen(vecPtr: UnsafeMutableRawPointer) -> UInt {
        __swift_bridge__$Vec_FfiPum$len(vecPtr)
    }
}


public class FfiPumItem: FfiPumItemRefMut {
    var isOwned: Bool = true

    public override init(ptr: UnsafeMutableRawPointer) {
        super.init(ptr: ptr)
    }

    deinit {
        if isOwned {
            __swift_bridge__$FfiPumItem$_free(ptr)
        }
    }
}
public class FfiPumItemRefMut: FfiPumItemRef {
    public override init(ptr: UnsafeMutableRawPointer) {
        super.init(ptr: ptr)
    }
}
public class FfiPumItemRef {
    var ptr: UnsafeMutableRawPointer

    public init(ptr: UnsafeMutableRawPointer) {
        self.ptr = ptr
    }
}
extension FfiPumItemRef {
    public func word() -> RustString {
        RustString(ptr: __swift_bridge__$FfiPumItem$word(ptr))
    }

    public func kind() -> RustString {
        RustString(ptr: __swift_bridge__$FfiPumItem$kind(ptr))
    }

    public func menu() -> RustString {
        RustString(ptr: __swift_bridge__$FfiPumItem$menu(ptr))
    }

    public func info() -> RustString {
        RustString(ptr: __swift_bridge__$FfiPumItem$info(ptr))
    }
}
extension FfiPumItem: Vectorizable {
    public static func vecOfSelfNew() -> UnsafeMutableRawPointer {
        __swift_bridge__$Vec_FfiPumItem$new()
    }

    public static func vecOfSelfFree(vecPtr: UnsafeMutableRawPointer) {
        __swift_bridge__$Vec_FfiPumItem$drop(vecPtr)
    }

    public static func vecOfSelfPush(vecPtr: UnsafeMutableRawPointer, value: FfiPumItem) {
        __swift_bridge__$Vec_FfiPumItem$push(vecPtr, {value.isOwned = false; return value.ptr;}())
    }

    public static func vecOfSelfPop(vecPtr: UnsafeMutableRawPointer) -> Optional<Self> {
        let pointer = __swift_bridge__$Vec_FfiPumItem$pop(vecPtr)
        if pointer == nil {
            return nil
        } else {
            return (FfiPumItem(ptr: pointer!) as! Self)
        }
    }

    public static func vecOfSelfGet(vecPtr: UnsafeMutableRawPointer, index: UInt) -> Optional<FfiPumItemRef> {
        let pointer = __swift_bridge__$Vec_FfiPumItem$get(vecPtr, index)
        if pointer == nil {
            return nil
        } else {
            return FfiPumItemRef(ptr: pointer!)
        }
    }

    public static func vecOfSelfGetMut(vecPtr: UnsafeMutableRawPointer, index: UInt) -> Optional<FfiPumItemRefMut> {
        let pointer = __swift_bridge__$Vec_FfiPumItem$get_mut(vecPtr, index)
        if pointer == nil {
            return nil
        } else {
            return FfiPumItemRefMut(ptr: pointer!)
        }
    }

    public static func vecOfSelfAsPtr(vecPtr: UnsafeMutableRawPointer) -> UnsafePointer<FfiPumItemRef> {
        UnsafePointer<FfiPumItemRef>(OpaquePointer(__swift_bridge__$Vec_FfiPumItem$as_ptr(vecPtr)))
    }

    public static func vecOfSelfLen(vecPtr: UnsafeMutableRawPointer) -> UInt {
        __swift_bridge__$Vec_FfiPumItem$len(vecPtr)
    }
}


public class FfiToast: FfiToastRefMut {
    var isOwned: Bool = true

    public override init(ptr: UnsafeMutableRawPointer) {
        super.init(ptr: ptr)
    }

    deinit {
        if isOwned {
            __swift_bridge__$FfiToast$_free(ptr)
        }
    }
}
public class FfiToastRefMut: FfiToastRef {
    public override init(ptr: UnsafeMutableRawPointer) {
        super.init(ptr: ptr)
    }
}
public class FfiToastRef {
    var ptr: UnsafeMutableRawPointer

    public init(ptr: UnsafeMutableRawPointer) {
        self.ptr = ptr
    }
}
extension FfiToastRef {
    public func visible() -> Bool {
        __swift_bridge__$FfiToast$visible(ptr)
    }

    public func kind() -> RustString {
        RustString(ptr: __swift_bridge__$FfiToast$kind(ptr))
    }

    public func lines() -> RustVec<FfiToastLine> {
        RustVec(ptr: __swift_bridge__$FfiToast$lines(ptr))
    }
}
extension FfiToast: Vectorizable {
    public static func vecOfSelfNew() -> UnsafeMutableRawPointer {
        __swift_bridge__$Vec_FfiToast$new()
    }

    public static func vecOfSelfFree(vecPtr: UnsafeMutableRawPointer) {
        __swift_bridge__$Vec_FfiToast$drop(vecPtr)
    }

    public static func vecOfSelfPush(vecPtr: UnsafeMutableRawPointer, value: FfiToast) {
        __swift_bridge__$Vec_FfiToast$push(vecPtr, {value.isOwned = false; return value.ptr;}())
    }

    public static func vecOfSelfPop(vecPtr: UnsafeMutableRawPointer) -> Optional<Self> {
        let pointer = __swift_bridge__$Vec_FfiToast$pop(vecPtr)
        if pointer == nil {
            return nil
        } else {
            return (FfiToast(ptr: pointer!) as! Self)
        }
    }

    public static func vecOfSelfGet(vecPtr: UnsafeMutableRawPointer, index: UInt) -> Optional<FfiToastRef> {
        let pointer = __swift_bridge__$Vec_FfiToast$get(vecPtr, index)
        if pointer == nil {
            return nil
        } else {
            return FfiToastRef(ptr: pointer!)
        }
    }

    public static func vecOfSelfGetMut(vecPtr: UnsafeMutableRawPointer, index: UInt) -> Optional<FfiToastRefMut> {
        let pointer = __swift_bridge__$Vec_FfiToast$get_mut(vecPtr, index)
        if pointer == nil {
            return nil
        } else {
            return FfiToastRefMut(ptr: pointer!)
        }
    }

    public static func vecOfSelfAsPtr(vecPtr: UnsafeMutableRawPointer) -> UnsafePointer<FfiToastRef> {
        UnsafePointer<FfiToastRef>(OpaquePointer(__swift_bridge__$Vec_FfiToast$as_ptr(vecPtr)))
    }

    public static func vecOfSelfLen(vecPtr: UnsafeMutableRawPointer) -> UInt {
        __swift_bridge__$Vec_FfiToast$len(vecPtr)
    }
}


public class FfiToastLine: FfiToastLineRefMut {
    var isOwned: Bool = true

    public override init(ptr: UnsafeMutableRawPointer) {
        super.init(ptr: ptr)
    }

    deinit {
        if isOwned {
            __swift_bridge__$FfiToastLine$_free(ptr)
        }
    }
}
public class FfiToastLineRefMut: FfiToastLineRef {
    public override init(ptr: UnsafeMutableRawPointer) {
        super.init(ptr: ptr)
    }
}
public class FfiToastLineRef {
    var ptr: UnsafeMutableRawPointer

    public init(ptr: UnsafeMutableRawPointer) {
        self.ptr = ptr
    }
}
extension FfiToastLineRef {
    public func chunks() -> RustVec<FfiToastChunk> {
        RustVec(ptr: __swift_bridge__$FfiToastLine$chunks(ptr))
    }
}
extension FfiToastLine: Vectorizable {
    public static func vecOfSelfNew() -> UnsafeMutableRawPointer {
        __swift_bridge__$Vec_FfiToastLine$new()
    }

    public static func vecOfSelfFree(vecPtr: UnsafeMutableRawPointer) {
        __swift_bridge__$Vec_FfiToastLine$drop(vecPtr)
    }

    public static func vecOfSelfPush(vecPtr: UnsafeMutableRawPointer, value: FfiToastLine) {
        __swift_bridge__$Vec_FfiToastLine$push(vecPtr, {value.isOwned = false; return value.ptr;}())
    }

    public static func vecOfSelfPop(vecPtr: UnsafeMutableRawPointer) -> Optional<Self> {
        let pointer = __swift_bridge__$Vec_FfiToastLine$pop(vecPtr)
        if pointer == nil {
            return nil
        } else {
            return (FfiToastLine(ptr: pointer!) as! Self)
        }
    }

    public static func vecOfSelfGet(vecPtr: UnsafeMutableRawPointer, index: UInt) -> Optional<FfiToastLineRef> {
        let pointer = __swift_bridge__$Vec_FfiToastLine$get(vecPtr, index)
        if pointer == nil {
            return nil
        } else {
            return FfiToastLineRef(ptr: pointer!)
        }
    }

    public static func vecOfSelfGetMut(vecPtr: UnsafeMutableRawPointer, index: UInt) -> Optional<FfiToastLineRefMut> {
        let pointer = __swift_bridge__$Vec_FfiToastLine$get_mut(vecPtr, index)
        if pointer == nil {
            return nil
        } else {
            return FfiToastLineRefMut(ptr: pointer!)
        }
    }

    public static func vecOfSelfAsPtr(vecPtr: UnsafeMutableRawPointer) -> UnsafePointer<FfiToastLineRef> {
        UnsafePointer<FfiToastLineRef>(OpaquePointer(__swift_bridge__$Vec_FfiToastLine$as_ptr(vecPtr)))
    }

    public static func vecOfSelfLen(vecPtr: UnsafeMutableRawPointer) -> UInt {
        __swift_bridge__$Vec_FfiToastLine$len(vecPtr)
    }
}


public class FfiToastChunk: FfiToastChunkRefMut {
    var isOwned: Bool = true

    public override init(ptr: UnsafeMutableRawPointer) {
        super.init(ptr: ptr)
    }

    deinit {
        if isOwned {
            __swift_bridge__$FfiToastChunk$_free(ptr)
        }
    }
}
public class FfiToastChunkRefMut: FfiToastChunkRef {
    public override init(ptr: UnsafeMutableRawPointer) {
        super.init(ptr: ptr)
    }
}
public class FfiToastChunkRef {
    var ptr: UnsafeMutableRawPointer

    public init(ptr: UnsafeMutableRawPointer) {
        self.ptr = ptr
    }
}
extension FfiToastChunkRef {
    public func text() -> RustString {
        RustString(ptr: __swift_bridge__$FfiToastChunk$text(ptr))
    }

    public func fg() -> FfiColor {
        __swift_bridge__$FfiToastChunk$fg(ptr).intoSwiftRepr()
    }

    public func bg() -> FfiColor {
        __swift_bridge__$FfiToastChunk$bg(ptr).intoSwiftRepr()
    }

    public func special() -> FfiColor {
        __swift_bridge__$FfiToastChunk$special(ptr).intoSwiftRepr()
    }

    public func flags() -> UInt8 {
        __swift_bridge__$FfiToastChunk$flags(ptr)
    }
}
extension FfiToastChunk: Vectorizable {
    public static func vecOfSelfNew() -> UnsafeMutableRawPointer {
        __swift_bridge__$Vec_FfiToastChunk$new()
    }

    public static func vecOfSelfFree(vecPtr: UnsafeMutableRawPointer) {
        __swift_bridge__$Vec_FfiToastChunk$drop(vecPtr)
    }

    public static func vecOfSelfPush(vecPtr: UnsafeMutableRawPointer, value: FfiToastChunk) {
        __swift_bridge__$Vec_FfiToastChunk$push(vecPtr, {value.isOwned = false; return value.ptr;}())
    }

    public static func vecOfSelfPop(vecPtr: UnsafeMutableRawPointer) -> Optional<Self> {
        let pointer = __swift_bridge__$Vec_FfiToastChunk$pop(vecPtr)
        if pointer == nil {
            return nil
        } else {
            return (FfiToastChunk(ptr: pointer!) as! Self)
        }
    }

    public static func vecOfSelfGet(vecPtr: UnsafeMutableRawPointer, index: UInt) -> Optional<FfiToastChunkRef> {
        let pointer = __swift_bridge__$Vec_FfiToastChunk$get(vecPtr, index)
        if pointer == nil {
            return nil
        } else {
            return FfiToastChunkRef(ptr: pointer!)
        }
    }

    public static func vecOfSelfGetMut(vecPtr: UnsafeMutableRawPointer, index: UInt) -> Optional<FfiToastChunkRefMut> {
        let pointer = __swift_bridge__$Vec_FfiToastChunk$get_mut(vecPtr, index)
        if pointer == nil {
            return nil
        } else {
            return FfiToastChunkRefMut(ptr: pointer!)
        }
    }

    public static func vecOfSelfAsPtr(vecPtr: UnsafeMutableRawPointer) -> UnsafePointer<FfiToastChunkRef> {
        UnsafePointer<FfiToastChunkRef>(OpaquePointer(__swift_bridge__$Vec_FfiToastChunk$as_ptr(vecPtr)))
    }

    public static func vecOfSelfLen(vecPtr: UnsafeMutableRawPointer) -> UInt {
        __swift_bridge__$Vec_FfiToastChunk$len(vecPtr)
    }
}



