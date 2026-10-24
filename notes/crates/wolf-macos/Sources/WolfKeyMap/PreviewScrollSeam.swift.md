# `crates/wolf-macos/Sources/WolfKeyMap/PreviewScrollSeam.swift`

Last verified at commit `cc60afb1257ee0c5474105e363d0d452ba050afb`.

## No JSON escaper

Every value this seam interpolates into JavaScript is a validated number, a `UInt32` line or column or a `Double` coordinate, and anchor ids are assembled inside the JavaScript from those numbers rather than in Swift. So no escaper is needed and none should be added; `PreviewLinkSeam` needs one only because it interpolates a user-controlled fragment string.

A `Double` is rendered with `String(format: "%.2f", v)` and no locale argument, so the decimal separator is always `.` and can never become a `,` that silently splits one JavaScript argument into two. Integers use `String(v)`.
