# tools/wolf-pointer-inject

**Linux-only.** A long-lived Wayland [`zwlr_virtual_pointer_v1`](https://gitlab.freedesktop.org/wlroots/wlr-protocols/-/blob/master/unstable/wlr-virtual-pointer-unstable-v1.xml) client that both **holds the seat's pointer capability open** and **injects** absolute motion, button press/release and axis events on command. It exists so the `wolf-gnome` cage smoke tests can drive real pointer input into the real GTK front-end running under a headless compositor.

Built by [`flake.nix`](../../flake.nix) into the Linux devShell as `wolf-pointer-inject`. It is not part of any shipped artifact.

## Why it must be long-lived

Under `cage` with `WLR_BACKENDS=headless` there are no real input devices. wlroots advertises `WL_SEAT_CAPABILITY_POINTER` on the seat **only while some pointer device exists**, so a tool that creates a virtual pointer, injects, and immediately destroys it flips the capability on and back off again before any client can bind `wl_pointer`. This process therefore creates exactly one virtual pointer at startup and keeps it alive for the whole scenario, which is what makes the seat carry the pointer capability continuously.

The recommended ordering in a scenario is **helper first, app second**: start `wolf-pointer-inject`, wait for its `ready` line, and only then launch the GTK app, so the seat already carries `WL_SEAT_CAPABILITY_POINTER` when GTK does its first `wl_seat` roundtrip and calls `get_pointer` at bind time.

## Why not `wlrctl`, and why it must stay out of `flake.nix`

`wlrctl` binds exactly this global, so it looks like the obvious choice. It was measured under cage and does not work: the `WAYLAND_DEBUG` trace shows `wl_seat.capabilities(3)` -> `get_pointer` -> `wl_seat.capabilities(2)` -> `release`, **59 microseconds apart**. Zero `wl_pointer.button` events ever reached the GTK client, and `wlrctl` exited `0` regardless — the exact vacuous-success shape this repo keeps documenting. It also has no press/release split, so a drag cannot be expressed at all. Adding it to the devShell would provide a tool that appears to work and silently delivers nothing.

## Why `ydotool` cannot work either

`ydotool` writes to a uinput kernel device. `WLR_BACKENDS=headless` means cage consumes **no** kernel input devices at all, so nothing `ydotool` writes is ever read.

## Command grammar

One command per line on stdin. Blank lines are ignored and do not advance the sequence counter.

| Line                                                 | Wayland requests emitted, in order                                                         |
| ---------------------------------------------------- | ------------------------------------------------------------------------------------------ |
| `motion <x> <y>` (uints, `0 <= x < W`, `0 <= y < H`) | `motion_absolute(time, x, y, extent_w, extent_h)` -> `frame`                               |
| `press <button>` (uint)                              | `button(time, button, PRESSED)` -> `frame`                                                 |
| `release <button>` (uint)                            | `button(time, button, RELEASED)` -> `frame`                                                |
| `axis <value>` (double)                              | `axis_source(CONTINUOUS)` -> `axis(time, VERTICAL_SCROLL, value)` -> `frame`               |
| `axis <value> <discrete>`                            | `axis_source(WHEEL)` -> `axis_discrete(time, VERTICAL_SCROLL, value, discrete)` -> `frame` |
| `sync`                                               | none — a pure barrier (roundtrip + ack)                                                    |
| `quit`                                               | `destroy` the pointer, flush, disconnect, exit 0                                           |

`axis_discrete` carries **both** the continuous `value` and the `discrete` step count in one request (its arguments are `time, axis, value, discrete`), so no separate `axis` call is needed alongside it. `discrete` must be non-zero; its sign gives the scroll direction.

Button codes are the `<linux/input-event-codes.h>` values, defined locally in `main.c` so the derivation needs no `linux-headers` dependency: `BTN_LEFT` = `272` (`0x110`), `BTN_RIGHT` = `273` (`0x111`), `BTN_MIDDLE` = `274` (`0x112`).

### The `frame` requirement

**`frame` after every event group is mandatory, not decorative.** GTK's Wayland backend buffers `wl_pointer` events and only converts them into `GdkEvent`s on `wl_pointer.frame`. Without it the compositor delivers bytes and GTK delivers nothing — a scenario that looks like it injected input and in fact injected none. Every command above that emits events ends with a `frame`.

## Ack protocol

stdout is set unbuffered (`setbuf(stdout, NULL)`) as the very first statement in `main`. Without that, stdout redirected to a file or a pipe is block-buffered and neither the readiness line nor any ack ever reaches the reader, so every poll hangs. If a scenario hangs on the first poll, check this first.

- `ready extents=<W>x<H> manager_version=<v> outputs=<n>` — printed once, after the virtual pointer has been created and round-tripped. `<W>x<H>` is the resolved motion extent, which is the authoritative output size for the scenario. Do not assume 1280x720; read this line.
- `ok <seq>` — command `<seq>` (1-based, counting non-blank lines) has been processed. The ack is printed **after** a `wl_display_roundtrip`, so it means "the compositor has processed this event", not "the bytes left our buffer". That is what makes polling for it meaningful rather than cosmetic. (The one exception is `quit`, whose ack follows a `wl_display_flush` + `wl_display_disconnect` — a roundtrip there would turn a normal teardown race into a spurious exit `8`.)
- `err <seq> <message>` — fatal. The message goes to both stdout and stderr, and the process exits.

## Options

- `-e WxH` — override the motion extents. Default: the single bound output's current mode.
- `-v` — echo each received command to stderr.
- `-h` / `--help` — print the grammar and exit 0. This is also the build-verification entry point: `nix develop --command wolf-pointer-inject --help`.

## Exit codes

| Code | Meaning                                                                         |
| ---- | ------------------------------------------------------------------------------- |
| `0`  | clean exit — `quit`, or EOF on stdin                                            |
| `1`  | bad command-line usage                                                          |
| `2`  | could not connect to the Wayland display (check `WAYLAND_DISPLAY`)              |
| `3`  | the compositor does not advertise `zwlr_virtual_pointer_manager_v1`             |
| `4`  | the compositor does not advertise `wl_seat`                                     |
| `5`  | no `wl_output` was advertised and no `-e WxH` was given                         |
| `6`  | more than one `wl_output` — the coordinate model assumes exactly one            |
| `7`  | the resolved extents are zero in at least one axis (the server divides by them) |
| `8`  | `wl_display_roundtrip` failed                                                   |
| `9`  | unknown or malformed command                                                    |
| `10` | a `motion` coordinate lies outside the resolved extents                         |

Exit `10` is the counterpart of exit `6`: a point at or beyond the extents is a hard error rather than a silent clamp. `motion_absolute` takes coordinates as a fraction of the extents, so `x >= extent_w` normalises to `>= 1.0` and wlroots positions the pointer off-output — the compositor accepts the request, the tool would ack `ok`, and the client receives nothing. That is the same "exits 0, delivers nothing" shape that makes `wlrctl` unusable, so it is refused instead. The message names the offending coordinate and the resolved extents.

Exit `3` is the permanent, executable answer to "does cage advertise the virtual-pointer manager at all". There are no signal handlers: default `SIGTERM`/`SIGINT` termination is what a scenario's teardown relies on. The process also exits on stdin EOF and on a failed roundtrip, so an orphaned helper reaps itself when the compositor tears the socket down.

## Coordinate model

`motion` takes absolute **output** pixels, `(0, 0)` at the top left. The extents default to the single output's current mode (read from `wl_output.mode` with the `WL_OUTPUT_MODE_CURRENT` flag set) and are reported on the readiness line; `-e WxH` overrides them.

Both coordinates must lie strictly **inside** those extents — `0 <= x < W` and `0 <= y < H`. A negative or out-of-range coordinate is fatal (exit `9` and exit `10` respectively), never clamped and never silently accepted; see the exit-code table for why the upper bound is a hard error.

When the manager binds at version 2 or newer and exactly one output was bound, the pointer is created with `create_virtual_pointer_with_output(seat, output)` rather than `create_virtual_pointer(seat)`. That pins `motion_absolute`'s coordinate space to that output rather than to the (single-output, but implicit) layout, which makes the extents unambiguously the output's pixel size. Under cage headless the two are identical today; `_with_output` is what makes that a stated fact rather than a coincidence.

More than one output is a hard error (exit `6`) rather than a guess.

## How it is built

`flake.nix` builds it with `pkgs.stdenv.mkDerivation`:

- `wayland-scanner` (a `nativeBuildInput`) generates `wlr-virtual-pointer-unstable-v1-client-protocol.h` and `…-protocol.c` from `${pkgs.wlr-protocols}/share/wlr-protocols/unstable/wlr-virtual-pointer-unstable-v1.xml`. The XML is taken from `wlr-protocols` rather than vendored, so the protocol revision stays tied to the wlroots the flake actually ships.
- `$CC -O2 -Wall -Wextra -I. -o wolf-pointer-inject main.c wlr-virtual-pointer-unstable-v1-protocol.c $(pkg-config --cflags --libs wayland-client)`.
- `wayland.xml` is deliberately **not** scanned. `wl_registry`, `wl_seat`, `wl_output`, the `WL_POINTER_*` enums and `wl_fixed_from_double` all come from `wayland-client.h` / `wayland-client-protocol.h` and `libwayland-client.so`, which `pkgs.wayland` provides.

The derivation is a thunk forced only by `linuxShell.packages`, so Nix's laziness keeps this Wayland-only program off the Darwin eval path. Do not add a top-level reference to it.

## No gate step covers this file

`cargo fmt`, `cargo clippy`, `prettier` and `cargo test` reach none of `main.c`. Its **only** automated check is `-Wall -Wextra` in the derivation, which makes `nix develop` the de facto gate. `-Werror` is deliberately not used: a nixpkgs gcc bump must not be able to break `nix develop` for the whole team over a new pedantic warning in wayland-scanner's generated `.c`.

## Who uses it

The pointer smokes in [`crates/wolf-gnome/tests/smoke_headless.rs`](../../crates/wolf-gnome/tests/smoke_headless.rs), which start it inside `cage` before the app, drive it over a FIFO with a poll-for-ack protocol, and assert on files the captive Neovim writes about its own state. It is listed in that file's `REQUIRED_TOOLS`, so a devShell without it fails the suite loudly rather than skipping.

## Files

- `main.c` — the whole program.
