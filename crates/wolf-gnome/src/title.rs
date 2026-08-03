//! The pure, GTK-free WINDOW-TITLE seam: the working directory, rendered.
//!
//! A 1:1 port of the macOS `CwdTitleSeam.displayTitle(cwd:home:)`, so the two
//! front-ends abbreviate a path identically. It takes scalar strings only — no
//! widget, no `PathBuf`, no filesystem, and above all **no implicit `$HOME`**:
//! `home` is ALWAYS injected by the caller, which is what makes the whole rule
//! decidable from its arguments and unit-testable headlessly, exactly like
//! [`crate::geometry`] and [`crate::workspace`]. The one impure `$HOME` read in
//! this crate stays in [`crate::workspace::home_dir`], and [`crate::shell`]
//! captures its result once at build time.
//!
//! [`display_title`]'s contract, mirrored exactly:
//!
//! 1. `cwd` empty or all-whitespace → `""` (the caller substitutes a fallback).
//! 2. `home` empty → `cwd` verbatim (abbreviation disabled).
//! 3. `home` is normalized by stripping ONE trailing `/`; if that leaves it empty
//!    (i.e. `home` was `"/"`) → `cwd` verbatim.
//! 4. `cwd == normalized_home` → `"~"`.
//! 5. `cwd` starts with `normalized_home + "/"` → `"~/" + remainder`. The match is
//!    on a full path-SEGMENT boundary only, so `/Users/janet/x` is NOT abbreviated
//!    against home `/Users/jane`.
//! 6. otherwise → `cwd` verbatim.
//!
//! [`window_title`] is [`display_title`] with the empty result replaced by
//! [`WINDOW_TITLE_FALLBACK`], so the widget layer has nothing left to decide.

/// The window title shown when the working directory is unknown — i.e. whenever
/// [`display_title`] returns `""`.
pub const WINDOW_TITLE_FALLBACK: &str = "Wolf";

/// The `~` abbreviation for the home directory itself.
const HOME_ABBREVIATION: &str = "~";

/// Returns the display title for `cwd`, abbreviating the `home` prefix to `~`.
///
/// See the module docs for the full contract. Returns `""` for an unknown
/// (empty or all-whitespace) `cwd`; [`window_title`] is the variant that
/// substitutes [`WINDOW_TITLE_FALLBACK`] instead.
#[must_use]
pub fn display_title(cwd: &str, home: &str) -> String {
    // Rule 1: an empty / all-whitespace cwd carries no information at all.
    if cwd.trim().is_empty() {
        return String::new();
    }

    // Rule 2: no home means no abbreviation is possible.
    if home.is_empty() {
        return cwd.to_string();
    }

    // Rule 3: strip ONE trailing separator. `"/"` normalizes to `""`, which
    // disables abbreviation — abbreviating every absolute path to `~/…` would be
    // actively misleading.
    let normalized_home = home.strip_suffix('/').unwrap_or(home);
    if normalized_home.is_empty() {
        return cwd.to_string();
    }

    // Rule 4: the home directory itself.
    if cwd == normalized_home {
        return HOME_ABBREVIATION.to_string();
    }

    // Rule 5: under home, on a SEGMENT boundary. Matching `normalized_home + "/"`
    // rather than `normalized_home` is the whole point: `/Users/janet/x` shares a
    // character prefix with `/Users/jane` but is not under it.
    let home_prefix = format!("{normalized_home}/");
    if let Some(rest) = cwd.strip_prefix(home_prefix.as_str()) {
        return format!("{HOME_ABBREVIATION}/{rest}");
    }

    // Rule 6: not under home — the full absolute path, verbatim.
    cwd.to_string()
}

/// The window title for `cwd`: [`display_title`], with an empty result replaced
/// by [`WINDOW_TITLE_FALLBACK`].
///
/// This is the function the widget layer calls, so "what do we show when there is
/// no cwd yet" is decided HERE (and unit-tested) rather than in a GTK closure.
#[must_use]
pub fn window_title(cwd: &str, home: &str) -> String {
    let title = display_title(cwd, home);
    if title.is_empty() {
        WINDOW_TITLE_FALLBACK.to_string()
    } else {
        title
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The home directory used across the table. Deliberately shares a character
    /// prefix with [`SIBLING`] so the segment-boundary rule has something to fail
    /// against.
    const HOME: &str = "/Users/jane";

    /// A directory whose path shares the `"/Users/jane"` CHARACTER prefix with
    /// [`HOME`] but is NOT under it — the segment-boundary case.
    const SIBLING: &str = "/Users/janet/x";

    /// One [`display_title`] table row.
    struct Case {
        desc: &'static str,
        cwd: &'static str,
        home: &'static str,
        want: &'static str,
    }

    #[test]
    fn display_title_table() {
        let cases = [
            // --- rule 1: an unknown cwd carries no information --------------
            Case {
                desc: "an empty cwd yields the empty string (the caller substitutes a fallback)",
                cwd: "",
                home: HOME,
                want: "",
            },
            Case {
                desc: "an ALL-WHITESPACE cwd is also unknown (trim, not is_empty)",
                cwd: "   \t ",
                home: HOME,
                want: "",
            },
            // --- rule 2: no home disables abbreviation ----------------------
            Case {
                desc: "an empty home disables abbreviation entirely",
                cwd: "/Users/jane/proj",
                home: "",
                want: "/Users/jane/proj",
            },
            // --- rule 3: home normalizes by stripping ONE trailing slash ----
            Case {
                // `"/"` normalizes to `""`, which disables abbreviation:
                // abbreviating EVERY absolute path to `~/…` would be misleading.
                desc: "home == \"/\" normalizes to empty, so the cwd is verbatim",
                cwd: "/Users/jane/proj",
                home: "/",
                want: "/Users/jane/proj",
            },
            Case {
                desc: "a trailing slash on home behaves exactly as without one",
                cwd: "/Users/jane/proj",
                home: "/Users/jane/",
                want: "~/proj",
            },
            // --- rule 4: the home directory itself --------------------------
            Case {
                desc: "cwd == home is `~`",
                cwd: HOME,
                home: HOME,
                want: "~",
            },
            Case {
                desc: "cwd == home + \"/\" is `~/` (the empty remainder)",
                cwd: "/Users/jane/",
                home: HOME,
                want: "~/",
            },
            // --- rule 5: under home, on a segment boundary ------------------
            Case {
                desc: "a directory under home abbreviates to `~/rest`",
                cwd: "/Users/jane/proj/wolf",
                home: HOME,
                want: "~/proj/wolf",
            },
            Case {
                // THE most important row in this table. `/Users/janet/x` shares the
                // CHARACTER prefix `/Users/jane` with home but is not under it, so a
                // `starts_with(home)` implementation would render it as `~t/x`.
                desc: "a SIBLING sharing a character prefix is NOT abbreviated (segment boundary)",
                cwd: SIBLING,
                home: HOME,
                want: SIBLING,
            },
            // --- rule 6: everything else is verbatim ------------------------
            Case {
                desc: "a path sharing no prefix with home is verbatim",
                cwd: "/opt/build/tree",
                home: HOME,
                want: "/opt/build/tree",
            },
            Case {
                desc: "a RELATIVE-looking cwd is still returned verbatim",
                cwd: "proj/wolf",
                home: HOME,
                want: "proj/wolf",
            },
        ];

        for case in &cases {
            let got = display_title(case.cwd, case.home);
            assert_eq!(
                got, case.want,
                "case: {} (cwd = {:?}, home = {:?})",
                case.desc, case.cwd, case.home
            );
        }
    }

    #[test]
    fn window_title_falls_back_to_wolf() {
        assert_eq!(
            WINDOW_TITLE_FALLBACK, "Wolf",
            "the fallback title is the product name; it is what a user sees before nvim has \
             reported a cwd, and the S3 title smoke's red-run revert leaves it on screen"
        );

        // The two cases `display_title` answers with "" are exactly the two the
        // fallback covers.
        for cwd in ["", "   \t "] {
            assert_eq!(
                window_title(cwd, HOME),
                WINDOW_TITLE_FALLBACK,
                "an unknown cwd ({cwd:?}) must render as {WINDOW_TITLE_FALLBACK:?}, not as an \
                 empty header-bar title"
            );
        }

        // Everything else is `display_title` verbatim — `window_title` decides the
        // fallback and NOTHING else.
        for (cwd, home) in [
            (HOME, HOME),
            ("/Users/jane/proj", HOME),
            (SIBLING, HOME),
            ("/opt/build/tree", ""),
        ] {
            assert_eq!(
                window_title(cwd, home),
                display_title(cwd, home),
                "window_title(cwd = {cwd:?}, home = {home:?}) must be display_title's value \
                 unchanged whenever that value is non-empty"
            );
        }
    }

    #[test]
    fn home_is_never_read_from_the_environment() {
        // The determinism property, made executable: `home` is injected, so an EMPTY
        // injected home disables abbreviation no matter what `$HOME` happens to be in
        // the process running this test. If this module ever grew a `std::env::var`
        // fallback, this assertion is what would catch it on a developer machine whose
        // real `$HOME` is a prefix of the cwd below.
        assert_eq!(
            display_title("/x", ""),
            "/x",
            "an empty injected `home` must disable abbreviation regardless of the process $HOME; a \
             different answer here means this module read the environment"
        );

        // And the same from the other side: a home the process certainly does not have
        // still abbreviates, so the injected value is the ONLY thing consulted.
        assert_eq!(
            display_title("/nonexistent-home-9/proj", "/nonexistent-home-9"),
            "~/proj",
            "the INJECTED home must drive the abbreviation even though no such directory exists; \
             this seam never touches the filesystem or the environment"
        );
    }
}
