//! Racine de bibliothèque du crate, introduite pour que l'agent embarqué dans
//! la VM macOS ([`bin/asmstudio-agent`](../bin/asmstudio_agent.rs)) puisse
//! réutiliser [`debugger`] tel quel — même moteur ptrace, exécuté dans un
//! vrai Linux invité plutôt que réimplémenté. `main.rs` ne fait plus que
//! consommer ce crate ; le découpage en modules n'a pas changé.
//!
//! Le feature `vm-agent` exclut tout ce dont l'agent n'a pas besoin (l'UI
//! egui et ses dépendances lourdes) : sans lui, ce fichier expose exactement
//! l'arborescence de modules qu'avait `main.rs` avant ce découpage — même
//! comportement, même code, sur Linux comme avant.

#[cfg(not(feature = "vm-agent"))]
pub mod abi;
#[cfg(not(feature = "vm-agent"))]
pub mod app;
#[cfg(not(feature = "vm-agent"))]
pub mod assemble;
#[cfg(not(feature = "vm-agent"))]
pub mod binfmt;
pub mod breakpoint;
// Le moteur ptrace : Linux uniquement, aussi bien pour le binaire principal
// (natif sur Linux) que pour l'agent (compilé pour la cible croisée
// x86_64-unknown-linux-musl, où `target_os` vaut "linux" quelle que soit la
// machine hôte qui compile). Absent du binaire principal sur macOS — c'est
// justement ce qui manque pour qu'il compile là-bas aujourd'hui.
#[cfg(target_os = "linux")]
pub mod debugger;
#[cfg(not(feature = "vm-agent"))]
pub mod desdec;
#[cfg(not(feature = "vm-agent"))]
pub mod diagnostic;
#[cfg(not(feature = "vm-agent"))]
pub mod disasm;
#[cfg(not(feature = "vm-agent"))]
pub mod effects;
#[cfg(not(feature = "vm-agent"))]
pub mod encoding;
#[cfg(not(feature = "vm-agent"))]
pub mod exercise;
#[cfg(not(feature = "vm-agent"))]
pub mod explain;
pub mod i18n;
#[cfg(not(feature = "vm-agent"))]
pub mod license;
#[cfg(not(feature = "vm-agent"))]
pub mod pe_link;
#[cfg(not(feature = "vm-agent"))]
pub mod project;
#[cfg(not(feature = "vm-agent"))]
pub mod simd;
#[cfg(not(feature = "vm-agent"))]
pub mod srcmap;
#[cfg(not(feature = "vm-agent"))]
pub mod stack_check;
#[cfg(not(feature = "vm-agent"))]
pub mod syntax;
#[cfg(not(feature = "vm-agent"))]
pub mod syscall;
#[cfg(not(feature = "vm-agent"))]
pub mod theme;
#[cfg(not(feature = "vm-agent"))]
pub mod trial;
#[cfg(not(feature = "vm-agent"))]
pub mod tutorial;
#[cfg(not(feature = "vm-agent"))]
pub mod updater;
#[cfg(not(feature = "vm-agent"))]
pub mod version;
// Format d'échange RPC hôte ↔ agent : pures structures de données (aucune
// dépendance à `debugger`/ptrace), compilée dans les deux sens — utile au
// binaire principal macOS (client) comme à l'agent (serveur).
pub mod vm_protocol;
// Client RPC + DTOs miroir de `debugger` : uniquement pour le binaire
// principal sur macOS (c'est lui qui a besoin de parler à la VM). Ni sur
// Linux (le chemin natif suffit), ni pour l'agent (qui est le serveur, pas
// le client, et tient un vrai `debugger::Debugger`).
#[cfg(all(target_os = "macos", not(feature = "vm-agent")))]
pub mod vm_debugger;
// Cycle de vie du processus QEMU : même restriction que `vm_debugger`.
#[cfg(all(target_os = "macos", not(feature = "vm-agent")))]
pub mod vm_session;
#[cfg(not(feature = "vm-agent"))]
pub mod win_debugger;
#[cfg(not(feature = "vm-agent"))]
pub mod winerun;
