1. Preview LaTeX math expressions — DONE
   - Any dependencies are local to the repo: no network access required to preview
   - Done: `render_html` enables `Options::ENABLE_MATH` and converts `$…$` / `$$…$$` LaTeX to native MathML (via the pure-Rust `pulldown-latex` crate), rendered by WebKit / WebKitGTK. See [`preview-math-and-fonts-decisions.md`](preview-math-and-fonts-decisions.md).
2. Vendor/bundle the Inter font, use it as the default font — DONE
   - Any dependencies are local to the repo: no network access required to preview
   - Done: the Inter variable woff2 is vendored under `crates/wolf-core/src/markdown/assets/`, inlined as a base64 `data:` `@font-face`, and fronts the `body.wolf-markdown` font stack (Latin Modern Math is bundled + pinned the same way for the math render). See [`preview-math-and-fonts-decisions.md`](preview-math-and-fonts-decisions.md).
3. Edit command - like the inverse of preview. Only has an effect in a Preview pane. If the active pane is a preview pane and the other pane is an editor pane, edit the file in the other pane; otherwise, open it in the same pane as the preview. If the preview pane is the only pane, open the editor in a new pane on the left side. — DONE
   - Done: the `Edit` command (`:WolfEdit` / `require('wolf').edit()`, "Edit Source" on the View menu, appended as discriminant 19) navigates a preview pane back to its source per three placement modes (`split_left` / `reuse_editor_pane` / `active`) chosen by the pure `EditSeam`, enabled only in a preview pane (`Snapshot::is_preview_active`, fed by the `wolf_current_file` notification's 3rd bool arg). See [`preview-edit-command-decisions.md`](preview-edit-command-decisions.md).
4. Syntax highlighting in a Markdown code block should mirror Neovim syntax highlighting. See the document [preview-code-block-syntax-highlighting-decisions.md](preview-code-block-syntax-highlighting-decisions.md).
5. Light mode theme - make the theme CSS respond to the dark/light mode media query
