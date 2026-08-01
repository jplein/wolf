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
public func wolf_ffi_load_state() -> FfiState {
    FfiState(ptr: __swift_bridge__$wolf_ffi_load_state())
}
public func wolf_ffi_state_snapshot_begin() {
    __swift_bridge__$wolf_ffi_state_snapshot_begin()
}
public func wolf_ffi_state_snapshot_add_window(_ x: Double, _ y: Double, _ width: Double, _ height: Double) {
    __swift_bridge__$wolf_ffi_state_snapshot_add_window(x, y, width, height)
}
public func wolf_ffi_state_snapshot_add_tab<GenericIntoRustString: IntoRustString>(_ cwd: GenericIntoRustString, _ sidebar_visible: Bool) {
    __swift_bridge__$wolf_ffi_state_snapshot_add_tab({ let rustString = cwd.intoRustString(); rustString.isOwned = false; return rustString.ptr }(), sidebar_visible)
}
public func wolf_ffi_state_layout_push_leaf<GenericIntoRustString: IntoRustString>(_ path: GenericIntoRustString, _ preview: Bool, _ weight: Double) {
    __swift_bridge__$wolf_ffi_state_layout_push_leaf({ let rustString = path.intoRustString(); rustString.isOwned = false; return rustString.ptr }(), preview, weight)
}
public func wolf_ffi_state_layout_push_split(_ orientation: UInt8, _ child_count: UInt32, _ weight: Double) {
    __swift_bridge__$wolf_ffi_state_layout_push_split(orientation, child_count, weight)
}
public func wolf_ffi_state_layout_commit_to_tab() {
    __swift_bridge__$wolf_ffi_state_layout_commit_to_tab()
}
public func wolf_ffi_state_snapshot_commit() {
    __swift_bridge__$wolf_ffi_state_snapshot_commit()
}
public func wolf_ffi_state_flush() -> Bool {
    __swift_bridge__$wolf_ffi_state_flush()
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

    public func rebuild_layout<GenericIntoRustString: IntoRustString>(_ spec_json: GenericIntoRustString) -> RustVec<Int64> {
        RustVec(ptr: __swift_bridge__$WolfCoreHandle$rebuild_layout(ptr, { let rustString = spec_json.intoRustString(); rustString.isOwned = false; return rustString.ptr }()))
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

    public func theme_titlebar_bg() -> FfiColor {
        __swift_bridge__$WolfCoreHandle$theme_titlebar_bg(ptr).intoSwiftRepr()
    }

    public func theme_titlebar_fg() -> FfiColor {
        __swift_bridge__$WolfCoreHandle$theme_titlebar_fg(ptr).intoSwiftRepr()
    }

    public func pane_status_for_grid(_ grid: UInt64) -> FfiPaneStatus {
        FfiPaneStatus(ptr: __swift_bridge__$WolfCoreHandle$pane_status_for_grid(ptr, grid))
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

    public func edit_source<GenericIntoRustString: IntoRustString>(_ mode: GenericIntoRustString) -> Int64 {
        __swift_bridge__$WolfCoreHandle$edit_source(ptr, { let rustString = mode.intoRustString(); rustString.isOwned = false; return rustString.ptr }())
    }

    public func open_preview<GenericIntoRustString: IntoRustString>(_ mode: GenericIntoRustString) -> Int64 {
        __swift_bridge__$WolfCoreHandle$open_preview(ptr, { let rustString = mode.intoRustString(); rustString.isOwned = false; return rustString.ptr }())
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


public class FfiState: FfiStateRefMut {
    var isOwned: Bool = true

    public override init(ptr: UnsafeMutableRawPointer) {
        super.init(ptr: ptr)
    }

    deinit {
        if isOwned {
            __swift_bridge__$FfiState$_free(ptr)
        }
    }
}
public class FfiStateRefMut: FfiStateRef {
    public override init(ptr: UnsafeMutableRawPointer) {
        super.init(ptr: ptr)
    }
}
public class FfiStateRef {
    var ptr: UnsafeMutableRawPointer

    public init(ptr: UnsafeMutableRawPointer) {
        self.ptr = ptr
    }
}
extension FfiStateRef {
    public func windows() -> RustVec<FfiWindowState> {
        RustVec(ptr: __swift_bridge__$FfiState$windows(ptr))
    }
}
extension FfiState: Vectorizable {
    public static func vecOfSelfNew() -> UnsafeMutableRawPointer {
        __swift_bridge__$Vec_FfiState$new()
    }

    public static func vecOfSelfFree(vecPtr: UnsafeMutableRawPointer) {
        __swift_bridge__$Vec_FfiState$drop(vecPtr)
    }

    public static func vecOfSelfPush(vecPtr: UnsafeMutableRawPointer, value: FfiState) {
        __swift_bridge__$Vec_FfiState$push(vecPtr, {value.isOwned = false; return value.ptr;}())
    }

    public static func vecOfSelfPop(vecPtr: UnsafeMutableRawPointer) -> Optional<Self> {
        let pointer = __swift_bridge__$Vec_FfiState$pop(vecPtr)
        if pointer == nil {
            return nil
        } else {
            return (FfiState(ptr: pointer!) as! Self)
        }
    }

    public static func vecOfSelfGet(vecPtr: UnsafeMutableRawPointer, index: UInt) -> Optional<FfiStateRef> {
        let pointer = __swift_bridge__$Vec_FfiState$get(vecPtr, index)
        if pointer == nil {
            return nil
        } else {
            return FfiStateRef(ptr: pointer!)
        }
    }

    public static func vecOfSelfGetMut(vecPtr: UnsafeMutableRawPointer, index: UInt) -> Optional<FfiStateRefMut> {
        let pointer = __swift_bridge__$Vec_FfiState$get_mut(vecPtr, index)
        if pointer == nil {
            return nil
        } else {
            return FfiStateRefMut(ptr: pointer!)
        }
    }

    public static func vecOfSelfAsPtr(vecPtr: UnsafeMutableRawPointer) -> UnsafePointer<FfiStateRef> {
        UnsafePointer<FfiStateRef>(OpaquePointer(__swift_bridge__$Vec_FfiState$as_ptr(vecPtr)))
    }

    public static func vecOfSelfLen(vecPtr: UnsafeMutableRawPointer) -> UInt {
        __swift_bridge__$Vec_FfiState$len(vecPtr)
    }
}


public class FfiWindowState: FfiWindowStateRefMut {
    var isOwned: Bool = true

    public override init(ptr: UnsafeMutableRawPointer) {
        super.init(ptr: ptr)
    }

    deinit {
        if isOwned {
            __swift_bridge__$FfiWindowState$_free(ptr)
        }
    }
}
public class FfiWindowStateRefMut: FfiWindowStateRef {
    public override init(ptr: UnsafeMutableRawPointer) {
        super.init(ptr: ptr)
    }
}
public class FfiWindowStateRef {
    var ptr: UnsafeMutableRawPointer

    public init(ptr: UnsafeMutableRawPointer) {
        self.ptr = ptr
    }
}
extension FfiWindowStateRef {
    public func x() -> Double {
        __swift_bridge__$FfiWindowState$x(ptr)
    }

    public func y() -> Double {
        __swift_bridge__$FfiWindowState$y(ptr)
    }

    public func width() -> Double {
        __swift_bridge__$FfiWindowState$width(ptr)
    }

    public func height() -> Double {
        __swift_bridge__$FfiWindowState$height(ptr)
    }

    public func tabs() -> RustVec<FfiTabState> {
        RustVec(ptr: __swift_bridge__$FfiWindowState$tabs(ptr))
    }
}
extension FfiWindowState: Vectorizable {
    public static func vecOfSelfNew() -> UnsafeMutableRawPointer {
        __swift_bridge__$Vec_FfiWindowState$new()
    }

    public static func vecOfSelfFree(vecPtr: UnsafeMutableRawPointer) {
        __swift_bridge__$Vec_FfiWindowState$drop(vecPtr)
    }

    public static func vecOfSelfPush(vecPtr: UnsafeMutableRawPointer, value: FfiWindowState) {
        __swift_bridge__$Vec_FfiWindowState$push(vecPtr, {value.isOwned = false; return value.ptr;}())
    }

    public static func vecOfSelfPop(vecPtr: UnsafeMutableRawPointer) -> Optional<Self> {
        let pointer = __swift_bridge__$Vec_FfiWindowState$pop(vecPtr)
        if pointer == nil {
            return nil
        } else {
            return (FfiWindowState(ptr: pointer!) as! Self)
        }
    }

    public static func vecOfSelfGet(vecPtr: UnsafeMutableRawPointer, index: UInt) -> Optional<FfiWindowStateRef> {
        let pointer = __swift_bridge__$Vec_FfiWindowState$get(vecPtr, index)
        if pointer == nil {
            return nil
        } else {
            return FfiWindowStateRef(ptr: pointer!)
        }
    }

    public static func vecOfSelfGetMut(vecPtr: UnsafeMutableRawPointer, index: UInt) -> Optional<FfiWindowStateRefMut> {
        let pointer = __swift_bridge__$Vec_FfiWindowState$get_mut(vecPtr, index)
        if pointer == nil {
            return nil
        } else {
            return FfiWindowStateRefMut(ptr: pointer!)
        }
    }

    public static func vecOfSelfAsPtr(vecPtr: UnsafeMutableRawPointer) -> UnsafePointer<FfiWindowStateRef> {
        UnsafePointer<FfiWindowStateRef>(OpaquePointer(__swift_bridge__$Vec_FfiWindowState$as_ptr(vecPtr)))
    }

    public static func vecOfSelfLen(vecPtr: UnsafeMutableRawPointer) -> UInt {
        __swift_bridge__$Vec_FfiWindowState$len(vecPtr)
    }
}


public class FfiTabState: FfiTabStateRefMut {
    var isOwned: Bool = true

    public override init(ptr: UnsafeMutableRawPointer) {
        super.init(ptr: ptr)
    }

    deinit {
        if isOwned {
            __swift_bridge__$FfiTabState$_free(ptr)
        }
    }
}
public class FfiTabStateRefMut: FfiTabStateRef {
    public override init(ptr: UnsafeMutableRawPointer) {
        super.init(ptr: ptr)
    }
}
public class FfiTabStateRef {
    var ptr: UnsafeMutableRawPointer

    public init(ptr: UnsafeMutableRawPointer) {
        self.ptr = ptr
    }
}
extension FfiTabStateRef {
    public func cwd() -> RustString {
        RustString(ptr: __swift_bridge__$FfiTabState$cwd(ptr))
    }

    public func sidebar_visible() -> Bool {
        __swift_bridge__$FfiTabState$sidebar_visible(ptr)
    }

    public func has_layout() -> Bool {
        __swift_bridge__$FfiTabState$has_layout(ptr)
    }

    public func layout() -> FfiPaneNode {
        FfiPaneNode(ptr: __swift_bridge__$FfiTabState$layout(ptr))
    }
}
extension FfiTabState: Vectorizable {
    public static func vecOfSelfNew() -> UnsafeMutableRawPointer {
        __swift_bridge__$Vec_FfiTabState$new()
    }

    public static func vecOfSelfFree(vecPtr: UnsafeMutableRawPointer) {
        __swift_bridge__$Vec_FfiTabState$drop(vecPtr)
    }

    public static func vecOfSelfPush(vecPtr: UnsafeMutableRawPointer, value: FfiTabState) {
        __swift_bridge__$Vec_FfiTabState$push(vecPtr, {value.isOwned = false; return value.ptr;}())
    }

    public static func vecOfSelfPop(vecPtr: UnsafeMutableRawPointer) -> Optional<Self> {
        let pointer = __swift_bridge__$Vec_FfiTabState$pop(vecPtr)
        if pointer == nil {
            return nil
        } else {
            return (FfiTabState(ptr: pointer!) as! Self)
        }
    }

    public static func vecOfSelfGet(vecPtr: UnsafeMutableRawPointer, index: UInt) -> Optional<FfiTabStateRef> {
        let pointer = __swift_bridge__$Vec_FfiTabState$get(vecPtr, index)
        if pointer == nil {
            return nil
        } else {
            return FfiTabStateRef(ptr: pointer!)
        }
    }

    public static func vecOfSelfGetMut(vecPtr: UnsafeMutableRawPointer, index: UInt) -> Optional<FfiTabStateRefMut> {
        let pointer = __swift_bridge__$Vec_FfiTabState$get_mut(vecPtr, index)
        if pointer == nil {
            return nil
        } else {
            return FfiTabStateRefMut(ptr: pointer!)
        }
    }

    public static func vecOfSelfAsPtr(vecPtr: UnsafeMutableRawPointer) -> UnsafePointer<FfiTabStateRef> {
        UnsafePointer<FfiTabStateRef>(OpaquePointer(__swift_bridge__$Vec_FfiTabState$as_ptr(vecPtr)))
    }

    public static func vecOfSelfLen(vecPtr: UnsafeMutableRawPointer) -> UInt {
        __swift_bridge__$Vec_FfiTabState$len(vecPtr)
    }
}


public class FfiPaneNode: FfiPaneNodeRefMut {
    var isOwned: Bool = true

    public override init(ptr: UnsafeMutableRawPointer) {
        super.init(ptr: ptr)
    }

    deinit {
        if isOwned {
            __swift_bridge__$FfiPaneNode$_free(ptr)
        }
    }
}
public class FfiPaneNodeRefMut: FfiPaneNodeRef {
    public override init(ptr: UnsafeMutableRawPointer) {
        super.init(ptr: ptr)
    }
}
public class FfiPaneNodeRef {
    var ptr: UnsafeMutableRawPointer

    public init(ptr: UnsafeMutableRawPointer) {
        self.ptr = ptr
    }
}
extension FfiPaneNodeRef {
    public func is_leaf() -> Bool {
        __swift_bridge__$FfiPaneNode$is_leaf(ptr)
    }

    public func is_preview() -> Bool {
        __swift_bridge__$FfiPaneNode$is_preview(ptr)
    }

    public func leaf_path() -> RustString {
        RustString(ptr: __swift_bridge__$FfiPaneNode$leaf_path(ptr))
    }

    public func orientation() -> UInt8 {
        __swift_bridge__$FfiPaneNode$orientation(ptr)
    }

    public func child_count() -> UInt32 {
        __swift_bridge__$FfiPaneNode$child_count(ptr)
    }

    public func weight(_ index: UInt32) -> Double {
        __swift_bridge__$FfiPaneNode$weight(ptr, index)
    }

    public func child(_ index: UInt32) -> FfiPaneNode {
        FfiPaneNode(ptr: __swift_bridge__$FfiPaneNode$child(ptr, index))
    }
}
extension FfiPaneNode: Vectorizable {
    public static func vecOfSelfNew() -> UnsafeMutableRawPointer {
        __swift_bridge__$Vec_FfiPaneNode$new()
    }

    public static func vecOfSelfFree(vecPtr: UnsafeMutableRawPointer) {
        __swift_bridge__$Vec_FfiPaneNode$drop(vecPtr)
    }

    public static func vecOfSelfPush(vecPtr: UnsafeMutableRawPointer, value: FfiPaneNode) {
        __swift_bridge__$Vec_FfiPaneNode$push(vecPtr, {value.isOwned = false; return value.ptr;}())
    }

    public static func vecOfSelfPop(vecPtr: UnsafeMutableRawPointer) -> Optional<Self> {
        let pointer = __swift_bridge__$Vec_FfiPaneNode$pop(vecPtr)
        if pointer == nil {
            return nil
        } else {
            return (FfiPaneNode(ptr: pointer!) as! Self)
        }
    }

    public static func vecOfSelfGet(vecPtr: UnsafeMutableRawPointer, index: UInt) -> Optional<FfiPaneNodeRef> {
        let pointer = __swift_bridge__$Vec_FfiPaneNode$get(vecPtr, index)
        if pointer == nil {
            return nil
        } else {
            return FfiPaneNodeRef(ptr: pointer!)
        }
    }

    public static func vecOfSelfGetMut(vecPtr: UnsafeMutableRawPointer, index: UInt) -> Optional<FfiPaneNodeRefMut> {
        let pointer = __swift_bridge__$Vec_FfiPaneNode$get_mut(vecPtr, index)
        if pointer == nil {
            return nil
        } else {
            return FfiPaneNodeRefMut(ptr: pointer!)
        }
    }

    public static func vecOfSelfAsPtr(vecPtr: UnsafeMutableRawPointer) -> UnsafePointer<FfiPaneNodeRef> {
        UnsafePointer<FfiPaneNodeRef>(OpaquePointer(__swift_bridge__$Vec_FfiPaneNode$as_ptr(vecPtr)))
    }

    public static func vecOfSelfLen(vecPtr: UnsafeMutableRawPointer) -> UInt {
        __swift_bridge__$Vec_FfiPaneNode$len(vecPtr)
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



