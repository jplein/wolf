# macOS follow-ups

Last verified at commit `1abfb83b013b22dd42b5e13ae533b4f0346c99cd`.

This file holds the cross-platform findings that the task which found them could not close: a divergence between the two front-ends, or a suspected defect on one platform found while working on the other.

New findings are rare. Every task runs `./gates.sh` on a macOS and a Linux worker ([The verification gate](../build.md#the-verification-gate)), so a change is built and tested on both platforms before it lands. A finding lands here only when closing it needs a decision or a check that no gate makes.

## Ledger

### 2026-11-12: closing the last editor pane beside the drawer

Found by reading. With one editor pane and the drawer open, `<C-w>c` and `:close` in the editor on both platforms, and GNOME's Ctrl+Shift+W, close the editor window: `wolf_do_close` and Neovim's `:close` both count the drawer's window, so neither sees the editor as the tabpage's last. That leaves the drawer as the tabpage's only window, which both front-ends hold outside their pane trees. macOS's Close Pane refuses it.

The owners are [`runtime/nvim/README.md`](../runtime/nvim/README.md) for `wolf_do_close` and [Terminal panes](../specs/terminal-panes.md) for the last-window rule. A decision on whether that rule counts the drawer closes it.

## Adding and closing a finding

Give each finding its own `###` section under the ledger, headed by the date it was found and its subject. Under it, say what was found, whether it was measured or found by reading, the document that owns each file involved, and what closes it.

A finding closes when it is fixed under the gate, or when the divergence is accepted and stated, present tense, in the document that owns the file. Closing a finding deletes its section; the commit holds the evidence.
