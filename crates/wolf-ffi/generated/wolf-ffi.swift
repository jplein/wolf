import WolfFFI
public func wolf_ffi_start<GenericIntoRustString: IntoRustString>(_ init_lua: GenericIntoRustString, _ cwd: GenericIntoRustString, _ cols: UInt16, _ rows: UInt16) -> Optional<WolfCoreHandle> {
    { let val = __swift_bridge__$wolf_ffi_start({ let rustString = init_lua.intoRustString(); rustString.isOwned = false; return rustString.ptr }(), { let rustString = cwd.intoRustString(); rustString.isOwned = false; return rustString.ptr }(), cols, rows); if val != nil { return WolfCoreHandle(ptr: val!) } else { return nil } }()
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
    public func read_frame() -> FfiFrame {
        FfiFrame(ptr: __swift_bridge__$WolfCoreHandle$read_frame(ptr))
    }

    public func send_key(_ ev: FfiKeyEvent) {
        __swift_bridge__$WolfCoreHandle$send_key(ptr, ev.intoFfiRepr())
    }

    public func resize(_ cols: UInt16, _ rows: UInt16) {
        __swift_bridge__$WolfCoreHandle$resize(ptr, cols, rows)
    }

    public func shutdown() {
        __swift_bridge__$WolfCoreHandle$shutdown(ptr)
    }

    public func start_file_search() -> FfiFileSearch {
        FfiFileSearch(ptr: __swift_bridge__$WolfCoreHandle$start_file_search(ptr))
    }

    public func open<GenericIntoRustString: IntoRustString>(_ path: GenericIntoRustString) {
        __swift_bridge__$WolfCoreHandle$open(ptr, { let rustString = path.intoRustString(); rustString.isOwned = false; return rustString.ptr }())
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



