# `crates/wolf-gnome/src/tree_view.rs`

Last verified at commit `cc60afb1257ee0c5474105e363d0d452ba050afb`.

## The reroot triple

`entries`, the memo and `entries_version` move together, before any GTK call. `refresh` keeps the invariant that the shared entries and the version the memo keys on advance together, and every instant between these statements and the next GTK call is one in which a re-entrant create-func could answer a query.

Clearing `entries` while the memo still holds the previous project's index at the old version would answer that query out of the wrong index. A fresh `FileSearch` restarts `version` at 1, so a version-keyed memo is only ever valid within one search session.

## Unbinding the current-file label

GTK does not order bind and unbind across recycled widgets. Collapsing a directory shifts rows up, so the same file is bound onto the surviving list item before the surplus one is unbound, and both carry that file's rel_path.

A blind `remove(&rel_path)` would delete the live registration the bind just made, leaving that row bold with no way to clear it — two bold rows until the next version advance. `gtk::Label`'s `PartialEq` is a glib pointer comparison, so comparing the mapped label against this widget is the only question that distinguishes them.

Under rule R1 the lookup borrow lives in its own statement and the removal takes a fresh one, so neither is held across a widget call.
