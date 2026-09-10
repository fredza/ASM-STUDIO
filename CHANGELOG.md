# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.7.14] - 2026-09-10

### Added
- **NX and RELRO in the FORMAT panel**, next to PIE, for the "Modern
  protections: NX, RELRO and PIE" chapter. Read straight from the ELF program
  headers (`PT_GNU_STACK`, `PT_GNU_RELRO`), not simulated. The interesting
  case is the common one: NASM never emits the `.note.GNU-stack` section gcc
  adds, so `ld` writes no `GNU_STACK` segment at all in anything this IDE
  produces — and that is *not* the same as an executable stack. Absent a
  marker, the kernel applies its own default, which on x86-64 is already
  non-executable; a note says so, backed by a test that actually runs code
  placed on the stack (segfault without a marker, success with one, and with
  `-z execstack` explicitly). RELRO appears once linking is `-pie` (an
  ASM Studio binary never reaches "full" RELRO — that needs `-z now`, which
  this linker never requests — so only presence/absence is shown, not the
  partial/full distinction). Both read `None` outside ELF, and `None` rather
  than a false "absent" if the program headers fail to parse.

### Fixed
- **`WinDebugger::available()` was launching two Wine processes on every UI
  frame.** It backs both the toolbar's Windows Step/Continue buttons and the
  step debugger window, so a session that stayed busy — most visibly, a
  program waiting on a `MessageBoxA` — requested a repaint every 30ms, and
  each one paid `wine --version` plus `winedbg --help` (~140ms combined).
  That is what surfaced as a runaway `winedbg`/`start.exe` process churn and
  high CPU right after answering a dialog box. The result is now memoized for
  five seconds — long enough that a frame costs nothing, short enough that
  installing Wine while the IDE runs still needs no restart to be picked up.
- **Answering a real dialog box could outlast the debugger's own patience.**
  `Continue` waited at most twenty seconds for winedbg's reply, the same
  budget used for an ordinary instruction — but a human reading a `MessageBoxA`
  and finding the OK button routinely takes longer, and the session used to
  close on a bare "Resource temporarily unavailable (os error 11)" that named
  nothing useful. Resuming execution now gets two minutes, and a genuine
  timeout closes the session with a message that says what actually
  happened: the program never got its answer, so try again. The step debugger
  window also now says, while a command is in flight, that a dialog box may
  be open elsewhere on screen and waiting for a click.

### Added
- **An optional position-independent link (`-pie`) for the Linux target**, for
  the "PIE and RIP-relative addressing" chapter. A checkbox under Run — shown
  only for the Linux target, since a PE64 is relocatable by birth — makes the
  IDE link with `ld -pie --no-dynamic-linker -z text`, producing an ELF of
  type `DYN` instead of `EXEC`: a program the kernel may load anywhere. **Off
  by default, and deliberately so**: it only accepts code written for it
  (`default rel`, `lea reg, [rel label]` instead of an absolute address), and
  everything written before that chapter addresses absolutely — `ld` refuses
  the relocation outright rather than producing a binary that reads at the
  wrong place, which is itself the symptom the chapter teaches to recognise.
  `-z text` is what makes that refusal reliable: without it, an absolute
  address links anyway on some `ld` versions, with only a warning, leaving a
  dynamic relocation that nothing ever applies (no dynamic linker, no static-
  PIE startup stub) — the program then runs and exits cleanly while reading
  from the wrong place, silently. The setting is remembered (`pie` in
  `settings.conf`), and every example, exercise and lesson starter shipped
  with the IDE still links exactly as before.
- **A new shipped example, `pie_rip_relatif.asm`**, that goes with it: one
  program addressing all of its data through RIP, reading and writing a
  counter in `.data`, and running identically whether linked position
  independently or not.
- **Single-stepping now follows a position-independent executable.** A `DYN`
  binary is loaded somewhere other than the address written in its headers —
  even with ASLR disabled, as the debugger already does for reproducible
  runs — so RIP matched no line of the listing and no breakpoint could ever
  fire. The debugger now reads the real load address from `/proc/<pid>/maps`
  at the first stop and offsets the listing, the disassembly (jump targets in
  operands included) and the memory panel by that much, once, at the root:
  breakpoints, "Resume here", the call stack, the microscope and the memory
  panel all keep working on a single scale of addresses. The FORMAT panel also
  stops calling such a binary a "shared library" — it is the same ELF type,
  but a program with an entry point, not a library.

### Fixed
- **The experimental Windows step debugger froze the whole IDE whenever the
  debugged program stopped on something that waits** — typically
  `MessageBoxA`, whose modal dialog only returns on a click. The GDB remote
  protocol behind `winedbg --gdb` is synchronous: `Continue` had nothing to
  return until the program stopped again, and it was being called straight
  from egui's event loop, so every part of the application — not just that
  window — stayed frozen until the protocol's own twenty-second timeout.
  GNOME eventually offered to kill "ASM Studio (not responding)".
  The debugger now lives on a thread of its own for the whole session, and
  the interface only posts commands and polls the answers once per frame,
  the same way the Wine runner and the native debugger already did.
  `Next`/`Continue` grey out with a "Working…" indicator while a command is
  in flight, so a wait reads as a wait rather than as a hang, and `Stop`
  stays clickable throughout: it really interrupts the pending command by
  killing `winedbg`, which closes the connection and frees the background
  thread within a second — instead of abandoning it, which would have left
  `winedbg` and the debugged program running invisibly in the background.

## [0.7.11] - 2026-09-10

### Added
- **A build-time warning for misaligned-stack calls.** The classic trap —
  an `add rsp, N` that undoes the alignment set up for one call before a
  second one — crashes deep inside a system DLL or libc, at an address
  that names none of the student's lines, sometimes only every other run.
  The new check follows the linear instruction path from the entry point,
  tallying `push`/`pop` and `sub`/`add rsp, N`, and warns on the first
  `call` reached with RSP not a multiple of 16 — but only when that call
  leaves the program (an import thunk or PLT stub); a misaligned call into
  the student's own code has no observable effect, and gets left alone.
  Gives up silently on the first jump, loop, or non-literal stack
  adjustment rather than risk a false positive. A warning, not an error:
  appended to the build log, in all three languages, never blocks
  assembly. Verified against every shipped example and lesson starter —
  the only one that triggers it is "The shadow space" (`win_pile`), whose
  starter contains exactly this bug on purpose.

### Fixed
- **The PE linker misaligned its import tables with an even number of
  imported DLLs** — the import descriptors (20 bytes each) didn't leave
  the thunk arrays on an 8-byte boundary. Found while chasing what turned
  out to be an unrelated crash (see below); real nonetheless, now covered
  by a test that links and actually runs a two-DLL binary under Wine.
- **A Wine crash blamed on the PE linker was actually a misaligned stack**
  in the test program itself — `add rsp, 40` undoing an earlier
  alignment before a second `call`. Wine reports every general-protection
  fault as "read access to `FFFFFFFFFFFFFFFF`", which sent the
  investigation toward the linker before the real cause surfaced. This is
  exactly the trap the new build-time warning above now catches.

## [0.7.1] - 2026-09-10

### Fixed
- **CI was red on every push, unrelated to what changed**: `cargo test`
  (debug builds only — `cargo test --release`, what the release workflow
  runs, was never affected) aborted on the first of about eighty headless
  UI tests, each dropping an `egui::Context::run_ui` output without
  handling its `textures_delta`. That's `egui`/`epaint` itself: any fresh
  `Context`'s very first `run_ui` uploads its default font atlas, and
  `epaint::TexturesDelta`'s `Drop` impl `debug_assert!`s that this got
  handled — a real bug in a real renderer (an unshown texture), a false
  positive in a layout-only test. Silenced with a single targeted
  `[profile.test.package.epaint] debug-assertions = false` in `Cargo.toml`,
  rather than touching every affected test — it disarms only `epaint`'s own
  `debug_assert!`s, not `asm_studio`'s.
- **A clippy lint** (`while_let_loop`) in the new Windows step debugger's
  `Continue` command, caught once `cargo clippy --all-targets -- -D
  warnings` could actually run to completion instead of failing to build
  first.

## [0.7.0] - 2026-09-10

### Added
- **Experimental single-stepping for the Windows (PE64) target**, from the
  Run menu or the toolbar's Next/Continue buttons (`winedbg --gdb` under
  Wine). Its own small window shows general registers, flags, and RIP's
  source line, and honours the breakpoints set in the editor's gutter. Runs
  alongside, not instead of, the existing "Run" button (direct Wine
  execution, with real output) — the new window has no program output, no
  exercise checking, and no prediction game: `winedbg` gives none of those
  today. Off entirely, with the usual message, when Wine or `winedbg` isn't
  installed.

### Changed
- **Close / Minimize / Maximize now sit on the left** of the title bar,
  macOS-style, instead of the right — a direct preference, not a platform
  convention this time.
- **Right-click the title bar for "Always on top"**, toggled and
  remembered across sessions. Wired through `ViewportCommand::WindowLevel`;
  unavailable under Wayland (checkbox shown disabled, with an explanation
  on hover) — no standard Wayland protocol lets an ordinary window force
  itself above others, and `winit`'s Wayland backend does not even attempt
  it. Works normally under X11, Windows, and macOS.

### Fixed
- **The three title-bar buttons had no real accessibility label** — a
  screen reader would have announced their glyph ("×", a generic square)
  rather than "Close", "Minimize", "Maximize", since `egui::Button` fills
  its AccessKit node from the visible text by default. Each button now
  carries an explicit label via `Context::accesskit_node_builder`,
  independent of what glyph happens to render.

## [0.6.1] - 2026-09-09

### Changed
- **Releases are now built, signed and published automatically** by
  `.github/workflows/release.yml` on every `vX.Y.Z` tag push: build,
  `cargo test`, package the archive, sign the raw update binary, and
  upload all four assets to the GitHub release — the same steps
  `install/package.sh` already documented as manual commands, now run by
  CI instead. The signing key lives only in the repository's
  `UPDATE_SIGNING_KEY` Actions secret; it was never written to any file in
  this repository. `src/bin/release_sign.rs` is the small maintainer tool
  that generates that key pair and performs the signing — not part of the
  distributed app.
- **New Ed25519 update-signing key pair**, replacing the previous
  `UPDATE_PUBLIC_KEY`: no private key matching the old value was available
  on the machine or in any tool used to publish releases, so no published
  release had ever actually been signed with it. Verified end to end
  before publishing — a real signed v0.6.0 binary was checked against the
  real embedded public key, not just a test key pair.

## [0.6.0] - 2026-09-09

### Changed
- **Custom title bar**, replacing the native window decorations
  (`main.rs`, `with_decorations(false)`). Minimize / maximize-restore /
  close now always sit on the right, regardless of what the desktop's own
  window manager theme would otherwise draw them as (several put them on
  the left). The title bar still drags to move the window and
  double-clicks to maximize/restore; since cutting native decorations also
  cuts the OS's own resize handles, the app now draws its own on all four
  edges and corners — active only while the window isn't maximized or
  fullscreen, where resizing wouldn't mean anything anyway.
- **Console panel is no longer part of the default docked layout**, in
  either Learning or Full mode. The "Sortie" (⏷) button — in the toolbar
  and in the Console panel's own header — covers the same need now that
  it toggles and stays interactive the whole time a program runs (see
  Fixed below), so keeping the panel permanently docked stopped earning
  its space. It remains one click away from the View menu whenever the
  fuller Console — with the IDE's own log messages, not just the
  program's output — is what's wanted.
- **eframe 0.33 → 0.36, egui_dock 0.18 → 0.21**: the biggest dependency
  bump this project has taken since it started. Adapted the whole UI layer
  to egui's new API — `TopBottomPanel`/`SidePanel` merged into a single
  `Panel` type shown against a `Ui` instead of a `Context`, `eframe::App`'s
  `update(&Context)` renamed to `ui(&mut Ui)`, `egui_dock`'s tab/node
  lookups returning named `TabPath`/`NodePath` structs instead of tuples,
  per-theme styles (`style_of`/`set_style_of`) replacing the single
  `ctx.style()`, and the text-cursor API now indexing by a `CharIndex`
  newtype instead of a bare `usize`. One toolbar glyph (`⬆`, "parent
  folder") no longer has a matching glyph in egui's default font and was
  swapped for `⏶`. base64 0.22 → 0.23 and ureq 3.3 → 3.4 came along for
  the ride (no code changes needed).

  eframe 0.36 also switched its *default* rendering backend from `glow`
  (OpenGL) to `wgpu` (Vulkan) — pinned back to `glow` explicitly in
  `Cargo.toml`. Left on `wgpu`, the release binary linked `libvulkan.so.1`
  and grew by close to 4 MB for a whole Vulkan binding (ray tracing and
  mesh shaders included) this app has no use for, on a machine a learner
  might not have a working Vulkan driver on. `glow` keeps the OpenGL
  2.0/3.2 baseline this project has always targeted, and keeps
  `install/install.sh` and `DEPENDENCIES.md` — which check for and
  document `libEGL`/`libGL`, not Vulkan — actually accurate.
- **License: GNU GPLv3 + Commons Clause**, replacing the ASM Studio
  Personal Free License (ASFL) v1.0. Source stays open, modification and
  redistribution under the same license are now explicitly protected by
  the GPLv3's copyleft; the Commons Clause keeps the one restriction that
  mattered — selling the software, original or modified, is still
  prohibited. See `LICENSE.md` for the full text and an honest note on
  why this combination isn't a "pure" GPLv3 in the FSF's sense.

### Added
- **"Program output" button in the main toolbar**, next to Build: shows the
  program's raw output alone, without the IDE's own log messages — the same
  view the Console panel's header button already opened, now reachable
  without that panel being open or visible.
- **Resuming the last file or project on launch**: the IDE used to always
  reopen the `hello_world.asm` example at startup, even with a project
  worked on for hours the day before — the recent-files list existed but
  nothing used it for this. It now reopens the most recent file or project
  automatically, and falls back to the usual welcome example when there is
  none, or when it has vanished from disk since the last session.
- **"Panneau Console…" link inside the "Sortie du programme" window**, to
  reach the full Console panel — IDE log messages included — from there in
  one click, now that the panel isn't docked by default.

### Fixed
- **`uninstall.sh --purge` left two of four data directories behind.** It
  removed `~/.config/asm_studio` and `~/.local/share/asm_studio`, but not
  `~/.cache/asm_studio` or `~/.local/state/asm_studio` — where the app also
  keeps internal state (see `trial_marker_paths` in `src/app/paths.rs`). A
  flag that promises to remove "personal data" left silent litter behind
  in two of the four XDG locations it uses; `--purge` now clears all four.
- **"Sortie du programme" window was effectively read-only** outside the
  exact instant a running program was blocked on a `read` — its input
  field only existed while that was true. It now stays available the
  whole time a program is alive with an open stdin, matching what the
  Console panel already did, so answering a prompt no longer means timing
  the click to the blocking instant.
- **Toolbar and console-header "Sortie" buttons only ever opened** the
  program output window, never closed it. A second click now toggles it,
  like every other panel button in the app.
- **Microscope button in the Instruction panel could get painted on top
  of a long instruction title** (e.g. "JNE — Jump if Not Equal / Not
  Zero") instead of beside it, on a narrow panel. The button now reserves
  its own space first; the title fills what's left and truncates with an
  ellipsis instead of running under it.

## [0.5.0] - 2026-09-09

First release out of beta: the version number goes back to plain semver, with
no prerelease suffix.

### Added
- **Memory watchpoints**: Breakpoints answer "stop at this line". They never
  answered the question a learner actually asks — *who overwrote my variable?*
  A buffer that runs past its end, an unbalanced `rsp`, an index one step too
  far: the only recourse was to scatter ten breakpoints and watch. The MEMORY
  panel now offers **👁 watch @ base**, and execution stops as soon as those
  eight bytes change, naming what they held, what they hold now, and the line
  responsible — `👁 0x7FFF… : 0x0 → 0x2A (line 14)`.

  Implemented by comparing the watched bytes after each step, not with the
  processor's DR0–DR3 debug registers. Those exist to avoid single-stepping,
  and this debugger single-steps by design: they would have bought nothing,
  while imposing four addresses at most, sizes limited to 1/2/4/8 bytes, and
  no way to report the *previous* value — which is the one that explains the
  bug. Watches survive "Resume here" and every relaunch.
- **Execution heat map in the gutter**: How many times each line actually ran,
  as a blue tint behind the line numbers, with `▶ ×N` on hover. A nested loop
  stops being a guess. The scale is logarithmic on purpose: with a linear one,
  a body run ten thousand times would make everything else invisible, when a
  line run three times is exactly what one needs to see. The count is
  accumulated step by step — a frame where nothing moved costs one integer
  comparison, and there are sixty of them per second.
- **A register over time**: The panels show the state at one instant; this
  window shows the trajectory — which is what one is really trying to follow
  inside a loop. A curve over the whole run, the list of steps where the value
  changed, and a click to jump straight there in the timeline. Reached by the
  📈 button in the REGISTERS panel or from the command palette. Everything
  comes from the history the debugger already records: nothing is re-executed.

### Fixed
- **Panels no longer shiver at the end of a scroll**: Scrolling the file
  explorer down to its last entry made the whole tree jitter. `egui_dock` wraps
  every tab's content in a `ScrollArea` of its own, scroll bars enabled by
  default — a zone with nothing to scroll, since each panel handles its own
  scrolling and fills the space it is given. It still decided, frame after
  frame, whether it needed a bar, and changed its mind: the bar appeared and
  vanished on alternate frames, shifting the panel's whole content by the
  eleven points a solid scroll bar reserves. The tab and the frame around it
  never moved, which is what pointed at that zone. The outer bars are now
  turned off, for every panel — the explorer was merely where it showed.

### Changed
- **Addresses no longer move between runs**: The debugged program was started
  with address space randomisation on, so the stack landed somewhere else at
  every launch — `0x7FFF38F69380`, then `0x7FFF58E867C0`. An address written
  down in the MEMORY panel meant nothing the next time, "Resume here" replayed
  the program at addresses the timeline no longer recognised, and a watched
  address could not be re-armed because it no longer pointed anywhere. Worse
  for a learner: the same program printed different numbers twice in a row,
  with nothing in the code to explain it. The child process now starts with
  `ADDR_NO_RANDOMIZE`, exactly as gdb does by default.

## [0.5.0-beta.5] - 2026-09-09

### Changed
- **The license system is off**: Disassembly, registers/flags, the timeline and
  the SIMD panel are no longer reserved — they open for everyone, with no
  license and no registration delay. The periodic reminder is gone, closing the
  window is never intercepted again, and *Help → Activate a license…*, the
  *Activation* row of the *About* window and the palette command that went with
  them have disappeared along with it. Nothing is read or written on disk any
  more, neither `license.txt` nor the trial markers.

  The mechanism itself — Ed25519 verification, the free-registration delay — is
  still compiled and covered by its tests, simply no longer consulted: a single
  constant, `license::LICENSING_ENABLED`, turns it back on exactly as it was.

## [0.5.0-beta.4] - 2026-08-30

### Added
- **Update check at startup**: The IDE asks GitHub once when it starts, in a
  background thread. The check is *quiet*: it opens a window only when a newer
  version really exists. "Nothing new" and "no network" fall back to silence —
  otherwise every offline launch would have opened on an error nobody asked
  for. A check asked for by hand (*Tools → Check for updates*) still answers in
  every case.

### Changed
- **Downloading and restarting are now two decisions**: A single "Install and
  restart" button asked for both at once, although only the second one
  interrupts the work in progress. A new version now offers **Download** or
  **Later**; once the download is verified, **Apply and restart** does exactly
  that — and *Later* keeps the update on disk, where the next launch picks it
  up anyway.

### Fixed
- **A background result no longer waited for a mouse movement**: The interface
  is painted on demand, and a result arriving through a channel is not an event
  for anyone. The frame that learned the result was also the one with no reason
  left to ask for another, so the window sat invisible until the mouse moved —
  the update found at startup was never announced. Polling now reports that a
  message arrived, and the frame is requested.
- **"You are on the latest version" was impossible to read**: The resting state
  and the answer "I have just looked, there is nothing new" were the same
  value, so the window closed at the very instant the result came in. They are
  two distinct states now, and a check asked for by hand shows its conclusion.

## [0.5.0-beta.3] - 2026-08-30

### Added
- **Send the assembled binary to Desdec**: Desdec — a binary explorer — already
  hands over to ASM Studio: it exports a function to NASM and opens the file
  here. The other direction was missing. *Tools → Send to Desdec*, the
  `Open in Desdec ↗` button in the FORMAT panel, or the command palette now
  assembles the current source and opens the binary produced in Desdec:
  sections and entropy, strings, import table, full disassembly. Both targets
  travel — Desdec reads a PE as it reads an ELF, and that is the best ASM
  Studio has to offer of an `.exe` it cannot run itself. The source is always
  re-assembled first, so what is read over there is what is on screen here.
  Desdec is installed separately: when its executable is on neither the `PATH`
  nor `~/.local/bin`, the console says so, and says where to put it, instead of
  failing in silence.

## [0.5.0-beta.2] - 2026-08-30

### Fixed
- **`Tab` on a selection that fits on one line**: It pushed the whole line
  aside instead of replacing what was selected. The condition guarding the two
  behaviours asked for an *empty* selection where its own comment promised
  "a selection held in one line". The criterion is now the line break rather
  than the line number — a whole line taken *with* its `\n` still counts as a
  single line internally, and has to be shifted, not erased.
- **Tabs in a file written elsewhere**: egui renders `\t` with a fixed
  four-space advance and no notion of tab stop, so a source indented with tabs
  showed its columns out of line — worse still on a file that mixed tabs and
  spaces. Tabs are now expanded to the next stop when the file is opened, and
  the console says so rather than doing it in silence.
- **Abyss dressed its windows in the wrong blue**: Settings and the Calculator
  wore `editorWidget.background` (`#262641`), which in VS Code dresses the small
  find box floating *over* the editor, where being lighter is the whole point.
  On full-size windows it became the lightest surface of the entire palette —
  lighter than `faint` and than `surface`, something no other theme in the
  catalogue does. Windows now take the side panel's background, and are still
  told apart by their border, their rounding and their shadow.

## [0.5.0-beta.1] - 2026-08-30

### Added
- **The Abyss theme**: VS Code's theme, carried over from its official file —
  the `#000C18` night blue of the editor, the `#060621` of the side panels, the
  `#08286B` of the selected row, the `#DDBB88` gold of the cursor. Two
  deliberate departures, both on syntax colouring: Abyss paints keywords at
  `#225588`, a contrast of 2.5:1 against its own background, so directives are
  lifted towards the theme's foreground blue; and mnemonics take `#9966B8`
  (Abyss's `support.function`) — in assembly, the instruction cannot be the
  dullest word on the line.
- **A real tree in the explorer**: Folders expand and collapse in place instead
  of replacing the displayed root. `←`/`→` collapse and expand — or step out to
  the parent, or into the first child — and `↑`/`↓` now walk the tree *as it is
  displayed*. They only ever saw the root before, and skipped over everything
  the mouse had unfolded.
- **A context menu worth the name**: open, expand, use as root, new file *here*,
  new folder *here*, rename, copy path, delete.
- **Explorer rows that read as a tree**: a full-width selection band with an
  accent edge, indentation guides, chevrons and icons drawn as vectors (no
  glyph that shifts with the system font), long names elided rather than cut,
  and a marker on the file currently open in the editor.

### Fixed
- **Renaming in the explorer validates with `Enter`**: The field asked for the
  focus on *every* frame, so it never lost it, so `lost_focus()` never fired and
  `Enter` was never seen. The focus is asked once. `Escape` gives up, clicking
  away validates, and the name's stem — not its extension — comes preselected.
- **Saving a file after renaming it**: The rename rebuilt every related path
  with `to.join(tail)`; for the renamed entry itself `tail` is empty, and
  `join("")` appends a separator. `src_path` became `file.asm/`, which the file
  system reads as a *folder* — every write failed with “Is a directory”, while
  “Save As…”, starting from the path handed over by the native dialog, kept
  working. Two `PathBuf` differing only by that separator compare equal, which
  is why nothing had caught it.
- **The mouse no longer behaves erratically when the focus changes**: while a
  rename was open, the explorer took the focus back from whatever field had just
  been clicked. And the keys of the floating windows — breakpoint condition, go
  to line, new project, calculator, palette, program input — were shared with the
  panel behind them: the arrows moved a selection there, `Enter` opened a file
  and `Escape` stopped the program.
- **A new folder is created in the folder that asked for it**, and the dialog
  says why a name is refused before you press “Create”, instead of reporting the
  failure to the console afterwards.
- **Renaming an expanded folder no longer collapses it.**

## [0.4.9-beta.1] - 2026-08-13

### Added
- **A “Learn” menu**: The guided path, its exercises and its progress used to be
  scattered across three unrelated menus — the tutorial under *Help*, the
  exercises under *File*, the progress under *Preferences*. Nobody looks for an
  exercise under *File*. They now share one top-level menu, which also names the
  lesson “Resume” would reopen and shows how far along the path you are.
- **Multi-file projects**: An `asmstudio.toml` gathers the entry point, the NASM
  sources and the `%include` directories; on Linux, ASM Studio assembles every
  source and links them together. `Ctrl+Shift+N` creates one.

### Changed
- **Learning mode and the guided path are now one state**: They were two
  independent flags with nothing keeping them in agreement, so the status bar
  could announce “Learning” while the ✦ panel no longer offered the path at all.
  The mode carries the path, and the mode label — shown in both modes now, not
  only in Learning — switches it on click.
- **The path is the tab you see in Learning mode**: It shared its band with
  *Instruction*, placed in front of it, so the mode meant for beginners opened
  on “Run the program, then click an instruction” — addressed to someone who has
  no program yet.
- **Full mode no longer carries the tutorial panel**: It belongs to Learning
  mode. It still reopens on its own as soon as a file declares expectations.
- **The contents fit on screen**: The four levels are collapsible, only the one
  you are on opens, and the list is no longer squeezed into a fixed 260 px.
  The exercise count (`· 2 ✎`) opens the lesson that carries them.

### Fixed
- **Asking for the welcome banner again no longer costs you your layout**: It
  went through the mode switch, which replaces the whole panel tree.
- **The `;@` directive reference stays out of a beginner's way**: It used to
  unfold under the path — an exercise *author's* topic — for someone who had not
  written a `mov` yet.

## [0.4.8-beta.9] - 2026-08-12

### Added
- **The file explorer can now be edited**: Rename in place with `F2`, create a
  sub-folder, delete an entry — each from the tree itself or the command
  palette. A name that would leave the displayed folder is refused, and the
  open file, the selection and the recent paths follow a rename, including
  when a parent folder is the one being renamed.
- **IDE keyboard shortcuts**: Find and replace (`Ctrl+F`, `Ctrl+H`, `F3`,
  `Shift+F3`) stay active while the editor has focus. Labels fold with
  `Ctrl+Shift+[` and unfold with `Ctrl+Shift+]`, `Ctrl+1`…`Ctrl+4` toggle the
  layout panels, `Ctrl+5` the predict-the-value mode and `Ctrl+Alt+T` the
  toolbar.
- **The four essential Windows examples**: `win_hello_world.asm`,
  `win_arithmetic.asm`, `win_boucle.asm` and `win_lire_ecrire.asm` ship with
  the archive and are installed into the workspace, so PE64 is usable without
  writing an import table by hand. Each one is checked as a real console PE64,
  and run under Wine when Wine is available.
- **Repository link in the About dialog**: The project's GitHub repository is
  now one click away from “About”.

### Changed
- **Build and debugger messages speak the interface language**: What NASM and
  `ld` write themselves keeps their own wording, but everything ASM Studio says
  around them now follows the language the student picked.
- **The explorer only draws what is visible**: The tree is flattened before
  rendering, so a large folder no longer redraws every row on each frame.
- **Dialog windows share one template**: The fifteen dialogs open centred on the
  workspace rather than where the previous one was left.

### Fixed
- **`install.sh` run through sudo**: The examples are installed into the
  workspace of the user who invoked sudo, not into root's.

## [0.4.8-beta.8] - 2026-08-12

### Added
- **Refined workspace UI**: The menu bar now carries a compact ASM Studio
  identity and the action bar keeps the program state, learning mode and output
  target visible at a glance. Docked-panel tabs gained clear visual markers,
  while cards and section headers now have a more deliberate hierarchy.
- **Signed updater binary**: Releases now carry a raw Linux x86-64 executable
  and its Ed25519 signature for the in-app updater, separately from the manual
  installation archive. The updater explicitly selects that executable, never
  the `.tar.gz` archive.

### Changed
- **The About dialog now states the exact beta**: Version information is again
  a single source of truth. `0.4.8-beta.8` automatically renders as
  “VERSION BÊTA 8” (and its English and Spanish equivalents), so the banner
  cannot drift from the package version.

## [0.4.7] - 2026-08-11

### Added
- **ASCII calculator mode**: The calculator now reads text as the bytes of a
  64-bit register: `Hi` is `0x4869`, with up to eight characters or decoded
  bytes. It accepts `\0`, `\t`, `\n`, `\r`, `\\` and `\xNN` escapes,
  renders non-printable result bytes with the same escapes, and keeps the
  normal arithmetic and bitwise operations available — `a AND \xDF` yields
  `A`.
- **SSE / x87 registers**: The tutorial has been teaching `movdqa xmm0, [rel a]` and `paddd xmm0, xmm1` for as long as it has existed, while the debugger showed only the sixteen general-purpose registers — the one place where the result of those instructions lived was the one place the student could not look. A SSE / FPU panel now reads the XMM registers, the x87 stack and MXCSR from the traced process (`PTRACE_GETFPREGS`), and shows each register the way the instruction reads it: two `double`, four `float`, four 32-bit integers, eight 16-bit integers, sixteen bytes, or raw hex — the low lane first, which is the one `addsd` writes to. Rounding mode and raised exceptions are decoded, with the reminder that those flags are sticky. Registers that changed pulse like the general-purpose ones; the ones still at zero can be hidden.
- **Windows target (PE64)**: The same source can now be assembled as a real Windows executable — `nasm -f win64`, then a linker built into ASM Studio. No `lld-link`, no Microsoft SDK: neither is installable on a student's Linux machine, and depending on them would have meant not offering the feature at all. The linker lays out the sections, resolves `extern ExitProcess` against a catalogue of DLL functions (kernel32, user32, msvcrt — plus `extern gdi32$CreatePen` for anything else), writes a complete import table, builds one `jmp [rip+…]` thunk per imported function, and applies the `REL32`, `REL32_1..5`, `ADDR64` and `ADDR32NB` relocations. Console and GUI subsystems both available from `Run ▸ Target`. The output is checked end to end: the import table is read by binutils, and — when wine is installed — the executable is actually run, printing its text and returning the exit code passed in ECX.
- **Binary format explorer**: A FORMAT panel that opens the binary just produced — header, sections with their address, memory size, file size and permissions, entry point, imported functions and global symbols. ELF and PE are shown through the same structure, which is the point: both answer the same questions, and a student who has understood one has understood three quarters of the other. It is also the only thing the IDE can offer of a `.exe`, since it cannot run one.
- **Running the Windows executable under Wine**: If `wine` is on the `PATH`, `Run` on a Windows target actually runs the `.exe` and its output lands in the same console as a Linux program's, followed by its exit code — input field included, so a program blocked on `ReadFile` waits for you. The process is polled once per frame through non-blocking pipes, so the first run (which creates `~/.wine` and takes seconds) never freezes the interface, and it is killed on `Stop`, on relaunch and when the IDE closes. Without wine, the IDE says so and offers what it can: the FORMAT panel and the disassembly. What stays out of reach either way is single-stepping — the debugger follows `ptrace` and the addresses of the image it just wrote, neither of which survives Wine's loader, and showing registers that are not the program's would be worse than showing none.
- **A Windows example**: `examples/hello-windows.asm`, commented around the three differences that matter — no `syscall`, the Microsoft calling convention (RCX, RDX, R8, R9), and the 32-byte shadow space.

- **Tutorial and exercises are one path again**: Twenty-nine lessons on one side, thirty-six self-checked exercises seeded in a folder on the other, and no link between them — a finished lesson led nowhere, and an opened exercise did not say which notion it belonged to. Each lesson now offers its practice exercises, each exercise links back to the lesson that explains it, and the contents page counts them (✎). Two tests hold the promise: no dead link, and no seeded exercise left out of the path.
- **A Windows path in the tutorial**: Five lessons — first Windows program, the Microsoft calling convention, the shadow space, importing from a DLL, and what an `.exe` holds. They assemble as PE64 and, when Wine is installed, actually run: their expectations bear on the exit code, the only thing observable without a debugger, and a test checks that each starter fails as given and passes once its TODO is applied. The level only appears when Windows assembling is enabled.
- **Windows assembling is now an option**: A setting (and a palette command) decides whether the Windows target is offered at all. Unchecked, the target menu disappears and the assembling goes back to Linux only — an extra target is one more question asked of someone learning assembly, and it should be possible to not ask it. Turning it off while a Windows target is active brings the target back to Linux rather than leaving a state no menu can undo.
- **The tutorial shows how far it goes**: A progress bar over the whole path — lessons done out of the total, and the rank of the one open ("lesson 7 / 29") — in the contents page as well as inside a lesson. The path had no visible length: you knew you were reading a lesson, not whether two or twenty were left. The contents page also offers one button to resume where you stopped, rather than making you find the line again.
- **Validate, and move on**: A lesson used to end on a single "mark as done" checkbox, and nothing said how to reach the next one without going back through the contents. Each lesson now ends on three buttons — previous, validate, next. Validating is not ticking a box: as long as the lesson's program does not satisfy its expectations the button stays inert and says what is missing, and it turns green the moment they all pass. Nobody is stuck either way, since "next" stays open to whoever wants to skip and come back; and validating moves to the lesson that *follows* rather than to the first unfinished one, so resuming a skipped lesson no longer sends you backwards.
- **The status bar names the binary format**: `ELF64` or `PE64`, right next to `NASM`. The same assembler produces one or the other depending on the target, and nothing on screen said which — you had to reopen `Run ▸ Target` to find out what `Build` was about to write. Green when the binary can be single-stepped here, amber when it can only be assembled and read.
- **New files ask which world they belong to**: An ELF skeleton and a PE skeleton do not start with the same lines — `_start` and `syscall` on one side, `main` and `ExitProcess` on the other. Creating a file now asks for its format, lays down the matching skeleton and sets the build target to match, instead of imposing Linux and letting the student discover the mismatch through a nasm error. The question only appears when Windows assembling is enabled, comes after the unsaved-work guard, and a test assembles all three skeletons as given. The explorer got the "new file" button it was missing along the way.
- **Paste button in the license dialog**: The license arrives by e-mail and is pasted in one gesture, without having to click into the field first — nothing said that was needed.
- **A way back to the tutorial**: The welcome banner was the only door to the guided path, and dismissing it closed it for good — the panel hosting the tutorial was called "Exercises", and the word "tutorial" appeared nowhere in the interface. The panel now names both, `Help ▸ Guided tutorial` opens the path, and the welcome screen can be brought back.

### Changed
- **Version numbering follows semver, build included**: `Cargo.toml` carries the full version, prerelease and all (`0.4.7-beta.4`), and `build.rs` appends an incrementing build number (`+build.127`) at every compilation. The beta banner read "BETA" with no number, because the number it wanted lived in a second place that no longer agreed with the first; it now comes from the version and nowhere else, and disappears by itself on a final release. Version comparison was fixed along the way: `0.4.7-beta.4` used to parse as "0.4.4", which could offer an update to an older release.
- **Examples are seeded once per version**: A stamp file records which version seeded the examples folder; while it matches, startup does one read instead of sixty `stat` calls. Files present are still never overwritten.
- Assembling now goes through a target (`Run ▸ Target`, or the command palette), persisted between sessions. On a Windows target, `Run` assembles, disassembles, opens the FORMAT panel and — with Wine installed — runs the program; what it never does is single-step it, and it says why: the debugger speaks `ptrace` and follows the addresses of the image it just wrote, neither of which survives Wine's loader.
- A snapshot carries the floating-point registers, shared between consecutive steps as long as they do not change. A program that never touches an XMM register therefore pays eight bytes per step instead of four hundred — measured at 11.3 µs per instruction, unchanged.

## [0.4.6] - 2026-08-10

### Added
- **System calls explained from their arguments**: A `syscall` used to show a number and a name. `write(fd=1, buf=0x402000, count=13)` says nothing to someone learning: the panel now says what the call is about to do — "writes the 13 bytes starting at address 0x402000 to standard output (the screen)" — then each register with the role it plays *in this call*, the contents of the buffer RSI points at, what RAX will hold on return, and the pitfall when there is one (`count = 0` from a `len` never computed, `exit(256)` the shell reads as `0`, `fork` returning twice, `execve` never returning). Values are interpreted rather than copied: a descriptor becomes "the screen", `open` flags become `O_WRONLY|O_CREAT|O_TRUNC`, a signal number becomes `SIGTERM`. Shown in the INSTRUCTION panel, in the microscope, and on hover in the SYSCALLS log.
- **A library of ~80 system calls**: Named and grouped by family (I/O and files, memory, signals, processes, time, network), each with a one-sentence statement of purpose in all three languages, and about twenty-five decoded argument by argument. Two tests hold the contract: every call that can be named can also be explained, in every language.
- **Program output box**: A window showing what the program writes, and nothing else.
- **Command palette**: The whole application reachable from the keyboard.

### Changed
- The SYSCALLS log keeps each call's registers rather than a pre-written sentence, so its explanations follow the interface language when it changes mid-session.

## [0.4.3-beta.3] - 2026-08-08

### Added
- **Unsaved-work guard**: Four actions used to replace the editor's contents without a word — New, Open (dialog or explorer), loading a lesson, and quitting. None of them looked at whether anything had been typed, so a half-written exercise could vanish for good. Each now goes through one dialog: save, discard, or cancel, with the file name and the number of changed lines. On close, the question comes before the license reminder — the work is the only thing that cannot be given back afterwards.
- **Conditional breakpoints**: A breakpoint can carry a condition — `RCX == 0`, `RAX > 0x100`, `ZF == 1`, `RSI != RDI` — and execution only stops when it holds. Stopping at the four-thousandth turn of a loop used to take four thousand `Continue`. Registers (including their low halves `EAX`, `R8D`), the six flags, and numbers in decimal, hex or binary are all accepted; a conditional breakpoint shows as a ring in the gutter, and the condition itself in a tooltip. Right-click the gutter or press `Ctrl+Shift+F8`.
- **Hover inspection**: Hovering a word in the editor shows what it is worth right now — a register in hex, decimal, signed decimal, as a character, and with the eight bytes it points at when it is an address; a flag with its state; a label with the line where it is defined and its address; a number in all three bases. The answer to “and what does RSI hold at this point?” no longer costs a round trip to another panel.
- **Recent files**: `File ▸ Recent` lists the last ten files opened, most recent first, persisted between sessions. Entries that no longer exist are dropped when the menu opens rather than offered for nothing.

### Changed
- The “modified” state is now derived from the text itself instead of a flag each editing path had to remember to raise — which is exactly how changes went unsignalled, and therefore lost. Undoing your way back to the saved text now clears the `●` marker, as it should.
- `Debugger::run_until` hands the whole register set to its stop condition instead of just RIP, which is what lets a breakpoint condition decide without the debugger knowing anything about its grammar.
- Settings reading and writing are separated from the disk, so the file format is covered by tests instead of being exercised only against the user's real settings.

## [0.4.0-beta.2] - 2026-08-07

### Added
- **Breakpoints**: Click the editor gutter (or `Ctrl+F8`) to mark a line; `Continue` (`F9`) runs straight to it. Marks sit on source lines rather than addresses, so they survive a rebuild that moves the code. A hollow circle flags a breakpoint on a line that carries no code.
- **Step over** (`Shift+F10`): Runs a whole `call` in one go instead of walking through the callee. Both commands keep single-stepping under the hood — every instruction still enters the timeline, which would otherwise have gaps.
- **Real console I/O**: The traced program's stdout and stderr are piped into the IDE's console instead of the parent terminal (invisible when launched from a desktop shortcut), and an input field feeds its stdin. A program blocked on `read` now waits for you: the step is non-blocking, and an interrupted `Continue` resumes on its own once the input arrives.
- **License system**: ASM Studio Personal Free License (ASFL) v1.0 replaces the MIT license. Disassembly, registers/flags and the timeline require an activated license, after a 14-day grace period counted from first launch. The license can be pasted, inspected and deactivated from the About window.
- **Calculator**: Hexadecimal by default, bit-by-bit view, and arithmetic/logic operations.
- **Continuous integration**: A GitHub Actions workflow checks build, clippy (`-D warnings`) and tests on every push.

### Changed
- **Distribution binary down from 22.8 MB to 12.4 MB**: capstone now builds x86 only (it was compiling all eighteen architectures it knows), and the release profile uses fat LTO, one codegen unit, symbol stripping and `opt-level = "s"`. `panic = "abort"` was deliberately left out: file dialogs and the update check run on background threads, where a panic currently kills only the thread instead of taking the unsaved source down with the IDE.
- `F1` toggles the shortcut help instead of only opening it; all shortcuts were reviewed for conflicts.
- The Exercise and Tutorial panels are merged into one.
- Stack/Heap views render as cards, and register chips are harmonized with the rest of the interface.

### Fixed
- Disassembly now uses its full width, without a stray rule along the top.

### Performance
- A step no longer costs an allocation and an `open`+`close` on `/proc/pid/mem`: the stack window is an inline array and the file is opened once per traced process.
- Call stack and syscall log are now built incrementally. Rebuilding them from scratch on every step made a full run quadratic in the number of instructions.
- Address-to-instruction lookups go through an index instead of scanning the whole disassembly.
- `Continue` and the history are both bounded, so an infinite loop hands control back to the interface rather than freezing the IDE or filling memory.

## [0.4.0-beta.1] - 2026-08-02

### Added
- **Tutorial module**: A guided path of 29 lessons over four levels — 9 beginner, 8 intermediate, 6 advanced, 6 expert — wired to the IDE's own panels: a lesson opens what it talks about. Progress is persisted, and can be reset from Settings.
- **Self-checked exercises**: Expected results declared in the source itself, with `;@interdit` / `;@requis` text constraints, plus a seeded set of exercise files. The File menu opens the examples folder, and missing examples are re-seeded on demand.
- **Predict before you reveal**: Guess a register or flag before stepping; a wrong prediction is explained in detail in a dedicated floating window.
- **Plain-language crash diagnosis**: A hardware fault is analyzed on the spot and explained, instead of leaving RIP frozen in silence.
- **Learning / Full display modes**: Learning keeps the essentials and opens the Tutorial by default, with a lighter toolbar and status bar and a welcome banner; Full shows everything.
- **Dockable, detachable panels** (egui_dock), with the layout persisted between sessions.
- **Full keyboard navigation**: The whole interface is drivable from the keyboard, including the memory, memory view and disassembly panels.
- **Teaching content**: The System V ABI (call frame and register roles), little-endianness in the memory view, and broader instruction coverage in `explain.rs`.
- **Microscope**: Machine encoding, effects and context for a single instruction, with a link to the Intel reference.
- **Internationalization**: French/English/Spanish across the interface, dialogs, status messages and instruction explanations, with the language persisted.
- **asmstd**: Completed, documented, and verified at run time.
- Multi-base calculator with negative decimals, syscall identification, and a visible exit code.
- Quick-start guide (`doc/GUIDE-DEMARRAGE-RAPIDE.md`) and an install/release build directory.

### Changed
- Upgraded to egui/eframe/egui_dock 0.33 (from 0.29); non-UI dependencies moved to their latest stable versions.
- Native GNOME file dialogs through the XDG portal (rfd), without GTK.
- Tree-style file explorer, in the manner of an IDE.
- Interface brought in line with the design mockup: accent toolbar, syscall badges, panel title bars, cards, rounded tabs, themed icons, softer colors and a much more discreet focus ring.
- Registers laid out in three columns, flags as cards; FLAGS moved to the bottom of the INSTRUCTION panel.
- `src/app.rs` (3128 lines) split into seven submodules, with the pedagogy mode extracted to `src/app/pedagogy.rs`.

### Fixed
- File dialogs no longer block the UI thread — the end of the "not responding" freeze.
- Zombie processes left behind on relaunch, and the Kill action now targets the right PID.
- `.rodata` and `.data`/`.bss` were unreadable in the memory view.
- SYSCALLS vanished when the PREDICTION column appeared; both panels gained horizontal scrollbars.
- The call stack and syscall trace are rebuilt from a single source of truth.
- Double close buttons on every panel (an egui_dock 0.18 regression), and a Tutorial close button that did nothing.
- Focus ring drawn over floating windows, scrolling that failed to follow the cursor, and a rendering pass missing when a dialog closed.
- Text contrast in the light theme, and scrollbars that overlapped panel content.

## [0.3.0] - 2026-07-25

### Added
- **Mode « CPU vivant »**: Real-time pulsation of modified registers and flags during execution steps.
- **Animation indicators**: Pulse badges for PUSH/POP operations in the stack view based on RSP changes.
- **Animations Toggle**: New "Animations" setting to enable or disable visual effects (persisted).
- **Memory Laboratory**: Ability to edit registers (ptrace SETREGS) and memory (/proc/pid/mem) directly while the debugger is paused.
- **Interactive Register Editing**: Click on register values in the UI to edit them using hexadecimal input.
- **Memory Writing**: Added a form to write bytes at specific memory addresses in the Memory panel.

## [0.2.1] - 2026-07-25

### Added
- **Settings Persistence**: Theme, tooltips, and asmstd settings are now saved to `~/.config/asm_studio/settings.conf`.
- **asmstd Library**: Included `asmstd.inc` for common syscall wrappers (write, read, exit, etc.) with a toggle in settings.
- **Heap Tab**: Dedicated tab to view the memory heap segment (read from `/proc/pid/maps`).
- **Shortcut Tooltips**: Added an option to show/hide keyboard shortcut tooltips in the toolbar and timeline.
- **Modernized Dialogs**: Improved Open/Save file browsers with a breadcrumb path and better layout.

### Fixed
- **Directory Creation**: Automatically create parent directories if they are missing when saving files.
- **Timeline Stability**: Fixed the timeline slider jumping by giving it a dedicated full-width line.
- **Editor Scrolling**: Added horizontal scrolling support to the editor while keeping line numbers fixed.
- **Layout Improvements**: Added vertical separators between Memory and Console panels.
- **Register View**: Added scrolling to the Registers panel to handle overflow.

[0.3.0]: https://github.com/fred/asm_studio/compare/v0.2.1...v0.3.0
[0.2.1]: https://github.com/fred/asm_studio/compare/v0.2.0...v0.2.1
