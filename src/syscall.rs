//! Décodage lisible des appels système Linux x86-64.
//!
//! Convention : RAX = numéro, arguments dans RDI, RSI, RDX, R10, R8, R9,
//! valeur de retour dans RAX.

use crate::debugger::Registers;

/// Nom de l'appel système d'après son numéro (les plus courants).
pub fn name(num: u64) -> &'static str {
    match num {
        0 => "read",
        1 => "write",
        2 => "open",
        3 => "close",
        8 => "lseek",
        9 => "mmap",
        10 => "mprotect",
        11 => "munmap",
        12 => "brk",
        60 => "exit",
        231 => "exit_group",
        _ => "syscall",
    }
}

/// Formate l'appel tel qu'il est SUR LE POINT de s'exécuter (registres avant).
pub fn format_call(regs: &Registers) -> String {
    match regs.rax {
        0 => format!("read(fd={}, buf=0x{:X}, count={})", regs.rdi, regs.rsi, regs.rdx),
        1 => format!("write(fd={}, buf=0x{:X}, count={})", regs.rdi, regs.rsi, regs.rdx),
        2 => format!("open(path=0x{:X}, flags={})", regs.rdi, regs.rsi),
        3 => format!("close(fd={})", regs.rdi),
        60 => format!("exit({})", regs.rdi),
        231 => format!("exit_group({})", regs.rdi),
        n => format!("{}(rdi={}, rsi={}, rdx={})", name(n), regs.rdi, regs.rsi, regs.rdx),
    }
}

/// Vrai si l'appel termine le processus (pas de valeur de retour à afficher).
pub fn is_exit(num: u64) -> bool {
    num == 60 || num == 231
}
