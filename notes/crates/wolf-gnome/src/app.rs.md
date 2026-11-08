# `crates/wolf-gnome/src/app.rs`

Last verified at commit `92471271522b098162728e788bb3ec4e08040a78`.

## Pinning gtk-xft-dpi

With no desktop portal answering in the UI process, GTK leaves `gtk-xft-dpi` at -1. A web view created after launch on a host that is not yet allocated, as the preview a split creates is, then reports a page viewport of 0×0 in every probe, while its GTK widget is realized, mapped and 301×497 and its DOM holds the document. The pane shows the preview background and no text. A view created on an allocated host, such as the first preview, is not affected.

Measured on the NixOS host, through the cage smokes, with the portal withheld by `GDK_DEBUG=no-portals` or by a session bus with no service directories. The dependency is in the UI process: withholding the portal from the web processes alone leaves the preview rendering. With the portal, GTK reads 98304 and the view's viewport follows its allocation.

Hiding and re-showing the view, reloading it once its host is allocated, and recreating it at that point all leave the viewport at 0×0. Pinning the dpi before any view exists is the one change measured to fix it, with or without a portal, so `pin_unset_xft_dpi` sets it only when GTK reports none and never overrides a session's value.

Where a portal answers, the pin stays idle and the desktop's text scaling survives. Measured on the NixOS host in a private cage session with an activatable portal and a private dconf holding `text-scaling-factor` 1.25: at `activate` GTK already holds the portal's 122880, so the pin does not fire and prints nothing. With `GDK_DEBUG=no-portals` in the same session GTK reports -1 there, and the pin fires and prints `wolf-xft-dpi-pinned: 98304`.

A dpi that arrives after the pin is ignored. A value the app sets on `GtkSettings` outranks every later backend or `XSETTINGS` value, so once the pin fires, which it does only in a session that supplied no dpi by `activate`, a dpi a late-starting portal or `xsettingsd` supplies does not apply for the life of the process.

The pin prints its one `XFT_DPI_PINNED_PREFIX` line on stderr when it fires and nothing otherwise. A preview renders the same whether the pin or the session supplied the dpi, so the cage smokes assert that line in their own log to show the pin ran: without it, a dpi reaching GTK from the host would keep them green with the pin deleted.
