# `crates/wolf-macos/Sources/WolfKeyMap/PreviewCopySeam.swift`

Last verified at commit `cc60afb1257ee0c5474105e363d0d452ba050afb`.

## Synchronous copy

Menu validation is synchronous, so whether the preview has a selection can never be an `evaluateJavaScript` round trip, and a cache is needed for the enable decision regardless. Carrying the text in that same cached snapshot makes the enable decision and the copied bytes unable to disagree.

It also keeps `copy()` fully synchronous. An asynchronous `NSPasteboard` write would let a ⌘C immediately followed by ⌘V read the previous clipboard.
