//! Désassemblage de la section `.text` d'un binaire ELF64 via Capstone.

use std::path::Path;

use capstone::prelude::*;
use object::{Object, ObjectSection};

/// Une instruction machine décodée, prête à l'affichage.
pub struct Insn {
    pub address: u64,
    pub bytes: Vec<u8>,
    pub mnemonic: String,
    pub operands: String,
}

impl Insn {
    /// Octets formatés en hexa (« B8 05 00 00 00 »), comme dans la maquette.
    pub fn bytes_hex(&self) -> String {
        self.bytes
            .iter()
            .map(|b| format!("{b:02X}"))
            .collect::<Vec<_>>()
            .join(" ")
    }
}

/// Désassemble la section `.text` du binaire (syntaxe Intel).
pub fn disassemble_text(binary: &Path) -> Result<Vec<Insn>, String> {
    let data = std::fs::read(binary).map_err(|e| format!("lecture {}: {e}", binary.display()))?;
    let file = object::File::parse(&*data).map_err(|e| format!("parse ELF: {e}"))?;

    let text = file
        .sections()
        .find(|s| s.name() == Ok(".text"))
        .ok_or_else(|| "section .text introuvable".to_string())?;
    let addr = text.address();
    let code = text.data().map_err(|e| format!("données .text: {e}"))?;

    let cs = Capstone::new()
        .x86()
        .mode(arch::x86::ArchMode::Mode64)
        .syntax(arch::x86::ArchSyntax::Intel)
        .detail(false)
        .build()
        .map_err(|e| format!("init capstone: {e}"))?;

    let insns = cs
        .disasm_all(code, addr)
        .map_err(|e| format!("désassemblage: {e}"))?;

    Ok(insns
        .iter()
        .map(|i| Insn {
            address: i.address(),
            bytes: i.bytes().to_vec(),
            mnemonic: i.mnemonic().unwrap_or("").to_string(),
            operands: i.op_str().unwrap_or("").to_string(),
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::assemble;

    #[test]
    fn first_instruction_is_mov_rax_5() {
        let out = assemble::assemble(Path::new("examples/test.asm"), Path::new("build"))
            .expect("assemblage");
        let insns = disassemble_text(&out.binary).expect("désassemblage");
        let first = &insns[0];
        assert_eq!(first.mnemonic, "mov");
        // NASM encode `mov rax, 5` en `mov eax, 5` (B8) : écrire eax remet à zéro
        // les 32 bits hauts de rax. Le désassemblage reflète donc l'encodage réel.
        assert_eq!(first.operands, "eax, 5");
        assert_eq!(first.bytes_hex(), "B8 05 00 00 00");
    }
}
