# `crates/wolf-gnome/src/pane_view.rs`

Last verified at commit `cc60afb1257ee0c5474105e363d0d452ba050afb`.

## Every due pane has a view

The render pass records `loaded` whatever the HTML was, the empty string included. An RPC failure answers empty, and leaving `loaded` behind on that would re-arm the reload and re-issue the blocking render on every wake forever; the next real edit bumps the tick and reloads anyway.

It is also recorded on `load_preview_document`'s no-view early return, which is sound only because every pane that reaches the record already has a web view. The argument must hold on both entry paths: the pass runs inline on a wake and from the coalescer's one-shot, where nothing precedes it, so it cannot rest on `sync_preview_view` having just run. It rests on three facts instead:

- `current_load` is written only by `refresh_pane_state`, and a pane is due only when it is `Some`.
- Every wake that runs `refresh_pane_state` runs `refresh_previews`, and so `sync_preview_view` over every pane, later in that same wake.
- `consume_pending_layout` returns early from its wake before `refresh_pane_state`, so a pane built by a layout rebuild carries `current_load == None`, and is not due, until a wake that does both.

Moving any of the three records a document that was loaded into nothing, leaving the pane blank until the source's tick moves again. The due set is re-derived in the pass rather than captured at arm time, so a pane that closed mid-burst is simply no longer in it.

## One drag gesture

The whole press, drag and release sequence is one `gtk::GestureDrag` rather than a `GestureClick` plus another controller. A gesture takes an implicit grab on its sequence and keeps delivering after the pointer leaves the widget, which drag-past-the-edge selection needs, where an `EventControllerMotion` stops at the pane edge. One gesture rather than two also removes any question of the pair fighting over sequence claiming.

The press path relies on `drag-begin` firing for a plain press with no movement. Checked against the GTK 4.22.4 C source, the version the devShell builds against: `GtkGestureDrag` installs `gesture_class->begin`, and `GtkGesture::handle_event` emits `begin` straight from `GDK_BUTTON_PRESS` once the point count matches `n-points` (default 1). So `drag-begin` is not motion-gated, and focus plus `MouseAction::Press` on it is sound.
