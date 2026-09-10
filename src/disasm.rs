//! Désassemblage de la section `.text` d'un binaire ELF64 via Capstone.

use std::path::Path;

use capstone::prelude::*;
use object::{Object, ObjectSection};

/// Une instruction machine décodée, prête à l'affichage.
#[derive(Clone)]
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
    disassemble_text_from(binary, 0)
}

/// Comme [`disassemble_text`], mais en décodant le code comme s'il était
/// chargé `bias` octets plus loin que ne le disent ses en-têtes — le cas d'un
/// exécutable position-indépendant (voir
/// [`crate::debugger::Debugger::load_bias`]).
///
/// Décaler après coup les adresses rendues ne suffirait pas : une cible de saut
/// est imprimée *dans* l'opérande (« jne 0x167 »), et resterait à l'adresse du
/// lien au milieu d'un listing affiché ailleurs. Capstone la calcule à partir de
/// l'adresse qu'on lui donne : c'est donc là qu'il faut le dire.
pub fn disassemble_text_from(binary: &Path, bias: u64) -> Result<Vec<Insn>, String> {
    let data = std::fs::read(binary).map_err(|e| format!("lecture {}: {e}", binary.display()))?;
    let file = object::File::parse(&*data).map_err(|e| format!("parse ELF: {e}"))?;

    let text = file
        .sections()
        .find(|s| s.name() == Ok(".text"))
        .ok_or_else(|| "section .text introuvable".to_string())?;
    let addr = text.address().wrapping_add(bias);
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

/// Désassemble au plus `count` instructions à partir de l'adresse virtuelle
/// `addr`, en repartant du code brut.
///
/// [`disassemble_text`] balaie `.text` d'un bout à l'autre, et ce balayage se
/// désynchronise sur le bourrage qui sépare les fonctions : les octets nuls
/// d'alignement se décodent en instructions, et les vraies instructions qui
/// suivent tombent alors à des adresses décalées. Pour aller regarder ce qui se
/// trouve *exactement* à une adresse connue — la cible d'un `call`, par
/// exemple — il faut redémarrer le décodage à cette adresse-là.
pub fn disassemble_at(binary: &Path, addr: u64, count: usize) -> Vec<Insn> {
    let Ok(data) = std::fs::read(binary) else {
        return Vec::new();
    };
    let Ok(file) = object::File::parse(&*data) else {
        return Vec::new();
    };
    let Some(text) = file.sections().find(|s| s.name() == Ok(".text")) else {
        return Vec::new();
    };
    let Ok(code) = text.data() else {
        return Vec::new();
    };
    let Some(offset) = addr.checked_sub(text.address()) else {
        return Vec::new();
    };
    let Some(from) = code.get(offset as usize..) else {
        return Vec::new();
    };

    let Ok(cs) = Capstone::new()
        .x86()
        .mode(arch::x86::ArchMode::Mode64)
        .syntax(arch::x86::ArchSyntax::Intel)
        .detail(false)
        .build()
    else {
        return Vec::new();
    };
    let Ok(insns) = cs.disasm_count(from, addr, count) else {
        return Vec::new();
    };
    insns
        .iter()
        .map(|i| Insn {
            address: i.address(),
            bytes: i.bytes().to_vec(),
            mnemonic: i.mnemonic().unwrap_or("").to_string(),
            operands: i.op_str().unwrap_or("").to_string(),
        })
        .collect()
}

/// Adresse virtuelle du point d'entrée du binaire.
///
/// ELF comme PE la portent dans leur en-tête, et c'est une adresse complète des
/// deux côtés (image base comprise pour le PE) : de quoi retrouver la première
/// instruction exécutée dans le désassemblage.
pub fn entry_address(binary: &Path) -> Option<u64> {
    let data = std::fs::read(binary).ok()?;
    let file = object::File::parse(&*data).ok()?;
    Some(file.entry())
}

/// Adresse virtuelle de début d'une section (ex. `.data`), si présente.
pub fn section_address(binary: &Path, name: &str) -> Option<u64> {
    let data = std::fs::read(binary).ok()?;
    let file = object::File::parse(&*data).ok()?;
    file.sections()
        .find(|s| s.name() == Ok(name))
        .map(|s| s.address())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::assemble;

    #[test]
    fn first_instruction_is_mov_rax_5() {
        let out = assemble::assemble_with_includes(Path::new("examples/test.asm"), Path::new("build/test-disasm"), &[])
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
