# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

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
