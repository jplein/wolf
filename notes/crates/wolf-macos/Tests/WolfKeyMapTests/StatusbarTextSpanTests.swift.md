# `crates/wolf-macos/Tests/WolfKeyMapTests/StatusbarTextSpanTests.swift`

Last verified at commit `cc60afb1257ee0c5474105e363d0d452ba050afb`.

## Manual check

That the statusbar text origin and the live `WKWebView` frame coincide on screen is an integration property spanning `paintStatusbar`, `syncPreviewWebViews` and the Markdown CSS. No unit test can pin it; these tests pin only the shared arithmetic both consumers derive from.

To check it by eye, open a preview in a window wider than 90 columns and confirm the statusbar filename's left edge sits on the centered content's left edge. Toggling full width must move it to the pane's left edge.
