import WolfFFI
public func wolf_ffi_start<GenericIntoRustString: IntoRustString>(_ init_lua: GenericIntoRustString, _ cwd: GenericIntoRustString, _ cols: UInt16, _ rows: UInt16, _ window_index: UInt16) -> Optional<WolfCoreHandle> {
    { let val = __swift_bridge__$wolf_ffi_start({ let rustString = init_lua.intoRustString(); rustString.isOwned = false; return rustString.ptr }(), { let rustString = cwd.intoRustString(); rustString.isOwned = false; return rustString.ptr }(), cols, rows, window_index); if val != nil { return WolfCoreHandle(ptr: val!) } else { return nil } }()
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

    public func windows() -> RustVec<FfiWinInfo> {
        RustVec(ptr: __swift_bridge__$WolfCoreHandle$windows(ptr))
    }

    public func grid_for_win(_ win: Int64) -> UInt64 {
        __swift_bridge__$WolfCoreHandle$grid_for_win(ptr, win)
    }

    public func active_win() -> Int64 {
        __swift_bridge__$WolfCoreHandle$active_win(ptr)
    }

    public func focus_win(_ win: Int64) {
        __swift_bridge__$WolfCoreHandle$focus_win(ptr, win)
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

    public func current_file() -> RustString {
        RustString(ptr: __swift_bridge__$WolfCoreHandle$current_file(ptr))
    }

    public func cwd() -> RustString {
        RustString(ptr: __swift_bridge__$WolfCoreHandle$cwd(ptr))
    }

    public func theme_titlebar_bg() -> FfiColor {
        __swift_bridge__$WolfCoreHandle$theme_titlebar_bg(ptr).intoSwiftRepr()
    }

    public func theme_titlebar_fg() -> FfiColor {
        __swift_bridge__$WolfCoreHandle$theme_titlebar_fg(ptr).intoSwiftRepr()
    }

    public func open<GenericIntoRustString: IntoRustString>(_ path: GenericIntoRustString) {
        __swift_bridge__$WolfCoreHandle$open(ptr, { let rustString = path.intoRustString(); rustString.isOwned = false; return rustString.ptr }())
    }

    public func state_set_geometry(_ x: Double, _ y: Double, _ width: Double, _ height: Double) {
        __swift_bridge__$WolfCoreHandle$state_set_geometry(ptr, x, y, width, height)
    }

    public func state_set_sidebar_visible(_ visible: Bool) {
        __swift_bridge__$WolfCoreHandle$state_set_sidebar_visible(ptr, visible)
    }

    public func state_set_toolbar_mode(_ mode: UInt8, _ set: Bool) {
        __swift_bridge__$WolfCoreHandle$state_set_toolbar_mode(ptr, mode, set)
    }

    public func state_set_window_count(_ count: UInt16) {
        __swift_bridge__$WolfCoreHandle$state_set_window_count(ptr, count)
    }

    public func state_flush() -> Bool {
        __swift_bridge__$WolfCoreHandle$state_flush(ptr)
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

    public func cwd() -> RustString {
        RustString(ptr: __swift_bridge__$FfiWindowState$cwd(ptr))
    }

    public func sidebar_visible() -> Bool {
        __swift_bridge__$FfiWindowState$sidebar_visible(ptr)
    }

    public func toolbar_mode() -> UInt8 {
        __swift_bridge__$FfiWindowState$toolbar_mode(ptr)
    }

    public func toolbar_mode_set() -> Bool {
        __swift_bridge__$FfiWindowState$toolbar_mode_set(ptr)
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



