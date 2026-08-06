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
//! ## Rows 1 and 2 exist because `about:blank` is what a NAMELESS buffer resolves
//! against
//!
//! WebKitGTK's `load_html(content, NULL)` does not leave a document with no base at
//! all. Its own API documentation is explicit — "When `base_uri` is `NULL`, it
//! defaults to `about:blank`" (`webkit_web_view_load_html`, WebKit-6.0). So in the
//! one case [`file_uri_for`] answers `None`, a click does NOT arrive schemeless: it
//! arrives carrying an `about` scheme, and without row 1 it would fall through to
//! row 3 and be handed to `gtk::UriLauncher`. Asking the desktop to open a browser
//! on `about:blank#intro` is wrong in a way that has no upside, which is why `about`
//! is refused outright rather than launched.
//!
//! Row 1 is the useful half: an anchor in a document with no location is still an
//! anchor, and scrolling to it needs the element id and nothing else. So a bare
//! `#section` in a nameless buffer's preview scrolls, exactly as it does in a named
//! one — where it never reaches these rows at all, because the `file://` base makes
//! it row 6.
//!
//! **What a click delivers for a RELATIVE link in a nameless buffer's preview is
//! not measured.** Rows 9 and 10 are the defensive answer for a schemeless target,
//! and whether WebKit ever produces one — rather than resolving it against
//! `about:blank` into something this table already covers — is a question only the
//! running application can answer. It is measured during the wiring pass; until
//! then, nothing here claims when those two rows fire.
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
    // `about` URI has a scheme and it is not `file`, so row 3 would launch it. See
    // the module docs — `about:blank` is the base WebKitGTK substitutes when a
    // document is loaded with none, so this is the nameless-buffer path, not an
    // exotic one.
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
