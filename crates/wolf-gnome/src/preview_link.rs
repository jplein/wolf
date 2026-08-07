//! The pure, GTK-free URI rulebook for a link CLICKED inside the Markdown preview:
//! what the click means, what base URI the preview document is loaded against, and
//! the one line of JavaScript that scrolls a fragment into view.
//!
//! A port of `WolfKeyMap/PreviewLinkSeam.swift` (see
//! `crates/wolf-macos/Sources/WolfKeyMap/README.md`). Data in, data out — `std` and
//! `serde_json` only, no `gtk`, no `gdk`, no `webkit6`, no `WolfCore` — which is
//! what lets the whole rulebook be unit-tested headlessly, the way [`crate::nav_ui`]
//! and [`crate::clipboard_ui`] are. The widget work of carrying the answers out
//! lives in [`crate::pane_view`].
//!
//! The split against its sibling [`crate::preview_ui`] is deliberate and worth
//! keeping: **this module owns URI arithmetic; [`crate::preview_ui`] owns the
//! liveness gates and the load strategy.** Neither knows the other's inputs.
//!
//! ## What [`classify`] answers, over the URI WebKit has ALREADY resolved
//!
//! The input is the resolved link target, not the raw `href`: WebKit has already
//! applied the document's base URI (see [`file_uri_for`]) by the time a navigation
//! decision exists. So relative-to-absolute resolution is the browser engine's job
//! in both front-ends, and this table never has to do it.
//!
//! The rows are in EVALUATION order, which is load-bearing twice: `about` is tested
//! before the foreign-scheme row, and the same-path comparison before the
//! different-path one.
//!
//! | # | URI shape | Answer |
//! | --- | --- | --- |
//! | 1 | `about:blank` with a non-empty fragment | [`PreviewLink::ScrollToFragment`] |
//! | 2 | any other `about:` URI | [`PreviewLink::Refuse`] |
//! | 3 | a scheme that is present and not `file` (`http`, `https`, `mailto`, `tel`, …) | [`PreviewLink::OpenExternally`] |
//! | 4 | `file`, an authority that is neither empty nor `localhost` | [`PreviewLink::Refuse`] |
//! | 5 | `file` with an empty path (`file://`) | [`PreviewLink::Refuse`] |
//! | 6 | `file`, the same path as the source, a non-empty fragment | [`PreviewLink::ScrollToFragment`] |
//! | 7 | `file`, the same path, an absent or empty fragment | [`PreviewLink::PreviewFile`] with no fragment |
//! | 8 | `file`, a different path (with or without a fragment) | [`PreviewLink::PreviewFile`] |
//! | 9 | no scheme, a non-empty path | [`PreviewLink::PreviewFile`] (defensive) |
//! | 10 | no scheme and no path (an empty href) | [`PreviewLink::Refuse`] |
//!
//! ## Rows 1, 2, 9 and 10 are DEFENSIVE, and the two pairs rest on different
//! evidence
//!
//! All four cover the document [`file_uri_for`] answers `None` for, which is the
//! document `load_html` is handed a `NULL` base for. What a click in it delivers
//! depends on the KIND of href, and only one of the two kinds has been observed on
//! this port — so they are recorded apart rather than folded into one inference.
//!
//! **Rows 1 and 2 rest on the API documentation and are UNMEASURED.**
//! `webkit_web_view_load_html` (WebKit-6.0) is explicit that a `NULL` base
//! "defaults to `about:blank`", so a FRAGMENT-ONLY href is expected to arrive as
//! `about:blank#intro` — carrying an `about` scheme, which without row 1 would fall
//! through to row 3 and be handed to `gtk::UriLauncher`. Asking the desktop to open
//! a browser on `about:blank#intro` is wrong in a way that has no upside, which is
//! why `about` is refused outright, and row 1 is the useful half: an anchor in a
//! document with no location is still an anchor, and scrolling to it needs the
//! element id and nothing else. That expectation is why both rows are kept. It has
//! not been observed here, and the citation is the whole of its support.
//!
//! **Rows 9 and 10 are MEASURED, and what arrives is not `about:` at all.** With
//! `load_html(html, None)` a RELATIVE href reaches the handler RAW — observed
//! `uri="lnkinto.md"` under `cage` on 2026-08-06 — rather than resolved against
//! `about:blank`. That is row 9 exactly, and row 10 is the empty-href sibling it
//! shares a branch with. **Do not write down that nothing produces a schemeless
//! target**: one `load_html` argument does, which is what the second falsifier on
//! `smoke_headless::preview_link_click_repoints_the_pane_under_cage` exercises.
//!
//! What makes those two unreachable is [`file_uri_for`]'s DOMAIN instead: it answers
//! `Some` for every source path the running application can produce, so the base is
//! never `None` in the first place. Exactly two inputs make it answer `None`, and
//! neither occurs.
//!
//! * An EMPTY source path. `runtime/nvim/init.lua:2229`, inside
//!   `wolf_setup_preview`'s `emit_previews`, filters an empty
//!   `b:wolf_preview_source` out of the `wolf_preview` notification, so a preview of
//!   a buffer with no name never reaches `Snapshot::previews`, never becomes a
//!   [`crate::pane_view`] preview pane, and never gets a web view at all. MEASURED
//!   under `cage` on 2026-08-06: `:enew` plus `:setlocal filetype=markdown` plus
//!   `:WolfPreview` binds a preview scratch inside Neovim, and the front end draws
//!   two ORDINARY editor panes — no `decide-policy` is emitted for any click in
//!   either.
//! * A RELATIVE source path, which `nvim_buf_get_name` never yields: it answers an
//!   absolute path or an empty one.
//!
//! An UNSAVED buffer is not the no-base document. `_G.wolf_new_untitled` names it,
//! so `nvim_buf_get_name` answers an absolute `…/Untitled N.md` for a file that was
//! never written, and the base URI is an ordinary `file:` URI. Measured in the same
//! run: base `file:///…/project/Untitled%202.md` — the space percent-encoded, which
//! is what makes [`percent_encode_path`] load-bearing on a path a user really gets —
//! against which a relative `beta.md` resolves through row 8 exactly as it does for a
//! saved file.
//!
//! Same-path comparison is LEXICAL normalization of both sides (drop `.`, pop on
//! `..`, keep the root), so `/a/./x.md` and `/a/x.md` are one file — matching
//! macOS's `standardizedFileURL.path`. There is deliberately **no symlink
//! resolution**: it would be a filesystem touch in a pure seam, and macOS does not
//! do it either, so the two front-ends would answer differently for the same click.
//!
//! ## The divergences from macOS, and why each is deliberate
//!
//! Every one of them narrows what may be launched or opened, and all four share a
//! reason: on GNOME the "hand it to the default handler" path can start an arbitrary
//! process, the trigger is a string in a document the user merely previewed, and
//! there is no Gatekeeper/quarantine counterpart to lean on. A refusal is the
//! conservative end of a decision a later unit can reverse with its own evidence; a
//! launch is not.
//!
//! **Rows 2, 5 and 10 REFUSE where `PreviewLinkSeam` calls `NSWorkspace.open` or
//! previews an empty path.** An `about:` URI, a `file:` URI with nothing after the
//! scheme, and a target with neither scheme nor path all name no file and no
//! resource a handler could open.
//!
//! **Row 4 REFUSES a non-local `file:` authority**, which macOS reads straight
//! through to the URL's path. `preview_file` takes a filesystem path, so passing
//! `file://evil.com/etc/passwd` through would open whatever sits at that path on
//! THIS machine while the document claimed to name another host's.
//!
//! **A `file:` link that core declines to preview is REFUSED, never handed to the OS
//! default handler.** That decision is not taken here — this module cannot know
//! whether the linked file is Markdown without touching the filesystem, and core's
//! `preview_file` is the one gate that does. Rows 8 and 9 therefore always answer
//! [`PreviewLink::PreviewFile`], and [`crate::pane_view`] prints when core answers
//! `0`. Both halves of the rule have to hold for the divergence to mean anything,
//! which is why it is stated in the module that produces the input as well as in the
//! one that consumes the answer.
//!
//! ## A query string is NOT stripped, and copying the server's stripping would be a
//! bug
//!
//! `file:///a/x.md?v=1` yields a path of `/a/x.md?v=1`. `?` is a legal byte in a
//! Unix filename, so a file genuinely named that way must reach core; core answers
//! `0` for the one that is not, and the click prints a refusal, which is an honest
//! failure rather than a wrong file opened quietly.
//!
//! `wolf_core::browser_preview`'s `resolve_path` DOES split on `?` and `#`, and
//! copying that here would be a bug. It is a SERVER ROUTE parser: its input is an
//! HTTP request target, where a query string is part of the grammar and cannot be
//! part of the filename. This function's input is a link in a local document, where
//! the opposite holds. The two look alike and solve different problems.
//!
//! ## The base URI names the FILE, and both trailing-slash mistakes are silent
//!
//! [`file_uri_for`] produces `file:///home/u/proj/alpha.md`, never the directory and
//! never a trailing slash. Under RFC 3986 a relative `beta.md` REPLACES the last
//! segment, so that base resolves to `file:///home/u/proj/beta.md`, which is the
//! answer a reader expects. Either trailing-slash mistake fails quietly rather than
//! loudly: `…/alpha.md/` resolves `beta.md` one level too deep, so core is handed a
//! path that does not exist, returns `0`, and the click appears to do nothing; and
//! `file:///home/u/proj` — the directory without a slash — resolves against its
//! PARENT. Neither produces an error anywhere.
//!
//! Percent-encoding is likewise correctness rather than tidiness: an unencoded `#`
//! in a source path truncates the base URI at a fragment, and an unencoded space
//! produces a URI WebKit will not accept at all.
//!
//! ## [`scroll_into_view_js`] encodes ONCE, and that is the difference from
//! [`crate::pane_view`]'s body swap
//!
//! `pane_view::preview_swap_payload` encodes twice because its script does
//! `JSON.parse(<literal>)` and the parse undoes the inner half. This script has no
//! parse: it embeds the literal directly as `document.getElementById(<literal>)`, so
//! a second encoding would make the id a QUOTED string — `"intro"` including the
//! quotes — and the scroll would silently target an element that does not exist.
//! The `U+2028` / `U+2029` replacements are shared, and for the reason stated there:
//! both are legal raw inside a JSON string but are line terminators to a JavaScript
//! grammar before ES2019, where they would end the literal mid-string.
//!
//! macOS's escaper additionally escapes `<` and `/`; this one deliberately does not.
//! This is JavaScript SOURCE handed to `evaluate_javascript`, never HTML, so a
//! `</script>` in a fragment has nothing to close. Carrying two escapers that differ
//! invites the belief that one of the differences is load-bearing.

use std::path::{Component, Path, PathBuf};

/// The uppercase hex alphabet [`percent_encode_path`] emits, indexed by nibble.
///
/// A table rather than `format!("{byte:02X}")` so the encoder allocates nothing per
/// byte and needs no `as` cast to get at either half.
const HEX_UPPER: [u8; 16] = *b"0123456789ABCDEF";

/// What a click on a link inside the preview means.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PreviewLink {
    /// A foreign scheme: hand the whole URI, fragment included, to the platform URI
    /// launcher — the browser, the mail client, whatever claims it.
    OpenExternally {
        /// The resolved URI verbatim.
        uri: String,
    },
    /// An anchor within the document already on screen: scroll to `fragment`
    /// through [`scroll_into_view_js`], with no reload and no RPC.
    ScrollToFragment {
        /// The percent-DECODED fragment, never empty.
        fragment: String,
    },
    /// Repoint the preview pane at `path` through core's `preview_file`, then — if
    /// `fragment` is `Some` — scroll to it once the new document has loaded.
    PreviewFile {
        /// The percent-DECODED path the link names. Absolute for every URI that
        /// carried a `file` scheme; possibly relative on the defensive row 9, which
        /// core's own absolute-path requirement then rejects.
        path: String,
        /// The percent-DECODED fragment, or `None` when the link carried none.
        fragment: Option<String>,
    },
    /// Do nothing at all; the string names why, as a mid-sentence clause (the
    /// [`crate::command_action`] / [`crate::nav_ui`] convention).
    Refuse(&'static str),
}

/// Classifies `uri` — the link target WebKit has already resolved against the
/// document's base — against `source_path`, the absolute path of the file the pane
/// is currently previewing.
///
/// The ten rows, their evaluation order, and the divergences from macOS are in the
/// module docs. An empty `source_path` (a nameless buffer) can never compare equal
/// to a link path, which is exactly why rows 6 and 7 are unreachable there.
#[must_use]
pub fn classify(uri: &str, source_path: &str) -> PreviewLink {
    let (before_fragment, raw_fragment) = split_fragment(uri);
    let fragment = raw_fragment.map(percent_decode).filter(|f| !f.is_empty());

    let Some(scheme) = scheme_of(uri) else {
        // Rows 9 and 10. No scheme at all.
        if before_fragment.is_empty() {
            return PreviewLink::Refuse(
                "the link resolved to no scheme and no path at all, so there is no file to \
                 preview and nothing an external handler could be given",
            );
        }
        return PreviewLink::PreviewFile {
            path: percent_decode(before_fragment),
            fragment,
        };
    };

    // Rows 1 and 2, and they must be tested BEFORE the foreign-scheme row: an
    // `about` URI has a scheme and it is not `file`, so row 3 would launch it.
    // `webkit_web_view_load_html` documents `about:blank` as the base a `NULL`
    // argument defaults to, so this is the shape a FRAGMENT-ONLY href is expected to
    // take in a document loaded with no base — expected from that documentation, not
    // observed here. The module docs carry what a RELATIVE href in the same document
    // was MEASURED to deliver (its raw self, which is row 9) and why neither shape is
    // reachable from the running application.
    if scheme.eq_ignore_ascii_case("about") {
        let anchor = fragment.filter(|_| before_fragment.eq_ignore_ascii_case("about:blank"));
        return match anchor {
            Some(fragment) => PreviewLink::ScrollToFragment { fragment },
            None => PreviewLink::Refuse(
                "the link resolved to an about: URI, the scheme WebKit uses for a document with \
                 no location of its own, and it names no anchor to scroll to — and an about: URI \
                 is never handed to an external handler",
            ),
        };
    }

    // Schemes are case-insensitive per RFC 3986, and a `FILE:` URI that fell through
    // to the external launcher would be exactly the row-3 hazard this gate exists to
    // keep local.
    if !scheme.eq_ignore_ascii_case("file") {
        // Row 3. The WHOLE uri, fragment included: the handler resolves it, not us.
        return PreviewLink::OpenExternally {
            uri: uri.to_string(),
        };
    }

    // Row 4.
    let Some(path) = path_from_file_uri(before_fragment) else {
        return PreviewLink::Refuse(
            "the link names a file on another host, and this front end previews only files the \
             captive Neovim can read from the local filesystem",
        );
    };
    if path.is_empty() {
        // Row 5.
        return PreviewLink::Refuse(
            "the link carries a file scheme with no path after it, so there is no file to hand \
             to the preview",
        );
    }

    // Rows 6, 7 and 8. Lexical normalization on BOTH sides, so `/a/./x.md` and
    // `/a/x.md` are the same document and a `#anchor` into it scrolls rather than
    // re-rendering the file that is already on screen.
    if normalize_path(&path) == normalize_path(source_path) {
        return match fragment {
            Some(fragment) => PreviewLink::ScrollToFragment { fragment },
            None => PreviewLink::PreviewFile {
                path,
                fragment: None,
            },
        };
    }
    PreviewLink::PreviewFile { path, fragment }
}

/// The base URI a preview document is loaded against, or `None` when there is none
/// to give.
///
/// `None` for an EMPTY source path and for a RELATIVE one, and the caller passes
/// that `None` straight through to `load_html`. Both refusals are deliberate:
///
/// - A nameless buffer's document has no location. Synthesizing one from the process
///   working directory would resolve every relative link against a directory the
///   document has no relationship to, which is worse than not resolving it — the
///   same reason [`crate::preview_ui::preview_status`] collapses an empty source
///   path to a bare `"Preview"` rather than inventing a name.
/// - Prefixing `file://` onto a relative path parses its FIRST SEGMENT as the
///   authority: `file://notes/x.md` names host `notes`, not a file. That is a total
///   silent breakage, and it is why this is a guard rather than a `debug_assert`.
///
/// The path is percent-encoded and is otherwise passed through verbatim — no
/// normalization. See the module docs for what each trailing-slash mistake costs.
///
/// A `source_path` that ALREADY ends in a slash is REFLECTED, not repaired:
/// `/a/x.md/` yields `file:///a/x.md/`. That is deliberate. `nvim_buf_get_name`
/// never produces one, so the input is already outside the contract, and silently
/// rewriting a caller's path is the worse failure of the two — it hides the
/// upstream defect and makes the base URI stop being a function of what it was
/// given.
#[must_use]
pub fn file_uri_for(source_path: &str) -> Option<String> {
    if source_path.is_empty() || !source_path.starts_with('/') {
        return None;
    }
    Some(format!("file://{}", percent_encode_path(source_path)))
}

/// The JavaScript that scrolls the element with id `fragment` into view, ready for
/// `evaluate_javascript`.
///
/// Encoded ONCE — see the module docs for why a second encoding would silently
/// target a quoted id — and `Err` only if `serde_json` cannot encode a `&str`, which
/// the caller prints and skips rather than falling back to anything.
///
/// Top-aligned (`scrollIntoView(true)`) so the preview CSS's `scroll-margin-top`
/// decides the final offset, matching the heading anchors core already emits.
///
/// # Errors
///
/// Propagates a [`serde_json::Error`] from encoding `fragment` as a JSON string.
#[must_use = "the caller must evaluate the script or print the error"]
pub fn scroll_into_view_js(fragment: &str) -> Result<String, serde_json::Error> {
    let literal = serde_json::to_string(fragment)?
        .replace('\u{2028}', "\\u2028")
        .replace('\u{2029}', "\\u2029");
    Ok(format!(
        "(function () {{ \
           var e = document.getElementById({literal}); \
           if (e) {{ e.scrollIntoView(true); }} \
         }})();"
    ))
}

/// Splits `uri` at its FIRST `#` into everything before it and the raw fragment
/// after it, collapsing an empty fragment to `None`.
///
/// The first `#`, not the last: everything after it is the fragment, `#` included,
/// so a second one belongs to the fragment's text.
fn split_fragment(uri: &str) -> (&str, Option<&str>) {
    match uri.find('#') {
        Some(hash) => {
            let fragment = &uri[hash + 1..];
            let fragment = if fragment.is_empty() {
                None
            } else {
                Some(fragment)
            };
            (&uri[..hash], fragment)
        }
        None => (uri, None),
    }
}

/// The URI's scheme, by RFC 3986's grammar
/// (`ALPHA *( ALPHA / DIGIT / "+" / "-" / "." ) ":"`), or `None` when `uri` has
/// none.
///
/// The grammar rather than "the text before the first colon", because a PATH may
/// contain a colon: `/notes/12:30 standup.md` would otherwise report a scheme of
/// `/notes/12` and be handed to an external launcher. The first character must be
/// alphabetic and every later one must be in the set, so any `/`, space or digit
/// leading character ends the scan with `None`.
fn scheme_of(uri: &str) -> Option<&str> {
    let mut end = None;
    for (index, ch) in uri.char_indices() {
        if ch == ':' {
            end = Some(index);
            break;
        }
        let legal = if index == 0 {
            ch.is_ascii_alphabetic()
        } else {
            ch.is_ascii_alphanumeric() || matches!(ch, '+' | '-' | '.')
        };
        if !legal {
            return None;
        }
    }
    let end = end?;
    if end == 0 {
        return None;
    }
    Some(&uri[..end])
}

/// The percent-DECODED path a `file:` URI names, or `None` when its authority names
/// a host this front end cannot read from.
///
/// An empty authority (`file:///a/x.md`) and `localhost` (`file://localhost/a/x.md`)
/// both mean the local filesystem and both yield `/a/x.md`. Any other authority is
/// `None`, because core's `preview_file` takes a filesystem path and would otherwise
/// be handed the remote URI's path as though it were local — reading whatever
/// happens to sit at that path on THIS machine.
///
/// `file:` with no `//` at all (`file:/a/x.md`, legal under RFC 8089) carries no
/// authority component and its whole remainder is the path.
fn path_from_file_uri(uri: &str) -> Option<String> {
    let scheme = scheme_of(uri)?;
    if !scheme.eq_ignore_ascii_case("file") {
        return None;
    }
    let rest = &uri[scheme.len() + 1..];
    let path = match rest.strip_prefix("//") {
        Some(after_slashes) => {
            let (authority, path) = match after_slashes.find('/') {
                Some(slash) => (&after_slashes[..slash], &after_slashes[slash..]),
                None => (after_slashes, ""),
            };
            if !authority.is_empty() && !authority.eq_ignore_ascii_case("localhost") {
                return None;
            }
            path
        }
        None => rest,
    };
    Some(percent_decode(path))
}

/// Decodes every `%XX` escape in `s` into the byte it names, leaving everything else
/// — a lone `%`, a `%` followed by anything that is not two hex digits — untouched.
///
/// The decoded bytes go through `String::from_utf8_lossy`, and the LOSS is
/// deliberate. A path that is not UTF-8 is one core's `preview_file(&str)` cannot be
/// handed at all, and a replacement character produces a path whose `filereadable`
/// check fails — a refusal the user sees, rather than a different file opened
/// quietly.
fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out: Vec<u8> = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        let byte = bytes[index];
        if byte == b'%' && index + 2 < bytes.len() {
            let high = bytes[index + 1];
            let low = bytes[index + 2];
            if high.is_ascii_hexdigit() && low.is_ascii_hexdigit() {
                // `char::from` rather than a cast: both bytes are ASCII hex digits,
                // so the conversion is total and needs no `clippy::cast_*` waiver.
                let mut hex = String::with_capacity(2);
                hex.push(char::from(high));
                hex.push(char::from(low));
                if let Ok(decoded) = u8::from_str_radix(&hex, 16) {
                    out.push(decoded);
                    index += 3;
                    continue;
                }
            }
        }
        out.push(byte);
        index += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Percent-encodes `path` for the path component of a `file:` URI: `A-Za-z0-9`,
/// `-`, `.`, `_`, `~` and `/` pass through, every other BYTE becomes an uppercase
/// `%XX`.
///
/// Bytes rather than characters, so a multi-byte character encodes to one `%XX` per
/// UTF-8 byte, which is what a URI's path component is defined over. `/` is kept
/// precisely because it is the separator the resolution in the module docs depends
/// on; encoding it would collapse the whole path into one segment.
fn percent_encode_path(path: &str) -> String {
    let mut out = String::with_capacity(path.len());
    for byte in path.bytes() {
        let ch = char::from(byte);
        if ch.is_ascii_alphanumeric() || matches!(ch, '-' | '.' | '_' | '~' | '/') {
            out.push(ch);
        } else {
            out.push('%');
            out.push(char::from(HEX_UPPER[usize::from(byte >> 4)]));
            out.push(char::from(HEX_UPPER[usize::from(byte & 0x0f)]));
        }
    }
    out
}

/// Normalizes `path` LEXICALLY: `.` components are dropped, a `..` pops the previous
/// named component, a `..` directly under the root is dropped, and a trailing
/// separator is not kept.
///
/// Lexical ONLY — no symlink resolution, no `canonicalize`, no filesystem access of
/// any kind. This module is pure, and macOS's `standardizedFileURL` is lexical too,
/// so resolving symlinks here would make the two front-ends disagree about whether
/// a link points at the file already on screen.
fn normalize_path(path: &str) -> String {
    let mut kept: Vec<Component<'_>> = Vec::new();
    for component in Path::new(path).components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => match kept.last() {
                Some(Component::Normal(_)) => {
                    kept.pop();
                }
                // `/..` is `/`: the root has no parent, so the component is spent
                // rather than kept, which is what `standardizedFileURL` answers too.
                Some(Component::RootDir) => {}
                _ => kept.push(component),
            },
            other => kept.push(other),
        }
    }

    let mut out = PathBuf::new();
    for component in kept {
        out.push(component.as_os_str());
    }
    out.to_string_lossy().into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::collections::HashSet;

    /// The source path most rows compare against: a saved file in a project
    /// directory, the ordinary case.
    const SOURCE: &str = "/a/x.md";

    /// The source path of a never-saved Ctrl+Shift+N buffer, MEASURED under `cage` on
    /// 2026-08-06 (M2 leg (a)): `nvim_buf_get_name` answers an ABSOLUTE path for one,
    /// with a space in the basename.
    ///
    /// It is the only source path in these tables that a user really gets without
    /// having chosen it, which is why [`file_uri_for`]'s row for it and [`classify`]'s
    /// carry messages of their own.
    const UNSAVED_SOURCE: &str = "/tmp/p/Untitled 2.md";

    /// Every [`PreviewLink`] variant, as the name a failure quotes.
    ///
    /// The `match` is deliberately `_`-FREE: appending a variant to [`PreviewLink`] is
    /// an `E0004` here rather than a class that silently stops being swept by
    /// [`classify_table`]'s range assertion.
    fn variant_name(link: &PreviewLink) -> &'static str {
        match link {
            PreviewLink::OpenExternally { .. } => "OpenExternally",
            PreviewLink::ScrollToFragment { .. } => "ScrollToFragment",
            PreviewLink::PreviewFile { .. } => "PreviewFile",
            PreviewLink::Refuse(_) => "Refuse",
        }
    }

    /// Every variant name [`classify`] must be able to answer with, written out as
    /// LITERALS rather than derived from a sweep of the enum.
    ///
    /// Comparing the observed range against a list computed from the same function
    /// would be a tautology; comparing it against four strings a human typed is what
    /// makes "a constant classifier fails this" true.
    const ALL_VARIANTS: [&str; 4] = [
        "OpenExternally",
        "ScrollToFragment",
        "PreviewFile",
        "Refuse",
    ];

    /// Whether `reason` opens like a SENTENCE (`Xy…`) rather than as a mid-sentence
    /// clause. Copied from [`crate::preview_ui`]'s test helper of the same name,
    /// including its deliberate allowance for an ALL-CAPS opener such as `GNOME`.
    fn is_sentence_cased(reason: &str) -> bool {
        let mut chars = reason.chars();
        let (Some(first), Some(second)) = (chars.next(), chars.next()) else {
            return false;
        };
        first.is_uppercase() && second.is_lowercase()
    }

    /// An exact [`PreviewLink::PreviewFile`] expectation, so the table below reads as
    /// data.
    fn preview(path: &str, fragment: Option<&str>) -> WantLink {
        WantLink::Exact(PreviewLink::PreviewFile {
            path: path.to_string(),
            fragment: fragment.map(str::to_string),
        })
    }

    /// An exact [`PreviewLink::ScrollToFragment`] expectation.
    fn scroll(fragment: &str) -> WantLink {
        WantLink::Exact(PreviewLink::ScrollToFragment {
            fragment: fragment.to_string(),
        })
    }

    /// An exact [`PreviewLink::OpenExternally`] expectation.
    fn external(uri: &str) -> WantLink {
        WantLink::Exact(PreviewLink::OpenExternally {
            uri: uri.to_string(),
        })
    }

    // =========================================================================
    // classify
    // =========================================================================

    /// One [`classify`] table row.
    struct ClassifyCase {
        /// What a failure should say this row was measuring.
        desc: &'static str,
        /// The resolved link target WebKit hands the handler.
        uri: &'static str,
        /// The path the pane is previewing at that moment.
        source: &'static str,
        /// The answer.
        want: WantLink,
    }

    /// What a [`ClassifyCase`] row expects.
    ///
    /// The three ACTING variants are matched on the WHOLE value, payload included:
    /// the path and the fragment ARE the decision here, and a right variant carrying
    /// a wrong path opens a different file. A refusal is matched on its CLASS only —
    /// the `preview_ui::tests::WantPress` precedent — because the reasons are
    /// documentation, pinning four sentences here would make a wording tweak look like
    /// a behaviour regression, and their real contract (non-empty, clause-shaped,
    /// distinct) is asserted by
    /// [`classify_refusal_reasons_are_distinct_and_mid_sentence`].
    enum WantLink {
        /// An exact [`PreviewLink`].
        Exact(PreviewLink),
        /// `PreviewLink::Refuse(_)`, whatever the reason says.
        Refuse,
    }

    /// Whether `got` satisfies `want`.
    fn satisfies(want: &WantLink, got: &PreviewLink) -> bool {
        match want {
            WantLink::Exact(expected) => got == expected,
            WantLink::Refuse => matches!(got, PreviewLink::Refuse(_)),
        }
    }

    /// How a [`WantLink`] reads in a failure message.
    fn want_name(want: &WantLink) -> String {
        match want {
            WantLink::Exact(expected) => format!("{expected:?}"),
            WantLink::Refuse => "PreviewLink::Refuse(_)".to_string(),
        }
    }

    /// How many named rows the classification table carries, as a LITERAL.
    ///
    /// Never `cases.len()`: a comparison between a table's length and the length of
    /// the same table is a tautology that shrinks with it, which is the
    /// `preview_ui::tests::NAMED_LIVENESS_INPUTS` precedent.
    const NAMED_CLASSIFY_ROWS: usize = 26;

    /// Pins what a click on a link inside the preview means, one row per named shape.
    ///
    /// Property 1: every row answers what the module's ten-row table says it does —
    /// PAYLOAD INCLUDED for the three acting variants, since the path and the fragment
    /// ARE the decision, and by CLASS for a refusal (see [`WantLink`]). Property 2
    /// (non-vacuity): the RANGE over the whole sweep is exactly the four variants. It
    /// is BELT-AND-BRACES against `classify` and a live guard on this TABLE — see the
    /// falsifier list below, which says which edits reach it and which do not.
    ///
    /// ## The rows that are DEFENSIVE, and what actually makes them so
    ///
    /// The three `about:` rows and the two schemeless ones all describe a preview
    /// document loaded with NO base URI, and the reason none of them is reachable is
    /// [`file_uri_for`]'s DOMAIN: it answers `Some` for every source path the running
    /// application can produce, and exactly two inputs make it answer `None`.
    ///
    /// * An EMPTY source path. `runtime/nvim/init.lua:2229` filters an empty
    ///   `b:wolf_preview_source` out of the `wolf_preview` notification, so a preview
    ///   of a nameless buffer never reaches `Snapshot::previews`, never becomes a pane
    ///   and never gets a web view at all. MEASURED under `cage` on 2026-08-06:
    ///   `:enew` + `:setlocal filetype=markdown` + `:WolfPreview` emitted ZERO
    ///   `decide-policy` lines and drew two ordinary editor panes.
    /// * A RELATIVE source path. `PreviewInfo.source_path` comes from
    ///   `nvim_buf_get_name`, which answers an absolute path or an empty one and never
    ///   a relative one — including for a never-saved `Untitled N.md`, which is what
    ///   the [`UNSAVED_SOURCE`] rows are here to show.
    ///
    /// Those two inputs are [`file_uri_for_table`]'s two `None` rows, so that table and
    /// this section state one fact from two sides.
    ///
    /// **Do not write down that "nothing produces a schemeless target".** It was
    /// measured FALSE: with `load_html(html, None)` a relative href arrives at the
    /// handler RAW — `uri="lnkinto.md"` — rather than resolved against `about:blank`,
    /// which is row 9 exactly. Row 9 is one `load_html` argument away from being live,
    /// and it is what the second falsifier on
    /// `smoke_headless::preview_link_click_repoints_the_pane_under_cage` exercises. The
    /// five rows are kept because a guard whose reachability rests on one Lua `if` and
    /// one function argument is worth keeping — not because nothing could ever reach
    /// them, and not because they are "the nameless-buffer path", which they are not.
    ///
    /// The row that carries real MEASURED behaviour is the link resolved against
    /// [`UNSAVED_SOURCE`], and its `desc` says so: a never-saved Ctrl+Shift+N buffer
    /// has an ABSOLUTE `nvim_buf_get_name`, so a relative link in its preview goes
    /// through row 8 exactly as it does for a saved file. That is the path a user
    /// really takes, and it is why [`percent_encode_path`] is load-bearing rather than
    /// a defensive nicety — see [`file_uri_for_table`]'s row for the same source.
    ///
    /// The O12 row (`/notes/12:30 standup.md`) is defensive AT THIS LEVEL for the same
    /// reason the other schemeless rows are, but the grammar it exercises is not:
    /// [`scheme_of_follows_the_rfc_grammar`] pins it on inputs every real click
    /// carries.
    ///
    /// Falsifiers, one clause each:
    /// * Delete the `!scheme.eq_ignore_ascii_case("file")` arm — the `https` and
    ///   `mailto` rows redden.
    /// * Compare the two paths raw instead of through [`normalize_path`] — the
    ///   `/a/./x.md` rows redden from both sides.
    /// * Answer [`PreviewLink::PreviewFile`] where a refusal is due — the five
    ///   refusing rows redden.
    ///
    /// ## The bare-`#` row has NO individual falsifier, and that is a property of the
    /// code rather than of the row
    ///
    /// The empty fragment is guarded twice, on either side of [`percent_decode`]:
    /// [`split_fragment`] collapses an empty raw fragment to `None`, and `classify`'s
    /// `.filter(|f| !f.is_empty())` collapses an empty DECODED one. Either guard alone
    /// satisfies the row, so **neither is individually falsifiable** — deleting the
    /// filter leaves the row green because `split_fragment` already answered `None`,
    /// and making `split_fragment` return `Some("")` leaves it green because the filter
    /// still collapses it. Both were run; both stayed green. The row is attached to the
    /// CONJUNCTION, and deleting both together reddens it.
    ///
    /// No row can earn the filter an individual falsifier, because [`percent_decode`]
    /// cannot answer `""` for a non-empty input: every loop iteration pushes at least
    /// one byte (a `%XX` escape consumes three and pushes one, including for `%00`,
    /// which yields a one-byte NUL string), and `String::from_utf8_lossy` REPLACES an
    /// invalid byte with U+FFFD rather than dropping it. So the filter is unreachable
    /// today and is kept anyway: it guards the decoder's output, which is a different
    /// step from the splitter's input, and removing a guard to win a falsifier is the
    /// wrong trade. This paragraph exists so a green run under either single edit is
    /// read as the redundancy it is, not as the row being vacuous.
    ///
    /// ## Property 2 is BELT-AND-BRACES against `classify`, and a guard on this table
    ///
    /// Making `classify` a constant does NOT reach it: the range is accumulated inside
    /// the loop, so property 1's first row (`about:blank#intro`, which a constant
    /// `Refuse` answers wrong) reddens first and the sweep never finishes. Measured.
    /// Over THIS row set property 2 is implied by property 1 — every row pins a variant
    /// exactly or by class, and all four classes appear — so no edit to `classify`
    /// alone can reach it.
    ///
    /// The edit it does catch, and catches alone, is a trimmed TABLE: delete the three
    /// [`PreviewLink::OpenExternally`] rows and lower [`NAMED_CLASSIFY_ROWS`] to match,
    /// and the length assertion and property 1 both stay green while a whole class of
    /// `classify`'s behaviour silently stops being swept. Property 2 reddens naming the
    /// missing variant. That is what it is for.
    #[test]
    fn classify_table() {
        let cases = [
            ClassifyCase {
                desc: "DEFENSIVE (module row 1): an anchor in a document with no location of \
                       its own still scrolls",
                uri: "about:blank#intro",
                source: "",
                want: scroll("intro"),
            },
            ClassifyCase {
                desc: "DEFENSIVE (module row 2): a bare about:blank names no anchor and no file",
                uri: "about:blank",
                source: "",
                want: WantLink::Refuse,
            },
            ClassifyCase {
                desc: "DEFENSIVE (module row 2): about:srcdoc carries a fragment but is not \
                       about:blank, so it is refused rather than scrolled",
                uri: "about:srcdoc#intro",
                source: "",
                want: WantLink::Refuse,
            },
            ClassifyCase {
                desc: "row 3: https goes to the platform launcher, fragment and all",
                uri: "https://example.com/docs#install",
                source: SOURCE,
                want: external("https://example.com/docs#install"),
            },
            ClassifyCase {
                desc: "row 3: a non-http foreign scheme is the launcher's business too",
                uri: "mailto:team@example.com",
                source: SOURCE,
                want: external("mailto:team@example.com"),
            },
            ClassifyCase {
                desc: "row 3: schemes are case-insensitive per RFC 3986, so an uppercase one \
                       still launches",
                uri: "HTTPS://example.com/docs",
                source: SOURCE,
                want: external("HTTPS://example.com/docs"),
            },
            ClassifyCase {
                desc: "row 8 through an UPPERCASE file scheme: it must stay LOCAL rather than \
                       fall through to the launcher",
                uri: "FILE:///a/beta.md",
                source: SOURCE,
                want: preview("/a/beta.md", None),
            },
            ClassifyCase {
                desc: "row 4: a file: URI naming another HOST is refused, never read off this \
                       machine's filesystem at the same path",
                uri: "file://evil.example/etc/passwd",
                source: SOURCE,
                want: WantLink::Refuse,
            },
            ClassifyCase {
                desc: "row 6 through a localhost AUTHORITY: it means the local filesystem, so \
                       the path compares equal to the source and the anchor scrolls",
                uri: "file://localhost/a/x.md#sec",
                source: SOURCE,
                want: scroll("sec"),
            },
            ClassifyCase {
                desc: "row 5: a file scheme with nothing after it names no file",
                uri: "file://",
                source: SOURCE,
                want: WantLink::Refuse,
            },
            ClassifyCase {
                desc: "row 6: an anchor into the document already on screen scrolls, with no \
                       reload and no RPC",
                uri: "file:///a/x.md#sec",
                source: SOURCE,
                want: scroll("sec"),
            },
            ClassifyCase {
                desc: "row 6: a percent-encoded fragment is DECODED before it becomes an \
                       element id",
                uri: "file:///a/x.md#a%20b",
                source: SOURCE,
                want: scroll("a b"),
            },
            ClassifyCase {
                desc: "row 7: the same file with NO fragment re-previews it rather than \
                       scrolling to nothing",
                uri: "file:///a/x.md",
                source: SOURCE,
                want: preview("/a/x.md", None),
            },
            ClassifyCase {
                desc: "row 7: a BARE `#` is an empty fragment, which collapses to None — \
                       guarded TWICE, so this row can only be reddened by removing both \
                       guards (see this table's doc comment)",
                uri: "file:///a/x.md#",
                source: SOURCE,
                want: preview("/a/x.md", None),
            },
            ClassifyCase {
                desc: "row 8: a different file with a fragment carries BOTH through, so the \
                       scroll can be applied once the new document loads",
                uri: "file:///a/beta.md#intro",
                source: SOURCE,
                want: preview("/a/beta.md", Some("intro")),
            },
            ClassifyCase {
                desc: "row 8, MEASURED under cage on 2026-08-06: a never-saved Ctrl+Shift+N \
                       buffer has an ABSOLUTE name, so a relative link in its preview resolves \
                       and follows exactly as it does for a saved file",
                uri: "file:///tmp/p/beta.md",
                source: UNSAVED_SOURCE,
                want: preview("/tmp/p/beta.md", None),
            },
            ClassifyCase {
                desc: "row 8 through RFC 8089's no-authority form (`file:/path`): the whole \
                       remainder is the path",
                uri: "file:/a/beta.md",
                source: SOURCE,
                want: preview("/a/beta.md", None),
            },
            ClassifyCase {
                desc: "row 6 with `.` in the LINK: lexical normalization makes it the same \
                       document",
                uri: "file:///a/./x.md#sec",
                source: SOURCE,
                want: scroll("sec"),
            },
            ClassifyCase {
                desc: "row 6 with `.` in the SOURCE: normalization is applied to BOTH sides",
                uri: "file:///a/x.md#sec",
                source: "/a/./x.md",
                want: scroll("sec"),
            },
            ClassifyCase {
                desc: "row 6 with a `..` that pops PAST the root: the root has no parent, so \
                       `/../x.md` is `/x.md`",
                uri: "file:///../x.md#top",
                source: "/x.md",
                want: scroll("top"),
            },
            ClassifyCase {
                desc: "row 8, a DELIBERATE choice: a query string is NOT stripped, because `?` \
                       is a legal byte in a Unix filename",
                uri: "file:///a/x.md?v=1",
                source: SOURCE,
                want: preview("/a/x.md?v=1", None),
            },
            ClassifyCase {
                desc: "row 8: a percent-encoded SPACE in the link path decodes back to a space",
                uri: "file:///a/my%20notes.md",
                source: SOURCE,
                want: preview("/a/my notes.md", None),
            },
            ClassifyCase {
                desc: "row 7: a `#` inside a FILENAME arrives as %23, so it is part of the path \
                       and never a fragment",
                uri: "file:///a/note%23x.md",
                source: "/a/note#x.md",
                want: preview("/a/note#x.md", None),
            },
            ClassifyCase {
                desc: "row 7: a multi-byte character decodes one %XX per UTF-8 BYTE",
                uri: "file:///tmp/%C3%BCn%C3%AFcode.md",
                source: "/tmp/ünïcode.md",
                want: preview("/tmp/ünïcode.md", None),
            },
            ClassifyCase {
                desc: "module row 9 (defensive only because `file_uri_for` answers Some for \
                       every real source path — a schemeless target IS produced when the base \
                       is None), O12: a colon inside a PATH SEGMENT is not a scheme, so the \
                       file is previewed rather than handed to an external launcher — the \
                       grammar itself is pinned on reachable inputs by \
                       scheme_of_follows_the_rfc_grammar",
                uri: "/notes/12:30 standup.md",
                source: SOURCE,
                want: preview("/notes/12:30 standup.md", None),
            },
            ClassifyCase {
                desc: "module row 10: an empty target names nothing at all — defensive for the \
                       same reason row 9 is, `file_uri_for`'s domain rather than anything \
                       about what WebKit delivers",
                uri: "",
                source: SOURCE,
                want: WantLink::Refuse,
            },
        ];

        assert_eq!(
            cases.len(),
            NAMED_CLASSIFY_ROWS,
            "the classification table must carry its {NAMED_CLASSIFY_ROWS} named rows, not {}. \
             They are not a sample: each names one clause of the module's ten-row table or one \
             edge the URI grammar has, and the five REFUSING rows — which carry four DISTINCT \
             reasons between them — are what stop `Refuse` from being unreachable in the range \
             assertion below.",
            cases.len()
        );

        let mut seen: HashSet<&'static str> = HashSet::new();
        for case in &cases {
            let got = classify(case.uri, case.source);
            assert!(
                satisfies(&case.want, &got),
                "classify({:?}, source = {:?}) answered {got:?}, not {} — {}. The four classes \
                 are not near misses of each other: an OpenExternally where a PreviewFile was \
                 meant hands a LOCAL path to the desktop's default handler, which on GNOME can \
                 start an arbitrary process from a string in a document the user merely \
                 previewed; a PreviewFile where a ScrollToFragment was meant spends a blocking \
                 RPC and re-renders the document the reader is already looking at, losing their \
                 scroll position; a PreviewFile carrying the WRONG PATH opens a different file \
                 with no error anywhere; and a Refuse where any of the three was meant makes the \
                 click do nothing at all.",
                case.uri,
                case.source,
                want_name(&case.want),
                case.desc
            );
            seen.insert(variant_name(&got));
        }

        // --- Property 2: the RANGE, which no individual row can state -------
        //
        // Reached only when every row above passed, so it is a statement about this
        // TABLE's coverage rather than about `classify` — see the doc comment.
        let want: HashSet<&'static str> = ALL_VARIANTS.into_iter().collect();
        assert_eq!(
            seen, want,
            "classify's range over all {NAMED_CLASSIFY_ROWS} rows must be exactly the four \
             PreviewLink variants. A MISSING variant means no row left in the table exercises \
             that class at all: every surviving row pins its class and they all passed, so the \
             only way to get here is by DELETING the rows that carried it and lowering \
             NAMED_CLASSIFY_ROWS to match. Restore them rather than shrinking this set — a class \
             nothing sweeps is a class `classify` can stop answering with no test noticing, and \
             for OpenExternally that means every external link in every preview silently going \
             dead. An EXTRA name is impossible while `variant_name` has no `_` arm, so if one \
             appears here a catch-all was added to it."
        );
    }

    /// The four refusal reasons [`classify`] can answer with are non-empty, read as a
    /// mid-sentence clause, and are pairwise DISTINCT.
    ///
    /// The reason is the whole of what a reader sees when a click appears to do
    /// nothing — [`crate::pane_view`] prints it — and the four name four different
    /// situations with four different responses: an `about:` URI is a document with no
    /// location, a foreign authority is a security refusal, an empty `file:` path is a
    /// malformed link, and no scheme with no path is an empty `href`. Two sharing a
    /// string collapse two of those into a message that cannot be acted on.
    ///
    /// Falsifier: give the `about:` arm and the empty-target arm the same string.
    #[test]
    fn classify_refusal_reasons_are_distinct_and_mid_sentence() {
        /// How many refusing arms [`classify`] has, as a literal.
        const NAMED_REFUSALS: usize = 4;

        let probes = [
            ("an about: URI naming no anchor", "about:blank", ""),
            (
                "a file: URI naming another host",
                "file://evil.example/a/x.md",
                SOURCE,
            ),
            ("a file: URI with an empty path", "file://", SOURCE),
            ("a target with neither scheme nor path", "", SOURCE),
        ];
        assert_eq!(
            probes.len(),
            NAMED_REFUSALS,
            "the probe list must select all {NAMED_REFUSALS} refusing arms, not {}. A missing \
             probe leaves one reason unchecked, and the distinctness below then holds over a \
             smaller set than it claims.",
            probes.len()
        );

        let mut seen: HashSet<&'static str> = HashSet::new();
        for (what, uri, source) in probes {
            let got = classify(uri, source);
            let PreviewLink::Refuse(reason) = got else {
                panic!(
                    "classify({uri:?}, source = {source:?}) — {what} — answered {got:?} rather \
                     than a refusal, so this test cannot check the reason it exists to check. \
                     Either that arm now acts (in which case `classify_table` is also red, and \
                     that is the finding) or the probe no longer selects the arm it names."
                )
            };
            assert!(
                !reason.trim().is_empty(),
                "the refusal reason for {what} is empty. `pane_view` prints it as \
                 \"wolf-gnome: preview link not followed: {{reason}}\", so an empty one makes a \
                 deliberate refusal indistinguishable from a click that was never delivered."
            );
            assert!(
                !is_sentence_cased(reason),
                "the refusal reason for {what} is sentence-cased ({reason:?}). It is \
                 interpolated MID-SENTENCE into the printed diagnostic, exactly as \
                 `command_action.rs`'s, `nav_ui.rs`'s and `preview_ui.rs`'s reasons are, so it \
                 must start lowercase (an ALL-CAPS proper noun such as \"GNOME\" is fine)."
            );
            assert!(
                seen.insert(reason),
                "the refusal reason for {what} ({reason:?}) duplicates another arm's. All \
                 {NAMED_REFUSALS} must be DISTINCT: a document with no location, a link to \
                 another host, a malformed file: URI and an empty href are four different \
                 situations, and only the printed string tells the user which one they hit."
            );
        }

        assert_eq!(
            seen.len(),
            NAMED_REFUSALS,
            "the {NAMED_REFUSALS} probed refusals produced {} distinct reason(s); see the \
             per-probe message above for which pair collapsed.",
            seen.len()
        );
    }

    // =========================================================================
    // file_uri_for
    // =========================================================================

    /// One [`file_uri_for`] table row.
    struct BaseUriCase {
        /// What a failure should say this row was measuring.
        desc: &'static str,
        /// The pane's `PreviewInfo.source_path`.
        source: &'static str,
        /// The base URI handed to `load_html`.
        want: Option<&'static str>,
    }

    /// How many named rows the base-URI table carries, as a LITERAL. See
    /// [`NAMED_CLASSIFY_ROWS`] for why it is not `cases.len()`.
    const NAMED_BASE_URI_ROWS: usize = 11;

    /// Pins the base URI a preview document is loaded against.
    ///
    /// Property 1: each named source path yields exactly the base URI stated.
    /// Property 2: the range over the sweep carries BOTH a `Some` and a `None`. As in
    /// [`classify_table`], property 2 is BELT-AND-BRACES against `file_uri_for` — a
    /// constant function fails property 1's first row long before the range is compared
    /// — and a live guard on this TABLE, catching a trim that deletes the last `Some`
    /// or the last `None` row and lowers [`NAMED_BASE_URI_ROWS`] to match.
    ///
    /// The two `None` rows name the only two INPUT SHAPES this function answers `None`
    /// for — an empty path and a relative one — and that is what makes [`classify`]'s
    /// `about:` and schemeless rows defensive rather than dead. [`classify_table`]'s
    /// doc comment states the same fact from the other side. What a `None` base really costs was MEASURED: a relative link arrives at
    /// the handler as its RAW href (`uri="lnkinto.md"`), not resolved against
    /// `about:blank`, so core is handed a relative path and answers 0 for every link
    /// that is not sitting in the captive Neovim's working directory.
    ///
    /// Falsifiers, one clause each:
    /// * Append a trailing `/` to the produced URI — the `"/a/x.md"` row reddens, and
    ///   its message carries the resolution consequence.
    /// * Return `Some("")` for an empty path — the empty row reddens.
    /// * Return `Some(...)` for a relative path — that row reddens, and its message
    ///   names the authority-parsing trap.
    /// * Delete the percent-encoder — the space, `#`, `?` and multi-byte rows redden.
    /// * Encode per CHARACTER rather than per BYTE — the multi-byte row alone reddens,
    ///   which is why it is here.
    #[test]
    fn file_uri_for_table() {
        let cases = [
            BaseUriCase {
                desc: "the ordinary case: the FILE, never its directory and never a trailing \
                       slash",
                source: "/a/x.md",
                want: Some("file:///a/x.md"),
            },
            BaseUriCase {
                desc: "DEFENSIVE, and one of the only two None inputs: an EMPTY source path has \
                       no location to synthesize one from (init.lua:2229 keeps such a preview \
                       from ever becoming a pane)",
                source: "",
                want: None,
            },
            BaseUriCase {
                desc: "DEFENSIVE, and the other None input: a RELATIVE path would parse its \
                       first segment as the AUTHORITY (nvim_buf_get_name never yields one)",
                source: "notes/x.md",
                want: None,
            },
            BaseUriCase {
                desc: "a SPACE must be encoded or WebKit rejects the URI outright",
                source: "/a/my notes.md",
                want: Some("file:///a/my%20notes.md"),
            },
            BaseUriCase {
                desc: "a `#` must be encoded or the base URI truncates at a fragment",
                source: "/a/note#x.md",
                want: Some("file:///a/note%23x.md"),
            },
            BaseUriCase {
                desc: "MEASURED: the never-saved Ctrl+Shift+N buffer, whose space really does \
                       come out as %20",
                source: UNSAVED_SOURCE,
                want: Some("file:///tmp/p/Untitled%202.md"),
            },
            BaseUriCase {
                desc: "a multi-byte character is encoded per UTF-8 BYTE, with UPPERCASE hex",
                source: "/tmp/ünïcode.md",
                want: Some("file:///tmp/%C3%BCn%C3%AFcode.md"),
            },
            BaseUriCase {
                desc: "an ASCII byte outside the unreserved set also takes UPPERCASE hex",
                source: "/a/what?.md",
                want: Some("file:///a/what%3F.md"),
            },
            BaseUriCase {
                desc: "O13: a trailing slash on the INPUT is REFLECTED, never repaired",
                source: "/a/x.md/",
                want: Some("file:///a/x.md/"),
            },
            BaseUriCase {
                desc: "a DIRECTORY with no trailing slash: reflected too, and it resolves \
                       against the PARENT",
                source: "/proj",
                want: Some("file:///proj"),
            },
            BaseUriCase {
                desc: "the whole unreserved set passes through UNENCODED: A-Za-z0-9 and -._~/",
                source: "/a/x-y_z.~q9.md",
                want: Some("file:///a/x-y_z.~q9.md"),
            },
        ];

        assert_eq!(
            cases.len(),
            NAMED_BASE_URI_ROWS,
            "the base-URI table must carry its {NAMED_BASE_URI_ROWS} named rows, not {}. The \
             last row is the one most likely to look redundant and is not: without it, an \
             encoder that percent-encoded EVERY byte would satisfy all ten others while \
             producing a base URI whose path has no separators left in it at all.",
            cases.len()
        );

        let mut answers: HashSet<bool> = HashSet::new();
        for case in &cases {
            let got = file_uri_for(case.source);
            assert_eq!(
                got.as_deref(),
                case.want,
                "file_uri_for({:?}) answered {got:?}, not {:?} — {}. Every failure on this seam \
                 is SILENT at runtime: a base URI one level too deep hands core a path that \
                 does not exist, so `preview_file` answers 0 and the click appears to do \
                 nothing; a base URI naming the parent directory opens a DIFFERENT file that \
                 happens to exist; an unencoded `#` truncates the URI at a fragment; and a \
                 `Some` built from a relative path names a HOST rather than a file. None of \
                 them produces an error anywhere.",
                case.source,
                case.want,
                case.desc
            );
            answers.insert(got.is_some());
        }

        // --- Property 2: the range ------------------------------------------
        let both: HashSet<bool> = [true, false].into_iter().collect();
        assert_eq!(
            answers, both,
            "file_uri_for's range over all {NAMED_BASE_URI_ROWS} rows must contain BOTH a Some \
             and a None. Every row above pins one or the other and they all passed, so reaching \
             here means the rows carrying one answer were DELETED and NAMED_BASE_URI_ROWS lowered \
             to match — restore them. With no `Some` row, nothing measures that a preview \
             document gets a base URI at all, and MEASURED the cost of not having one is that a \
             relative link reaches the handler as its RAW href, so core is handed a relative path \
             and answers 0 for every file outside the captive Neovim's working directory. With no \
             `None` row, nothing measures the two guards this function exists for — the empty \
             path and the relative path — and a relative source path would produce a URI naming a \
             HOST."
        );
    }

    // =========================================================================
    // scroll_into_view_js
    // =========================================================================

    /// The identifier the emitted script must call, as a literal.
    ///
    /// `getElementById` and not `querySelector`: a fragment is an element ID, and
    /// `querySelector` would read `#`, `.` and `[` in one as CSS syntax — so an id
    /// containing any of them would select the wrong element or throw.
    const LOOKUP_CALL: &str = "document.getElementById(";

    /// The scroll call the emitted script must make, top-aligned so the preview CSS's
    /// `scroll-margin-top` decides the final offset.
    const SCROLL_CALL: &str = "scrollIntoView(true)";

    /// The emitted script embeds the fragment as a JSON string literal, encoded ONCE,
    /// and pairs it with the two calls that make it do anything.
    ///
    /// Property 1: for every fragment the script contains the ENCODED literal, the
    /// `getElementById` call and the `scrollIntoView(true)` call — the pairing matters,
    /// because a script carrying the literal and no call is a no-op that looks correct
    /// in a diff.
    ///
    /// Property 2: the encoding is applied ONCE. `pane_view::preview_swap_payload`
    /// encodes twice because its script does `JSON.parse(<literal>)`; this one embeds
    /// the literal directly, so a second encoding would make the id the QUOTED string
    /// `"intro"` — quotes included — and the scroll would silently target an element
    /// that does not exist. The script must therefore contain neither a doubly-encoded
    /// literal nor `JSON.parse` at all.
    ///
    /// Falsifiers, one clause each:
    /// * Interpolate the fragment raw — the row whose fragment carries a `"` reddens.
    /// * Drop either `U+2028` / `U+2029` replacement — that row reddens.
    /// * Emit `querySelector` instead of `getElementById` — the pairing clause reddens.
    /// * Encode twice — property 2 reddens.
    #[test]
    fn scroll_into_view_js_encodes_once_and_pairs_with_the_call() {
        let fragments = [
            ("a plain heading slug", "intro"),
            (
                "a slug carrying a DOUBLE QUOTE, which would close the literal",
                "he\"llo",
            ),
            ("a slug carrying a BACKSLASH", "back\\slash"),
            ("a slug carrying U+2028 LINE SEPARATOR", "a\u{2028}b"),
            ("a slug carrying U+2029 PARAGRAPH SEPARATOR", "a\u{2029}b"),
            (
                "a slug carrying a `<` and a `/`, which are DELIBERATELY not escaped",
                "a</b",
            ),
        ];

        for (desc, fragment) in fragments {
            let script = scroll_into_view_js(fragment).unwrap_or_else(|e| {
                panic!(
                    "scroll_into_view_js({fragment:?}) — {desc} — answered Err({e}). \
                     `serde_json::to_string` on a `&str` cannot fail, so an Err here means the \
                     function stopped encoding a plain string and the call site (which prints \
                     and skips) would silently never scroll."
                )
            });

            assert!(
                script.contains(LOOKUP_CALL),
                "the script for {fragment:?} — {desc} — does not call {LOOKUP_CALL:?}. Script \
                 was:\n{script}\nA fragment is an element ID: `querySelector` would read a `#`, \
                 a `.` or a `[` in one as CSS syntax and select the wrong element or throw, and \
                 a script with no lookup at all scrolls nowhere while still evaluating cleanly."
            );
            assert!(
                script.contains(SCROLL_CALL),
                "the script for {fragment:?} — {desc} — does not call {SCROLL_CALL:?}. Script \
                 was:\n{script}\nWithout it the script finds the element and does nothing, \
                 which is indistinguishable from a fragment that did not exist — and \
                 `evaluate_javascript` reports success either way, so nothing anywhere would \
                 notice. The `true` is load-bearing too: top alignment is what lets the preview \
                 CSS's `scroll-margin-top` decide the final offset."
            );

            let literal = serde_json::to_string(fragment)
                .expect("serde_json can encode a &str")
                .replace('\u{2028}', "\\u2028")
                .replace('\u{2029}', "\\u2029");
            assert!(
                script.contains(&literal),
                "the script for {fragment:?} — {desc} — does not carry the encoded literal \
                 {literal:?}. Script was:\n{script}\nThe literal is JavaScript SOURCE handed to \
                 `evaluate_javascript`: an unescaped `\"` closes the string and the rest of the \
                 fragment becomes syntax, and a raw U+2028 or U+2029 is a LINE TERMINATOR to a \
                 pre-ES2019 grammar and ends the literal mid-string. Either way the whole \
                 script throws and the scroll never happens."
            );
        }

        // --- Property 2: encoded ONCE ---------------------------------------
        //
        // Stated on its own rather than as a row, because it is a claim about what the
        // script must NOT contain and every row above is satisfied by a double encode.
        let script = scroll_into_view_js("intro").expect("serde_json can encode a &str");
        assert!(
            script.contains("(\"intro\")"),
            "the script must pass the fragment as the bare JSON string \"intro\". It was:\n\
             {script}\nA SECOND encoding makes the argument the six-character string \
             `\"intro\"` — quotes included — so `getElementById` looks for an element whose id \
             carries literal quote marks, finds nothing, and the scroll silently does nothing. \
             This is the difference from `pane_view::preview_swap_payload`, which encodes twice \
             BECAUSE its script does JSON.parse; copying that here is the likely edit."
        );
        assert!(
            !script.contains("JSON.parse"),
            "the script calls JSON.parse. It was:\n{script}\nThis script embeds its literal \
             directly, so a parse means the literal was double-encoded to match — the \
             `preview_swap_payload` shape copied across. Both halves have to change together \
             or the id gains quote marks."
        );

        // The two DELIBERATE non-escapes, so nobody "completes" macOS's escaper here.
        let raw_markup = scroll_into_view_js("a</b").expect("serde_json can encode a &str");
        assert!(
            raw_markup.contains("a</b"),
            "the script for a fragment containing `</` escaped it. It was:\n{raw_markup}\nThis \
             is JavaScript SOURCE handed to `evaluate_javascript` and never HTML, so a \
             `</script>` in a fragment has nothing to close. macOS's escaper does escape `<` \
             and `/`; carrying two escapers that differ invites the belief that one of the \
             differences is load-bearing, so the divergence is pinned rather than left to \
             drift."
        );
    }

    // =========================================================================
    // the private helpers, exercised through their own edges
    // =========================================================================

    /// [`scheme_of`] implements RFC 3986's grammar rather than "the text before the
    /// first colon", and the difference decides whether a LOCAL path is handed to an
    /// external launcher.
    ///
    /// O12: there is deliberately NO `C:\…` row. Wolf runs on Linux and macOS, so a
    /// drive letter is not an input this function can receive, and pinning one would
    /// constrain a correct change for a case that cannot occur. `/notes/12:30
    /// standup.md` is the real input with a colon in a path segment, and the grammar
    /// rejects it because its first character is not ALPHA.
    ///
    /// Falsifier: implement `scheme_of` as `uri.split_once(':').map(|(s, _)| s)`. The
    /// `12:30` row answers `Some("/notes/12")`, and `classify`'s row-9 case for the
    /// same input becomes an `OpenExternally` handing a local path to the desktop.
    #[test]
    fn scheme_of_follows_the_rfc_grammar() {
        let cases: [(&str, Option<&str>); 9] = [
            ("https://example.com", Some("https")),
            ("HTTPS://example.com", Some("HTTPS")),
            ("file:///a/x.md", Some("file")),
            ("about:blank", Some("about")),
            ("web+wolf:x", Some("web+wolf")),
            ("/notes/12:30 standup.md", None),
            ("notes/x.md", None),
            (":leading", None),
            ("", None),
        ];

        for (uri, want) in cases {
            assert_eq!(
                scheme_of(uri),
                want,
                "scheme_of({uri:?}) must be {want:?}. A FALSE scheme is the dangerous \
                 direction: `classify` hands anything with a non-file scheme to the platform \
                 URI launcher, so a path segment mistaken for one would ask the desktop to open \
                 a handler for a string out of a document the user merely previewed. A MISSED \
                 scheme is the quiet direction: an `https` link would be previewed as a local \
                 file, core would answer 0, and the click would appear to do nothing."
            );
        }
    }

    /// [`normalize_path`] is LEXICAL and touches no filesystem, which is what keeps
    /// this front end's answer identical to macOS's `standardizedFileURL.path`.
    ///
    /// Falsifier: reach for `std::fs::canonicalize`. Every row below still passes for
    /// paths that exist and the function starts answering differently for paths that
    /// do not — and a pure seam gains a filesystem touch that a headless test cannot
    /// see.
    #[test]
    fn normalize_path_is_lexical_only() {
        let cases: [(&str, &str); 7] = [
            ("/a/x.md", "/a/x.md"),
            ("/a/./x.md", "/a/x.md"),
            ("/a/b/../x.md", "/a/x.md"),
            ("/../x.md", "/x.md"),
            ("/a/x.md/", "/a/x.md"),
            ("/a/./b/./../x.md", "/a/x.md"),
            ("", ""),
        ];

        for (input, want) in cases {
            assert_eq!(
                normalize_path(input),
                want,
                "normalize_path({input:?}) must be {want:?}. It exists so `classify` can tell \
                 \"this link names the document already on screen\" from \"this link names \
                 another file\": under-normalizing spends a blocking `preview_file` and a full \
                 reload where an in-page scroll was meant, losing the reader's position, and \
                 over-normalizing makes two genuinely different files compare equal so a \
                 cross-file link scrolls instead of opening anything."
            );
        }

        // The paths this function is asked about need not exist, and MUST NOT have to.
        assert_eq!(
            normalize_path("/definitely/not/on/this/disk/../x.md"),
            "/definitely/not/on/this/x.md",
            "normalize_path must answer for a path that does not exist. If it ever consults \
             the filesystem — `canonicalize`, `symlink_metadata`, anything — this module stops \
             being pure, a click's meaning starts depending on what happens to be on disk, and \
             the two front ends disagree about whether a link points at the file on screen \
             (macOS's `standardizedFileURL` is lexical too)."
        );
    }
}
