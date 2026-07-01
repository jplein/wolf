# Configuration Decisions Report

This report pins how Wolf is configured: the layers of configuration, what each layer may express, how they merge, and where non-user-edited state is stored. It records decisions; it is not a task list. Implementation is deferred until other in-flight changes land.

## Context

Wolf embeds a pinned, hermetic Neovim, spawned `--embed --clean` with a single bundled `init.lua` as its `-u` entrypoint and its state directories isolated so it never reads or writes the user's Neovim config or state. Wolf needs a configuration story that covers two kinds of settings at once: Neovim-level settings (keymaps, editor options) and GUI-app-level settings (font, font size, window chrome), and it needs both a user-wide layer and a per-project layer.

A guiding principle is that anything a user edits by hand lives in one file, in a language the user already reasons about, and that opening someone else's project never executes code that project ships.

## Decisions

### Layers and entrypoint

1. **Single bundled entrypoint.** The bundled `init.lua` remains the only `-u` target. It prepends the bundled Wolf plugin to `runtimepath`, runs the protected Wolf core setup, then sources the user layer, then applies project overrides, and finally applies the resolved configuration once. Neovim's native user-config loading is not used; the bundled entrypoint sources every layer explicitly so ordering and isolation stay under Wolf's control.

2. **No project `init.lua`.** A project never contributes Lua. Project configuration is a simple declarative file (referred to here as `config.yaml`; the actual format — YAML, TOML, or similar — is decided later) in the project's `.wolf` directory. Because the project layer is pure data with a fixed, known set of levers, opening an untrusted project cannot execute arbitrary code. This removes the project-config trust/allowlist problem entirely rather than mitigating it.

### User layer

3. **User layer is either Lua or declarative, never both.** A user has either a user-level Wolf `init.lua` or a user-level `config.yaml`, not both. The `config.yaml` form exposes a simple, fixed set of levers. The `init.lua` form can configure anything, including behavior entirely outside Wolf's own surface. There is a migration path from `config.yaml` to `init.lua` for a user who outgrows the declarative levers.

4. **User Lua configures both Neovim and the GUI app.** User `init.lua` is the single place a user expresses configuration regardless of whether a given setting is a Neovim-level setting or a GUI-app-level setting. This keeps a user's whole configuration in one file.

### The Neovim-to-GUI bridge

5. **GUI-app settings are bridged from Neovim to the app.** A GUI-level setting such as font or font size is not a Neovim option; the embedded Neovim is a grid of cells and has no concept of a GUI font. The Wolf plugin marshals GUI-affecting settings up to the Rust core / front-end over RPC, which applies them. Neovim-level settings (keymaps, editor options) are applied within Neovim directly. Both are expressed by the user in the same file; they differ only in how they are routed.

### Merge semantics

6. **Project overrides and merges with the user layer.** Settings in the project `config.yaml` override the corresponding settings from the user layer — whether the user layer is a `config.yaml` or an `init.lua` — and are otherwise merged with it: a lever the project does not set falls through to the user's value. Because the project layer is declarative with a fixed lever set, it can only override the subset of settings those levers express; everything else the user configured (including anything set through the full power of user Lua) stands.

7. **Precedence and single apply.** The effective precedence is bundled defaults, then the user layer, then the project `config.yaml`. Layers accumulate into one resolved configuration that is applied once, after all layers have been read, rather than each layer applying as it loads. This keeps behavior independent of load order and avoids leaving partial state if a later layer is malformed.

### Protected core

8. **The Wolf core is protected from the user layer.** The shell depends on the Wolf-internal machinery in the bundled entrypoint — current-file tracking, directory tracking, and the split-pane layout-intent bindings and autocommands. The user layer runs on top of the core and may add its own bindings and settings, but the reserved Wolf core bindings and autogroups are re-asserted after the user layer loads so a user configuration cannot break the shell's pane model.

### Using a system Neovim configuration

9. **System `init.lua` is opt-in by symlink only, at the user's risk.** A user may use their own system Neovim configuration only by symlinking it into their Wolf user-config location; it is never auto-loaded. This keeps the default hermetic. A wholesale system configuration commonly bootstraps a plugin manager that is not on Wolf's `packpath` and sets options that conflict with Wolf's chrome-free grid, so it is supported only as "pull in what you want, at your own risk," not as parity with a standalone Neovim.

### State that users do not edit

10. **Non-user-edited state lives in SQLite in the project `.wolf` directory.** Window geometry, sidebar width, and the file-picker MRU cache are stored in a SQLite file in the project's `.wolf` directory. This file is machine-written; the Rust core owns it exclusively and Neovim does not touch it. Wolf automatically writes a `.wolf/.gitignore` that ignores the state SQLite file, so the machine-local state is not committed while a shared, hand-edited project `config.yaml` can be.

### Reload and timing

11. **Restart-only.** Configuration is read at startup. Editing any layer takes effect on the next launch; there is no live re-source of a changed configuration within a running session.

12. **Project configuration resolves once, at startup.** The project layer is resolved from the launch context when the session starts and is not re-resolved when the working directory changes during the session.

### Determinism for tests

13. **The user and project layers are skippable for tests.** Wolf's render and highlight tests spawn a real Neovim and assert on the grid, so they must be insulated from a developer's own Wolf configuration. The layering can be disabled (for example via an environment flag or by pointing the Wolf config location at an empty directory) so tests run against the bundled defaults alone.

## Rationale

- **One hand-edited file, in the right language.** A user keeps their whole configuration — Neovim-level and GUI-level alike — in a single file, in a language they already use, with a simple declarative on-ramp (`config.yaml`) and a full-power escape hatch (`init.lua`) reachable by a defined migration.
- **Untrusted projects cannot run code.** Making the project layer pure declarative data is what makes opening an arbitrary Markdown repository safe. It sidesteps the arbitrary-code-execution risk that any project-local Lua would carry, without needing a trust prompt or allowlist.
- **The bridge keeps the model coherent.** Because GUI settings cannot be Neovim options, routing them over RPC lets the user still express everything in one place while each setting is applied where it actually lives.
- **Determinism is preserved end to end.** Explicit sourcing from the single entrypoint, single-apply merge, a protected core, and skippable layers together keep the captive editor's behavior reproducible — which the render/highlight tests depend on.

## Consequences and follow-up (not part of these decisions)

- The concrete declarative format for `config.yaml` is deferred.
- The exact set of levers the declarative form exposes, and their mapping to the full Lua surface, must be defined; this also defines the `config.yaml`-to-`init.lua` migration.
- Where the declarative file is parsed (Rust core versus Lua) and how parsed values reach both Neovim and the GUI over the bridge is an implementation detail to be decided.
- The spawn setup must separate the ephemeral Neovim state directory (isolated, throwaway) from the persistent Wolf user-config location (a stable path hosting the user layer); these can no longer be the same directory.
- The catalog of which settings are Neovim-level versus GUI-level, and the RPC messages that carry the GUI-level ones, must be specified.
- The list of reserved Wolf core bindings and autogroups that are re-asserted after the user layer must be enumerated.

These are implementation steps deferred to later changes; this report only fixes the strategy above.
