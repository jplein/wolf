# macOS follow-ups

Last verified at commit `f3b145d36e29d4db83d48d86e73e32b51ce6d434`.

This file holds the cross-platform findings that the task which found them could not close: a divergence between the two front-ends, or a suspected defect on one platform found while working on the other.

New findings are rare. Every task runs `./gates.sh` on a macOS and a Linux worker ([The verification gate](../build.md#the-verification-gate)), so a change is built and tested on both platforms before it lands. A finding lands here only when closing it needs a decision or a check that no gate makes.

## Ledger

### 2026-08-08: a restart keeps stale editor-pane window handles on macOS

After a crash-restart, `PaneContainerView.rebind(tabpageHandle:)` drops only the terminal drawer, and every editor pane of a split keeps its pre-crash window handle. `resolvePendingGrids` binds an unbound pane to whatever grid `grid_for_win` returns for that handle. A fresh Neovim numbers its windows from 1000 again, so a later window, such as a new drawer or split, can bind into a stale editor pane.

wolf-gnome's `reset_after_restart` resets to one fresh pane instead. The files involved are owned by the [`PaneContainerView.swift`](../crates/wolf-macos/Sources/WolfMacOS/README.md#panecontainerviewswift) and [`pane_view.rs`](../crates/wolf-gnome/src/README.md#pane_viewrs) entries.

Found by reading, and not reproduced. It closes by reproducing it on a macOS host: a two-pane split, kill `nvim --embed`, restart, open the drawer. Then either fix macOS under the gate, or state the divergence as accepted in the `PaneContainerView.swift` entry.

## Adding and closing a finding

Give each finding its own `###` section under the ledger, headed by the date it was found and its subject. Under it, say what was found, whether it was measured or found by reading, the document that owns each file involved, and what closes it.

A finding closes when it is fixed under the gate, or when the divergence is accepted and stated, present tense, in the document that owns the file. Closing a finding deletes its section; the commit holds the evidence.
