# Verification results: bundle pinned Neovim 0.12.3 into Wolf.app

Recovery/verification run. Each step appended verbatim (command + output + PASS/FAIL) as it completes.

Started: 2026-06-30

---

## Step 1: `bazel test //bazel/nvim:nvim_version_test --test_output=all`

**Result: PASS** (no fixes needed; no `# NOTE to coder` markers required changes — runfiles resolver and VIMRUNTIME incantation worked as written)

```
Located nvim: .../bin/external/+nvim+nvim_macos_arm64/nvim_tree/nvim/bin/nvim
Bundle prefix: .../nvim_tree/nvim
PASS check (a): nvim binary exists and is executable
PASS check (b): nvim reports NVIM v0.12.3
Reported VIMRUNTIME: .../nvim_tree/nvim/share/nvim/runtime
PASS check (c): VIMRUNTIME resolves inside the bundled tree
PASS check (d): bundled treesitter parser present (.../nvim_tree/nvim/lib/nvim/parser/markdown.so)
PASS: bundled nvim is v0.12.3, executable, self-locates its runtime inside the tree, and ships the markdown parser
//bazel/nvim:nvim_version_test  (cached) PASSED in 0.5s
```

---

## Step 2: `bazel build //crates/wolf-macos:Wolf`

**Result: PASS** — builds. rules_apple 4.5.3 delivers `:Wolf` as `bazel-bin/crates/wolf-macos/Wolf.zip` (an archive, NOT an unzipped Wolf.app dir).

```
Target //crates/wolf-macos:Wolf up-to-date:
  bazel-bin/crates/wolf-macos/Wolf.zip
INFO: Build completed successfully, 1 total action
```

---

## Step 3: Structure-preservation inspection (RISK #1)

`:Wolf` delivered as `Wolf.zip`; unzipped to /tmp/wolf_inspect/Wolf.app. The `nvim/` prefix SURVIVED (not flattened), exec bits PRESERVED. **RISK #1 mitigated.**

```
=== Contents/Resources (top) ===
-rw-r--r--  init.lua
drwxr-xr-x  nvim
=== nvim/bin/nvim (ls -l, exec bit) ===
-rwxr-xr-x 1 jplein wheel 7537672 .../Contents/Resources/nvim/bin/nvim
=== markdown.so ===
-rwxr-xr-x 1 jplein wheel 434120 .../Contents/Resources/nvim/lib/nvim/parser/markdown.so
=== share/nvim/runtime (dir) ===
drwxr-xr-x 29 jplein wheel 928 .../Contents/Resources/nvim/share/nvim/runtime
=== init.lua ===
-rw-r--r-- 1 jplein wheel 7659 .../Contents/Resources/init.lua
```

**Result: PASS** (nvim/bin/nvim executable; markdown.so present; share/nvim/runtime is a dir; init.lua at Resources/init.lua). No fallback needed.

---

## Step 4: `bazel test //crates/wolf-macos:bundle_layout_test --test_output=all`

**Result: PASS** — the resolver correctly handled the `Wolf.zip` archive delivery form (found Wolf.zip, unzipped, asserted all 4 paths). No `# NOTE to coder` change required.

```
Found app archive: .../bundle_layout_test.runfiles/_main/crates/wolf-macos/Wolf.zip
Located bundle: /var/folders/.../Wolf.app
PASS check (a): .../Contents/Resources/nvim/bin/nvim exists and is executable
PASS check (b): .../Contents/Resources/nvim/lib/nvim/parser/markdown.so exists
PASS check (c): .../Contents/Resources/nvim/share/nvim/runtime exists and is a directory
PASS check (d): .../Contents/Resources/init.lua exists and is a regular file
//crates/wolf-macos:bundle_layout_test  PASSED in 0.7s
```

---

## Step 5: Signing (RISK #2)

Bundled `bin/nvim` and `markdown.so` are already ad-hoc (linker-signed) Mach-O from the upstream release; `codesign --verify --deep --strict` on the app PASSES (exit 0). **RISK #2 mitigated — no re-sign fallback needed.**

```
===== codesign -dvv on bundled bin/nvim =====
Executable=.../Wolf.app/Contents/Resources/nvim/bin/nvim
Identifier=nvim
Format=Mach-O thin (arm64)
CodeDirectory v=20400 size=58525 flags=0x20002(adhoc,linker-signed) hashes=1826+0 location=embedded
Signature=adhoc
TeamIdentifier=not set

===== codesign -dvv on bundled markdown.so =====
Executable=.../Wolf.app/Contents/Resources/nvim/lib/nvim/parser/markdown.so
Identifier=markdown.so
Format=Mach-O thin (arm64)
CodeDirectory v=20400 size=3492 flags=0x20002(adhoc,linker-signed) hashes=106+0 location=embedded
Signature=adhoc

===== codesign --verify --deep --strict --verbose=2 on Wolf.app =====
/tmp/wolf_inspect/Wolf.app: valid on disk
/tmp/wolf_inspect/Wolf.app: satisfies its Designated Requirement
exit=0
```

**Result: PASS** — no unsigned nested code; app verifies deep+strict. No changes made.

---

## Step 6: Real nvim start against the BUNDLED binary

**Result: PARTIAL — bundled nvim PROVEN to really start + take keys + shut down; bridge_smoke harness hangs HOST-WIDE (not bundling-related).**

### 6a. bridge_smoke against BUNDLED nvim (sandbox) — TIMEOUT (300s)
```
BUNDLED=/tmp/wolf_inspect/Wolf.app/Contents/Resources/nvim/bin/nvim
running 1 test
test bridge_smoke has been running for over 60 seconds
-- Test timed out at ... --
//crates/wolf-ffi:bridge_smoke   TIMEOUT in 300.1s
```
It RAN (did NOT skip): WOLF_NVIM_BIN was set, so `wolf_ffi_start` was invoked against the real bundled nvim — but the harness blocked.

### 6b. bridge_smoke against BUNDLED nvim, NO sandbox (`--strategy=TestRunner=local`) — TIMEOUT (120s)
Same hang without the sandbox, so it is NOT a sandbox file-access problem.

### 6c. CONTROL: bridge_smoke against the system nix nvim (also NVIM v0.12.3) — TIMEOUT (90s)
```
=== nix nvim version === NVIM v0.12.3
running 1 test
test bridge_smoke has been running for over 60 seconds
//crates/wolf-ffi:bridge_smoke   TIMEOUT in 90.0s
```
**KEY FINDING:** bridge_smoke hangs with a KNOWN-GOOD, system-installed nvim 0.12.3 too. So the hang is a PRE-EXISTING bridge_smoke/host issue (its read/wake pump never completes on this macOS Bazel host), INDEPENDENT of the bundling work. The plan itself marks bridge_smoke "(loud-skip ok)" on macOS. I did NOT modify the test.

### 6d. Cannot use wolf-core cargo integration tests as a proxy
wolf-core's real-nvim integration tests (tests/*.rs) are NOT Bazel-wired, and `cargo` is not provisioned even inside the nix devShell (all-Bazel; Rust via rules_rust). So bridge_smoke is the only Bazel real-nvim test, and it cannot complete on this host regardless of binary.

### 6e. DIRECT honest proxy: drive the BUNDLED nvim headlessly (start -> insert keys -> write -> quit) — PASS
```
BUNDLED .../Wolf.app/Contents/Resources/nvim/bin/nvim --headless -u NONE -i NONE -n \
  -c 'normal! ihello from bundled nvim' -c 'write! /tmp/wolf_bundled_edit.txt' -c 'qa!'
exit=0
--- file content ---
hello from bundled nvim
```
The bundled binary really starts, enters insert mode, processes typed keys, writes a buffer, and shuts down cleanly (exit 0) using its bundled runtime. Combined with Step 1 (self-locates VIMRUNTIME inside the tree), this is real evidence the bundled nvim launches and works.

**Honesty:** The GUI window appearing on Finder double-click is NOT headlessly assertable — not faked. bridge_smoke is the intended proxy but is non-functional on this macOS Bazel host for ALL nvim binaries (pre-existing); flagging to orchestrator.

---

## Step 7: Regression — `bazel test //crates/wolf-ffi:frame_pack //crates/wolf-macos:WolfKeyMapTests --test_output=errors`

**Result: PASS** (both green)
```
//crates/wolf-ffi:frame_pack          (cached) PASSED in 0.4s
//crates/wolf-macos:WolfKeyMapTests   (cached) PASSED in 0.4s
Executed 0 out of 2 tests: 2 tests pass.
```

---

## Step 8: Boundary — GUI denylist + wolf-core untouched

### 8a. `bazel query 'deps(//crates/wolf-core:wolf_core)' | grep -iE 'gtk|...'`
Raw grep matched only `@@rules_rust++crate+crates__libc-0.2.186//:src/new/glibc/...` paths — i.e. the `glib` substring inside the `libc` crate's vendored GNU-C-Library (`glibc`) source file paths, NOT the GLib GUI library. Re-running with those excluded:
```
clean (no GUI deps; all prior matches were libc glibc-source paths)
```
**Result: PASS — denylist clean.**

### 8b. `git diff --stat crates/wolf-core`
Empty output → **wolf-core UNTOUCHED** (no source change, no new dep, no CARGO_BAZEL_REPIN needed). **PASS.**

---

## Summary table (steps 1-8)

| Step | What | Result |
|------|------|--------|
| 1 | nvim_version_test (pinned 0.12.3, exec bit, self-locate VIMRUNTIME, markdown.so) | PASS |
| 2 | build //crates/wolf-macos:Wolf (delivered as Wolf.zip) | PASS |
| 3 | structure preservation (RISK #1): nvim/ prefix + exec bits + init.lua | PASS |
| 4 | bundle_layout_test (4 bundle paths over the built .app) | PASS |
| 5 | signing (RISK #2): nested adhoc-signed; codesign --verify --deep --strict OK | PASS |
| 6 | real start vs bundled nvim | PARTIAL: bundled nvim proven to start+key+shutdown; bridge_smoke hangs HOST-WIDE (also with system nvim) — pre-existing, not bundling |
| 7 | regression frame_pack + WolfKeyMapTests | PASS |
| 8 | boundary: GUI denylist clean + wolf-core untouched | PASS |

No implementation files were changed during verification (no `.sh` runfiles fix or signing fallback was needed — both `# NOTE to coder` resolvers worked as written, and the nested Mach-O was already adhoc-signed and verifies deep+strict).

GUI-window-on-double-click is NOT headlessly assertable — not faked.
