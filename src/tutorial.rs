//! Parcours guidé : des leçons qui pilotent l'IDE au lieu d'afficher du texte.
//!
//! Une leçon n'est pas une page à lire. Elle porte un programme de départ, dit
//! quels panneaux ouvrir pour l'observer, et embarque ses propres attentes.
//!
//! Ces attentes sont écrites dans le programme lui-même, en directives
//! `;@attendu` — exactement celles du module [`crate::exercise`]. La leçon n'a
//! donc aucune machinerie de vérification propre : charger son programme suffit
//! à armer le contrôle, et l'élève VOIT les directives, ce qui lui apprend à en
//! écrire.

use crate::i18n::{self, Lang};

/// Un texte dans les trois langues de l'application.
#[derive(Debug, Clone, Copy)]
pub struct Text {
    pub fr: &'static str,
    pub en: &'static str,
    pub es: &'static str,
}

impl Text {
    pub const fn new(fr: &'static str, en: &'static str, es: &'static str) -> Text {
        Text { fr, en, es }
    }
    pub fn get(&self, lang: Lang) -> &'static str {
        i18n::tr3(lang, self.fr, self.en, self.es)
    }
}

/// Niveau du parcours.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Level {
    Beginner,
    Intermediate,
    Advanced,
    Expert,
}

impl Level {
    pub const ALL: [Level; 4] = [
        Level::Beginner,
        Level::Intermediate,
        Level::Advanced,
        Level::Expert,
    ];

    pub fn title(self, lang: Lang) -> &'static str {
        match self {
            Level::Beginner => i18n::tr3(lang, "Débutant", "Beginner", "Principiante"),
            Level::Intermediate => i18n::tr3(lang, "Intermédiaire", "Intermediate", "Intermedio"),
            Level::Advanced => i18n::tr3(lang, "Avancé", "Advanced", "Avanzado"),
            Level::Expert => i18n::tr3(lang, "Expert", "Expert", "Experto"),
        }
    }

}

/// Panneau qu'une leçon demande d'ouvrir, désigné par sa clé stable.
///
/// On passe par une chaîne plutôt que par le type `Panel` : le catalogue reste
/// une donnée pure, sans dépendre de la couche d'interface.
pub type PanelKey = &'static str;

/// Une leçon du parcours.
#[derive(Debug, Clone)]
pub struct Lesson {
    /// Identifiant stable — sert à mémoriser la progression.
    pub id: &'static str,
    pub level: Level,
    pub title: Text,
    /// Ce que l'élève saura faire à la fin.
    pub goal: Text,
    /// Étapes à suivre, dans l'ordre.
    pub steps: Vec<Text>,
    /// Panneaux à ouvrir : la leçon met sous les yeux ce qu'elle explique.
    pub panels: Vec<PanelKey>,
    /// Programme de départ, directives `;@attendu` comprises.
    /// `None` pour une leçon purement explicative.
    pub starter: Option<&'static str>,
}

impl Lesson {
    /// Vrai si la leçon a du contenu exécutable à charger.
    pub fn has_starter(&self) -> bool {
        self.starter.is_some()
    }
}

// ======================================================================
//  Catalogue
// ======================================================================
//
//  Les quatre niveaux sont écrits intégralement : chaque leçon porte son
//  programme de départ, ses étapes, ses panneaux et ses attentes
//  vérifiables. Seule « installation » reste purement explicative — c'est
//  le premier contact, avant tout code.
//
//  Un programme de départ doit ÉCHOUER tel quel et PASSER une fois son
//  TODO appliqué : les deux moitiés sont vérifiées par le test
//  `every_written_lesson_fails_then_passes`, qui applique à la lettre la
//  correction que le commentaire dicte.

macro_rules! t {
    ($fr:expr, $en:expr, $es:expr) => {
        Text::new($fr, $en, $es)
    };
}

const L_PREMIER: &str = r#";@titre Premier programme
;@enonce Ce programme se termine avec le code 0. Change-le pour qu'il rende 7.
;@attendu exit == 7

; Tout programme Linux se termine par l'appel système « exit ».
;   RAX = 60  -> numéro de l'appel « exit »
;   RDI = code de sortie
section .text
    global _start

_start:
    mov rax, 60         ; numéro de l'appel système
    xor rdi, rdi        ; code de sortie = 0  (xor d'un registre avec lui-même)
    syscall             ; passe la main au noyau
"#;

const L_REGISTRES: &str = r#";@titre Les registres
;@enonce Fais en sorte que RBX contienne 100 à la fin, en n'utilisant que des
;@enonce additions et des copies entre registres.
;@attendu rbx == 100
;@attendu exit == 0

; Un registre est une case mémoire interne au processeur, très rapide.
; Il y en a seize de 64 bits. RAX, RBX, RCX, RDX sont les plus utilisés.
section .text
    global _start

_start:
    mov rax, 40         ; charge une valeur immédiate dans RAX
    mov rbx, rax        ; copie RAX dans RBX
    add rbx, 10         ; RBX = RBX + 10

    ; TODO : atteindre 100 dans RBX

    mov rax, 60
    xor rdi, rdi
    syscall
"#;

const L_MEMOIRE: &str = r#";@titre La mémoire
;@enonce Lis la valeur rangée en mémoire et place-la dans RBX.
;@attendu rbx == 1234
;@attendu exit == 0

; Les registres ne suffisent pas : les données vivent en mémoire.
; « section .data » contient les valeurs initialisées du programme.
section .data
    valeur  dq 1234     ; dq = « define quadword » : 8 octets

section .text
    global _start

_start:
    ; Les crochets signifient « le contenu à cette adresse ».
    ;   mov rbx, valeur    -> l'ADRESSE
    ;   mov rbx, [valeur]  -> le CONTENU
    mov rbx, valeur     ; TODO : ce n'est pas ce qu'on veut

    mov rax, 60
    xor rdi, rdi
    syscall
"#;

const L_FLAGS: &str = r#";@titre Les flags
;@enonce Après la comparaison, place 1 dans RBX si les valeurs sont égales,
;@enonce 0 sinon. Utilise « sete bl ».
;@attendu rbx == 1
;@attendu exit == 0

; Une comparaison ne produit pas de résultat : elle positionne des DRAPEAUX.
; ZF vaut 1 quand la soustraction donne zéro, donc quand les valeurs sont égales.
section .text
    global _start

_start:
    xor rbx, rbx        ; RBX = 0
    mov rax, 42
    mov rcx, 42
    cmp rax, rcx        ; calcule RAX - RCX SANS stocker : positionne ZF

    ; TODO : « sete bl » écrit 1 dans BL si ZF vaut 1, sinon 0.

    mov rax, 60
    xor rdi, rdi
    syscall
"#;

const L_PILE: &str = r#";@titre La pile
;@enonce Échange le contenu de RAX et RBX en passant par la pile.
;@attendu rax == 2
;@attendu rbx == 1
;@attendu exit == 0

; La pile croît vers les adresses BASSES. « push » y range une valeur et
; diminue RSP de 8 ; « pop » fait l'inverse. Dernier entré, premier sorti.
section .text
    global _start

_start:
    mov rax, 1
    mov rbx, 2

    push rax            ; la pile contient : 1
    push rbx            ; la pile contient : 1, 2

    ; TODO : deux « pop » bien ordonnés suffisent à échanger.
    pop rax
    pop rbx

    mov rax, 60
    xor rdi, rdi
    syscall
"#;

const L_SAUTS: &str = r#";@titre Les sauts
;@enonce Place la plus grande des deux valeurs dans RBX.
;@attendu rbx == 42
;@attendu exit == 0

; Un saut conditionnel lit les flags posés par la comparaison précédente.
;   jg  = jump if greater (signé)      jl = jump if less
;   je  = jump if equal                jne = jump if not equal
section .text
    global _start

_start:
    mov rsi, 42
    mov rdi, 17

    cmp rsi, rdi
    ; TODO : sauter à .rsi_gagne si RSI est le plus grand
    mov rbx, rdi
    jmp .fin

.rsi_gagne:
    mov rbx, rsi

.fin:
    mov rax, 60
    xor rdi, rdi
    syscall
"#;

const L_BOUCLES: &str = r#";@titre Les boucles
;@enonce Calcule 1+2+…+10 dans RBX.
;@attendu rbx == 55
;@attendu exit == 0

; Une boucle, c'est un saut en arrière conditionné par un compteur.
section .text
    global _start

_start:
    xor rbx, rbx        ; accumulateur
    mov rcx, 10         ; compteur : 10, 9, 8 … 1

.boucle:
    ; TODO : ajouter RCX à RBX
    dec rcx             ; décrémente et positionne ZF
    jnz .boucle         ; recommence tant que RCX n'est pas nul

    mov rax, 60
    xor rdi, rdi
    syscall
"#;

const L_FONCTIONS: &str = r#";@titre Fonctions
;@enonce La fonction « carre » doit rendre le carré de son argument.
;@enonce Complète-la pour que RBX vaille 49 à la fin.
;@attendu rbx == 49
;@attendu exit == 0

; Une fonction, c'est un bout de code qu'on atteint par « call » et qu'on
; quitte par « ret ». Entre les deux, elle emprunte la pile de l'appelant.
section .text
    global _start

; carre(n) -> n*n        n arrive dans RDI, le résultat repart dans RAX.
carre:
    push rbp            ; PROLOGUE : met de côté le cadre de l'appelant
    mov rbp, rsp        ;            et ouvre le sien

    mov rax, rdi
    ; TODO : multiplier RAX par RDI  (« imul rax, rdi »)

    mov rsp, rbp        ; ÉPILOGUE : referme le cadre
    pop rbp             ;            et rend celui de l'appelant
    ret                 ; dépile l'adresse de retour et y saute

_start:
    mov rdi, 7          ; l'argument
    call carre          ; empile l'adresse de retour, puis saute
    mov rbx, rax        ; met le résultat de côté

    mov rax, 60
    xor rdi, rdi
    syscall
"#;

const L_SYSTEM_V: &str = r#";@titre Convention System V
;@enonce La fonction « somme3 » écrase RBX, que l'appelant croyait à l'abri.
;@enonce Sauve-le et rends-le, pour que RBX vaille encore 111 au retour.
;@attendu r12 == 60
;@attendu rbx == 111
;@attendu exit == 0

; La convention System V dit QUI met quoi où, et QUELS registres survivent
; à un appel. Le processeur ne l'impose pas : c'est un contrat entre codes.
section .text
    global _start

; somme3(a, b, c) -> a+b+c      a=RDI, b=RSI, c=RDX, résultat dans RAX
somme3:
    ; TODO : « push rbx » ici, car RBX appartient à l'appelant…
    mov rbx, rdi
    add rbx, rsi
    add rbx, rdx
    mov rax, rbx
    ; TODO : … et « pop rbx » juste avant de rendre la main
    ret

_start:
    mov rbx, 111        ; valeur précieuse, confiée à un registre PRÉSERVÉ
    mov rdi, 10         ; 1er argument
    mov rsi, 20         ; 2e
    mov rdx, 30         ; 3e
    call somme3
    mov r12, rax        ; le résultat, dans un registre préservé lui aussi
    ; RBX devrait valoir encore 111 : la convention le promet.

    mov rax, 60
    xor rdi, rdi
    syscall
"#;

const L_SYSCALLS: &str = r#";@titre Appels système Linux
;@enonce Le programme veut écrire son message sur la sortie standard, puis
;@enonce terminer avec le nombre d'octets écrits. Complète l'appel « write ».
;@attendu rbx == 12
;@attendu exit == 12

; L'ABI des appels système n'est PAS celle des fonctions :
;   RAX = numéro de l'appel
;   arguments : RDI, RSI, RDX, R10, R8, R9   (R10, et non RCX)
;   RAX = valeur de retour, négative en cas d'erreur
section .data
    msg     db "Bonjour ASM", 10   ; 10 = saut de ligne
    msg_len equ $ - msg            ; $ = adresse courante : la longueur se calcule

section .text
    global _start

_start:
    mov rax, 1          ; 1 = write
    mov rdi, 1          ; fd 1 = sortie standard
    mov rsi, msg        ; adresse du tampon (sans crochets : on veut l'ADRESSE)
    ; TODO : RDX doit porter le nombre d'octets à écrire, c'est-à-dire msg_len
    xor rdx, rdx
    syscall             ; RAX reçoit le nombre d'octets réellement écrits

    mov rbx, rax        ; on garde le compte

    mov rax, 60
    mov rdi, rbx        ; code de sortie = octets écrits
    syscall
"#;

const L_TAS: &str = r#";@titre Le tas
;@enonce Repousse la limite du tas de 4096 octets, pour que l'écriture qui suit
;@enonce tombe dans une page bien à nous. RBX doit valoir 1234.
;@attendu rbx == 1234
;@attendu exit == 0

; Le tas n'existe pas au démarrage : il se demande. « brk » déplace la limite
; haute du segment de données ; tout ce qui passe en dessous devient utilisable.
section .text
    global _start

_start:
    ; 1) Où en est la limite ? brk(0) la renvoie sans rien changer.
    mov rax, 12         ; 12 = brk
    xor rdi, rdi
    syscall
    mov r12, rax        ; début de NOTRE zone

    ; 2) La repousser : c'est cela, et cela seul, qui crée le tas.
    mov rax, 12
    mov rdi, r12        ; TODO : demander r12 + 4096  (« lea rdi, [r12 + 4096] »)
    syscall
    mov r13, rax        ; la NOUVELLE limite ; si elle n'a pas bougé, c'est un refus

    ; 3) Sans le pas 2, l'écriture ci-dessous tombe hors de toute page réservée
    ;    et le noyau tue le programme sur SIGSEGV. Essayez avant de corriger.
    mov qword [r12], 1234
    mov rbx, [r12]

    mov rax, 60
    xor rdi, rdi
    syscall
"#;

const L_TABLEAUX: &str = r#";@titre Tableaux
;@enonce Fais la somme des cinq entiers du tableau dans RBX.
;@attendu rbx == 150
;@attendu exit == 0

; Un tableau, c'est une suite d'éléments de même taille. Le processeur sait
; en atteindre un sans calcul séparé : base + index × échelle.
section .data
    tab dq 10, 20, 30, 40, 50
    n   equ ($ - tab) / 8   ; laisse l'assembleur compter : 5

section .text
    global _start

_start:
    xor rbx, rbx        ; la somme
    xor rcx, rcx        ; l'index

.boucle:
    ; TODO : ajouter l'élément courant à RBX  (« add rbx, [tab + rcx*8] »)
    inc rcx
    cmp rcx, n
    jb .boucle          ; jb = below, non signé : un index n'est jamais négatif

    mov rax, 60
    xor rdi, rdi
    syscall
"#;

const L_STRUCTURES: &str = r#";@titre Structures
;@enonce Le tableau contient deux points. Mets dans RBX la somme x + y du
;@enonce SECOND point.
;@attendu rbx == 70
;@attendu exit == 0

; Une structure n'existe pas dans le processeur : ce sont des décalages, qu'on
; se contente de nommer. « equ » leur donne un nom lisible.
pt_x      equ 0         ; champ x : 8 octets, au décalage 0
pt_y      equ 8         ; champ y : 8 octets, au décalage 8
pt_taille equ 16        ; taille d'un point, pour passer au suivant

section .data
    points  dq 1, 2     ; points[0] : x=1,  y=2
            dq 30, 40   ; points[1] : x=30, y=40

section .text
    global _start

_start:
    mov rsi, points
    add rsi, pt_taille          ; RSI pointe maintenant le point n° 1

    mov rbx, [rsi + pt_x]       ; décalage 0
    ; TODO : ajouter le champ y  (« add rbx, [rsi + pt_y] »)

    mov rax, 60
    xor rdi, rdi
    syscall
"#;

const L_CHAINES: &str = r#";@titre Chaînes de caractères
;@enonce Calcule dans RBX la longueur de la chaîne, sans compter le zéro final.
;@attendu rbx == 11
;@attendu exit == 0

; Une chaîne C ne range nulle part sa longueur : elle s'arrête au premier
; octet nul. La mesurer, c'est donc la parcourir en entier.
section .data
    texte db "Bonjour ASM", 0   ; 11 caractères, puis le zéro qui termine

section .text
    global _start

_start:
    mov rsi, texte
    xor rbx, rbx                ; la longueur en construction

.boucle:
    mov al, [rsi + rbx]         ; UN octet : « al », pas « rax »
    ; TODO : s'arrêter quand AL vaut 0  (« test al, al » puis « jz .fin »)
    inc rbx
    cmp rbx, 64                 ; garde-fou : sans le test ci-dessus, la boucle
    jb .boucle                  ; partirait au loin dans la mémoire

.fin:
    mov rax, 60
    xor rdi, rdi
    syscall
"#;

const L_ELF: &str = r#";@titre Le format ELF
;@enonce Ton programme peut relire son propre en-tête. Vérifie que le champ
;@enonce e_entry contient bien l'adresse de _start : RBX doit tomber à 0.
;@attendu rcx == 0x464c457f
;@attendu rbx == 0
;@attendu exit == 0

; Un exécutable ELF commence par son en-tête, et le noyau le mappe en mémoire
; AVEC le reste du programme. Rien n'empêche donc de se relire soi-même.
;   +0x00  e_ident   7F 'E' 'L' 'F', puis classe, boutisme, version
;   +0x10  e_type    2 = EXEC (adresses figées), 3 = DYN (PIE ou bibliothèque)
;   +0x18  e_entry   adresse de la première instruction exécutée
;   +0x20  e_phoff   table des SEGMENTS — ce que le noyau lit pour charger
;   +0x28  e_shoff   table des SECTIONS — ce que ld lit pour assembler le tout
extern __ehdr_start         ; symbole fabriqué par ld : où l'en-tête est mappé

section .text
    global _start

_start:
    mov rsi, __ehdr_start

    mov ecx, [rsi]          ; le nombre magique, 4 octets
    ; En mémoire : 7F 45 4C 46. Relu comme un entier : 0x464C457F.
    ; C'est le petit-boutisme de la leçon « La mémoire », sur un cas réel.

    mov rbx, [rsi + 0x18]   ; e_entry
    ; TODO : retrancher l'adresse de _start, il ne doit rien rester
    ;        (« sub rbx, _start »)

    mov rax, 60
    xor rdi, rdi
    syscall
"#;

const L_LINKING: &str = r#";@titre Édition de liens
;@enonce Calcule la taille de la section .bss dans RBX, en te servant des deux
;@enonce symboles que ld fabrique. Elle vaut 4096.
;@attendu rbx == 4096
;@attendu exit == 0

; nasm ne connaît qu'un fichier à la fois. Il ne SAIT pas où .bss atterrira,
; ni quelle taille elle aura une fois toutes les sections rassemblées : il
; laisse un trou et une consigne. C'est ld qui remplit, tout à la fin.
;
; ld fabrique au passage quelques symboles qu'aucun fichier ne définit :
;   __ehdr_start  début de l'en-tête ELF
;   __bss_start   début de la zone non initialisée
;   _end          fin de l'image mémoire du programme — et début du tas
extern __bss_start
extern _end

section .bss
    tampon resb 4096        ; resb = réserver des octets, sans les écrire

section .text
    global _start

_start:
    mov rbx, _end
    ; TODO : retrancher __bss_start pour obtenir la taille
    ;        (« sub rbx, __bss_start »)

    mov rax, 60
    xor rdi, rdi
    syscall
"#;

const L_PLT_GOT: &str = r#";@titre PLT et GOT
;@enonce Appelle « triple » sans écrire son nom : passe par la table, dont
;@enonce c'est la deuxième entrée. RBX doit valoir 21.
;@attendu rbx == 21
;@attendu exit == 0

; Un appel de bibliothèque partagée ne saute pas à la fonction : il saute à un
; talon (la PLT) qui lit une ADRESSE dans une table (la GOT) et y va. Cette
; indirection permet de placer la bibliothèque n'importe où, et de ne résoudre
; l'adresse qu'au premier appel.
;
; Ici, pas de bibliothèque : on fabrique la table à la main. Le mécanisme est
; le même, seul le remplissage automatique manque.
section .data
    table dq double, triple     ; notre GOT : un simple tableau d'adresses
                                ; entrée 0 = double, entrée 1 = triple

section .text
    global _start

double:
    lea rax, [rdi + rdi]        ; n*2
    ret

triple:
    lea rax, [rdi + rdi*2]      ; n*3
    ret

_start:
    mov rdi, 7
    lea rsi, [rel table]

    ; « call rsi » sauterait DANS la table. Les crochets font la différence :
    ; on veut appeler l'adresse RANGÉE là, pas la table elle-même.
    call [rsi]                  ; TODO : viser la deuxième entrée, « call [rsi + 8] »
    mov rbx, rax

    mov rax, 60
    xor rdi, rdi
    syscall
"#;

const L_RELOCATIONS: &str = r#";@titre Relocations
;@enonce Les deux façons de désigner « valeur » doivent tomber sur le même
;@enonce octet. Vise-la en RIP-relatif, et l'écart RBX deviendra nul.
;@attendu rbx == 0
;@attendu exit == 0

; nasm ne connaît aucune adresse définitive : il écrit des zéros et joint une
; consigne, la RELOCATION. « readelf -r » les montre dans le .o. Ici, deux
; types, que ld traite différemment :
;
;   mov rbx, valeur        R_X86_64_64    « écris ici l'adresse ABSOLUE »
;   lea rcx, [rel valeur]  R_X86_64_PC32  « écris ici l'ÉCART depuis RIP »
;
; Le premier fige le programme à son adresse de chargement. Le second le rend
; déplaçable : c'est pourquoi tout code PIE ou partagé n'utilise que celui-là.
section .data
    valeur dq 0x1234

section .text
    global _start

_start:
    mov rbx, valeur         ; absolu : ld écrit 0x40… dans l'instruction
    lea rcx, [rel _start]   ; TODO : viser « valeur », pas « _start »

    sub rbx, rcx            ; même cible ⇒ écart nul

    mov rax, 60
    xor rdi, rdi
    syscall
"#;

const L_SIMD: &str = r#";@titre SIMD et AVX
;@enonce Additionne les deux tableaux d'un coup, avec « paddd ». Les deux
;@enonce premières sommes, 65536 et 22, doivent se retrouver dans RBX.
;@attendu rbx == 94489346048
;@attendu exit == 0

; SIMD : une instruction, plusieurs données. Un registre XMM fait 128 bits,
; soit quatre entiers de 32 bits côte à côte. « paddd » les additionne tous
; les quatre en une fois — d = doubleword, la dernière lettre dit la découpe.
;
; Les valeurs ne sont pas choisies au hasard : 65535 + 1 déborde de 16 bits.
; Avec « paddw », l'addition serait découpée en huit morceaux de 16 bits et la
; retenue serait PERDUE — chaque tranche s'arrête au bord. Le résultat ne
; passerait pas. La découpe n'est donc pas un détail d'écriture.
;
; « align 16 » n'est pas décoratif non plus : movdqa exige une adresse
; multiple de 16 et plante sinon. (movdqu accepte tout, un peu plus lentement.)
section .data
    align 16
    a dd 65535, 2, 3, 4
    b dd     1, 20, 30, 40

section .text
    global _start

_start:
    movdqa xmm0, [rel a]    ; les quatre entiers de a, d'un seul chargement
    movdqa xmm1, [rel b]

    ; TODO : additionner les deux vecteurs  (« paddd xmm0, xmm1 »)

    movq rbx, xmm0          ; ne redescend que les 64 bits bas : 65536 et 22
    ; 65536 + (22 << 32) = 94489346048 : deux résultats dans un seul registre.

    mov rax, 60
    xor rdi, rdi
    syscall
"#;

const L_OPTIMISATION: &str = r#";@titre Optimisations
;@enonce Multiplie RAX par 10 sans « imul », avec deux instructions seulement.
;@enonce RBX doit valoir 70.
;@attendu rbx == 70
;@attendu exit == 0

; « lea » calcule une adresse — mais rien n'oblige à s'en servir comme d'une
; adresse. C'est l'additionneur-multiplieur du processeur, accessible
; gratuitement : base + index × 1, 2, 4 ou 8.
;
;   x*5  =  lea rbx, [rax + rax*4]
;   x*10 =  x*5, puis doublé
section .text
    global _start

_start:
    mov rax, 7

    mov rbx, rax            ; TODO : « lea rbx, [rax + rax*4] » …
    add rbx, 0              ; TODO : … puis « add rbx, rbx »

    mov rax, 60
    xor rdi, rdi
    syscall
"#;

const L_REVERSE: &str = r#";@titre Rétro-ingénierie
;@enonce Ces cinq octets sont chiffrés par un XOR avec la clé 0x2A. Déchiffre-les
;@enonce et mets la somme des octets clairs dans RBX. Elle vaut 532.
;@attendu rbx == 532
;@attendu exit == 0

; Sans les sources, on lit ce que le programme FAIT. Le XOR est le chiffrement
; le plus courant qu'on rencontre en analyse : réversible, et sa propre inverse
; — déchiffrer, c'est refaire exactement la même opération.
;
; Astuce d'analyste : « xor al, al » met à zéro (le résultat est toujours nul),
; tandis que « xor al, cle » chiffre ou déchiffre. Même instruction, deux rôles.
section .data
    secret db 0x42, 0x4f, 0x46, 0x46, 0x45   ; un mot de 5 lettres, chiffré
    cle    equ 0x2a
    n      equ 5

section .text
    global _start

_start:
    xor rcx, rcx        ; index
    xor rbx, rbx        ; somme des octets clairs

.boucle:
    mov al, [secret + rcx]
    ; TODO : déchiffrer l'octet  (« xor al, cle »)
    add rbx, rax
    inc rcx
    cmp rcx, n
    jb .boucle

    mov rax, 60
    xor rdi, rdi
    syscall
"#;

const L_DESASSEMBLAGE: &str = r#";@titre Désassemblage
;@enonce Ces sept octets SONT une instruction : « mov rbx, imm32 ». Corrige
;@enonce l'octet de l'immédiat pour que RBX vaille 42.
;@attendu rbx == 42
;@attendu exit == 0

; Un désassembleur ne voit que des octets. Il reconnaît une instruction à son
; premier octet, en déduit sa longueur, et sait ainsi où commence la suivante.
; C'est ce découpage, et lui seul, qui sépare le code des données.
;
;   48        préfixe REX.W : « l'opérande fait 64 bits »
;   C7        opcode : mov registre, immédiat sur 32 bits
;   C3        ModR/M : désigne RBX comme destination
;   2A 00 00 00   l'immédiat, 42 en petit-boutisme
section .text
    global _start

_start:
    ; TODO : le 4e octet porte l'immédiat ; 0x00 donne 0, il faut 42 (0x2A).
    db 0x48, 0xc7, 0xc3, 0x00, 0x00, 0x00, 0x00

    mov rax, 60
    xor rdi, rdi
    syscall
"#;

const L_SYSCALLS_AVANCES: &str = r#";@titre Appels système avancés
;@enonce Ce programme se parle à lui-même par un tube : il y écrit 42, puis le
;@enonce relit. Complète la lecture pour que RBX récupère l'octet.
;@attendu rbx == 42
;@attendu exit == 0

; Au-delà de read et write sur les flux standard, le noyau ouvre des canaux
; entre processus. « pipe2 » en crée un : deux descripteurs, une extrémité pour
; écrire, l'autre pour lire. Ici un seul processus s'en sert des deux côtés.
;   fds[0] = lecture (fd bas)     fds[1] = écriture (fd haut)
section .bss
    fds  resd 2         ; deux entiers de 32 bits, remplis par pipe2
    buf  resb 1         ; là où atterrira l'octet relu

section .data
    octet db 42

section .text
    global _start

_start:
    mov rax, 293        ; pipe2(fds, 0)
    lea rdi, [rel fds]
    xor rsi, rsi
    syscall

    mov rax, 1          ; write(fds[1], octet, 1)
    mov edi, [rel fds + 4]      ; fds[1] : le descripteur d'écriture
    lea rsi, [rel octet]
    mov rdx, 1
    syscall

    mov rax, 0          ; read(fds[0], buf, 1)
    ; TODO : charger fds[0] dans EDI  (« mov edi, [rel fds] »)
    xor edi, edi
    lea rsi, [rel buf]
    mov rdx, 1
    syscall

    movzx rbx, byte [rel buf]

    mov rax, 60
    xor rdi, rdi
    syscall
"#;

const L_SHELLCODE: &str = r#";@titre Shellcode
;@enonce Un shellcode transporte ses données avec lui, sur la pile — pas de
;@enonce section .data. Dépose la chaîne "Hi", puis mesure-la : RBX doit valoir 2.
;@attendu rbx == 2
;@attendu exit == 0

; Un shellcode est injecté dans un programme déjà lancé : il ne connaît aucune
; adresse fixe et ne peut compter sur aucune section à lui. Deux contraintes en
; découlent, que le Microscope met en évidence dans l'encodage :
;
;   • AUCUN octet nul : un octet nul terminerait la saisie qui le transporte.
;     « mov rax, 60 » en contient (b'... 3C 00 00 00') ; « xor rax,rax / mov al,60 »
;     n'en a aucun. Comparez les deux dans le panneau Instruction.
;   • AUCUNE adresse absolue : d'où la chaîne construite sur la pile, ci-dessous.
section .text
    global _start

_start:
    xor rax, rax
    push rax            ; le zéro terminal de la chaîne
    ; TODO : déposer "Hi" sur la pile
    ;        (« mov rax, 0x6948 » puis « push rax » — 'H'=0x48, 'i'=0x69)
    mov rsi, rsp        ; RSI pointe la chaîne, là où on vient de l'écrire

    xor rbx, rbx        ; longueur
.mesure:
    mov al, [rsi + rbx]
    test al, al
    jz .fin
    inc rbx
    jmp .mesure
.fin:
    mov rax, 60
    xor rdi, rdi
    syscall
"#;

const L_EXPLOITATION: &str = r#";@titre Exploitation de binaires
;@enonce Un débordement écrit au-delà du tampon jusqu'à l'adresse de retour.
;@enonce Vise le bon décalage pour détourner « vulnerable » vers « gagne » :
;@enonce le programme se terminera alors avec le code 57.
;@attendu exit == 57

; Voici, en miniature et de façon contrôlée, ce qu'exploite un débordement de
; pile. « vulnerable » réserve un tampon, puis écrit AU-DELÀ — comme le ferait
; une copie de saisie sans vérification de taille. Assez loin, on atteint
; l'adresse de retour que « call » avait empilée, et « ret » y obéit.
;
; Disposition de la pile après le prologue, à partir de RBP :
;   [rbp - 8]  … tampon local …
;   [rbp + 0]  ancien RBP sauvegardé
;   [rbp + 8]  ADRESSE DE RETOUR   <- la cible du détournement
;
; Ce qui l'empêche, dans un vrai programme : un canari entre le tampon et
; l'adresse de retour (modifié = arrêt immédiat), une pile non exécutable (NX),
; et des adresses rendues imprévisibles au chargement (ASLR).
section .text
    global _start

gagne:                  ; jamais atteinte par le flot normal
    mov rax, 60
    mov rdi, 57
    syscall

vulnerable:
    push rbp
    mov rbp, rsp
    sub rsp, 16         ; le tampon

    lea rax, [rel gagne]
    ; TODO : écrire cette adresse SUR l'adresse de retour, en [rbp + 8].
    ;        Un mauvais décalage rate la cible et le programme sort par 1.
    mov [rbp + 0], rax

    mov rsp, rbp
    pop rbp
    ret                 ; ret ordinaire — mais la cible a été remplacée

_start:
    call vulnerable
    mov rax, 60         ; chemin normal : atteint seulement si l'exploit rate
    mov rdi, 1
    syscall
"#;

const L_PERFORMANCE: &str = r#";@titre Analyse de performances
;@enonce Multiplie RAX par 8 avec l'instruction la plus rapide qui soit, un
;@enonce décalage. RBX doit valoir 72.
;@attendu rbx == 72
;@attendu exit == 0

; Avant de récrire, il faut MESURER. La Timeline compte les instructions
; réellement exécutées : c'est la seule donnée qui ne ment pas.
;
; Multiplier par une puissance de deux, c'est décaler les bits vers la gauche.
; « shl rax, 3 » fait ×8 en un cycle et sans dépendance ; « imul » demande la
; même chose en trois. La règle n'est pourtant pas « fuir imul » : c'est
; « mesurer », car sur un multiplicateur quelconque imul redevient le bon choix.
section .text
    global _start

_start:
    mov rax, 9
    ; TODO : multiplier par 8 par décalage  (« shl rax, 3 »)
    add rax, 0
    mov rbx, rax

    mov rax, 60
    xor rdi, rdi
    syscall
"#;

/// Toutes les leçons, dans l'ordre du parcours.
pub fn catalogue() -> Vec<Lesson> {
    vec![
        // ---------------- Débutant ----------------
        Lesson {
            id: "installation",
            level: Level::Beginner,
            title: t!("Installation et premiers repères", "Setup and first landmarks", "Instalación y primeras referencias"),
            goal: t!(
                "Savoir ce qu'ASM Studio met sous vos yeux, et vérifier que nasm et ld répondent.",
                "Know what ASM Studio puts in front of you, and check that nasm and ld respond.",
                "Saber qué muestra ASM Studio y comprobar que nasm y ld responden."
            ),
            steps: vec![
                t!(
                    "L'éditeur au centre contient votre code source. À droite, le panneau Instruction explique l'instruction en cours.",
                    "The editor in the middle holds your source. On the right, the Instruction panel explains the current instruction.",
                    "El editor central contiene su código. A la derecha, el panel Instrucción explica la instrucción actual."
                ),
                t!(
                    "Appuyez sur F5 : ASM Studio appelle nasm pour assembler, puis ld pour lier. La Console montre les deux commandes.",
                    "Press F5: ASM Studio calls nasm to assemble, then ld to link. The Console shows both commands.",
                    "Pulse F5: ASM Studio llama a nasm y luego a ld. La Consola muestra ambos comandos."
                ),
                t!(
                    "Si la Console affiche « impossible de lancer nasm », installez le paquet nasm — voir INSTALL.md.",
                    "If the Console says \"cannot run nasm\", install the nasm package — see INSTALL.md.",
                    "Si la Consola dice «no se puede ejecutar nasm», instale el paquete nasm — vea INSTALL.md."
                ),
                t!(
                    "F10 exécute UNE instruction. La Timeline en bas garde chaque étape : vous pouvez revenir en arrière.",
                    "F10 runs ONE instruction. The Timeline at the bottom keeps every step: you can go back.",
                    "F10 ejecuta UNA instrucción. La Línea de tiempo guarda cada paso: puede retroceder."
                ),
            ],
            panels: vec!["editor", "console", "timeline"],
            starter: None,
        },
        Lesson {
            id: "premier_programme",
            level: Level::Beginner,
            title: t!("Premier programme", "First program", "Primer programa"),
            goal: t!(
                "Écrire un programme qui démarre, se termine proprement, et rend le code voulu.",
                "Write a program that starts, ends cleanly, and returns the intended code.",
                "Escribir un programa que arranca, termina limpiamente y devuelve el código deseado."
            ),
            steps: vec![
                t!(
                    "Un programme Linux commence à l'étiquette _start, déclarée « global » pour que ld la trouve.",
                    "A Linux program starts at the _start label, declared \"global\" so ld can find it.",
                    "Un programa Linux empieza en la etiqueta _start, declarada «global» para que ld la encuentre."
                ),
                t!(
                    "Il n'y a pas de « return » : on demande au noyau de nous arrêter, via l'appel système exit.",
                    "There is no \"return\": you ask the kernel to stop you, via the exit system call.",
                    "No hay «return»: se pide al núcleo que nos detenga, con la llamada exit."
                ),
                t!(
                    "RAX porte le NUMÉRO de l'appel (60 = exit), RDI son premier argument (le code de sortie).",
                    "RAX carries the call NUMBER (60 = exit), RDI its first argument (the exit code).",
                    "RAX lleva el NÚMERO de llamada (60 = exit), RDI su primer argumento (el código de salida)."
                ),
                t!(
                    "Remplacez « xor rdi, rdi » par « mov rdi, 7 », puis F5. Le panneau Exercice validera.",
                    "Replace \"xor rdi, rdi\" with \"mov rdi, 7\", then F5. The Exercise panel will check it.",
                    "Sustituya «xor rdi, rdi» por «mov rdi, 7» y pulse F5. El panel Ejercicio lo comprobará."
                ),
            ],
            panels: vec!["editor", "exercise", "console"],
            starter: Some(L_PREMIER),
        },
        Lesson {
            id: "registres",
            level: Level::Beginner,
            title: t!("Les registres", "Registers", "Los registros"),
            goal: t!(
                "Comprendre ce qu'est un registre et voir sa valeur changer à chaque pas.",
                "Understand what a register is and watch its value change at every step.",
                "Entender qué es un registro y ver su valor cambiar en cada paso."
            ),
            steps: vec![
                t!(
                    "Ouvrez le panneau Registres et avancez avec F10 : la valeur modifiée s'allume à chaque pas.",
                    "Open the Registers panel and step with F10: the changed value lights up at each step.",
                    "Abra el panel Registros y avance con F10: el valor modificado se enciende en cada paso."
                ),
                t!(
                    "« mov » copie sans rien lire d'autre. « add » relit sa destination avant d'écrire — le Microscope le montre.",
                    "\"mov\" copies without reading anything else. \"add\" re-reads its destination before writing — the Microscope shows it.",
                    "«mov» copia sin leer más. «add» relee su destino antes de escribir — el Microscopio lo muestra."
                ),
                t!(
                    "Le registre retenu se déplace aussi au clavier : ↑↓ change de ligne, Entrée ouvre l'édition.",
                    "The selected register also moves with the keyboard: ↑↓ changes row, Enter opens editing.",
                    "El registro seleccionado también se mueve con el teclado: ↑↓ cambia de fila, Enter edita."
                ),
            ],
            panels: vec!["editor", "registers", "instruction"],
            starter: Some(L_REGISTRES),
        },
        Lesson {
            id: "memoire",
            level: Level::Beginner,
            title: t!("La mémoire", "Memory", "La memoria"),
            goal: t!(
                "Distinguer une adresse de son contenu, et lire un octet dans le vidage hexadécimal.",
                "Tell an address from its contents, and read a byte in the hex dump.",
                "Distinguir una dirección de su contenido, y leer un byte en el volcado hexadecimal."
            ),
            steps: vec![
                t!(
                    "« mov rbx, valeur » charge l'ADRESSE. « mov rbx, [valeur] » charge le CONTENU. Les crochets déréférencent.",
                    "\"mov rbx, valeur\" loads the ADDRESS. \"mov rbx, [valeur]\" loads the CONTENTS. Brackets dereference.",
                    "«mov rbx, valeur» carga la DIRECCIÓN. «mov rbx, [valeur]» carga el CONTENIDO."
                ),
                t!(
                    "Ouvrez le panneau Mémoire : 1234 s'y lit « D2 04 00 00 … », octet de poids faible en premier.",
                    "Open the Memory panel: 1234 reads as \"D2 04 00 00 …\", least significant byte first.",
                    "Abra el panel Memoria: 1234 se lee «D2 04 00 00 …», el byte menos significativo primero."
                ),
                t!(
                    "La section « Petit-boutisme » du panneau Mémoire explique cet ordre, qui surprend tout le monde.",
                    "The \"Little-endian\" section of the Memory panel explains that order, which surprises everyone.",
                    "La sección «Little-endian» del panel Memoria explica ese orden, que sorprende a todos."
                ),
            ],
            panels: vec!["editor", "memory", "registers"],
            starter: Some(L_MEMOIRE),
        },
        Lesson {
            id: "flags",
            level: Level::Beginner,
            title: t!("Les flags", "Flags", "Los flags"),
            goal: t!(
                "Comprendre qu'une comparaison ne calcule rien : elle positionne des drapeaux.",
                "Understand that a comparison computes nothing: it sets flags.",
                "Entender que una comparación no calcula nada: posiciona flags."
            ),
            steps: vec![
                t!(
                    "« cmp a, b » effectue a − b et JETTE le résultat. Seuls les drapeaux restent.",
                    "\"cmp a, b\" computes a − b and DISCARDS the result. Only the flags remain.",
                    "«cmp a, b» calcula a − b y DESCARTA el resultado. Solo quedan los flags."
                ),
                t!(
                    "Ouvrez le panneau Flags et faites F10 sur le cmp : ZF passe à 1 quand les valeurs sont égales.",
                    "Open the Flags panel and press F10 on the cmp: ZF becomes 1 when the values are equal.",
                    "Abra el panel Flags y pulse F10 sobre el cmp: ZF pasa a 1 cuando los valores son iguales."
                ),
                t!(
                    "« sete bl » transforme un drapeau en valeur : 1 si ZF, 0 sinon. C'est un test sans saut.",
                    "\"sete bl\" turns a flag into a value: 1 if ZF, 0 otherwise. A test without branching.",
                    "«sete bl» convierte un flag en valor: 1 si ZF, 0 si no. Una prueba sin salto."
                ),
            ],
            panels: vec!["editor", "flags", "instruction"],
            starter: Some(L_FLAGS),
        },
        Lesson {
            id: "pile",
            level: Level::Beginner,
            title: t!("La pile", "The stack", "La pila"),
            goal: t!(
                "Voir RSP descendre à chaque push, et comprendre pourquoi l'ordre des pop compte.",
                "Watch RSP go down on each push, and understand why the order of pops matters.",
                "Ver RSP bajar en cada push y entender por qué importa el orden de los pop."
            ),
            steps: vec![
                t!(
                    "Ouvrez le panneau Pile. Chaque « push » diminue RSP de 8 et écrit au nouveau sommet.",
                    "Open the Stack panel. Each \"push\" lowers RSP by 8 and writes at the new top.",
                    "Abra el panel Pila. Cada «push» baja RSP en 8 y escribe en la nueva cima."
                ),
                t!(
                    "Dernier entré, premier sorti : après « push rax » puis « push rbx », le premier pop rend RBX.",
                    "Last in, first out: after \"push rax\" then \"push rbx\", the first pop returns RBX.",
                    "Último en entrar, primero en salir: tras «push rax» y «push rbx», el primer pop devuelve RBX."
                ),
                t!(
                    "Comptez toujours vos push et vos pop : un déséquilibre fait sauter « ret » dans le vide.",
                    "Always count your pushes and pops: an imbalance makes \"ret\" jump into nowhere.",
                    "Cuente siempre sus push y pop: un desequilibrio hace que «ret» salte al vacío."
                ),
            ],
            panels: vec!["editor", "stack", "registers"],
            starter: Some(L_PILE),
        },
        Lesson {
            id: "sauts",
            level: Level::Beginner,
            title: t!("Les sauts", "Jumps", "Los saltos"),
            goal: t!(
                "Enchaîner une comparaison et un saut conditionnel, et prévoir s'il sera pris.",
                "Chain a comparison and a conditional jump, and predict whether it will be taken.",
                "Encadenar una comparación y un salto condicional, y prever si se tomará."
            ),
            steps: vec![
                t!(
                    "Un saut conditionnel lit les drapeaux posés juste avant. Rien d'autre ne les relie.",
                    "A conditional jump reads the flags set just before. Nothing else links them.",
                    "Un salto condicional lee los flags puestos justo antes. Nada más los conecta."
                ),
                t!(
                    "Sur un jcc, le panneau Instruction affiche la condition ET si le saut sera pris.",
                    "On a jcc, the Instruction panel shows the condition AND whether the jump will be taken.",
                    "En un jcc, el panel Instrucción muestra la condición Y si el salto se tomará."
                ),
                t!(
                    "Attention au signe : « jg » compare des entiers signés, « ja » des non signés. −1 est plus grand que 1 en non signé.",
                    "Mind the sign: \"jg\" compares signed integers, \"ja\" unsigned ones. −1 is greater than 1 when unsigned.",
                    "Cuidado con el signo: «jg» compara con signo, «ja» sin signo. −1 es mayor que 1 sin signo."
                ),
            ],
            panels: vec!["editor", "flags", "instruction"],
            starter: Some(L_SAUTS),
        },
        Lesson {
            id: "boucles",
            level: Level::Beginner,
            title: t!("Les boucles", "Loops", "Los bucles"),
            goal: t!(
                "Écrire une boucle qui s'arrête, et la parcourir à l'envers avec la Timeline.",
                "Write a loop that terminates, and walk it backwards with the Timeline.",
                "Escribir un bucle que termina, y recorrerlo hacia atrás con la Línea de tiempo."
            ),
            steps: vec![
                t!(
                    "Une boucle est un saut en arrière conditionné. « dec » positionne ZF, « jnz » relance tant qu'il vaut 0.",
                    "A loop is a conditional backward jump. \"dec\" sets ZF, \"jnz\" repeats while it is 0.",
                    "Un bucle es un salto atrás condicional. «dec» posiciona ZF, «jnz» repite mientras valga 0."
                ),
                t!(
                    "La Timeline garde chaque tour : ← et → parcourent l'historique sans réexécuter.",
                    "The Timeline keeps every iteration: ← and → walk the history without re-running.",
                    "La Línea de tiempo guarda cada vuelta: ← y → recorren el historial sin reejecutar."
                ),
                t!(
                    "Oubliez le « dec » et la boucle ne s'arrête jamais. C'est l'erreur la plus fréquente.",
                    "Forget the \"dec\" and the loop never ends. That is the most common mistake.",
                    "Olvide el «dec» y el bucle no termina nunca. Es el error más común."
                ),
            ],
            panels: vec!["editor", "registers", "timeline"],
            starter: Some(L_BOUCLES),
        },
        // ---------------- Intermédiaire ----------------
        Lesson {
            id: "fonctions",
            level: Level::Intermediate,
            title: t!("Fonctions", "Functions", "Funciones"),
            goal: t!(
                "Écrire une fonction avec prologue et épilogue, et suivre la pile d'appels.",
                "Write a function with prologue and epilogue, and follow the call stack.",
                "Escribir una función con prólogo y epílogo, y seguir la pila de llamadas."
            ),
            steps: vec![
                t!(
                    "« call » empile l'adresse de retour puis saute ; « ret » la dépile et y revient. Le panneau Pile montre les deux mouvements.",
                    "\"call\" pushes the return address then jumps; \"ret\" pops it and goes back. The Stack panel shows both moves.",
                    "«call» apila la dirección de retorno y salta; «ret» la desapila y vuelve. El panel Pila muestra ambos movimientos."
                ),
                t!(
                    "Le prologue « push rbp / mov rbp, rsp » ouvre un cadre stable : c'est cette chaîne que remonte le panneau Pile d'appels.",
                    "The prologue \"push rbp / mov rbp, rsp\" opens a stable frame: the Call stack panel walks that very chain.",
                    "El prólogo «push rbp / mov rbp, rsp» abre un marco estable: el panel Pila de llamadas recorre esa cadena."
                ),
                t!(
                    "L'argument arrive dans RDI, le résultat repart dans RAX. Le processeur ne l'impose pas : c'est une convention, celle de la leçon suivante.",
                    "The argument comes in RDI, the result leaves in RAX. The processor does not impose this: it is a convention, the topic of the next lesson.",
                    "El argumento llega en RDI y el resultado sale en RAX. El procesador no lo impone: es un convenio, el tema de la próxima lección."
                ),
                t!(
                    "Un « push » sans son « pop » décale RSP : « ret » reprend alors n'importe quelle valeur comme adresse de retour.",
                    "A \"push\" without its \"pop\" shifts RSP: \"ret\" then takes whatever value lies there as a return address.",
                    "Un «push» sin su «pop» descoloca RSP: entonces «ret» toma cualquier valor como dirección de retorno."
                ),
            ],
            panels: vec!["editor", "callstack", "stack"],
            starter: Some(L_FONCTIONS),
        },
        Lesson {
            id: "system_v",
            level: Level::Intermediate,
            title: t!("Convention System V", "System V convention", "Convención System V"),
            goal: t!(
                "Passer des arguments dans RDI, RSI, RDX… et savoir quels registres survivent à un call.",
                "Pass arguments in RDI, RSI, RDX… and know which registers survive a call.",
                "Pasar argumentos en RDI, RSI, RDX… y saber qué registros sobreviven a un call."
            ),
            steps: vec![
                t!(
                    "Six arguments passent par registres, dans l'ordre RDI, RSI, RDX, RCX, R8, R9. Au-delà, par la pile. Le résultat revient dans RAX.",
                    "Six arguments go in registers, in the order RDI, RSI, RDX, RCX, R8, R9. Beyond that, on the stack. The result comes back in RAX.",
                    "Seis argumentos van en registros, en el orden RDI, RSI, RDX, RCX, R8, R9. Más allá, por la pila. El resultado vuelve en RAX."
                ),
                t!(
                    "Deux familles : RBX, RBP et R12 à R15 sont PRÉSERVÉS — une fonction doit les rendre intacts. Tous les autres appartiennent à l'appelant.",
                    "Two families: RBX, RBP and R12–R15 are CALLEE-SAVED — a function must hand them back untouched. All the others belong to the caller.",
                    "Dos familias: RBX, RBP y R12 a R15 son PRESERVADOS — una función debe devolverlos intactos. Los demás pertenecen al llamador."
                ),
                t!(
                    "Après un « call », ne croyez donc plus RAX, RCX, RDX, RSI, RDI ni R8 à R11 : le panneau Registres montre ce que l'appel a emporté.",
                    "So after a \"call\", stop trusting RAX, RCX, RDX, RSI, RDI or R8–R11: the Registers panel shows what the call took away.",
                    "Tras un «call», no confíe ya en RAX, RCX, RDX, RSI, RDI ni R8 a R11: el panel Registros muestra lo que la llamada se llevó."
                ),
                t!(
                    "Préserver un registre, c'est un « push » au début et un « pop » à la fin. Toute fonction qui touche RBX doit cette politesse.",
                    "Preserving a register means a \"push\" at the start and a \"pop\" at the end. Any function that touches RBX owes that courtesy.",
                    "Preservar un registro es un «push» al principio y un «pop» al final. Toda función que toca RBX debe esa cortesía."
                ),
            ],
            panels: vec!["editor", "registers", "callstack"],
            starter: Some(L_SYSTEM_V),
        },
        Lesson {
            id: "syscalls",
            level: Level::Intermediate,
            title: t!("Appels système Linux", "Linux system calls", "Llamadas al sistema Linux"),
            goal: t!(
                "Lire et écrire sur un descripteur, et comprendre pourquoi R10 remplace RCX.",
                "Read and write on a descriptor, and understand why R10 replaces RCX.",
                "Leer y escribir en un descriptor, y entender por qué R10 sustituye a RCX."
            ),
            steps: vec![
                t!(
                    "Un appel système n'est pas un appel de fonction : RAX porte le numéro, et les arguments vont dans RDI, RSI, RDX, R10, R8, R9.",
                    "A system call is not a function call: RAX carries the number, and the arguments go in RDI, RSI, RDX, R10, R8, R9.",
                    "Una llamada al sistema no es una llamada a función: RAX lleva el número, y los argumentos van en RDI, RSI, RDX, R10, R8, R9."
                ),
                t!(
                    "R10 remplace RCX parce que l'instruction « syscall » écrase RCX, où elle range l'adresse de retour, et R11, où elle range les flags.",
                    "R10 replaces RCX because the \"syscall\" instruction clobbers RCX, where it stores the return address, and R11, where it stores the flags.",
                    "R10 sustituye a RCX porque la instrucción «syscall» destruye RCX, donde guarda la dirección de retorno, y R11, donde guarda los flags."
                ),
                t!(
                    "RAX reçoit la réponse : un nombre positif ou nul en cas de succès, −errno en cas d'échec. RAX = −9, c'est EBADF, mauvais descripteur.",
                    "RAX receives the answer: zero or positive on success, −errno on failure. RAX = −9 is EBADF, bad file descriptor.",
                    "RAX recibe la respuesta: cero o positivo si va bien, −errno si falla. RAX = −9 es EBADF, descriptor incorrecto."
                ),
                t!(
                    "« $ - msg » fait compter l'assembleur : une longueur écrite à la main devient fausse dès qu'on retouche le message.",
                    "\"$ - msg\" lets the assembler count: a hand-written length goes stale the moment you edit the message.",
                    "«$ - msg» deja contar al ensamblador: una longitud escrita a mano se vuelve falsa en cuanto se retoca el mensaje."
                ),
            ],
            panels: vec!["editor", "syscalls", "console"],
            starter: Some(L_SYSCALLS),
        },
        Lesson {
            id: "tas",
            level: Level::Intermediate,
            title: t!("Le tas", "The heap", "El montículo"),
            goal: t!(
                "Demander de la mémoire au noyau avec brk ou mmap, et la voir apparaître.",
                "Ask the kernel for memory with brk or mmap, and watch it appear.",
                "Pedir memoria al núcleo con brk o mmap, y verla aparecer."
            ),
            steps: vec![
                t!(
                    "« .data » et « .bss » sont figés à l'assemblage. Le tas, lui, se demande au noyau pendant l'exécution — et il n'existe pas avant.",
                    "\".data\" and \".bss\" are fixed at assembly time. The heap is asked of the kernel while running — and does not exist before that.",
                    "«.data» y «.bss» quedan fijados al ensamblar. El montículo se pide al núcleo en ejecución — y antes no existe."
                ),
                t!(
                    "« brk(0) » ne fait que lire la limite. « brk(limite + n) » la repousse et renvoie la NOUVELLE limite : comparez-la, le noyau peut refuser.",
                    "\"brk(0)\" merely reads the limit. \"brk(limit + n)\" pushes it up and returns the NEW limit: compare it, the kernel may refuse.",
                    "«brk(0)» solo lee el límite. «brk(límite + n)» lo empuja y devuelve el NUEVO límite: compárelo, el núcleo puede negarse."
                ),
                t!(
                    "Tel quel, le programme meurt sur SIGSEGV : il écrit au-dessus d'une limite qu'il n'a pas repoussée. Lancez-le une fois avant de corriger.",
                    "As given, the program dies on SIGSEGV: it writes past a limit it never pushed up. Run it once before fixing it.",
                    "Tal cual, el programa muere con SIGSEGV: escribe más allá de un límite que no empujó. Ejecútelo una vez antes de corregirlo."
                ),
                t!(
                    "Le tas monte, la pile descend, et ils se font face. malloc n'est rien d'autre qu'un découpeur posé sur brk et mmap.",
                    "The heap grows up, the stack grows down, and they face each other. malloc is nothing but a slicer sitting on brk and mmap.",
                    "El montículo sube, la pila baja, y se miran de frente. malloc no es más que un repartidor montado sobre brk y mmap."
                ),
            ],
            panels: vec!["editor", "memory", "stack"],
            starter: Some(L_TAS),
        },
        Lesson {
            id: "tableaux",
            level: Level::Intermediate,
            title: t!("Tableaux", "Arrays", "Arrays"),
            goal: t!(
                "Parcourir un tableau avec l'adressage base + index × échelle.",
                "Walk an array with base + index × scale addressing.",
                "Recorrer un array con direccionamiento base + índice × escala."
            ),
            steps: vec![
                t!(
                    "« [tab + rcx*8] » se lit base + index × échelle. Le processeur calcule cette adresse dans l'instruction même, sans rien coûter de plus.",
                    "\"[tab + rcx*8]\" reads as base + index × scale. The processor computes that address inside the instruction itself, at no extra cost.",
                    "«[tab + rcx*8]» se lee base + índice × escala. El procesador calcula esa dirección dentro de la propia instrucción, sin coste extra."
                ),
                t!(
                    "L'échelle ne peut valoir que 1, 2, 4 ou 8 — les tailles d'un élément. Pour tout autre pas, il faut multiplier à la main.",
                    "The scale can only be 1, 2, 4 or 8 — the element sizes. For any other stride, you must multiply by hand.",
                    "La escala solo puede valer 1, 2, 4 u 8 — los tamaños de un elemento. Para otro paso, hay que multiplicar a mano."
                ),
                t!(
                    "Le panneau Désassemblage le confirme : une seule instruction, aucun calcul d'adresse séparé avant la lecture.",
                    "The Disassembly panel confirms it: a single instruction, no separate address computation before the load.",
                    "El panel Desensamblado lo confirma: una sola instrucción, sin cálculo de dirección aparte antes de la lectura."
                ),
                t!(
                    "« equ ($ - tab) / 8 » laisse l'assembleur compter les éléments. Un 5 écrit à la main devient faux au premier ajout.",
                    "\"equ ($ - tab) / 8\" lets the assembler count the elements. A hand-written 5 becomes wrong the first time you add a value.",
                    "«equ ($ - tab) / 8» deja contar los elementos al ensamblador. Un 5 escrito a mano se vuelve falso al primer añadido."
                ),
            ],
            panels: vec!["editor", "memory", "disasm"],
            starter: Some(L_TABLEAUX),
        },
        Lesson {
            id: "structures",
            level: Level::Intermediate,
            title: t!("Structures", "Structs", "Estructuras"),
            goal: t!(
                "Ranger plusieurs champs à des décalages fixes, et lire le déplacement dans l'encodage.",
                "Lay out fields at fixed offsets, and read the displacement in the encoding.",
                "Colocar campos en desplazamientos fijos, y leer el desplazamiento en la codificación."
            ),
            steps: vec![
                t!(
                    "Une structure n'existe pas dans le processeur : ce sont des décalages, que « equ » se contente de nommer. La machine ne voit qu'un déplacement.",
                    "A struct does not exist in the processor: they are offsets, which \"equ\" merely gives names to. The machine only ever sees a displacement.",
                    "Una estructura no existe en el procesador: son desplazamientos, a los que «equ» solo pone nombre. La máquina solo ve un desplazamiento."
                ),
                t!(
                    "Le panneau Instruction montre ce déplacement dans l'encodage : « [rsi+8] » ajoute un seul octet, 08. Lire un champ ne coûte pas plus qu'une lecture simple.",
                    "The Instruction panel shows that displacement in the encoding: \"[rsi+8]\" adds a single byte, 08. Reading a field costs no more than a plain load.",
                    "El panel Instrucción muestra ese desplazamiento en la codificación: «[rsi+8]» añade un solo byte, 08. Leer un campo no cuesta más que una lectura simple."
                ),
                t!(
                    "Alignement : un champ de 8 octets veut une adresse multiple de 8. Rangez les champs du plus grand au plus petit et vous ne perdez aucun octet de bourrage.",
                    "Alignment: an 8-byte field wants an address that is a multiple of 8. Order the fields largest first and you lose no padding bytes.",
                    "Alineación: un campo de 8 bytes quiere una dirección múltiplo de 8. Ordene los campos de mayor a menor y no perderá bytes de relleno."
                ),
                t!(
                    "L'échelle s'arrête à 8 : pour un tableau de structures de 16 octets, on avance avec « add rsi, 16 » plutôt qu'avec un index.",
                    "The scale stops at 8: for an array of 16-byte structs, you step with \"add rsi, 16\" rather than with an index.",
                    "La escala se detiene en 8: para un array de estructuras de 16 bytes, se avanza con «add rsi, 16» en vez de con un índice."
                ),
            ],
            panels: vec!["editor", "memory", "instruction"],
            starter: Some(L_STRUCTURES),
        },
        Lesson {
            id: "chaines",
            level: Level::Intermediate,
            title: t!("Chaînes de caractères", "Strings", "Cadenas de caracteres"),
            goal: t!(
                "Mesurer, comparer et transformer une chaîne terminée par zéro.",
                "Measure, compare and transform a zero-terminated string.",
                "Medir, comparar y transformar una cadena terminada en cero."
            ),
            steps: vec![
                t!(
                    "Une chaîne C ne range nulle part sa longueur : elle s'arrête au premier octet nul. La mesurer coûte un parcours complet, à chaque fois.",
                    "A C string stores its length nowhere: it stops at the first null byte. Measuring it costs a full walk, every single time.",
                    "Una cadena C no guarda su longitud en ninguna parte: termina en el primer byte nulo. Medirla cuesta un recorrido completo, cada vez."
                ),
                t!(
                    "« mov al, [rsi + rbx] » lit UN octet. Avec RAX on en prendrait huit d'un coup, et on lirait au-delà de la chaîne.",
                    "\"mov al, [rsi + rbx]\" reads ONE byte. With RAX you would grab eight at once, and read past the end of the string.",
                    "«mov al, [rsi + rbx]» lee UN byte. Con RAX se tomarían ocho de golpe, y se leería más allá de la cadena."
                ),
                t!(
                    "« test al, al » est un ET dont on jette le résultat : comme « cmp al, 0 », mais plus court à encoder. ZF passe à 1 sur le zéro final.",
                    "\"test al, al\" is an AND whose result is discarded: like \"cmp al, 0\", but shorter to encode. ZF turns 1 on the terminating zero.",
                    "«test al, al» es un Y cuyo resultado se descarta: como «cmp al, 0», pero más corto de codificar. ZF pasa a 1 en el cero final."
                ),
                t!(
                    "Le panneau Mémoire montre les octets à l'adresse « texte » : 42 6F 6E … puis 00. Ce 00 fait partie de la chaîne autant que les lettres.",
                    "The Memory panel shows the bytes at address \"texte\": 42 6F 6E … then 00. That 00 belongs to the string just as much as the letters do.",
                    "El panel Memoria muestra los bytes en la dirección «texte»: 42 6F 6E … y luego 00. Ese 00 forma parte de la cadena tanto como las letras."
                ),
            ],
            panels: vec!["editor", "memory", "console"],
            starter: Some(L_CHAINES),
        },
        // ---------------- Avancé ----------------
        Lesson {
            id: "elf",
            level: Level::Advanced,
            title: t!("Le format ELF", "The ELF format", "El formato ELF"),
            goal: t!(
                "Reconnaître les en-têtes et les sections d'un exécutable Linux.",
                "Recognise the headers and sections of a Linux executable.",
                "Reconocer las cabeceras y secciones de un ejecutable Linux."
            ),
            steps: vec![
                t!(
                    "Le noyau mappe le fichier ELF en entier, en-tête compris. Un programme peut donc se relire lui-même : c'est ce que fait celui-ci.",
                    "The kernel maps the whole ELF file, header included. A program can therefore read itself back: that is what this one does.",
                    "El núcleo mapea el archivo ELF entero, cabecera incluida. Un programa puede así releerse a sí mismo: eso hace este."
                ),
                t!(
                    "« __ehdr_start » n'est défini nulle part dans le source : c'est ld qui le fabrique. La leçon Édition de liens y revient.",
                    "\"__ehdr_start\" is defined nowhere in the source: ld manufactures it. The Linking lesson comes back to this.",
                    "«__ehdr_start» no está definido en ninguna parte del fuente: lo fabrica ld. La lección Enlazado vuelve sobre ello."
                ),
                t!(
                    "Deux tables, deux lecteurs : les SEGMENTS (e_phoff) disent au noyau quoi charger, les SECTIONS (e_shoff) servent à ld et disparaissent à l'exécution.",
                    "Two tables, two readers: the SEGMENTS (e_phoff) tell the kernel what to load, the SECTIONS (e_shoff) serve ld and are irrelevant at run time.",
                    "Dos tablas, dos lectores: los SEGMENTOS (e_phoff) dicen al núcleo qué cargar, las SECCIONES (e_shoff) sirven a ld y desaparecen al ejecutar."
                ),
                t!(
                    "Le panneau Carte mémoire montre le résultat : où chaque morceau a atterri, et avec quels droits — lecture, écriture, exécution.",
                    "The Memory map panel shows the result: where each piece landed, and with which rights — read, write, execute.",
                    "El panel Mapa de memoria muestra el resultado: dónde cayó cada trozo, y con qué permisos — lectura, escritura, ejecución."
                ),
            ],
            panels: vec!["editor", "memmap", "memory"],
            starter: Some(L_ELF),
        },
        Lesson {
            id: "linking",
            level: Level::Advanced,
            title: t!("Édition de liens", "Linking", "Enlazado"),
            goal: t!(
                "Comprendre ce que ld assemble entre le fichier objet et l'exécutable.",
                "Understand what ld puts together between object file and executable.",
                "Entender qué une ld entre el archivo objeto y el ejecutable."
            ),
            steps: vec![
                t!(
                    "La Console montre les deux commandes : nasm fabrique un .o où rien n'a d'adresse définitive, ld en fait un exécutable où tout en a une.",
                    "The Console shows both commands: nasm builds a .o where nothing has a final address, ld turns it into an executable where everything does.",
                    "La Consola muestra ambos comandos: nasm fabrica un .o donde nada tiene dirección definitiva, ld hace un ejecutable donde todo la tiene."
                ),
                t!(
                    "Cette soustraction, nasm ne peut PAS la calculer : les deux symboles ne sont pas à lui, et .bss n'a pas encore de place. Seul ld a la vue d'ensemble.",
                    "nasm CANNOT compute this subtraction: neither symbol is its own, and .bss has no place yet. Only ld has the whole picture.",
                    "nasm NO puede calcular esta resta: ninguno de los dos símbolos es suyo, y .bss aún no tiene sitio. Solo ld tiene la vista completa."
                ),
                t!(
                    "« resb » ne met aucun octet dans le fichier : .bss est une simple promesse de place, que le noyau remplit de zéros au chargement.",
                    "\"resb\" puts no byte in the file: .bss is merely a promise of room, which the kernel fills with zeros at load time.",
                    "«resb» no pone ningún byte en el archivo: .bss es solo una promesa de espacio, que el núcleo llena de ceros al cargar."
                ),
                t!(
                    "« _end » marque la fin de l'image mémoire — et c'est précisément là que commence le tas. La leçon Le tas partait de ce même endroit.",
                    "\"_end\" marks the end of the memory image — and that is exactly where the heap begins. The Heap lesson started from that very spot.",
                    "«_end» marca el fin de la imagen en memoria — y ahí justo empieza el montículo. La lección El montículo partía de ese mismo punto."
                ),
            ],
            panels: vec!["editor", "console", "memmap"],
            starter: Some(L_LINKING),
        },
        Lesson {
            id: "plt_got",
            level: Level::Advanced,
            title: t!("PLT et GOT", "PLT and GOT", "PLT y GOT"),
            goal: t!(
                "Suivre un appel de bibliothèque partagée à travers ses tables d'indirection.",
                "Follow a shared-library call through its indirection tables.",
                "Seguir una llamada a biblioteca compartida por sus tablas de indirección."
            ),
            steps: vec![
                t!(
                    "Une GOT n'est rien d'autre qu'un tableau d'adresses de fonctions. Celle de cette leçon est écrite à la main : le mécanisme est le vrai, seul l'automatisme manque.",
                    "A GOT is nothing but an array of function addresses. This lesson's is written by hand: the mechanism is the real one, only the automation is missing.",
                    "Una GOT no es más que un array de direcciones de funciones. La de esta lección está escrita a mano: el mecanismo es el real, solo falta el automatismo."
                ),
                t!(
                    "« call rsi » saute à l'adresse CONTENUE dans RSI ; « call [rsi] » va d'abord chercher l'adresse en mémoire. Une indirection de plus, et tout devient déplaçable.",
                    "\"call rsi\" jumps to the address HELD in RSI; \"call [rsi]\" first fetches the address from memory. One more indirection, and everything becomes relocatable.",
                    "«call rsi» salta a la dirección CONTENIDA en RSI; «call [rsi]» busca antes la dirección en memoria. Una indirección más, y todo se vuelve reubicable."
                ),
                t!(
                    "Dans un vrai binaire dynamique, la GOT contient d'abord l'adresse du résolveur. Le premier appel le réveille, il écrit la vraie adresse, et les suivants vont droit au but.",
                    "In a real dynamic binary, the GOT first holds the resolver's address. The first call wakes it, it writes the true address, and later calls go straight there.",
                    "En un binario dinámico real, la GOT contiene primero la dirección del resolvedor. La primera llamada lo despierta, escribe la dirección real, y las siguientes van directas."
                ),
                t!(
                    "C'est aussi la faiblesse : une GOT accessible en écriture se récrit. « relro » existe pour la refermer une fois la résolution faite.",
                    "That is also the weakness: a writable GOT can be rewritten. \"relro\" exists to seal it once resolution is done.",
                    "Esa es también la debilidad: una GOT escribible se puede reescribir. «relro» existe para cerrarla una vez hecha la resolución."
                ),
            ],
            panels: vec!["editor", "disasm", "memory"],
            starter: Some(L_PLT_GOT),
        },
        Lesson {
            id: "relocations",
            level: Level::Advanced,
            title: t!("Relocations", "Relocations", "Relocalizaciones"),
            goal: t!(
                "Voir quelles adresses sont corrigées au chargement, et pourquoi.",
                "See which addresses are fixed up at load time, and why.",
                "Ver qué direcciones se corrigen al cargar, y por qué."
            ),
            steps: vec![
                t!(
                    "Une relocation est une consigne laissée à ld : « à cet endroit du code, écris l'adresse de tel symbole ». nasm y met des zéros en attendant.",
                    "A relocation is an instruction left for ld: \"at this spot in the code, write the address of that symbol\". nasm puts zeros there meanwhile.",
                    "Una relocalización es un encargo dejado a ld: «en este punto del código, escribe la dirección de tal símbolo». nasm pone ceros mientras tanto."
                ),
                t!(
                    "R_X86_64_64 demande une adresse absolue, R_X86_64_PC32 un écart depuis RIP. Même cible, deux façons de la dire — d'où la soustraction nulle.",
                    "R_X86_64_64 asks for an absolute address, R_X86_64_PC32 for an offset from RIP. Same target, two ways of saying it — hence the zero difference.",
                    "R_X86_64_64 pide una dirección absoluta, R_X86_64_PC32 un desplazamiento desde RIP. Mismo destino, dos maneras de decirlo — de ahí la resta nula."
                ),
                t!(
                    "Le panneau Désassemblage montre le résultat une fois ld passé : les zéros ont disparu, l'adresse est là, en clair dans l'instruction.",
                    "The Disassembly panel shows the result once ld has run: the zeros are gone, the address sits there in plain sight inside the instruction.",
                    "El panel Desensamblado muestra el resultado tras pasar ld: los ceros han desaparecido, la dirección está ahí, a la vista en la instrucción."
                ),
                t!(
                    "Le RIP-relatif ne dépend d'aucune adresse de chargement : c'est ce qui rend un code déplaçable, et c'est pourquoi PIE et bibliothèques n'utilisent que lui.",
                    "RIP-relative depends on no load address: that is what makes code relocatable, and why PIE binaries and libraries use nothing else.",
                    "El RIP-relativo no depende de ninguna dirección de carga: eso hace el código reubicable, y por eso PIE y bibliotecas solo usan esa forma."
                ),
            ],
            panels: vec!["editor", "disasm", "memory"],
            starter: Some(L_RELOCATIONS),
        },
        Lesson {
            id: "simd",
            level: Level::Advanced,
            title: t!("SIMD et AVX", "SIMD and AVX", "SIMD y AVX"),
            goal: t!(
                "Traiter plusieurs valeurs par instruction avec les registres vectoriels.",
                "Process several values per instruction with vector registers.",
                "Procesar varios valores por instrucción con registros vectoriales."
            ),
            steps: vec![
                t!(
                    "Un registre XMM fait 128 bits : quatre entiers de 32 bits côte à côte. « paddd » les additionne tous les quatre en une seule instruction.",
                    "An XMM register is 128 bits: four 32-bit integers side by side. \"paddd\" adds all four in a single instruction.",
                    "Un registro XMM tiene 128 bits: cuatro enteros de 32 bits uno al lado del otro. «paddd» los suma los cuatro en una sola instrucción."
                ),
                t!(
                    "La dernière lettre donne la découpe : paddb par octets, paddw par mots, paddd par doubles mots, paddq par quadruples. Le registre ne change pas, le sens si.",
                    "The last letter gives the slicing: paddb by bytes, paddw by words, paddd by doublewords, paddq by quadwords. The register stays, the meaning changes.",
                    "La última letra da el troceado: paddb por bytes, paddw por palabras, paddd por dobles, paddq por cuádruples. El registro no cambia, el sentido sí."
                ),
                t!(
                    "Chaque tranche s'arrête à son bord : la retenue ne passe pas à la voisine. C'est pourquoi 65535 + 1 ne donne le bon résultat qu'en découpe 32 bits.",
                    "Each lane stops at its edge: the carry does not cross into its neighbour. That is why 65535 + 1 only gives the right answer at 32-bit slicing.",
                    "Cada carril se detiene en su borde: el acarreo no pasa al vecino. Por eso 65535 + 1 solo da el resultado correcto con troceado de 32 bits."
                ),
                t!(
                    "« align 16 » n'est pas décoratif : movdqa exige une adresse multiple de 16 et plante sinon. movdqu accepte tout, contre un peu de vitesse.",
                    "\"align 16\" is not decorative: movdqa demands a 16-multiple address and faults otherwise. movdqu takes anything, at a small speed cost.",
                    "«align 16» no es decorativo: movdqa exige una dirección múltiplo de 16 y falla si no. movdqu acepta cualquiera, a cambio de algo de velocidad."
                ),
                t!(
                    "AVX élargit à 256 bits (YMM) puis 512 (ZMM), avec la même idée. Mais tout processeur ne les a pas : un binaire qui les suppose plante ailleurs.",
                    "AVX widens to 256 bits (YMM) then 512 (ZMM), on the same idea. But not every processor has them: a binary that assumes them crashes elsewhere.",
                    "AVX amplía a 256 bits (YMM) y luego 512 (ZMM), con la misma idea. Pero no todo procesador los tiene: un binario que los supone falla en otra máquina."
                ),
            ],
            panels: vec!["editor", "registers", "instruction"],
            starter: Some(L_SIMD),
        },
        Lesson {
            id: "optimisation",
            level: Level::Advanced,
            title: t!("Optimisations", "Optimisations", "Optimizaciones"),
            goal: t!(
                "Mesurer avant de récrire : coût des instructions et prédiction de branchement.",
                "Measure before rewriting: instruction cost and branch prediction.",
                "Medir antes de reescribir: coste de instrucciones y predicción de saltos."
            ),
            steps: vec![
                t!(
                    "« lea » calcule une adresse, mais rien n'oblige à s'en servir comme telle : c'est l'additionneur-multiplieur du processeur, offert par l'adressage.",
                    "\"lea\" computes an address, but nothing forces you to use it as one: it is the processor's adder-multiplier, handed over by the addressing mode.",
                    "«lea» calcula una dirección, pero nada obliga a usarla como tal: es el sumador-multiplicador del procesador, regalado por el direccionamiento."
                ),
                t!(
                    "Et pourtant : « imul » coûte trois cycles sur un processeur moderne, et il est le seul à dire ce qu'on veut dire. La ruse d'hier est souvent la lourdeur d'aujourd'hui.",
                    "And yet: \"imul\" costs three cycles on a modern processor, and is the only form that says what you mean. Yesterday's trick is often today's clutter.",
                    "Y sin embargo: «imul» cuesta tres ciclos en un procesador moderno, y es la única forma que dice lo que se quiere decir. La astucia de ayer suele ser el estorbo de hoy."
                ),
                t!(
                    "La Timeline compte les instructions réellement exécutées. C'est la seule mesure qui vaille : le reste est une intuition, et l'intuition se trompe.",
                    "The Timeline counts the instructions actually executed. That is the only measure worth having: the rest is intuition, and intuition is wrong.",
                    "La Línea de tiempo cuenta las instrucciones realmente ejecutadas. Es la única medida que vale: lo demás es intuición, y la intuición se equivoca."
                ),
                t!(
                    "Un saut bien prédit ne coûte presque rien ; un saut imprévisible vide le pipeline. Supprimer un branchement imprévisible rapporte plus que dix « lea ».",
                    "A well-predicted branch costs almost nothing; an unpredictable one flushes the pipeline. Removing one unpredictable branch pays more than ten \"lea\"s.",
                    "Un salto bien predicho casi no cuesta; uno impredecible vacía el cauce. Quitar un salto impredecible rinde más que diez «lea»."
                ),
            ],
            panels: vec!["editor", "disasm", "timeline"],
            starter: Some(L_OPTIMISATION),
        },
        // ---------------- Expert ----------------
        Lesson {
            id: "reverse",
            level: Level::Expert,
            title: t!("Rétro-ingénierie", "Reverse engineering", "Ingeniería inversa"),
            goal: t!(
                "Reconstituer l'intention d'un binaire dont on n'a pas les sources.",
                "Reconstruct the intent of a binary you have no source for.",
                "Reconstruir la intención de un binario sin fuentes."
            ),
            steps: vec![
                t!(
                    "Sans les sources, on lit ce que le code FAIT, pas ce qu'il voulait dire. Ici, une boucle qui XOR chaque octet d'un bloc : la signature d'un déchiffrement.",
                    "Without sources, you read what the code DOES, not what it meant. Here, a loop that XORs each byte of a block: the signature of a decryption.",
                    "Sin fuentes, se lee lo que el código HACE, no lo que quería decir. Aquí, un bucle que hace XOR a cada byte de un bloque: la firma de un descifrado."
                ),
                t!(
                    "Le XOR est sa propre inverse : chiffrer et déchiffrer sont la même opération. C'est pourquoi on le retrouve partout, du plus naïf des malwares aux vrais protocoles.",
                    "XOR is its own inverse: encrypting and decrypting are the same operation. That is why it turns up everywhere, from the crudest malware to real protocols.",
                    "El XOR es su propia inversa: cifrar y descifrar son la misma operación. Por eso aparece en todas partes, del malware más burdo a protocolos reales."
                ),
                t!(
                    "Le panneau Mémoire montre les octets clairs apparaître un à un au fil des pas : la donnée se reconstitue sous les yeux, ce que ne montre aucun listing statique.",
                    "The Memory panel shows the plaintext bytes appear one by one as you step: the data rebuilds before your eyes, which no static listing shows.",
                    "El panel Memoria muestra los bytes claros aparecer uno a uno al avanzar: el dato se reconstruye ante los ojos, algo que ningún listado estático muestra."
                ),
                t!(
                    "Trouver la clé, c'est souvent deviner un octet connu du clair — un octet nul de bourrage, une lettre attendue — et en déduire le reste.",
                    "Finding the key is often guessing one known plaintext byte — a null padding byte, an expected letter — and deducing the rest.",
                    "Hallar la clave suele ser adivinar un byte conocido del claro — un byte nulo de relleno, una letra esperada — y deducir el resto."
                ),
            ],
            panels: vec!["editor", "memory", "disasm"],
            starter: Some(L_REVERSE),
        },
        Lesson {
            id: "desassemblage",
            level: Level::Expert,
            title: t!("Désassemblage", "Disassembly", "Desensamblado"),
            goal: t!(
                "Lire le code machine sans étiquettes, et retrouver les frontières d'instruction.",
                "Read machine code without labels, and find instruction boundaries.",
                "Leer código máquina sin etiquetas y hallar los límites de instrucción."
            ),
            steps: vec![
                t!(
                    "Le processeur ne voit pas d'instructions, mais des octets. Il reconnaît chacune à son opcode, en déduit sa longueur, et sait où commence la suivante.",
                    "The processor sees no instructions, only bytes. It knows each by its opcode, deduces its length, and thus where the next one starts.",
                    "El procesador no ve instrucciones, sino bytes. Reconoce cada una por su opcode, deduce su longitud, y así dónde empieza la siguiente."
                ),
                t!(
                    "Ces sept octets déposés en « db » sont exécutés comme n'importe quels autres : rien ne distingue le code des données, sinon l'endroit où l'on saute.",
                    "These seven \"db\" bytes execute like any others: nothing tells code from data, save the place you jump to.",
                    "Estos siete bytes en «db» se ejecutan como cualesquiera: nada distingue código de datos, salvo el punto al que se salta."
                ),
                t!(
                    "Le panneau Désassemblage relit ces octets et retrouve « mov rbx, … ». Le panneau Instruction en détaille l'encodage : REX, opcode, ModR/M, immédiat.",
                    "The Disassembly panel reads these bytes back as \"mov rbx, …\". The Instruction panel breaks down the encoding: REX, opcode, ModR/M, immediate.",
                    "El panel Desensamblado relee estos bytes como «mov rbx, …». El panel Instrucción detalla la codificación: REX, opcode, ModR/M, inmediato."
                ),
                t!(
                    "Se tromper d'un octet sur le point d'entrée décale TOUT le désassemblage qui suit : les frontières glissent, et le code lu devient une fiction.",
                    "Being one byte off on the entry point shifts ALL the disassembly that follows: boundaries slide, and the code you read becomes fiction.",
                    "Equivocarse en un byte en el punto de entrada desplaza TODO el desensamblado que sigue: las fronteras se corren, y el código leído se vuelve ficción."
                ),
            ],
            panels: vec!["editor", "disasm", "instruction"],
            starter: Some(L_DESASSEMBLAGE),
        },
        Lesson {
            id: "syscalls_avances",
            level: Level::Expert,
            title: t!("Appels système avancés", "Advanced system calls", "Llamadas al sistema avanzadas"),
            goal: t!(
                "Processus, signaux, mémoire partagée : au-delà de read et write.",
                "Processes, signals, shared memory: beyond read and write.",
                "Procesos, señales, memoria compartida: más allá de read y write."
            ),
            steps: vec![
                t!(
                    "« pipe2 » fabrique un canal : deux descripteurs reliés, ce qu'on écrit dans l'un ressort de l'autre. C'est ainsi qu'un shell relie deux commandes par « | ».",
                    "\"pipe2\" makes a channel: two linked descriptors, what you write into one comes out of the other. That is how a shell links two commands with \"|\".",
                    "«pipe2» fabrica un canal: dos descriptores enlazados, lo que se escribe en uno sale por el otro. Así enlaza un shell dos comandos con «|»."
                ),
                t!(
                    "pipe2 écrit les deux descripteurs en mémoire, pas dans un registre : on lui passe l'ADRESSE d'un tableau, et on relit fds[0] et fds[1] ensuite.",
                    "pipe2 writes the two descriptors to memory, not to a register: you pass the ADDRESS of an array, then read fds[0] and fds[1] back.",
                    "pipe2 escribe los dos descriptores en memoria, no en un registro: se le pasa la DIRECCIÓN de un array, y luego se leen fds[0] y fds[1]."
                ),
                t!(
                    "Le panneau Appels système journalise chaque appel avec ses arguments et sa réponse : on y suit le tube se créer, puis l'octet passer d'un descripteur à l'autre.",
                    "The System calls panel logs each call with its arguments and its answer: you watch the pipe being created, then the byte cross from one descriptor to the other.",
                    "El panel Llamadas al sistema registra cada llamada con sus argumentos y su respuesta: se ve crearse el tubo, y luego el byte pasar de un descriptor al otro."
                ),
                t!(
                    "Les autres canaux du noyau suivent la même logique : fork duplique le processus, les signaux l'interrompent, mmap partage une page. Tous passent par des descripteurs ou des adresses.",
                    "The kernel's other channels follow the same logic: fork duplicates the process, signals interrupt it, mmap shares a page. All go through descriptors or addresses.",
                    "Los demás canales del núcleo siguen la misma lógica: fork duplica el proceso, las señales lo interrumpen, mmap comparte una página. Todos pasan por descriptores o direcciones."
                ),
            ],
            panels: vec!["editor", "syscalls", "memory"],
            starter: Some(L_SYSCALLS_AVANCES),
        },
        Lesson {
            id: "shellcode",
            level: Level::Expert,
            title: t!("Shellcode", "Shellcode", "Shellcode"),
            goal: t!(
                "Écrire du code sans octet nul ni adresse absolue, et comprendre les contraintes.",
                "Write code with no null byte and no absolute address, and understand the constraints.",
                "Escribir código sin bytes nulos ni direcciones absolutas, y entender las restricciones."
            ),
            steps: vec![
                t!(
                    "Un shellcode est injecté dans un programme déjà lancé : il n'a ni section .data ni adresse fixe. Il transporte ses données avec lui, sur la pile.",
                    "Shellcode is injected into a running program: it has no .data section and no fixed address. It carries its data with it, on the stack.",
                    "Un shellcode se inyecta en un programa ya en marcha: no tiene sección .data ni dirección fija. Lleva sus datos consigo, en la pila."
                ),
                t!(
                    "Zéro octet nul : souvent transporté par une fonction de chaîne, il serait tronqué au premier 00. « mov rax, 60 » en contient ; « xor rax,rax / mov al,60 » n'en a aucun.",
                    "No null byte: often carried by a string function, it would be cut at the first 00. \"mov rax, 60\" contains some; \"xor rax,rax / mov al,60\" has none.",
                    "Cero bytes nulos: transportado a menudo por una función de cadena, se cortaría en el primer 00. «mov rax, 60» contiene algunos; «xor rax,rax / mov al,60» ninguno."
                ),
                t!(
                    "Le panneau Instruction montre l'encodage octet par octet : c'est LÀ qu'on vérifie l'absence de nul, pas dans le résultat. Comparez-y les deux façons d'écrire 60.",
                    "The Instruction panel shows the encoding byte by byte: THAT is where you check for the absence of nulls, not in the result. Compare there the two ways of writing 60.",
                    "El panel Instrucción muestra la codificación byte a byte: AHÍ se comprueba la ausencia de nulos, no en el resultado. Compare ahí las dos formas de escribir 60."
                ),
                t!(
                    "Le contrôle automatique ne juge que le résultat : ici, la longueur mesurée. C'est à vous de lire l'encodage pour la seconde contrainte — l'outil la montre, il ne la note pas.",
                    "The automatic check only judges the result: here, the measured length. Reading the encoding for the second constraint is up to you — the tool shows it, it does not grade it.",
                    "El control automático solo juzga el resultado: aquí, la longitud medida. Leer la codificación para la segunda restricción le toca a usted — la herramienta la muestra, no la califica."
                ),
            ],
            panels: vec!["editor", "instruction", "stack"],
            starter: Some(L_SHELLCODE),
        },
        Lesson {
            id: "exploitation",
            level: Level::Expert,
            title: t!("Exploitation de binaires", "Binary exploitation", "Explotación de binarios"),
            goal: t!(
                "Comprendre comment un débordement de pile détourne l'adresse de retour — et ce qui l'en empêche.",
                "Understand how a stack overflow hijacks the return address — and what prevents it.",
                "Entender cómo un desbordamiento de pila secuestra la dirección de retorno — y qué lo impide."
            ),
            steps: vec![
                t!(
                    "« call » empile l'adresse de retour ; « ret » y revient. Toute la sécurité de ce va-et-vient tient à ce que cette adresse, posée sur la pile, reste intacte.",
                    "\"call\" pushes the return address; \"ret\" goes back to it. The safety of that round trip rests entirely on that address, sitting on the stack, staying intact.",
                    "«call» apila la dirección de retorno; «ret» vuelve a ella. Toda la seguridad de ese ida y vuelta depende de que esa dirección, puesta en la pila, quede intacta."
                ),
                t!(
                    "Un tampon local est en dessous de cette adresse sur la pile. Écrire au-delà de sa taille — une copie de saisie non vérifiée — finit par l'atteindre et la remplacer.",
                    "A local buffer sits below that address on the stack. Writing past its size — an unchecked input copy — eventually reaches and overwrites it.",
                    "Un tampón local está por debajo de esa dirección en la pila. Escribir más allá de su tamaño — una copia de entrada sin verificar — acaba alcanzándola y sustituyéndola."
                ),
                t!(
                    "Le panneau Pile montre l'écriture atteindre le décalage [rbp+8] : à ce pas précis, l'adresse de retour change de valeur, et « ret » emmènera vers « gagne ».",
                    "The Stack panel shows the write reaching offset [rbp+8]: at that exact step, the return address changes value, and \"ret\" will lead to \"gagne\".",
                    "El panel Pila muestra la escritura alcanzar el desplazamiento [rbp+8]: en ese paso exacto, la dirección de retorno cambia de valor, y «ret» llevará a «gagne»."
                ),
                t!(
                    "Trois défenses le contrent : un canari entre tampon et adresse (modifié = arrêt), une pile non exécutable (NX), et l'aléa d'adressage (ASLR) qui cache où sauter.",
                    "Three defences counter it: a canary between buffer and address (modified = abort), a non-executable stack (NX), and address randomisation (ASLR) hiding where to jump.",
                    "Tres defensas lo contrarrestan: un canario entre tampón y dirección (modificado = aborto), una pila no ejecutable (NX), y la aleatoriedad de direcciones (ASLR) que oculta adónde saltar."
                ),
            ],
            panels: vec!["editor", "stack", "callstack"],
            starter: Some(L_EXPLOITATION),
        },
        Lesson {
            id: "performance",
            level: Level::Expert,
            title: t!("Analyse de performances", "Performance analysis", "Análisis de rendimiento"),
            goal: t!(
                "Compter les cycles, repérer les dépendances, et distinguer le coût réel du coût supposé.",
                "Count cycles, spot dependencies, and tell real cost from assumed cost.",
                "Contar ciclos, detectar dependencias y distinguir coste real de coste supuesto."
            ),
            steps: vec![
                t!(
                    "Mesurer d'abord : la Timeline compte les instructions réellement exécutées. Une boucle courte parcourue mille fois pèse plus qu'un long code linéaire.",
                    "Measure first: the Timeline counts the instructions actually executed. A short loop run a thousand times weighs more than a long straight-line block.",
                    "Medir primero: la Línea de tiempo cuenta las instrucciones realmente ejecutadas. Un bucle corto mil veces pesa más que un bloque largo lineal."
                ),
                t!(
                    "Multiplier par une puissance de deux, c'est décaler les bits. « shl rax, 3 » fait ×8 en un cycle ; « imul » en demande trois pour le même résultat.",
                    "Multiplying by a power of two is a bit shift. \"shl rax, 3\" does ×8 in one cycle; \"imul\" needs three for the same result.",
                    "Multiplicar por una potencia de dos es un desplazamiento de bits. «shl rax, 3» hace ×8 en un ciclo; «imul» necesita tres para lo mismo."
                ),
                t!(
                    "Mais la règle n'est pas « fuir imul » : sur un multiplicateur quelconque, imul redevient le meilleur choix. Le coût réel dépend des données, jamais du seul opcode.",
                    "But the rule is not \"avoid imul\": for an arbitrary multiplier, imul is best again. Real cost depends on the data, never on the opcode alone.",
                    "Pero la regla no es «evitar imul»: para un multiplicador cualquiera, imul vuelve a ser lo mejor. El coste real depende de los datos, nunca solo del opcode."
                ),
                t!(
                    "Le vrai frein est ailleurs : une chaîne de dépendances où chaque instruction attend la précédente, ou un saut imprévisible qui vide le pipeline. Les rompre rapporte plus que tout décalage.",
                    "The real brake lies elsewhere: a dependency chain where each instruction waits on the last, or an unpredictable branch that flushes the pipeline. Breaking those pays more than any shift.",
                    "El verdadero freno está en otra parte: una cadena de dependencias donde cada instrucción espera a la anterior, o un salto impredecible que vacía el cauce. Romperlos rinde más que cualquier desplazamiento."
                ),
            ],
            panels: vec!["editor", "timeline", "instruction"],
            starter: Some(L_PERFORMANCE),
        },
    ]
}

/// Leçons d'un niveau, dans l'ordre.
pub fn lessons_of(level: Level) -> Vec<Lesson> {
    catalogue().into_iter().filter(|l| l.level == level).collect()
}

/// Retrouve une leçon par son identifiant.
pub fn find(id: &str) -> Option<Lesson> {
    catalogue().into_iter().find(|l| l.id == id)
}

/// Progression : identifiants des leçons terminées.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Progress {
    done: Vec<String>,
}

impl Progress {
    pub fn is_done(&self, id: &str) -> bool {
        self.done.iter().any(|d| d == id)
    }

    pub fn mark_done(&mut self, id: &str) {
        if !self.is_done(id) {
            self.done.push(id.to_string());
        }
    }

    pub fn mark_undone(&mut self, id: &str) {
        self.done.retain(|d| d != id);
    }

    /// (terminées, total) pour un niveau.
    pub fn tally(&self, level: Level) -> (usize, usize) {
        let l = lessons_of(level);
        (l.iter().filter(|x| self.is_done(x.id)).count(), l.len())
    }

    /// Sérialisation pour les réglages : identifiants séparés par des virgules.
    pub fn to_string(&self) -> String {
        self.done.join(",")
    }

    /// Relecture. Les identifiants inconnus sont écartés : renommer une leçon
    /// ne doit pas laisser de progression fantôme.
    pub fn parse(s: &str) -> Progress {
        let mut p = Progress::default();
        for id in s.split(',').map(str::trim).filter(|s| !s.is_empty()) {
            if find(id).is_some() {
                p.mark_done(id);
            }
        }
        p
    }

    /// Première leçon non terminée du parcours, pour reprendre où l'on en était.
    pub fn next_lesson(&self) -> Option<Lesson> {
        catalogue().into_iter().find(|l| !self.is_done(l.id))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_lesson_has_a_unique_id() {
        let c = catalogue();
        let mut ids: Vec<&str> = c.iter().map(|l| l.id).collect();
        let n = ids.len();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), n, "identifiants de leçon dupliqués");
        assert!(n >= 27, "le parcours annoncé compte au moins 27 leçons, vu {n}");
    }

    #[test]
    fn every_lesson_is_titled_in_every_language() {
        for l in catalogue() {
            for lang in [Lang::Fr, Lang::En, Lang::Es] {
                assert!(!l.title.get(lang).is_empty(), "{} sans titre en {lang:?}", l.id);
                assert!(!l.goal.get(lang).is_empty(), "{} sans objectif en {lang:?}", l.id);
                for s in &l.steps {
                    assert!(s.get(lang).len() > 20, "{} : étape trop courte en {lang:?}", l.id);
                }
            }
        }
    }

    #[test]
    fn the_four_levels_are_all_represented() {
        for lvl in Level::ALL {
            assert!(!lessons_of(lvl).is_empty(), "{lvl:?} sans aucune leçon");
            for lang in [Lang::Fr, Lang::En, Lang::Es] {
                assert!(!lvl.title(lang).is_empty());
            }
        }
    }

    /// Le niveau Débutant doit être RÉELLEMENT écrit : des étapes, et un
    /// programme de départ pour toutes les leçons pratiques.
    #[test]
    fn the_beginner_level_is_fully_written() {
        let b = lessons_of(Level::Beginner);
        assert!(b.len() >= 8, "huit leçons débutant attendues, vu {}", b.len());
        for l in &b {
            assert!(!l.steps.is_empty(), "{} n'a aucune étape", l.id);
            assert!(!l.panels.is_empty(), "{} n'ouvre aucun panneau", l.id);
        }
        // Seule « installation » est purement explicative.
        let with_code = b.iter().filter(|l| l.has_starter()).count();
        assert_eq!(with_code, b.len() - 1, "toutes sauf l'installation ont un programme");
    }

    /// Passé le niveau Débutant, tout le parcours est écrit et entièrement
    /// pratique : plus aucune leçon ne se contente d'expliquer.
    #[test]
    fn the_upper_levels_are_fully_written() {
        for (level, count) in [
            (Level::Intermediate, 7),
            (Level::Advanced, 6),
            (Level::Expert, 6),
        ] {
            let ls = lessons_of(level);
            assert_eq!(ls.len(), count, "{level:?} : {count} leçons attendues, vu {}", ls.len());
            for l in &ls {
                assert!(l.steps.len() >= 3, "{} : moins de trois étapes", l.id);
                assert!(l.has_starter(), "{} n'a pas de programme de départ", l.id);
                assert!(!l.panels.is_empty(), "{} n'ouvre aucun panneau", l.id);
            }
        }
    }

    /// Plus aucune leçon « planned » (sans étape) ne doit subsister : le
    /// parcours entier a maintenant du contenu. Seule « installation » reste
    /// purement explicative — c'est le tout premier contact, sans code encore.
    #[test]
    fn no_lesson_remains_a_stub() {
        for l in catalogue() {
            assert!(!l.steps.is_empty(), "{} est encore un plan vide", l.id);
            assert!(
                l.has_starter() || l.id == "installation",
                "{} n'a toujours pas de programme",
                l.id
            );
        }
    }

    /// Chaque programme de leçon porte ses propres attentes : c'est ce qui
    /// permet de le vérifier sans machinerie supplémentaire.
    #[test]
    fn every_starter_carries_checkable_expectations() {
        for l in catalogue() {
            let Some(src) = l.starter else { continue };
            let ex = crate::exercise::parse(src);
            assert!(ex.is_exercise(), "{} : aucune attente déclarée", l.id);
            assert!(ex.title.is_some(), "{} : pas de titre d'exercice", l.id);
            assert!(ex.statement.is_some(), "{} : pas d'énoncé", l.id);
            assert!(ex.errors.is_empty(), "{} : directives fautives {:?}", l.id, ex.errors);
        }
    }

    /// Les programmes de leçon doivent s'ASSEMBLER : un exemple qui ne compile
    /// pas apprendrait la mauvaise leçon.
    #[test]
    fn every_starter_assembles() {
        use std::path::Path;
        std::fs::create_dir_all("build/tutorial").ok();
        for l in catalogue() {
            let Some(src) = l.starter else { continue };
            let path = format!("build/tutorial/{}.asm", l.id);
            std::fs::write(&path, src).expect("écriture");
            let out = crate::assemble::assemble_with_includes(
                Path::new(&path),
                Path::new("build/tutorial"),
                &[],
            );
            assert!(out.is_ok(), "{} ne s'assemble pas : {:?}", l.id, out.err());
        }
    }

    #[test]
    fn progress_round_trips_and_ignores_unknown_ids() {
        let mut p = Progress::default();
        p.mark_done("registres");
        p.mark_done("flags");
        p.mark_done("registres"); // idempotent
        assert_eq!(p.to_string().split(',').count(), 2);

        let back = Progress::parse(&p.to_string());
        assert_eq!(back, p);
        assert!(back.is_done("flags"));

        // Un identifiant disparu du catalogue ne doit pas rester fantôme.
        let ghost = Progress::parse("registres,lecon_supprimee_2019");
        assert!(ghost.is_done("registres"));
        assert!(!ghost.is_done("lecon_supprimee_2019"));
        assert_eq!(Progress::parse("").to_string(), "");
    }

    #[test]
    fn tally_counts_per_level() {
        let mut p = Progress::default();
        let (done, total) = p.tally(Level::Beginner);
        assert_eq!(done, 0);
        assert!(total >= 8);

        p.mark_done("premier_programme");
        p.mark_done("registres");
        assert_eq!(p.tally(Level::Beginner).0, 2);
        assert_eq!(p.tally(Level::Expert).0, 0, "une leçon d'un autre niveau ne compte pas");
    }

    #[test]
    fn next_lesson_resumes_where_you_left_off() {
        let mut p = Progress::default();
        assert_eq!(p.next_lesson().map(|l| l.id), Some("installation"));
        p.mark_done("installation");
        assert_eq!(p.next_lesson().map(|l| l.id), Some("premier_programme"));

        // Tout terminé : plus rien à proposer.
        for l in catalogue() {
            p.mark_done(l.id);
        }
        assert!(p.next_lesson().is_none());
    }

    #[test]
    fn mark_undone_reverts() {
        let mut p = Progress::default();
        p.mark_done("boucles");
        assert!(p.is_done("boucles"));
        p.mark_undone("boucles");
        assert!(!p.is_done("boucles"));
    }
}
