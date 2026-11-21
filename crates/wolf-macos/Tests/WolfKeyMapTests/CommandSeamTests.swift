// See crates/wolf-macos/Tests/WolfKeyMapTests/README.md#commandseamtestsswift

import XCTest

@testable import WolfKeyMap

final class CommandSeamTests: XCTestCase {

  // MARK: - every registry discriminant decodes

  func testEveryRegistryDiscriminantDecodes() {
    let count = wolf_ffi_command_count()
    XCTAssertGreaterThan(count, 0, "the registry read over the FFI is empty")
    for kind in 0..<count {
      XCTAssertNotNil(
        CommandKind(ffi: kind),
        "core has a command at discriminant \(kind) that Swift cannot decode: "
          + "CommandKind lacks its case or init?(ffi:) lacks its arm")
    }
  }

  // MARK: - the first discriminant past the registry decodes to nil

  func testFirstOutOfRangeDiscriminantIsNil() {
    let count = wolf_ffi_command_count()
    XCTAssertNil(
      CommandKind(ffi: count),
      "discriminant \(count), the registry's count, decodes to a case: Swift maps a value "
        + "core never sends, or the count over the FFI is short")
    XCTAssertNil(CommandKind(ffi: .max), "a far out-of-range discriminant -> nil")
  }

  // MARK: - each case decodes from the discriminant Rust gives its id

  func testEachCaseDecodesFromItsRustDiscriminant() {
    for kindCase in CommandKind.allCases {
      let name = String(describing: kindCase)
      let id = snakeCase(name)
      let kind = wolf_ffi_command_kind(id)
      XCTAssertEqual(
        CommandKind(ffi: kind), kindCase,
        ".\(name) is id \(id), which Rust's command_kind gives discriminant \(kind), but "
          + "init?(ffi:) decodes that to \(String(describing: CommandKind(ffi: kind))). "
          + "\(UInt8.max) means no core id matches the case's name; otherwise an arm or a "
          + "command_kind constant moved")
    }
  }

  /// The registry id a case names by the lowerCamelCase rule: `goToFile` is `go_to_file`.
  private func snakeCase(_ camel: String) -> String {
    var id = ""
    for ch in camel {
      if ch.isUppercase {
        id += "_" + ch.lowercased()
      } else {
        id.append(ch)
      }
    }
    return id
  }
}
