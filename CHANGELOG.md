# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

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
