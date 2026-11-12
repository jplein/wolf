# wolf-macos/Sources/WolfMacOS/Preview

The `WKWebView` delegate and script-message handlers of a Markdown preview pane, each of which reports into the `PaneContainerView` that owns the pane. The directory compiles into the [`WolfMacOS`](../README.md) module, not a module of its own.

## Files

#### `PreviewClickFocusHandler.swift`

Purpose: The `WKScriptMessageHandler` that turns a preview web view's capture-phase `mousedown` (posted by `PreviewSeam.clickFocusJS`) into `PaneContainerView.focusPreviewPane(win:)`.

**Public API**

- `init(win:container:)` — `win` is the Neovim window handle identifying the pane in the container's per-window maps.

**Decisions**

- It exists because a `WKWebView` swallows AppKit mouse events, so the container's `mouseDown` never fires over a preview pane. The injected script message is the only signal that a preview pane was clicked.
- `container` is weak: the web view's `WKUserContentController` retains this handler strongly, so a strong back-reference would be a cycle.

**Gotchas** — the click also leaves the web view first responder, so keys reach WebKit before the container, and a key equivalent reaches the menu only if the container passes WebKit's `noop:` on ([why](../../../../../notes/wolf-macos-decisions.md#key-equivalents-over-a-preview)).

#### `PreviewNavigationDelegate.swift`

Purpose: The `WKNavigationDelegate` that routes a preview web view's clicked links through `PreviewLinkSeam` back into the owning `PaneContainerView`.

**Public API**

- `init(win:container:)` — `win` is the Neovim window handle identifying the pane in the container's per-window maps.

**Decisions**

- Every `.linkActivated` navigation is cancelled and handled out of band (open in the browser, scroll to a fragment, or preview another file), so the pane only ever shows core-rendered HTML. Non-link navigations — the initial `loadHTMLString` and programmatic `evaluateJavaScript` scrolls — must keep passing through with `.allow`, so the guard cannot be widened to cancel everything.
- Link classification lives in `WolfKeyMap.PreviewLinkSeam`; this file only dispatches the resulting action.
- `container` is weak because the container retains this delegate (`WKWebView.navigationDelegate` is itself weak).
- The two `didFail…` callbacks are routine bookkeeping, not an error path: they decrement that window's outstanding-navigation count — see [wolf-macos-decisions.md](../../../../../notes/wolf-macos-decisions.md) for why it is a count. A cancelled policy decision, meaning every clicked link, also arrives here, so the container ignores a callback for which it counted no navigation.

#### `PreviewScrollStateHandler.swift`

Purpose: The `WKScriptMessageHandler` that forwards a preview page's pushed scroll state, posted by `PreviewScrollSeam.stateScriptJS`, into its `PaneContainerView`'s per-window cache.

**Public API**

- `init(win:container:)` — `win` is the Neovim window handle identifying the pane in the container's per-window maps.

**Decisions**

- The push direction, and why it cannot be a pull, is in [wolf-macos-decisions.md](../../../../../notes/wolf-macos-decisions.md).
- A body missing `s` degrades to `0` rather than dropping the message, so a partially decodable push still refreshes the cache.
- `container` is weak: the web view's `WKUserContentController` retains this handler strongly.

#### `PreviewSelectionHandler.swift`

Purpose: The `WKScriptMessageHandler` that forwards a preview page's pushed selected text, posted by `PreviewCopySeam.selectionScriptJS`, into its `PaneContainerView`'s per-window cache, where Edit > Copy reads it synchronously.

**Public API**

- `init(win:container:)` — `win` is the Neovim window handle identifying the pane in the container's per-window maps.

**Decisions**

- The push direction, and why it cannot be a pull, is in [wolf-macos-decisions.md](../../../../../notes/wolf-macos-decisions.md).
- An empty payload is a positive clear and drops the cached entry, while a body `PreviewCopySeam.decodeSelectionMessage` rejects leaves the cache untouched: a malformed message says nothing about the selection, so treating it as a clear would discard a good entry.
- `container` is weak: the web view's `WKUserContentController` retains this handler strongly.
