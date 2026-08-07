//! Découpage de l'encodage machine d'une instruction x86-64.
//!
//! `48 89 e5` n'est pas une suite opaque : c'est `REX.W` + opcode `89`
//! (« MOV r/m64, r64 ») + ModR/M `e5` (mod=11, reg=100=RSP, rm=101=RBP), soit
//! `mov rbp, rsp`. Voir ce découpage, c'est comprendre pourquoi la même
//! instruction s'écrit différemment selon les registres qu'elle touche.
//!
//! Le décodage exploite une propriété commode : la LONGUEUR totale est connue
//! (le désassembleur l'a donnée). Préfixes, REX, opcode, ModR/M, SIB et
//! déplacement se déduisent de proche en proche ; tout ce qui reste est
//! l'immédiat. Cela évite d'embarquer la table des tailles d'immédiat, qui
//! serait énorme pour un gain pédagogique nul.

use crate::i18n::{self, Lang};

/// Rôle d'un octet dans l'encodage.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Part {
    /// Préfixe hérité : taille d'opérande, verrou, répétition, segment…
    LegacyPrefix,
    /// Préfixe REX (0x40–0x4F) : accès aux registres 64 bits et à r8–r15.
    Rex,
    /// Octet(s) d'opcode, échappements 0F / 0F 38 / 0F 3A compris.
    Opcode,
    /// ModR/M : mode d'adressage + deux champs de registre.
    ModRm,
    /// SIB : base, index et facteur d'échelle d'une adresse complexe.
    Sib,
    /// Déplacement ajouté à l'adresse calculée.
    Displacement,
    /// Valeur immédiate encodée dans l'instruction.
    Immediate,
}

impl Part {
    pub fn label(self, lang: Lang) -> &'static str {
        match self {
            Part::LegacyPrefix => i18n::tr3(lang, "Préfixe", "Prefix", "Prefijo"),
            Part::Rex => "REX",
            Part::Opcode => i18n::tr3(lang, "Opcode", "Opcode", "Opcode"),
            Part::ModRm => "ModR/M",
            Part::Sib => "SIB",
            Part::Displacement => i18n::tr3(lang, "Déplacement", "Displacement", "Desplazamiento"),
            Part::Immediate => i18n::tr3(lang, "Immédiat", "Immediate", "Inmediato"),
        }
    }
}

/// Un morceau décodé : ses octets, son rôle, et ce qu'il signifie ici.
#[derive(Debug, Clone)]
pub struct Field {
    pub part: Part,
    pub bytes: Vec<u8>,
    /// Détail propre à CETTE occurrence, ex. « W=1 : opérandes 64 bits ».
    pub detail: String,
}

/// Encodage complet, dans l'ordre des octets.
#[derive(Debug, Clone, Default)]
pub struct Encoding {
    pub fields: Vec<Field>,
    /// Vrai si un octet n'a pas pu être attribué (encodage exotique).
    pub incomplete: bool,
}

impl Encoding {
    /// Somme des octets attribués — doit valoir la longueur de l'instruction.
    pub fn covered(&self) -> usize {
        self.fields.iter().map(|f| f.bytes.len()).sum()
    }
}

/// Noms des 16 registres 64 bits, indexés par le numéro encodé.
const REG64: [&str; 16] = [
    "rax", "rcx", "rdx", "rbx", "rsp", "rbp", "rsi", "rdi",
    "r8", "r9", "r10", "r11", "r12", "r13", "r14", "r15",
];

/// Opcodes d'un octet qui n'ont PAS de ModR/M.
///
/// Le reste en a un. Cette liste couvre ce qu'un programme d'apprentissage
/// rencontre ; un encodage exotique sera simplement marqué incomplet plutôt
/// que mal découpé.
fn one_byte_without_modrm(op: u8) -> bool {
    matches!(op,
        0x50..=0x5F        // push/pop r64
        | 0xB0..=0xBF      // mov r, imm
        | 0x70..=0x7F      // jcc rel8
        | 0x68 | 0x6A      // push imm
        | 0xE8 | 0xE9 | 0xEB   // call/jmp relatifs
        | 0xC3 | 0xC2      // ret
        | 0xC9             // leave
        | 0x90             // nop
        | 0xCC | 0xCE      // int3, into
        | 0x98 | 0x99      // cwde/cdqe, cdq/cqo
        | 0x9C | 0x9D      // pushf, popf
        | 0xF4             // hlt
        | 0xFC | 0xFD      // cld, std
        | 0xEC..=0xEF      // in/out
        | 0xA8 | 0xA9      // test al/eax, imm
        // Formes « accumulateur, immédiat » : 04/0C/14/1C/24/2C/34/3C et
        // 05/0D/15/1D/25/2D/35/3D.
        | 0x04 | 0x0C | 0x14 | 0x1C | 0x24 | 0x2C | 0x34 | 0x3C
        | 0x05 | 0x0D | 0x15 | 0x1D | 0x25 | 0x2D | 0x35 | 0x3D
    )
}

/// Opcodes à deux octets (après 0F) sans ModR/M.
fn two_byte_without_modrm(op: u8) -> bool {
    matches!(op,
        0x05            // syscall
        | 0x0B          // ud2
        | 0x30..=0x37   // wrmsr, rdtsc, rdmsr…
        | 0x80..=0x8F   // jcc rel32
        | 0xA2          // cpuid
        | 0xC8..=0xCF   // bswap
    )
}

/// Décode l'encodage d'une instruction.
///
/// `bytes` doit être la séquence complète telle que produite par le
/// désassembleur. Un octet non attribuable marque l'encodage `incomplete`
/// plutôt que de produire un découpage faux.
pub fn decode(bytes: &[u8], lang: Lang) -> Encoding {
    let t = |fr: &'static str, en: &'static str, es: &'static str| i18n::tr3(lang, fr, en, es);
    let mut enc = Encoding::default();
    if bytes.is_empty() {
        return enc;
    }
    let mut i = 0usize;

    // ---- 1. Préfixes hérités (dans n'importe quel ordre, avant REX) ----
    while i < bytes.len() {
        let b = bytes[i];
        let detail = match b {
            0x66 => t(
                "Taille d'opérande : bascule en 16 bits (ou sélectionne une variante SSE).",
                "Operand size: switches to 16-bit (or selects an SSE variant).",
                "Tamaño de operando: cambia a 16 bits (o selecciona una variante SSE).",
            ),
            0x67 => t(
                "Taille d'adresse : calcule l'adresse sur 32 bits au lieu de 64.",
                "Address size: computes the address in 32 bits instead of 64.",
                "Tamaño de dirección: calcula la dirección en 32 bits en vez de 64.",
            ),
            0xF0 => t(
                "LOCK : rend l'opération atomique vis-à-vis des autres cœurs.",
                "LOCK: makes the operation atomic with respect to other cores.",
                "LOCK: hace la operación atómica frente a otros núcleos.",
            ),
            0xF2 => t(
                "REPNE : répète tant que ZF=0 (ou sélectionne une variante SSE).",
                "REPNE: repeats while ZF=0 (or selects an SSE variant).",
                "REPNE: repite mientras ZF=0 (o selecciona una variante SSE).",
            ),
            0xF3 => t(
                "REP : répète RCX fois (ou sélectionne une variante SSE).",
                "REP: repeats RCX times (or selects an SSE variant).",
                "REP: repite RCX veces (o selecciona una variante SSE).",
            ),
            0x2E | 0x36 | 0x3E | 0x26 => t(
                "Préfixe de segment (sans effet en mode 64 bits).",
                "Segment prefix (no effect in 64-bit mode).",
                "Prefijo de segmento (sin efecto en modo 64 bits).",
            ),
            0x64 => t(
                "Segment FS : sert notamment au stockage par thread.",
                "FS segment: used for thread-local storage.",
                "Segmento FS: usado para almacenamiento por hilo.",
            ),
            0x65 => t(
                "Segment GS : sert notamment au stockage par thread.",
                "GS segment: used for thread-local storage.",
                "Segmento GS: usado para almacenamiento por hilo.",
            ),
            _ => break,
        };
        enc.fields.push(Field { part: Part::LegacyPrefix, bytes: vec![b], detail: detail.to_string() });
        i += 1;
    }

    // ---- 2. REX ----
    let rex_w;
    let mut rex_r = false;
    let mut rex_x = false;
    let mut rex_b = false;
    let has_rex = i < bytes.len() && (0x40..=0x4F).contains(&bytes[i]);
    if has_rex {
        let b = bytes[i];
        rex_w = b & 0b1000 != 0;
        rex_r = b & 0b0100 != 0;
        rex_x = b & 0b0010 != 0;
        rex_b = b & 0b0001 != 0;
        let mut d = format!(
            "W={} R={} X={} B={} — ",
            rex_w as u8, rex_r as u8, rex_x as u8, rex_b as u8
        );
        d.push_str(if rex_w {
            t("opérandes 64 bits", "64-bit operands", "operandos de 64 bits")
        } else {
            t("opérandes 32 bits", "32-bit operands", "operandos de 32 bits")
        });
        if rex_r || rex_x || rex_b {
            d.push_str(t(
                " ; étend les numéros de registre à r8–r15",
                "; extends register numbers to r8–r15",
                "; extiende los números de registro a r8–r15",
            ));
        }
        enc.fields.push(Field { part: Part::Rex, bytes: vec![b], detail: d });
        i += 1;
    } else {
        rex_w = false;
    }
    let _ = rex_w; // sert à l'explication de REX, pas au découpage

    // ---- 3. Opcode (1 à 3 octets) ----
    if i >= bytes.len() {
        enc.incomplete = true;
        return enc;
    }
    let op_start = i;
    let has_modrm;
    if bytes[i] == 0x0F {
        i += 1;
        if i < bytes.len() && matches!(bytes[i], 0x38 | 0x3A) {
            i += 1; // échappement à trois octets : toujours un ModR/M
            has_modrm = true;
            i += 1;
        } else if i < bytes.len() {
            has_modrm = !two_byte_without_modrm(bytes[i]);
            i += 1;
        } else {
            enc.incomplete = true;
            return enc;
        }
    } else {
        has_modrm = !one_byte_without_modrm(bytes[i]);
        i += 1;
    }
    let op_bytes = bytes[op_start..i].to_vec();
    let op_detail = if op_bytes.first() == Some(&0x0F) {
        t(
            "L'octet 0F est un échappement : il ouvre une seconde table d'opcodes.",
            "The 0F byte is an escape: it opens a second opcode table.",
            "El byte 0F es un escape: abre una segunda tabla de opcodes.",
        )
        .to_string()
    } else {
        t(
            "Désigne l'opération. Ses bits de poids faible encodent souvent la taille et le sens des opérandes.",
            "Designates the operation. Its low bits often encode operand size and direction.",
            "Designa la operación. Sus bits bajos suelen codificar tamaño y sentido de los operandos.",
        )
        .to_string()
    };
    enc.fields.push(Field { part: Part::Opcode, bytes: op_bytes, detail: op_detail });

    // ---- 4. ModR/M ----
    let mut modrm_mod = 0u8;
    let mut rm = 0u8;
    if has_modrm {
        if i >= bytes.len() {
            enc.incomplete = true;
            return enc;
        }
        let b = bytes[i];
        modrm_mod = b >> 6;
        let reg = (b >> 3) & 0b111;
        rm = b & 0b111;
        let reg_full = reg | ((rex_r as u8) << 3);
        let rm_full = rm | ((rex_b as u8) << 3);
        let mode_txt = match modrm_mod {
            0b11 => t("mod=11 : les deux opérandes sont des registres", "mod=11: both operands are registers", "mod=11: ambos operandos son registros"),
            0b00 => t("mod=00 : opérande mémoire, sans déplacement", "mod=00: memory operand, no displacement", "mod=00: operando en memoria, sin desplazamiento"),
            0b01 => t("mod=01 : opérande mémoire + déplacement sur 1 octet", "mod=01: memory operand + 1-byte displacement", "mod=01: operando en memoria + desplazamiento de 1 byte"),
            _ => t("mod=10 : opérande mémoire + déplacement sur 4 octets", "mod=10: memory operand + 4-byte displacement", "mod=10: operando en memoria + desplazamiento de 4 bytes"),
        };
        let rm_txt = if modrm_mod == 0b11 {
            REG64.get(rm_full as usize).copied().unwrap_or("?").to_string()
        } else if modrm_mod == 0b00 && rm == 0b101 {
            t("[RIP + déplacement]", "[RIP + displacement]", "[RIP + desplazamiento]").to_string()
        } else if rm == 0b100 {
            t("(adresse décrite par l'octet SIB)", "(address described by the SIB byte)", "(dirección descrita por el byte SIB)").to_string()
        } else {
            format!("[{}]", REG64.get(rm_full as usize).copied().unwrap_or("?"))
        };
        let detail = format!(
            "{mode_txt}\nreg={reg:03b} → {}   rm={rm:03b} → {rm_txt}",
            REG64.get(reg_full as usize).copied().unwrap_or("?"),
        );
        enc.fields.push(Field { part: Part::ModRm, bytes: vec![b], detail });
        i += 1;
    }

    // ---- 5. SIB ----
    if has_modrm && modrm_mod != 0b11 && rm == 0b100 {
        if i >= bytes.len() {
            enc.incomplete = true;
            return enc;
        }
        let b = bytes[i];
        let scale = 1u32 << (b >> 6);
        let index = (b >> 3) & 0b111;
        let base = b & 0b111;
        let index_full = index | ((rex_x as u8) << 3);
        let base_full = base | ((rex_b as u8) << 3);
        let index_txt = if index == 0b100 && !rex_x {
            t("aucun", "none", "ninguno").to_string()
        } else {
            REG64.get(index_full as usize).copied().unwrap_or("?").to_string()
        };
        let detail = format!(
            "{}\nbase={} index={} échelle={scale}",
            t(
                "Adresse = base + index × échelle + déplacement.",
                "Address = base + index × scale + displacement.",
                "Dirección = base + índice × escala + desplazamiento.",
            ),
            REG64.get(base_full as usize).copied().unwrap_or("?"),
            index_txt,
        );
        enc.fields.push(Field { part: Part::Sib, bytes: vec![b], detail });
        i += 1;
    }

    // ---- 6. Déplacement ----
    let disp_len = if !has_modrm {
        0
    } else {
        match modrm_mod {
            0b01 => 1,
            0b10 => 4,
            0b00 if rm == 0b101 => 4, // RIP-relatif
            _ => 0,
        }
    };
    if disp_len > 0 {
        if i + disp_len > bytes.len() {
            enc.incomplete = true;
            return enc;
        }
        let d = bytes[i..i + disp_len].to_vec();
        let val = match disp_len {
            1 => d[0] as i8 as i64,
            _ => i32::from_le_bytes([d[0], d[1], d[2], d[3]]) as i64,
        };
        enc.fields.push(Field {
            part: Part::Displacement,
            bytes: d,
            detail: format!(
                "{val} ({}) — {}",
                t("décimal signé", "signed decimal", "decimal con signo"),
                t(
                    "ajouté à l'adresse calculée ; stocké en petit-boutiste",
                    "added to the computed address; stored little-endian",
                    "sumado a la dirección calculada; almacenado en little-endian",
                ),
            ),
        });
        i += disp_len;
    }

    // ---- 7. Immédiat : tout ce qui reste ----
    if i < bytes.len() {
        let imm = bytes[i..].to_vec();
        let val: i64 = match imm.len() {
            1 => imm[0] as i8 as i64,
            2 => i16::from_le_bytes([imm[0], imm[1]]) as i64,
            4 => i32::from_le_bytes([imm[0], imm[1], imm[2], imm[3]]) as i64,
            8 => i64::from_le_bytes([
                imm[0], imm[1], imm[2], imm[3], imm[4], imm[5], imm[6], imm[7],
            ]),
            _ => {
                enc.incomplete = true;
                0
            }
        };
        enc.fields.push(Field {
            part: Part::Immediate,
            bytes: imm,
            detail: format!(
                "{val} (0x{val:X}) — {}",
                t(
                    "valeur écrite dans l'instruction, en petit-boutiste",
                    "value written inside the instruction, little-endian",
                    "valor escrito dentro de la instrucción, en little-endian",
                ),
            ),
        });
    }

    // Garde-fou : si la somme des morceaux ne fait pas la longueur totale,
    // c'est que le découpage est faux. Mieux vaut le signaler que présenter à
    // l'élève une décomposition qui ne tient pas.
    if enc.covered() != bytes.len() {
        enc.incomplete = true;
    }
    enc
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parts(e: &Encoding) -> Vec<Part> {
        e.fields.iter().map(|f| f.part).collect()
    }
    fn find(e: &Encoding, p: Part) -> Option<&Field> {
        e.fields.iter().find(|f| f.part == p)
    }

    /// L'exemple du cahier des charges : 48 89 e5 = mov rbp, rsp.
    #[test]
    fn decodes_the_canonical_mov_rbp_rsp() {
        let e = decode(&[0x48, 0x89, 0xE5], Lang::Fr);
        assert!(!e.incomplete);
        assert_eq!(parts(&e), vec![Part::Rex, Part::Opcode, Part::ModRm]);
        assert_eq!(e.covered(), 3, "tous les octets attribués");

        let rex = find(&e, Part::Rex).unwrap();
        assert_eq!(rex.bytes, vec![0x48]);
        assert!(rex.detail.contains("W=1"), "REX.W attendu : {}", rex.detail);
        assert!(rex.detail.contains("64"), "doit dire « 64 bits » : {}", rex.detail);

        let m = find(&e, Part::ModRm).unwrap();
        assert!(m.detail.contains("mod=11"), "{}", m.detail);
        assert!(m.detail.contains("rsp"), "reg=100 → rsp : {}", m.detail);
        assert!(m.detail.contains("rbp"), "rm=101 → rbp : {}", m.detail);
    }

    /// Un préfixe hérité doit être isolé du reste.
    #[test]
    fn legacy_prefix_is_separated() {
        // F3 0F 1E FA = endbr64
        let e = decode(&[0xF3, 0x0F, 0x1E, 0xFA], Lang::Fr);
        assert_eq!(e.fields[0].part, Part::LegacyPrefix);
        assert_eq!(e.fields[0].bytes, vec![0xF3]);
        assert_eq!(e.fields[1].part, Part::Opcode);
        assert_eq!(e.fields[1].bytes, vec![0x0F, 0x1E], "échappement 0F inclus");
        assert_eq!(e.covered(), 4);
    }

    /// Sans REX, l'opcode vient en premier ; l'immédiat prend le reste.
    #[test]
    fn immediate_takes_the_remaining_bytes() {
        // B8 3C 00 00 00 = mov eax, 60
        let e = decode(&[0xB8, 0x3C, 0x00, 0x00, 0x00], Lang::Fr);
        assert_eq!(parts(&e), vec![Part::Opcode, Part::Immediate]);
        let imm = find(&e, Part::Immediate).unwrap();
        assert_eq!(imm.bytes.len(), 4);
        assert!(imm.detail.starts_with("60 "), "valeur relue : {}", imm.detail);
        assert_eq!(e.covered(), 5);
    }

    /// Déplacement sur un octet : mod=01.
    #[test]
    fn one_byte_displacement_is_signed() {
        // 48 8B 45 F8 = mov rax, [rbp-8]
        let e = decode(&[0x48, 0x8B, 0x45, 0xF8], Lang::Fr);
        assert_eq!(parts(&e), vec![Part::Rex, Part::Opcode, Part::ModRm, Part::Displacement]);
        let d = find(&e, Part::Displacement).unwrap();
        assert!(d.detail.starts_with("-8"), "déplacement signé : {}", d.detail);
        assert_eq!(e.covered(), 4);
    }

    /// Octet SIB : rm=100 en mode mémoire.
    #[test]
    fn sib_is_decoded_with_scale_and_index() {
        // 48 8B 04 D8 = mov rax, [rax + rbx*8]
        let e = decode(&[0x48, 0x8B, 0x04, 0xD8], Lang::Fr);
        assert!(parts(&e).contains(&Part::Sib), "un SIB était attendu : {:?}", parts(&e));
        let sib = find(&e, Part::Sib).unwrap();
        assert!(sib.detail.contains("échelle=8"), "{}", sib.detail);
        assert!(sib.detail.contains("rbx"), "index attendu : {}", sib.detail);
        assert_eq!(e.covered(), 4);
    }

    /// Adressage relatif à RIP : mod=00 et rm=101, déplacement de 4 octets.
    #[test]
    fn rip_relative_addressing_is_recognised() {
        // 48 8D 3D 00 00 00 00 = lea rdi, [rip+0]
        let e = decode(&[0x48, 0x8D, 0x3D, 0x00, 0x00, 0x00, 0x00], Lang::Fr);
        let m = find(&e, Part::ModRm).unwrap();
        assert!(m.detail.contains("RIP"), "adressage RIP-relatif : {}", m.detail);
        assert_eq!(find(&e, Part::Displacement).unwrap().bytes.len(), 4);
        assert_eq!(e.covered(), 7);
    }

    /// Les instructions sans ModR/M ne doivent pas en inventer un.
    #[test]
    fn opcodes_without_modrm_are_respected() {
        // C3 = ret ; 0F 05 = syscall ; 55 = push rbp
        for bytes in [vec![0xC3], vec![0x0F, 0x05], vec![0x55]] {
            let e = decode(&bytes, Lang::Fr);
            assert!(
                !parts(&e).contains(&Part::ModRm),
                "{bytes:02X?} ne doit pas avoir de ModR/M : {:?}",
                parts(&e)
            );
            assert_eq!(e.covered(), bytes.len(), "{bytes:02X?}");
        }
    }

    /// REX.B étend le numéro de registre vers r8–r15.
    #[test]
    fn rex_b_extends_register_numbers() {
        // 49 89 C0 = mov r8, rax
        let e = decode(&[0x49, 0x89, 0xC0], Lang::Fr);
        let rex = find(&e, Part::Rex).unwrap();
        assert!(rex.detail.contains("B=1"), "{}", rex.detail);
        assert!(rex.detail.contains("r8"), "doit annoncer l'extension : {}", rex.detail);
        let m = find(&e, Part::ModRm).unwrap();
        assert!(m.detail.contains("r8"), "rm étendu vers r8 : {}", m.detail);
    }

    /// Invariant central : la somme des morceaux fait la longueur totale.
    /// C'est ce qui garantit qu'aucun octet n'est perdu ni compté deux fois.
    #[test]
    fn every_byte_is_accounted_for() {
        let samples: [&[u8]; 10] = [
            &[0x48, 0x89, 0xE5],
            &[0x55],
            &[0xC3],
            &[0xB8, 0x3C, 0x00, 0x00, 0x00],
            &[0x48, 0x8B, 0x45, 0xF8],
            &[0x48, 0x8B, 0x04, 0xD8],
            &[0x0F, 0x05],
            &[0xF3, 0x0F, 0x1E, 0xFA],
            &[0x48, 0x83, 0xEC, 0x20],
            &[0xE8, 0x00, 0x00, 0x00, 0x00],
        ];
        for s in samples {
            let e = decode(s, Lang::Fr);
            assert!(!e.incomplete, "{s:02X?} marqué incomplet");
            assert_eq!(e.covered(), s.len(), "{s:02X?} : couverture partielle");
        }
    }

    /// Entrées dégénérées : ne jamais paniquer, marquer incomplet.
    #[test]
    fn truncated_input_is_flagged_not_fatal() {
        assert!(decode(&[], Lang::Fr).fields.is_empty());
        assert!(decode(&[0x48], Lang::Fr).incomplete, "REX seul");
        assert!(decode(&[0x0F], Lang::Fr).incomplete, "échappement seul");
        assert!(decode(&[0x48, 0x89], Lang::Fr).incomplete, "ModR/M manquant");
    }

    #[test]
    fn every_field_is_explained_in_every_language() {
        for lang in [Lang::Fr, Lang::En, Lang::Es] {
            let e = decode(&[0x48, 0x8B, 0x04, 0xD8], lang);
            for f in &e.fields {
                assert!(!f.part.label(lang).is_empty(), "{:?} sans libellé", f.part);
                assert!(!f.detail.is_empty(), "{:?} sans détail en {lang:?}", f.part);
            }
        }
    }
}
