#!/usr/bin/env bash
# Checks Markdown files, passed as "$@", against the documentation rules in CLAUDE.md.
# The rules, thresholds and exemptions: tools/lint/README.md#what-markdown-checks
set -euo pipefail

MAX_LINE="${WOLF_MD_MAX_LINE:-800}"
MAX_BYTES="${WOLF_MD_MAX_BYTES:-50000}"
MAX_ENTRY_WORDS="${WOLF_MD_MAX_ENTRY_WORDS:-250}"

# Acronyms, proper nouns and repo filenames; any other all-caps word without a digit is emphasis.
# Keep sorted.
ACRONYMS="ABI ANSI API APIs ARC ARGB ASCII BMP BSD BUILD CJK CLAUDE CLI CPU CRLF CSS CWD DECISIONS DEL DFS DOM EOF EOL ESC FFI FIFO GDK GNOME GNU GTK GUI GUST HTML HTTP HTTPS IDE IME JSON KVO LLDB LOC LSP MIT MODULE MRU NTP NVIM OCR OFL OOM PID PNG POSIX README RGB RHS RPC RRGGBB RTL SDK SHA SIGKILL SIGTERM SIL SVG TCC TCP TLS TMPDIR TODO TOML TSM TTF TTY URI URL URLs UTF VIMRUNTIME XDG XML YAML ZIP"

if [[ "$#" -eq 0 ]]; then
  echo "ERROR: no Markdown files were passed to the checker." >&2
  exit 1
fi

echo "Checking $# Markdown file(s)..."
echo "  max line: ${MAX_LINE} chars | max entry: ${MAX_ENTRY_WORDS} words | size warning: ${MAX_BYTES} bytes"
echo

status=0

# The size ceiling only warns: a directory with many source files legitimately needs a long README.
for f in "$@"; do
  bytes=$(wc -c <"${f}")
  if [[ "${bytes}" -gt "${MAX_BYTES}" ]]; then
    echo "${f}: warning: [big-file] ${bytes} bytes exceeds the ${MAX_BYTES}-byte ceiling"
  fi
done

awk \
  -v max_line="${MAX_LINE}" \
  -v max_entry_words="${MAX_ENTRY_WORDS}" \
  -v acronyms="${ACRONYMS}" \
  '
BEGIN {
  n = split(acronyms, arr, " ")
  for (i = 1; i <= n; i++) allowed[arr[i]] = 1
  findings = 0
  header_msg = "first line must be: Last verified at commit `<40-hex sha>`."
}

function report(file, line, rule, msg) {
  printf "%s:%d: [%s] %s\n", file, line, rule, msg
  findings++
}

# Close the open #### entry and report it if it blew the word budget.
function close_entry() {
  # Blame entry_file, not FILENAME: the FNR == 1 caller runs after awk has
  # advanced to the next file.
  if (entry != "" && entry_words > max_entry_words)
    report(entry_file, entry_line, "entry-budget", \
           "entry " entry " is " entry_words " words (budget " max_entry_words ")")
  entry = ""
  entry_words = 0
}

# Report a notes/ file that ended before its verification line, against its own name.
function close_note() {
  if (note_file != "" && note_pending)
    report(note_file, 1, "notes-header", header_msg)
  note_file = ""
  note_pending = 0
}

function is_header(line,   sha) {
  if (line !~ /^Last verified at commit `[0-9a-f]+`\.$/) return 0
  sha = line
  sub(/^Last verified at commit `/, "", sha)
  sub(/`\.$/, "", sha)
  return length(sha) == 40
}

FNR == 1 {
  close_entry()
  close_note()
  fence = 0
  in_notes = (FILENAME ~ /(^|\/)notes\//)
  if (in_notes) {
    note_file = FILENAME
    note_pending = 1
  }
}

# ---- notes header ----------------------------------------------------------
# The first line that is neither blank nor a heading must be the verification line.
note_pending && $0 !~ /^[ \t]*$/ && $0 !~ /^(#|##|###|####|#####|######)[ \t]/ {
  note_pending = 0
  if (!is_header($0)) report(FILENAME, FNR, "notes-header", header_msg)
}

# Fenced code blocks are exempt from every content rule.
/^[ \t]*(```|~~~)/ { fence = !fence; next }
fence { next }

{
  # Strip what is not prose: inline code, link/image targets, HTML comments,
  # and bare URLs. Identifiers and paths are not emphasis and not changelog voice.
  prose = $0
  gsub(/`[^`]*`/, " ", prose)
  gsub(/\]\([^)]*\)/, "] ", prose)
  gsub(/<!--.*-->/, " ", prose)
  gsub(/https?:\/\/[^ )]*/, " ", prose)
}

# ---- entry word budget -----------------------------------------------------
/^####[ \t]/ {
  close_entry()
  entry = $0
  sub(/^####[ \t]+/, "", entry)
  entry_line = FNR
  entry_file = FILENAME
}

# Any heading at level 4 or shallower ends the previous entry.
/^#{1,3}[ \t]/ { close_entry() }

entry != "" && $0 !~ /^####[ \t]/ { entry_words += NF }

# ---- line length -----------------------------------------------------------
# Table rows are exempt: a wide table is legitimate and prettier aligns it.
length($0) > max_line && $0 !~ /^[ \t]*\|/ {
  report(FILENAME, FNR, "long-line", \
         "line is " length($0) " chars (max " max_line "); split the paragraph")
}

# ---- changelog voice (notes/ is exempt) -------------------------------------
!in_notes {
  low = tolower(prose)
  if (low ~ /(^|[^a-z0-9_])now([^a-z0-9_]|$)/) report(FILENAME, FNR, "changelog", "\"now\"")
  if (low ~ /no longer/)                       report(FILENAME, FNR, "changelog", "\"no longer\"")
  if (low ~ /previously/)                      report(FILENAME, FNR, "changelog", "\"previously\"")
  if (low ~ /(^|[^a-z0-9_])the old([^a-z0-9_]|$)/) report(FILENAME, FNR, "changelog", "\"the old\"")
  if (low ~ /renamed from/)                    report(FILENAME, FNR, "changelog", "\"renamed from\"")
  if (low ~ /this increment/)                  report(FILENAME, FNR, "changelog", "\"this increment\"")
  if (low ~ /used to be/)                      report(FILENAME, FNR, "changelog", "\"used to be\"")
}

# ---- ALL-CAPS emphasis -----------------------------------------------------
{
  rest = prose
  while (match(rest, /[A-Z][A-Z0-9]{2,}/)) {
    tok = substr(rest, RSTART, RLENGTH)
    before = (RSTART > 1) ? substr(rest, RSTART - 1, 1) : " "
    after = substr(rest, RSTART + RLENGTH, 1)
    rest = substr(rest, RSTART + RLENGTH)
    # A digit means an error code, a version, or an identifier (E325, GTK4, X11).
    if (tok ~ /[0-9]/) continue
    if (tok in allowed) continue
    # A letter on either side means this is a run of capitals inside a CamelCase
    # identifier (NSMenu, CGColor, wolfAPIThing), not shouting.
    if (after ~ /[a-z]/) continue
    if (before ~ /[a-zA-Z]/) continue
    report(FILENAME, FNR, "allcaps", "\"" tok "\" reads as shouting — use bold, or lowercase it")
  }
}

END {
  close_entry()
  close_note()
  if (findings > 0) {
    printf "\n%d finding(s).\n", findings
    exit 1
  }
  print "No findings."
}
' "$@" || status=$?

exit "${status}"
