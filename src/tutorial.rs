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
    /// Parcours à part : le même assembleur, mais assemblé pour Windows.
    ///
    /// Un niveau plutôt qu'un champ sur chaque leçon, parce que c'en est un :
    /// on n'y va qu'après avoir compris les registres, la pile et les appels,
    /// et tout ce qu'on y apprend — convention d'appel Microsoft, imports de
    /// DLL, espace d'ombre — n'a de sens qu'une fois le modèle Linux acquis.
    /// La cible d'assemblage s'en déduit ([`Lesson::target`]), sans table
    /// annexe à tenir à jour.
    Windows,
}

impl Level {
    pub const ALL: [Level; 5] = [
        Level::Beginner,
        Level::Intermediate,
        Level::Advanced,
        Level::Expert,
        Level::Windows,
    ];

    pub fn title(self, lang: Lang) -> &'static str {
        match self {
            Level::Beginner => i18n::tr3(lang, "Débutant", "Beginner", "Principiante"),
            Level::Intermediate => i18n::tr3(lang, "Intermédiaire", "Intermediate", "Intermedio"),
            Level::Advanced => i18n::tr3(lang, "Avancé", "Advanced", "Avanzado"),
            Level::Expert => i18n::tr3(lang, "Expert", "Expert", "Experto"),
            Level::Windows => i18n::tr3(lang, "Windows (PE64)", "Windows (PE64)", "Windows (PE64)"),
        }
    }

    /// Ce niveau demande-t-il que l'assemblage Windows soit activé ?
    pub fn needs_pe(self) -> bool {
        self == Level::Windows
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
    /// À quoi la notion sert : le problème réel qu'elle résout, en une phrase.
    ///
    /// L'objectif dit ce que l'élève saura faire ; celui-ci dit pourquoi
    /// quelqu'un a eu besoin de l'inventer. Sans lui, une leçon sur les flags
    /// est une liste de lettres à retenir plutôt qu'une réponse à une question.
    pub why: Option<Text>,
    /// Indices de plus en plus précis, à ne délivrer qu'un par un.
    ///
    /// Le dernier dicte la solution : le parcours ne s'ouvre qu'en validant,
    /// donc rester bloqué n'est plus une gêne mais un cul-de-sac. Mieux vaut
    /// une leçon finie avec l'indice ultime qu'un élève qui referme l'IDE.
    pub hints: Vec<Text>,
    /// Ce qu'il faut retenir une fois la leçon finie — deux ou trois points.
    ///
    /// Ce qui reste quand le programme est refermé. La leçon se lit dans le
    /// mouvement du pas à pas ; ceci se relit avant de passer à la suite.
    pub takeaway: Vec<Text>,
}

impl Lesson {
    /// Vrai si la leçon a du contenu exécutable à charger.
    pub fn has_starter(&self) -> bool {
        self.starter.is_some()
    }

    /// Cible d'assemblage que la leçon suppose. Charger une leçon Windows bascule
    /// l'IDE sur la bonne cible : sans cela, `nasm -f elf64` refuserait son
    /// `extern ExitProcess`, et l'élève lirait une erreur qui ne parle pas de
    /// ce qu'il apprend.
    pub fn target(&self) -> crate::assemble::Target {
        match self.level {
            Level::Windows => crate::assemble::Target::Windows,
            _ => crate::assemble::Target::Linux,
        }
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
;@enonce Échange le contenu de RBX et RCX en passant par la pile.
;@attendu rbx == 2
;@attendu rcx == 1
;@attendu exit == 0

; La pile croît vers les adresses BASSES. « push » y range une valeur et
; diminue RSP de 8 ; « pop » fait l'inverse. Dernier entré, premier sorti.
;
; L'échange se fait sur RBX et RCX, pas sur RAX : les trois dernières lignes
; du programme écrasent RAX avec 60, le numéro de l'appel « exit ». Un
; registre qu'on veut observer à la fin ne doit pas être un registre de
; travail — c'est déjà une leçon de convention d'appel.
section .text
    global _start

_start:
    mov rbx, 1
    mov rcx, 2

    push rbx            ; la pile contient : 1
    push rcx            ; la pile contient : 1, 2

    ; TODO : deux « pop » bien ordonnés suffisent à échanger.
    ;        Dernier entré, premier sorti : le premier « pop » rend 2.

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
;@requis rel

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
;@interdit imul

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
    mov edi, [rel fds + 4]  ; TODO : fds+4 est l'extrémité d'ÉCRITURE ; lire dessus est refusé. Vise fds[0] (« mov edi, [rel fds] »)
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
;@interdit imul

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

// ----------------------------------------------------------------------
//  Parcours Windows : le même assembleur, un autre système
// ----------------------------------------------------------------------
//
//  Ces programmes s'assemblent en PE64 (`nasm -f win64` + le lieur intégré) et
//  s'exécutent sous Wine quand il est installé. Leurs attentes ne portent donc
//  que sur ce qui reste observable sans débogueur : le code de sortie et le
//  texte du programme. C'est une contrainte, et c'est aussi la leçon — sous
//  Windows, ASM Studio regarde le résultat, pas les registres.

const L_WIN_PREMIER: &str = r#";@titre Premier programme Windows
;@enonce Ce programme se termine avec le code 0. Fais-lui rendre 7, comme dans
;@enonce la toute première leçon — mais sans « syscall ».
;@attendu exit == 7

; Windows ne se parle pas directement. Là où Linux met un numéro dans RAX et
; exécute « syscall », un programme Windows APPELLE une fonction d'une DLL :
;   extern ExitProcess     -> déclare qu'elle vient d'ailleurs
;   call   ExitProcess     -> le lieur l'inscrit dans la table d'import
;
; Le « sub rsp, 40 » n'est pas décoratif : voir la leçon sur la pile.
bits 64
default rel

section .text
    global main
    extern ExitProcess

main:
    sub     rsp, 40
    mov     ecx, 0          ; TODO : le code de sortie voulu
    call    ExitProcess
"#;

const L_WIN_APPEL: &str = r#";@titre La convention d'appel Microsoft
;@enonce Le premier argument ne passe pas par le même registre que sous Linux.
;@enonce Corrige l'appel pour que le programme rende 42.
;@attendu exit == 42

; Deux conventions, deux ordres de registres, pour les mêmes arguments :
;
;   Linux (System V) :  RDI  RSI  RDX  RCX  R8  R9
;   Windows (MS x64) :  RCX  RDX  R8   R9   puis la pile
;
; C'est l'erreur la plus fréquente en passant d'un monde à l'autre : le
; programme s'assemble, se lie, s'exécute — et travaille sur la mauvaise valeur.
bits 64
default rel

section .text
    global main
    extern ExitProcess

main:
    sub     rsp, 40
    mov     edi, 42         ; TODO : le réflexe Linux ; quel registre attend Windows ?
    call    ExitProcess
"#;

const L_WIN_IMPORTS: &str = r#";@titre Importer une fonction d'une DLL
;@enonce strlen mesure la chaîne dont l'adresse est dans RCX. Le programme
;@enonce mesure la mauvaise : vise « mot » pour que le code de sortie soit 7.
;@attendu exit == 7

; Un « extern » ne contient aucun code : il dit au lieur d'inscrire le nom dans
; la TABLE D'IMPORT du .exe. Au chargement, Windows y écrit l'adresse réelle de
; la fonction — c'est l'IAT. Ouvre le panneau FORMAT après avoir assemblé : les
; fonctions importées y sont listées, avec la DLL qui les fournit.
;
; ASM Studio connaît les fonctions usuelles de kernel32, user32 et msvcrt. Pour
; toute autre, le nom porte sa bibliothèque : « extern gdi32$CreatePen ».
bits 64
default rel

section .data
    mot     db "Bonjour", 0     ; 7 lettres
    autre   db "Salut", 0       ; 5 lettres

section .text
    global main
    extern strlen
    extern ExitProcess

main:
    sub     rsp, 40
    lea     rcx, [autre]    ; TODO : mesurer « mot », pas « autre »
    call    strlen
    mov     ecx, eax        ; le résultat de strlen devient le code de sortie
    call    ExitProcess
"#;

const L_WIN_FORMAT: &str = r#";@titre Ce que contient un .exe
;@enonce Le code de sortie doit valoir 12, lu depuis la mémoire. Le programme
;@enonce lit la mauvaise variable.
;@attendu exit == 12

; Assemble (Ctrl+B), puis regarde le panneau FORMAT. Un PE et un ELF répondent
; aux mêmes questions, avec d'autres mots :
;
;   .text   le code             (les deux)
;   .data   les variables       (les deux)
;   .bss    zéro octet dans le fichier, de la place en mémoire  (les deux)
;   .idata  la table d'import   (PE seulement — ELF a .plt/.got)
;
; Le point d'entrée d'un PE est une RVA : une adresse RELATIVE à la base de
; l'image (0x140000000 ici). Celui d'un ELF est une adresse absolue.
bits 64
default rel

section .data
    douze   dq 12
    treize  dq 13

section .text
    global main
    extern ExitProcess

main:
    sub     rsp, 40
    mov     rcx, [treize]   ; TODO : c'est « douze » qu'il faut lire
    call    ExitProcess
"#;

const L_WIN_PILE: &str = r#";@titre L'espace d'ombre
;@enonce Réserve les 40 octets attendus avant les appels. Le programme doit
;@enonce rendre 5, et le code doit contenir « sub rsp, 40 ».
;@attendu exit == 5
;@requis sub rsp, 40

; Avant TOUT appel, Windows exige que l'appelant réserve 32 octets sur la pile :
; l'espace d'ombre. L'appelé peut y ranger les quatre premiers arguments, même
; s'il ne le fait pas. C'est de la place que l'appelant doit à l'appelé.
;
; Pourquoi 40 et non 32 ? Parce que RSP doit être multiple de 16 au moment du
; « call ». À l'entrée de main, l'adresse de retour empilée a décalé RSP de 8 :
; 32 + 8 remet le compte juste. Oublier ces huit octets fait planter les appels
; qui utilisent des instructions SSE alignées — c'est-à-dire beaucoup.
bits 64
default rel

section .text
    global main
    extern ExitProcess

main:
    sub     rsp, 0          ; TODO : réserver l'espace d'ombre et l'alignement
    mov     ecx, 5
    call    ExitProcess
"#;

const L_TAILLES: &str = r#";@titre Les tailles
;@enonce RAX contient 0x1234. Écris 0xFF dans son seul octet bas (AL), pour
;@enonce que RBX reçoive 0x12FF.
;@attendu rbx == 0x12FF
;@attendu exit == 0

; Un registre 64 bits se lit aussi par morceaux, qui se recouvrent :
;   RAX = 64 bits    EAX = 32 bas    AX = 16 bas    AL = 8 bas    AH = bits 8-15
; Écrire dans un petit morceau ne touche que lui — À UNE EXCEPTION : écrire dans
; EAX met à zéro les 32 bits hauts. C'est le piège le plus célèbre de x86-64.
section .text
    global _start

_start:
    mov rax, 0x1234
    ; TODO : écrire 0xFF dans l'octet bas seulement  (« mov al, 0xFF »)
    mov rbx, rax

    mov rax, 60
    xor rdi, rdi
    syscall
"#;

const L_MUL_DIV: &str = r#";@titre Multiplication et division
;@enonce Multiplie 7 par 6, divise le tout par 4, et garde le quotient dans
;@enonce RBX (10) et le reste dans RCX (2).
;@attendu rbx == 10
;@attendu rcx == 2
;@attendu exit == 0

; « mul » et « div » ne travaillent qu'avec des registres imposés :
;   « div r9 » calcule RDX:RAX ÷ r9 -> quotient dans RAX, reste dans RDX.
; Oublier de remettre RDX à zéro avant une division non signée, c'est diviser
; un nombre de 128 bits par accident — et souvent planter (#DE).
section .text
    global _start

_start:
    mov rax, 7
    mov r8, 6
    ; TODO : multiplier RAX par R8  (« imul rax, r8 »)  -> 42

    xor rdx, rdx        ; le reste part de zéro : on divise un nombre de 64 bits
    mov r9, 4           ; div n'accepte pas d'immédiat, il faut un registre
    div r9              ; RAX = quotient, RDX = reste

    mov rbx, rax        ; quotient
    mov rcx, rdx        ; reste

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
            why: Some(t!(
                "L'assembleur est le seul langage où l'on peut voir la machine travailler. Un éditeur qui ne montrerait que du texte vous priverait de la seule chose qui rende ce langage apprenable : le spectacle de ce qui se passe entre deux instructions.",
                "Assembly is the one language where you can actually watch the machine work. An editor showing only text would deny you the one thing that makes this language learnable: the sight of what happens between two instructions.",
                "El ensamblador es el único lenguaje donde se puede ver trabajar a la máquina. Un editor que solo mostrara texto le privaría de lo único que hace aprendible este lenguaje: ver lo que ocurre entre dos instrucciones."
            )),
            hints: vec![],
            takeaway: vec![
                t!(
                    "nasm transforme votre texte en fichier objet, ld en fait un exécutable. Deux outils, donc deux erreurs possibles — et la Console dit toujours lequel a parlé.",
                    "nasm turns your text into an object file, ld turns that into an executable. Two tools, so two possible errors — and the Console always says which one spoke.",
                    "nasm convierte su texto en un archivo objeto, ld lo convierte en ejecutable. Dos herramientas, dos errores posibles — y la Consola siempre dice cuál habló."
                ),
                t!(
                    "F5 lance, F10 avance d'UNE instruction, Échap arrête. Ces trois touches suffisent pour tout le niveau débutant.",
                    "F5 runs, F10 advances by ONE instruction, Esc stops. These three keys are enough for the whole beginner level.",
                    "F5 ejecuta, F10 avanza UNA instrucción, Esc detiene. Estas tres teclas bastan para todo el nivel principiante."
                ),
                t!(
                    "La Timeline garde chaque étape franchie : revenir en arrière ne coûte rien, et se tromper non plus.",
                    "The Timeline keeps every step taken: going back costs nothing, and neither does being wrong.",
                    "La Línea de tiempo guarda cada paso: retroceder no cuesta nada, y equivocarse tampoco."
                ),
            ],
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
            why: Some(t!(
                "Un programme doit annoncer au système qu'il a fini, et avec quel résultat. C'est ce code de sortie que le shell teste dans un « si », et sans lui aucun script ne saurait jamais si la commande qu'il vient de lancer a réussi.",
                "A program must tell the system that it is finished, and with what result. That exit code is what the shell tests in an if, and without it no script could ever know whether the command it just ran succeeded.",
                "Un programa debe anunciar al sistema que ha terminado, y con qué resultado. Ese código de salida es lo que la shell comprueba en un si, y sin él ningún script sabría si el comando que acaba de lanzar tuvo éxito."
            )),
            hints: vec![
                t!(
                    "Le code de sortie ne se met pas où l'on croit : RAX est déjà pris, il porte le NUMÉRO de l'appel système. Cherchez ailleurs.",
                    "The exit code does not go where you would think: RAX is already taken, it carries the system call NUMBER. Look elsewhere.",
                    "El código de salida no va donde uno cree: RAX ya está ocupado, lleva el NÚMERO de la llamada. Busque en otro sitio."
                ),
                t!(
                    "Une seule ligne décide du code rendu : celle qui met RDI à zéro. C'est elle qu'il faut changer.",
                    "One single line decides the returned code: the one setting RDI to zero. That is the line to change.",
                    "Una sola línea decide el código devuelto: la que pone RDI a cero. Esa es la línea a cambiar."
                ),
                t!(
                    "Remplacez « xor rdi, rdi » par « mov rdi, 7 », puis F5.",
                    "Replace xor rdi, rdi with mov rdi, 7, then press F5.",
                    "Sustituya «xor rdi, rdi» por «mov rdi, 7» y pulse F5."
                ),
            ],
            takeaway: vec![
                t!(
                    "_start est le point d'entrée : c'est le nom que ld cherche, et « global » est ce qui le lui rend visible.",
                    "_start is the entry point: it is the name ld looks for, and global is what makes it visible to it.",
                    "_start es el punto de entrada: es el nombre que busca ld, y global es lo que se lo hace visible."
                ),
                t!(
                    "Pour un appel système : RAX porte le numéro (60 = exit), RDI le premier argument.",
                    "For a system call: RAX carries the number (60 = exit), RDI the first argument.",
                    "Para una llamada al sistema: RAX lleva el número (60 = exit), RDI el primer argumento."
                ),
                t!(
                    "Il n'y a pas de « return » en assembleur : on demande au noyau de nous arrêter, et il ne rend jamais la main.",
                    "There is no return in assembly: you ask the kernel to stop you, and it never hands control back.",
                    "No hay «return» en ensamblador: se pide al núcleo que nos detenga, y nunca devuelve el control."
                ),
            ],
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
            why: Some(t!(
                "Le processeur ne sait calculer que sur ses registres. Une valeur rangée en mémoire doit y monter avant d'être touchée, et redescendre ensuite : tout programme, quel que soit le langage dans lequel il est écrit, finit par être ce va-et-vient.",
                "The processor can only compute on its registers. A value stored in memory must come up into one before being touched, and go back down afterwards: every program, whatever language it is written in, ends up being that back-and-forth.",
                "El procesador solo sabe calcular sobre sus registros. Un valor en memoria debe subir a uno antes de ser tocado, y bajar después: todo programa, sea cual sea su lenguaje, acaba siendo ese ir y venir."
            )),
            hints: vec![
                t!(
                    "Avant d'ajouter quoi que ce soit, regardez le panneau Registres à l'endroit du TODO : RBX n'est pas vide, il vaut déjà quelque chose.",
                    "Before adding anything, look at the Registers panel where the TODO is: RBX is not empty, it already holds something.",
                    "Antes de añadir nada, mire el panel Registros donde está el TODO: RBX no está vacío, ya vale algo."
                ),
                t!(
                    "RBX vaut 50 à cet endroit : 40 copiés depuis RAX, plus les 10 ajoutés juste après. Il en manque donc 50.",
                    "RBX holds 50 at that point: 40 copied from RAX, plus the 10 added right after. So 50 are missing.",
                    "RBX vale 50 ahí: 40 copiados de RAX, más los 10 añadidos justo después. Faltan pues 50."
                ),
                t!(
                    "Écrivez « add rbx, 50 » à la place du TODO.",
                    "Write add rbx, 50 in place of the TODO.",
                    "Escriba «add rbx, 50» en lugar del TODO."
                ),
            ],
            takeaway: vec![
                t!(
                    "Seize registres de 64 bits, et le processeur ne calcule que là-dedans.",
                    "Sixteen 64-bit registers, and the processor computes nowhere else.",
                    "Dieciséis registros de 64 bits, y el procesador no calcula en ningún otro sitio."
                ),
                t!(
                    "« mov » écrit sans lire la destination ; « add » la relit d'abord. Le Microscope montre la différence.",
                    "mov writes without reading the destination; add re-reads it first. The Microscope shows the difference.",
                    "«mov» escribe sin leer el destino; «add» lo relee antes. El Microscopio muestra la diferencia."
                ),
                t!(
                    "À chaque F10, le panneau Registres allume ce qui vient de changer — une valeur au plus, et c'est ce qui rend le programme lisible.",
                    "At each F10, the Registers panel lights up what just changed — at most one value, and that is what makes the program readable.",
                    "En cada F10, el panel Registros enciende lo que acaba de cambiar — un valor a lo sumo, y eso hace legible el programa."
                ),
            ],
        },
        Lesson {
            id: "tailles",
            level: Level::Beginner,
            title: t!("Les tailles", "Register sizes", "Los tamaños"),
            goal: t!(
                "Voir qu'un registre se lit par morceaux, et connaître le piège d'EAX.",
                "See that a register is read in parts, and know the EAX trap.",
                "Ver que un registro se lee por partes, y conocer la trampa de EAX."
            ),
            steps: vec![
                t!(
                    "RAX (64), EAX (32), AX (16), AL (8) désignent le MÊME registre, par tranches qui se recouvrent. Le panneau Registres montre la valeur entière changer.",
                    "RAX (64), EAX (32), AX (16), AL (8) name the SAME register, in overlapping slices. The Registers panel shows the whole value change.",
                    "RAX (64), EAX (32), AX (16), AL (8) nombran el MISMO registro, en tramos que se solapan. El panel Registros muestra cambiar el valor entero."
                ),
                t!(
                    "« mov al, 0xFF » n'écrit que l'octet bas : les autres octets de RAX restent intacts. 0x1234 devient 0x12FF, pas 0xFF.",
                    "\"mov al, 0xFF\" writes only the low byte: the other bytes of RAX stay untouched. 0x1234 becomes 0x12FF, not 0xFF.",
                    "«mov al, 0xFF» escribe solo el byte bajo: los demás bytes de RAX quedan intactos. 0x1234 pasa a 0x12FF, no 0xFF."
                ),
                t!(
                    "L'exception à retenir : écrire dans EAX MET À ZÉRO les 32 bits hauts. « mov eax, 5 » donne RAX = 5, quoi qu'il y eût avant.",
                    "The exception to remember: writing to EAX ZEROES the upper 32 bits. \"mov eax, 5\" gives RAX = 5, whatever was there before.",
                    "La excepción a recordar: escribir en EAX PONE A CERO los 32 bits altos. «mov eax, 5» da RAX = 5, hubiera lo que hubiera antes."
                ),
                t!(
                    "Choisir la bonne taille, c'est dire combien d'octets on touche : lire un caractère, c'est AL ; une adresse, c'est RAX. Le Microscope montre l'octet exact lu ou écrit.",
                    "Choosing the right size says how many bytes you touch: reading a character is AL; an address is RAX. The Microscope shows the exact byte read or written.",
                    "Elegir el tamaño correcto dice cuántos bytes se tocan: leer un carácter es AL; una dirección es RAX. El Microscopio muestra el byte exacto leído o escrito."
                ),
            ],
            panels: vec!["editor", "registers", "instruction"],
            starter: Some(L_TAILLES),
            why: Some(t!(
                "Un caractère tient sur un octet, un compteur de boucle rarement sur plus de quatre. Savoir écrire dans un morceau de registre sans écraser le reste est ce qui sépare un programme juste d'un programme qui perd la moitié de ses données sans prévenir.",
                "A character fits in one byte, a loop counter rarely needs more than four. Writing into part of a register without wiping the rest is what separates a correct program from one that silently loses half its data.",
                "Un carácter cabe en un byte, un contador de bucle rara vez necesita más de cuatro. Escribir en parte de un registro sin borrar el resto separa un programa correcto de otro que pierde la mitad de sus datos sin avisar."
            )),
            hints: vec![
                t!(
                    "RAX vaut 0x1234 et doit finir à 0x12FF : comparez les deux nombres, seuls les deux chiffres de droite changent.",
                    "RAX holds 0x1234 and must end at 0x12FF: compare the two numbers, only the two rightmost digits change.",
                    "RAX vale 0x1234 y debe acabar en 0x12FF: compare ambos números, solo cambian los dos dígitos de la derecha."
                ),
                t!(
                    "Deux chiffres hexadécimaux, c'est un octet. L'octet bas de RAX porte un nom : AL.",
                    "Two hexadecimal digits make one byte. The low byte of RAX has a name: AL.",
                    "Dos dígitos hexadecimales son un byte. El byte bajo de RAX tiene nombre: AL."
                ),
                t!(
                    "Écrivez « mov al, 0xFF ».",
                    "Write mov al, 0xFF.",
                    "Escriba «mov al, 0xFF»."
                ),
            ],
            takeaway: vec![
                t!(
                    "RAX, EAX, AX, AL sont le même registre lu à quatre tailles — pas quatre registres différents.",
                    "RAX, EAX, AX and AL are the same register read at four sizes — not four different registers.",
                    "RAX, EAX, AX y AL son el mismo registro leído en cuatro tamaños — no cuatro registros distintos."
                ),
                t!(
                    "Écrire dans AL ou AX ne touche que ces bits-là : le reste du registre est préservé.",
                    "Writing into AL or AX touches only those bits: the rest of the register is preserved.",
                    "Escribir en AL o AX solo toca esos bits: el resto del registro se conserva."
                ),
                t!(
                    "L'exception à retenir : écrire dans EAX met à zéro les 32 bits hauts. C'est le piège le plus célèbre de x86-64, et il ne prévient jamais.",
                    "The exception to remember: writing into EAX zeroes the upper 32 bits. It is the most famous x86-64 pitfall, and it never warns you.",
                    "La excepción a recordar: escribir en EAX pone a cero los 32 bits altos. Es la trampa más famosa de x86-64, y nunca avisa."
                ),
            ],
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
            why: Some(t!(
                "Les registres sont seize. Tout ce qu'un programme manipule au-delà vit en mémoire, et il faut donc pouvoir désigner deux choses différentes : l'endroit où une donnée est rangée, et la donnée elle-même. Confondre les deux est l'erreur la plus fréquente du débutant.",
                "There are sixteen registers. Everything a program handles beyond that lives in memory, so you must be able to name two different things: where a value is stored, and the value itself. Confusing them is the beginner's most frequent mistake.",
                "Hay dieciséis registros. Todo lo demás vive en memoria, así que hay que poder nombrar dos cosas distintas: dónde está guardado un dato y el dato mismo. Confundirlos es el error más frecuente del principiante."
            )),
            hints: vec![
                t!(
                    "La ligne écrit bien quelque chose dans RBX, et le programme s'assemble sans erreur. Ce n'est pas une faute de syntaxe : c'est la mauvaise valeur.",
                    "The line does write something into RBX, and the program assembles without error. This is not a syntax mistake: it is the wrong value.",
                    "La línea sí escribe algo en RBX, y el programa ensambla sin error. No es un fallo de sintaxis: es el valor equivocado."
                ),
                t!(
                    "Relisez le commentaire juste au-dessus : sans crochets on obtient l'ADRESSE de la case, avec crochets son CONTENU.",
                    "Re-read the comment just above: without brackets you get the ADDRESS of the slot, with brackets its CONTENTS.",
                    "Relea el comentario de arriba: sin corchetes se obtiene la DIRECCIÓN de la casilla, con corchetes su CONTENIDO."
                ),
                t!(
                    "Écrivez « mov rbx, [valeur] ».",
                    "Write mov rbx, [valeur].",
                    "Escriba «mov rbx, [valeur]»."
                ),
            ],
            takeaway: vec![
                t!(
                    "Les crochets se lisent « le contenu à cette adresse ». Sans eux, on manipule l'adresse elle-même.",
                    "Brackets read as the contents at this address. Without them, you handle the address itself.",
                    "Los corchetes se leen «el contenido en esta dirección». Sin ellos, se manipula la dirección misma."
                ),
                t!(
                    "« section .data » range les valeurs initialisées du programme ; « dq » en réserve huit octets.",
                    "section .data holds the program's initialised values; dq reserves eight bytes.",
                    "«section .data» guarda los valores inicializados del programa; «dq» reserva ocho bytes."
                ),
                t!(
                    "Une adresse est un nombre comme un autre : rien ne la distingue dans un registre, c'est ce qu'on en fait qui diffère.",
                    "An address is a number like any other: nothing marks it out inside a register, only what you do with it differs.",
                    "Una dirección es un número como otro: nada la distingue dentro de un registro, solo cambia lo que se hace con ella."
                ),
            ],
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
            why: Some(t!(
                "Le processeur ne sait pas répondre « oui ». Il compare, laisse des drapeaux levés derrière lui, et c'est l'instruction suivante qui en tire une conclusion. Chaque « si » que vous avez écrit dans un langage de haut niveau finit ici : une comparaison, puis un test de drapeau.",
                "The processor cannot answer yes. It compares, leaves flags raised behind it, and the next instruction draws a conclusion from them. Every if you ever wrote in a high-level language ends up here: a comparison, then a flag test.",
                "El procesador no sabe responder «sí». Compara, deja banderas levantadas, y la instrucción siguiente saca la conclusión. Cada «si» que ha escrito en un lenguaje de alto nivel acaba aquí: una comparación y una prueba de bandera."
            )),
            hints: vec![
                t!(
                    "Le cmp est déjà écrit et il a déjà travaillé : avancez jusqu'à lui avec F10 et ouvrez le panneau Flags. ZF vaut 1.",
                    "The cmp is already written and has already done its work: step to it with F10 and open the Flags panel. ZF is 1.",
                    "El cmp ya está escrito y ya ha trabajado: avance hasta él con F10 y abra el panel Flags. ZF vale 1."
                ),
                t!(
                    "« sete » écrit 1 ou 0 selon ZF, dans un registre de 8 bits. BL est l'octet bas de RBX, qui a été mis à zéro tout en haut.",
                    "sete writes 1 or 0 depending on ZF, into an 8-bit register. BL is the low byte of RBX, which was zeroed at the top.",
                    "«sete» escribe 1 o 0 según ZF, en un registro de 8 bits. BL es el byte bajo de RBX, puesto a cero arriba."
                ),
                t!(
                    "Écrivez « sete bl » à la place du TODO.",
                    "Write sete bl in place of the TODO.",
                    "Escriba «sete bl» en lugar del TODO."
                ),
            ],
            takeaway: vec![
                t!(
                    "cmp soustrait sans rien ranger : il ne laisse que des drapeaux derrière lui.",
                    "cmp subtracts without storing anything: all it leaves behind are flags.",
                    "cmp resta sin guardar nada: solo deja banderas."
                ),
                t!(
                    "ZF vaut 1 quand le résultat est nul, donc quand les deux valeurs comparées sont égales.",
                    "ZF is 1 when the result is zero, therefore when the two compared values are equal.",
                    "ZF vale 1 cuando el resultado es cero, es decir cuando ambos valores son iguales."
                ),
                t!(
                    "Les instructions « setcc » transforment un drapeau en 0 ou 1 : la comparaison devient une valeur qu'on peut ranger.",
                    "The setcc instructions turn a flag into 0 or 1: the comparison becomes a value you can store.",
                    "Las instrucciones «setcc» convierten una bandera en 0 o 1: la comparación se vuelve un valor almacenable."
                ),
            ],
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
            why: Some(t!(
                "Sans pile, une fonction ne pourrait pas en appeler une autre : il n'y aurait nulle part où garder l'adresse de retour, ni les valeurs qu'on veut retrouver intactes après l'appel. C'est la structure qui rend les fonctions possibles, et c'est aussi celle que les failles de sécurité visent en premier.",
                "Without a stack, a function could not call another one: there would be nowhere to keep the return address, nor the values you want to find intact afterwards. It is the structure that makes functions possible, and also the first one security exploits aim at.",
                "Sin pila, una función no podría llamar a otra: no habría dónde guardar la dirección de retorno ni los valores que se quieren recuperar intactos. Es la estructura que hace posibles las funciones, y también la primera que atacan las vulnerabilidades."
            )),
            hints: vec![
                t!(
                    "Ouvrez le panneau Pile et avancez jusqu'aux deux push : vous verrez les deux valeurs empilées, et laquelle est au sommet.",
                    "Open the Stack panel and step to the two pushes: you will see both values stacked, and which one is on top.",
                    "Abra el panel Pila y avance hasta los dos push: verá ambos valores apilados y cuál está encima."
                ),
                t!(
                    "Dernier entré, premier sorti : c'est 2 qui sortira au premier « pop ». Or c'est justement RBX qui doit finir à 2.",
                    "Last in, first out: 2 will come out on the first pop. And RBX is precisely the one that must end at 2.",
                    "Último en entrar, primero en salir: el primer «pop» devolverá 2. Y es justo RBX quien debe acabar en 2."
                ),
                t!(
                    "Écrivez « pop rbx » puis « pop rcx », dans cet ordre.",
                    "Write pop rbx then pop rcx, in that order.",
                    "Escriba «pop rbx» y luego «pop rcx», en ese orden."
                ),
            ],
            takeaway: vec![
                t!(
                    "La pile croît vers les adresses BASSES : « push » diminue RSP de 8, « pop » l'augmente d'autant.",
                    "The stack grows towards LOW addresses: push decreases RSP by 8, pop increases it by the same.",
                    "La pila crece hacia direcciones BAJAS: «push» reduce RSP en 8, «pop» lo aumenta otro tanto."
                ),
                t!(
                    "Dernier entré, premier sorti. Deux push suivis de deux pop dans le même ordre échangent les valeurs.",
                    "Last in, first out. Two pushes followed by two pops in the same order swap the values.",
                    "Último en entrar, primero en salir. Dos push seguidos de dos pop en el mismo orden intercambian los valores."
                ),
                t!(
                    "Un registre qu'on veut observer à la fin ne doit pas être un registre de travail : ici RAX est écrasé par le 60 de l'appel exit.",
                    "A register you want to inspect at the end must not be a working register: here RAX is overwritten by the 60 of the exit call.",
                    "Un registro que se quiere observar al final no debe ser un registro de trabajo: aquí RAX queda sobrescrito por el 60 de la llamada exit."
                ),
            ],
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
            why: Some(t!(
                "Un processeur exécute les instructions l'une après l'autre, sans jamais choisir. Le saut conditionnel est la seule façon de rompre cette ligne droite — et donc la brique dont sont faits tous les « si », tous les « tant que » et tous les « pour » de tous les langages.",
                "A processor runs instructions one after another, never choosing. The conditional jump is the only way to break that straight line — and therefore the brick every if, every while and every for of every language is made of.",
                "Un procesador ejecuta las instrucciones una tras otra, sin elegir jamás. El salto condicional es la única forma de romper esa línea recta — y por tanto el ladrillo del que están hechos todos los «si», «mientras» y «para» de todos los lenguajes."
            )),
            hints: vec![
                t!(
                    "Le cmp est déjà là. Il vous manque la ligne qui décide, juste après : sans elle, le programme tombe droit sur « mov rbx, rdi » et prend toujours le plus petit.",
                    "The cmp is already there. What is missing is the deciding line right after it: without it the program falls straight into mov rbx, rdi and always takes the smaller one.",
                    "El cmp ya está. Falta la línea que decide, justo después: sin ella el programa cae en «mov rbx, rdi» y siempre toma el menor."
                ),
                t!(
                    "« cmp rsi, rdi » a calculé RSI moins RDI. Le saut à prendre quand le premier est le plus grand s'appelle « jg », et l'étiquette existe déjà.",
                    "cmp rsi, rdi computed RSI minus RDI. The jump to take when the first is greater is jg, and the label already exists.",
                    "«cmp rsi, rdi» calculó RSI menos RDI. El salto para cuando el primero es mayor se llama «jg», y la etiqueta ya existe."
                ),
                t!(
                    "Écrivez « jg .rsi_gagne » à la place du TODO.",
                    "Write jg .rsi_gagne in place of the TODO.",
                    "Escriba «jg .rsi_gagne» en lugar del TODO."
                ),
            ],
            takeaway: vec![
                t!(
                    "Un saut conditionnel ne décide de rien par lui-même : il lit les drapeaux posés par la comparaison précédente.",
                    "A conditional jump decides nothing by itself: it reads the flags left by the preceding comparison.",
                    "Un salto condicional no decide nada por sí mismo: lee las banderas dejadas por la comparación anterior."
                ),
                t!(
                    "jg, jl, je, jne : « greater », « less », « equal », « not equal ». Les versions signées et non signées portent des noms différents, et les confondre est un bug classique.",
                    "jg, jl, je, jne: greater, less, equal, not equal. Signed and unsigned versions have different names, and mixing them up is a classic bug.",
                    "jg, jl, je, jne: mayor, menor, igual, distinto. Las versiones con y sin signo tienen nombres distintos, y confundirlas es un error clásico."
                ),
                t!(
                    "Un « si » de langage de haut niveau, c'est toujours cela : une comparaison, un saut conditionnel, deux chemins qui se rejoignent.",
                    "A high-level if is always this: a comparison, a conditional jump, two paths that meet again.",
                    "Un «si» de alto nivel es siempre esto: una comparación, un salto condicional, dos caminos que se reencuentran."
                ),
            ],
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
            why: Some(t!(
                "Écrire dix fois la même ligne fonctionne pour dix ; pour un million, il faut une boucle. Elle n'a rien d'une construction nouvelle : c'est le saut de la leçon précédente, tourné vers l'arrière, et conditionné par un compteur qui descend.",
                "Writing the same line ten times works for ten; for a million you need a loop. It is not a new construct at all: it is the jump from the previous lesson, turned backwards, conditioned by a counter running down.",
                "Escribir diez veces la misma línea funciona para diez; para un millón hace falta un bucle. No es una construcción nueva: es el salto de la lección anterior, vuelto hacia atrás y condicionado por un contador que baja."
            )),
            hints: vec![
                t!(
                    "Le squelette de la boucle est déjà écrit : le compteur descend, le saut revient en arrière. Ce qui manque, c'est le travail à faire à chaque tour.",
                    "The loop skeleton is already written: the counter goes down, the jump comes back. What is missing is the work to do on each turn.",
                    "El esqueleto del bucle ya está escrito: el contador baja, el salto vuelve atrás. Falta el trabajo de cada vuelta."
                ),
                t!(
                    "RCX vaut 10, puis 9, puis 8… et RBX doit accumuler la somme de ces valeurs. Une seule instruction suffit.",
                    "RCX holds 10, then 9, then 8… and RBX must accumulate the sum of those values. One instruction is enough.",
                    "RCX vale 10, luego 9, luego 8… y RBX debe acumular la suma. Basta una instrucción."
                ),
                t!(
                    "Écrivez « add rbx, rcx » à la place du TODO.",
                    "Write add rbx, rcx in place of the TODO.",
                    "Escriba «add rbx, rcx» en lugar del TODO."
                ),
            ],
            takeaway: vec![
                t!(
                    "Une boucle, c'est un saut en arrière conditionné par un compteur — rien de plus.",
                    "A loop is a backwards jump conditioned by a counter — nothing more.",
                    "Un bucle es un salto hacia atrás condicionado por un contador — nada más."
                ),
                t!(
                    "« dec » décrémente ET positionne ZF : c'est ce qui permet à « jnz » de savoir quand s'arrêter, sans comparaison supplémentaire.",
                    "dec decrements AND sets ZF: that is what lets jnz know when to stop, with no extra comparison.",
                    "«dec» decrementa Y posiciona ZF: eso permite a «jnz» saber cuándo parar, sin comparación adicional."
                ),
                t!(
                    "Compter à l'envers, jusqu'à zéro, est la forme la plus économique : la condition d'arrêt est offerte par l'instruction qui décrémente.",
                    "Counting down to zero is the cheapest form: the stopping condition comes free with the instruction that decrements.",
                    "Contar hacia atrás hasta cero es la forma más económica: la condición de parada la regala la instrucción que decrementa."
                ),
            ],
        },
        // ---------------- Intermédiaire ----------------
        Lesson {
            id: "mul_div",
            level: Level::Intermediate,
            title: t!("Multiplication et division", "Multiply and divide", "Multiplicación y división"),
            goal: t!(
                "Utiliser mul et div, qui imposent RAX et RDX, et récupérer le reste.",
                "Use mul and div, which force RAX and RDX, and recover the remainder.",
                "Usar mul y div, que imponen RAX y RDX, y recuperar el resto."
            ),
            steps: vec![
                t!(
                    "Contrairement à add, mul et div n'ont qu'un opérande explicite : l'autre est TOUJOURS RAX. Le panneau Registres montre RAX changer sans qu'on le nomme.",
                    "Unlike add, mul and div take a single explicit operand: the other is ALWAYS RAX. The Registers panel shows RAX change without naming it.",
                    "A diferencia de add, mul y div tienen un solo operando explícito: el otro es SIEMPRE RAX. El panel Registros muestra RAX cambiar sin nombrarlo."
                ),
                t!(
                    "« div r9 » divise le nombre 128 bits RDX:RAX par r9 : le quotient revient dans RAX, le reste dans RDX. Une seule instruction donne les deux.",
                    "\"div r9\" divides the 128-bit number RDX:RAX by r9: the quotient comes back in RAX, the remainder in RDX. One instruction yields both.",
                    "«div r9» divide el número de 128 bits RDX:RAX entre r9: el cociente vuelve en RAX, el resto en RDX. Una sola instrucción da ambos."
                ),
                t!(
                    "D'où le « xor rdx, rdx » avant : sans lui, RDX garde une vieille valeur et la division porte sur un nombre de 128 bits — résultat faux, ou plantage (#DE).",
                    "Hence the \"xor rdx, rdx\" first: without it, RDX keeps an old value and the division runs on a 128-bit number — wrong result, or a crash (#DE).",
                    "De ahí el «xor rdx, rdx» antes: sin él, RDX guarda un valor viejo y la división opera sobre un número de 128 bits — resultado falso, o fallo (#DE)."
                ),
                t!(
                    "« imul » a une forme à deux opérandes (imul rax, r8) bien commode ; « div », non — son diviseur doit être un registre ou de la mémoire, jamais un immédiat.",
                    "\"imul\" has a handy two-operand form (imul rax, r8); \"div\" does not — its divisor must be a register or memory, never an immediate.",
                    "«imul» tiene una forma de dos operandos (imul rax, r8) muy cómoda; «div» no — su divisor debe ser registro o memoria, nunca un inmediato."
                ),
            ],
            panels: vec!["editor", "registers", "instruction"],
            starter: Some(L_MUL_DIV),
            why: Some(t!(
                "Additionner ne coûte presque rien, diviser coûte cher — et le processeur ne s'en cache pas : la division impose ses registres, refuse les immédiats et plante le programme si on l'aborde mal. Savoir ce qu'elle demande évite le crash le plus déroutant du débutant, celui qui n'affiche aucun message.",
                "Adding costs almost nothing, dividing costs a lot — and the processor makes no secret of it: division imposes its registers, refuses immediates, and crashes the program if approached carelessly. Knowing what it demands avoids the beginner's most baffling crash, the one with no message.",
                "Sumar casi no cuesta, dividir cuesta caro — y el procesador no lo oculta: la división impone sus registros, rechaza los inmediatos y bloquea el programa si se aborda mal. Saber qué exige evita el fallo más desconcertante, el que no muestra ningún mensaje."
            )),
            hints: vec![
                t!(
                    "Le TODO ne demande qu'une multiplication : la division qui suit est déjà écrite, et elle attend 42 dans RAX.",
                    "The TODO only asks for a multiplication: the division that follows is already written, and it expects 42 in RAX.",
                    "El TODO solo pide una multiplicación: la división que sigue ya está escrita y espera 42 en RAX."
                ),
                t!(
                    "« imul » est la forme moderne, à deux opérandes : elle prend une destination et une source, comme « add ». RAX vaut 7, R8 vaut 6.",
                    "imul is the modern two-operand form: it takes a destination and a source, like add. RAX holds 7, R8 holds 6.",
                    "«imul» es la forma moderna de dos operandos: toma destino y origen, como «add». RAX vale 7, R8 vale 6."
                ),
                t!(
                    "Écrivez « imul rax, r8 » à la place du TODO.",
                    "Write imul rax, r8 in place of the TODO.",
                    "Escriba «imul rax, r8» en lugar del TODO."
                ),
            ],
            takeaway: vec![
                t!(
                    "« div r9 » divise le couple RDX:RAX par R9, et non RAX seul : le quotient revient dans RAX, le reste dans RDX.",
                    "div r9 divides the RDX:RAX pair by R9, not RAX alone: the quotient lands in RAX, the remainder in RDX.",
                    "«div r9» divide el par RDX:RAX entre R9, no RAX solo: el cociente va a RAX, el resto a RDX."
                ),
                t!(
                    "Oublier « xor rdx, rdx » avant une division non signée, c'est diviser un nombre de 128 bits par accident — et souvent lever une exception #DE que rien n'annonce.",
                    "Forgetting xor rdx, rdx before an unsigned division means dividing a 128-bit number by accident — and often raising a #DE exception nothing warns about.",
                    "Olvidar «xor rdx, rdx» antes de una división sin signo es dividir un número de 128 bits por accidente — y a menudo lanzar una excepción #DE sin aviso."
                ),
                t!(
                    "« div » n'accepte pas d'immédiat : le diviseur doit être dans un registre ou en mémoire.",
                    "div takes no immediate: the divisor must be in a register or in memory.",
                    "«div» no acepta inmediatos: el divisor debe estar en un registro o en memoria."
                ),
            ],
        },
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
            why: Some(t!(
                "Une fonction est la première chose qu'un langage de haut niveau vous donne et la dernière que l'assembleur vous demande de construire vous-même : personne ne sauvegarde le cadre à votre place, personne ne dépile l'adresse de retour. Voir ce mécanisme une fois, c'est comprendre pourquoi un débordement de pile peut détourner un programme.",
                "A function is the first thing a high-level language gives you and the last thing assembly asks you to build yourself: nobody saves the frame for you, nobody pops the return address. Seeing this mechanism once is understanding why a stack overflow can hijack a program.",
                "Una función es lo primero que da un lenguaje de alto nivel y lo último que el ensamblador le pide construir usted mismo: nadie guarda el marco por usted, nadie desapila la dirección de retorno. Verlo una vez es entender por qué un desbordamiento de pila puede secuestrar un programa."
            )),
            hints: vec![
                t!(
                    "Le prologue et l'épilogue sont déjà écrits : ne les touchez pas. Il manque le calcul lui-même, une seule ligne.",
                    "The prologue and epilogue are already written: leave them alone. What is missing is the computation itself, a single line.",
                    "El prólogo y el epílogo ya están escritos: no los toque. Falta el cálculo en sí, una sola línea."
                ),
                t!(
                    "L'argument est arrivé dans RDI et vient d'être copié dans RAX. Le carré, c'est ce nombre multiplié par lui-même — donc par RDI, qui n'a pas bougé.",
                    "The argument arrived in RDI and has just been copied into RAX. The square is that number times itself — so times RDI, which has not moved.",
                    "El argumento llegó en RDI y acaba de copiarse en RAX. El cuadrado es ese número por sí mismo — es decir por RDI, que no ha cambiado."
                ),
                t!(
                    "Écrivez « imul rax, rdi » à la place du TODO.",
                    "Write imul rax, rdi in place of the TODO.",
                    "Escriba «imul rax, rdi» en lugar del TODO."
                ),
            ],
            takeaway: vec![
                t!(
                    "« call » empile l'adresse de retour puis saute ; « ret » la dépile et y retourne. La pile est ce qui rend l'aller-retour possible.",
                    "call pushes the return address then jumps; ret pops it and goes back. The stack is what makes the round trip possible.",
                    "«call» apila la dirección de retorno y salta; «ret» la desapila y vuelve. La pila hace posible el ida y vuelta."
                ),
                t!(
                    "Prologue et épilogue encadrent la fonction : ils ouvrent un cadre de pile et rendent celui de l'appelant intact.",
                    "Prologue and epilogue frame the function: they open a stack frame and give the caller's back untouched.",
                    "Prólogo y epílogo enmarcan la función: abren un marco de pila y devuelven intacto el del llamante."
                ),
                t!(
                    "L'argument arrive dans RDI, le résultat repart dans RAX. Ce n'est pas le processeur qui l'impose, c'est une convention — celle de la leçon suivante.",
                    "The argument arrives in RDI, the result leaves in RAX. The processor does not impose this, a convention does — the one of the next lesson.",
                    "El argumento llega en RDI, el resultado sale en RAX. No lo impone el procesador sino una convención — la de la lección siguiente."
                ),
            ],
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
            why: Some(t!(
                "Deux codes écrits par deux personnes doivent pouvoir s'appeler sans se détruire mutuellement. Le processeur, lui, ne protège rien : c'est un contrat écrit, pas une règle matérielle. Le respecter est ce qui permet à votre assembleur d'appeler la bibliothèque C, et à elle de vous rendre la main sans dégâts.",
                "Two pieces of code written by two people must be able to call each other without wrecking each other. The processor protects nothing: this is a written contract, not a hardware rule. Honouring it is what lets your assembly call the C library, and lets it return without damage.",
                "Dos códigos escritos por dos personas deben poder llamarse sin destruirse. El procesador no protege nada: es un contrato escrito, no una regla del hardware. Respetarlo permite que su ensamblador llame a la biblioteca C y que esta devuelva el control sin daños."
            )),
            hints: vec![
                t!(
                    "Lancez le programme tel quel et regardez RBX à la fin : il ne vaut plus 111. La fonction s'en est servie comme d'un brouillon.",
                    "Run the program as is and look at RBX at the end: it is no longer 111. The function used it as scratch paper.",
                    "Ejecute el programa tal cual y mire RBX al final: ya no vale 111. La función lo usó como borrador."
                ),
                t!(
                    "RBX est un registre PRÉSERVÉ : celui qui s'en sert doit le rendre comme il l'a trouvé. La pile est faite pour cela, et il y a deux TODO — un pour le mettre de côté, un pour le reprendre.",
                    "RBX is a CALLEE-SAVED register: whoever uses it must return it as found. The stack exists for that, and there are two TODOs — one to set it aside, one to take it back.",
                    "RBX es un registro PRESERVADO: quien lo usa debe devolverlo como lo encontró. La pila sirve para eso, y hay dos TODO — uno para apartarlo, otro para recuperarlo."
                ),
                t!(
                    "Écrivez « push rbx » au premier TODO et « pop rbx » au second, juste avant le « ret ».",
                    "Write push rbx at the first TODO and pop rbx at the second, just before the ret.",
                    "Escriba «push rbx» en el primer TODO y «pop rbx» en el segundo, justo antes del «ret»."
                ),
            ],
            takeaway: vec![
                t!(
                    "Arguments entiers : RDI, RSI, RDX, RCX, R8, R9, dans cet ordre. Résultat dans RAX.",
                    "Integer arguments: RDI, RSI, RDX, RCX, R8, R9, in that order. Result in RAX.",
                    "Argumentos enteros: RDI, RSI, RDX, RCX, R8, R9, en ese orden. Resultado en RAX."
                ),
                t!(
                    "Préservés par l'appelé : RBX, RBP, R12 à R15. Tous les autres peuvent être écrasés sans prévenir.",
                    "Callee-saved: RBX, RBP, R12 to R15. All the others may be clobbered without warning.",
                    "Preservados por el llamado: RBX, RBP, R12 a R15. Todos los demás pueden ser sobrescritos sin aviso."
                ),
                t!(
                    "Le processeur n'impose rien de tout cela. C'est un contrat entre codes, et le seul moyen de le voir rompu est de regarder un registre après un appel.",
                    "The processor imposes none of this. It is a contract between codes, and the only way to see it broken is to look at a register after a call.",
                    "El procesador no impone nada de esto. Es un contrato entre códigos, y la única forma de verlo roto es mirar un registro tras una llamada."
                ),
            ],
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
            why: Some(t!(
                "Un programme seul ne peut rien afficher, rien lire, rien ouvrir : il n'a pas le droit de toucher au matériel. L'appel système est la porte unique par laquelle il demande au noyau de le faire pour lui — et donc le seul endroit où un programme sort vraiment de lui-même.",
                "A program alone can display nothing, read nothing, open nothing: it is not allowed to touch hardware. The system call is the single door through which it asks the kernel to do it — and therefore the only place a program truly steps outside itself.",
                "Un programa solo no puede mostrar, leer ni abrir nada: no tiene derecho a tocar el hardware. La llamada al sistema es la única puerta por la que pide al núcleo que lo haga — y por tanto el único lugar donde un programa sale realmente de sí mismo."
            )),
            hints: vec![
                t!(
                    "Lancez le programme tel quel : rien ne s'affiche, et pourtant l'appel a bien eu lieu. Regardez le panneau Appels système, il dit combien d'octets write a reçu l'ordre d'écrire.",
                    "Run the program as is: nothing appears, yet the call did happen. Look at the System calls panel, it says how many bytes write was told to write.",
                    "Ejecute el programa tal cual: no aparece nada, y sin embargo la llamada ocurrió. Mire el panel Llamadas al sistema, dice cuántos bytes se ordenó escribir a write."
                ),
                t!(
                    "RDX porte le nombre d'octets, et la ligne « xor rdx, rdx » le met à zéro : écrire zéro octet réussit, sans rien écrire. La longueur a déjà été calculée sous le nom msg_len.",
                    "RDX carries the byte count, and the xor rdx, rdx line sets it to zero: writing zero bytes succeeds, and writes nothing. The length has already been computed under the name msg_len.",
                    "RDX lleva el número de bytes, y «xor rdx, rdx» lo pone a cero: escribir cero bytes tiene éxito y no escribe nada. La longitud ya está calculada como msg_len."
                ),
                t!(
                    "Remplacez « xor rdx, rdx » par « mov rdx, msg_len ».",
                    "Replace xor rdx, rdx with mov rdx, msg_len.",
                    "Sustituya «xor rdx, rdx» por «mov rdx, msg_len»."
                ),
            ],
            takeaway: vec![
                t!(
                    "L'ABI des appels système n'est PAS celle des fonctions : le quatrième argument passe par R10, et non par RCX, que l'instruction syscall écrase.",
                    "The system call ABI is NOT the function ABI: the fourth argument goes through R10, not RCX, which the syscall instruction clobbers.",
                    "La ABI de las llamadas al sistema NO es la de las funciones: el cuarto argumento va por R10, no por RCX, que la instrucción syscall sobrescribe."
                ),
                t!(
                    "RAX porte le numéro à l'aller et la valeur de retour au retour — négative en cas d'erreur.",
                    "RAX carries the number on the way in and the return value on the way out — negative on error.",
                    "RAX lleva el número a la ida y el valor de retorno a la vuelta — negativo si hay error."
                ),
                t!(
                    "« $ - msg » laisse l'assembleur compter la longueur : une constante calculée ne se désynchronise jamais du texte qu'elle mesure.",
                    "$ - msg lets the assembler count the length: a computed constant never drifts from the text it measures.",
                    "«$ - msg» deja que el ensamblador cuente la longitud: una constante calculada nunca se desincroniza del texto que mide."
                ),
            ],
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
            why: Some(t!(
                "La mémoire d'un programme ne lui est pas donnée : elle se demande. Tant qu'aucune page n'a été réservée, écrire à une adresse plausible ne produit pas une valeur fausse mais la mort immédiate du processus. C'est ce que malloc cache, et ce que cette leçon montre à nu.",
                "A program's memory is not given to it: it must be asked for. Until a page has been reserved, writing to a plausible address does not produce a wrong value but the immediate death of the process. That is what malloc hides, and what this lesson lays bare.",
                "La memoria de un programa no se le da: se pide. Mientras no se haya reservado una página, escribir en una dirección plausible no da un valor erróneo sino la muerte inmediata del proceso. Eso es lo que oculta malloc, y lo que esta lección muestra al desnudo."
            )),
            hints: vec![
                t!(
                    "Lancez le programme tel quel, comme le commentaire y invite : il meurt sur SIGSEGV. C'est la bonne façon de commencer cette leçon.",
                    "Run the program as is, as the comment invites you to: it dies on SIGSEGV. That is the right way to start this lesson.",
                    "Ejecute el programa tal cual, como invita el comentario: muere con SIGSEGV. Es la forma correcta de empezar."
                ),
                t!(
                    "Le deuxième brk redemande la limite qu'il a déjà : elle ne bouge pas, donc aucune page n'est réservée. Il faut demander PLUS HAUT que R12.",
                    "The second brk asks for the limit it already has: it does not move, so no page is reserved. You must ask for something HIGHER than R12.",
                    "El segundo brk pide el límite que ya tiene: no se mueve, así que no se reserva ninguna página. Hay que pedir MÁS ALTO que R12."
                ),
                t!(
                    "Remplacez « mov rdi, r12 » par « lea rdi, [r12 + 4096] » : lea calcule l'adresse sans lire la mémoire.",
                    "Replace mov rdi, r12 with lea rdi, [r12 + 4096]: lea computes the address without reading memory.",
                    "Sustituya «mov rdi, r12» por «lea rdi, [r12 + 4096]»: lea calcula la dirección sin leer memoria."
                ),
            ],
            takeaway: vec![
                t!(
                    "« brk(0) » renvoie la limite actuelle sans rien changer : c'est la façon de savoir où l'on est avant de pousser.",
                    "brk(0) returns the current limit without changing anything: that is how you learn where you stand before pushing.",
                    "«brk(0)» devuelve el límite actual sin cambiar nada: así se sabe dónde se está antes de empujar."
                ),
                t!(
                    "Si la nouvelle limite n'a pas bougé, la demande a été refusée. Le noyau ne lève pas d'erreur, il rend l'ancienne valeur — il faut comparer.",
                    "If the new limit has not moved, the request was refused. The kernel raises no error, it returns the old value — you have to compare.",
                    "Si el nuevo límite no cambió, la petición fue rechazada. El núcleo no lanza error, devuelve el valor antiguo — hay que comparar."
                ),
                t!(
                    "« lea » calcule une adresse sans jamais lire la mémoire : c'est de l'arithmétique déguisée en accès, et c'est aussi ce qui en fait une instruction d'optimisation.",
                    "lea computes an address without ever reading memory: it is arithmetic disguised as an access, which is also what makes it an optimisation instruction.",
                    "«lea» calcula una dirección sin leer memoria: es aritmética disfrazada de acceso, y por eso también es una instrucción de optimización."
                ),
            ],
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
            why: Some(t!(
                "Parcourir un tableau est ce que font la plupart des programmes la plupart du temps. Le processeur a une instruction faite pour cela : base plus index fois échelle, calculé pendant l'accès, sans instruction séparée. Comprendre cette forme, c'est lire d'un coup d'œil la moitié du code compilé qu'on rencontre.",
                "Walking an array is what most programs do most of the time. The processor has an addressing mode made for it: base plus index times scale, computed during the access, with no separate instruction. Understanding that form means reading half of all compiled code at a glance.",
                "Recorrer un arreglo es lo que hacen casi todos los programas casi siempre. El procesador tiene un modo de direccionamiento para eso: base más índice por escala, calculado durante el acceso. Entender esa forma es leer de un vistazo la mitad del código compilado."
            )),
            hints: vec![
                t!(
                    "La boucle est complète : l'index monte, la comparaison arrête au bon moment. Il manque uniquement la ligne qui lit et accumule.",
                    "The loop is complete: the index goes up, the comparison stops at the right time. Only the line that reads and accumulates is missing.",
                    "El bucle está completo: el índice sube, la comparación para a tiempo. Solo falta la línea que lee y acumula."
                ),
                t!(
                    "Un élément fait 8 octets, et RCX porte l'index — pas l'adresse. La forme « [base + index*échelle] » fait la multiplication pour vous.",
                    "An element is 8 bytes, and RCX holds the index — not the address. The [base + index*scale] form does the multiplication for you.",
                    "Un elemento ocupa 8 bytes, y RCX lleva el índice — no la dirección. La forma «[base + índice*escala]» hace la multiplicación por usted."
                ),
                t!(
                    "Écrivez « add rbx, [tab + rcx*8] » à la place du TODO.",
                    "Write add rbx, [tab + rcx*8] in place of the TODO.",
                    "Escriba «add rbx, [tab + rcx*8]» en lugar del TODO."
                ),
            ],
            takeaway: vec![
                t!(
                    "L'échelle ne peut valoir que 1, 2, 4 ou 8 : exactement les tailles des types entiers, et ce n'est pas un hasard.",
                    "The scale can only be 1, 2, 4 or 8: exactly the sizes of integer types, and that is no accident.",
                    "La escala solo puede ser 1, 2, 4 u 8: justo los tamaños de los tipos enteros, y no es casualidad."
                ),
                t!(
                    "Un index n'est jamais négatif : on le compare avec « jb » (non signé), pas avec « jl ». La confusion produit des boucles qui ne s'arrêtent pas.",
                    "An index is never negative: compare it with jb (unsigned), not jl. Getting this wrong produces loops that never end.",
                    "Un índice nunca es negativo: se compara con «jb» (sin signo), no con «jl». Confundirlos produce bucles que no terminan."
                ),
                t!(
                    "« equ » avec « $ - tab » laisse l'assembleur compter les éléments : ajouter une valeur au tableau met la borne à jour toute seule.",
                    "equ with $ - tab lets the assembler count the elements: adding a value to the array updates the bound by itself.",
                    "«equ» con «$ - tab» deja que el ensamblador cuente los elementos: añadir un valor actualiza el límite solo."
                ),
            ],
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
            why: Some(t!(
                "Une structure n'existe pas dans le processeur. Ce que votre langage appelle un champ n'est qu'un nombre d'octets à ajouter à une adresse — et le voir ainsi explique d'un coup pourquoi l'ordre des champs change la taille d'une structure, et pourquoi un compilateur y insère parfois du vide.",
                "A struct does not exist inside the processor. What your language calls a field is just a number of bytes added to an address — and seeing it that way explains at once why field order changes a struct's size, and why a compiler sometimes inserts padding.",
                "Una estructura no existe en el procesador. Lo que su lenguaje llama campo es solo un número de bytes que se suma a una dirección — y verlo así explica de golpe por qué el orden de los campos cambia el tamaño y por qué el compilador a veces inserta relleno."
            )),
            hints: vec![
                t!(
                    "RSI a déjà été avancé d'une taille de point : il désigne le second point, pas le premier. Le champ x est lu, il ne manque que l'autre.",
                    "RSI has already been advanced by one point size: it points at the second point, not the first. Field x is read, only the other one is missing.",
                    "RSI ya avanzó el tamaño de un punto: apunta al segundo punto, no al primero. El campo x ya se lee, solo falta el otro."
                ),
                t!(
                    "pt_y vaut 8 : c'est le décalage du champ y depuis le début de la structure. La forme « [rsi + pt_y] » se lit « le champ y du point pointé par RSI ».",
                    "pt_y is 8: the offset of field y from the start of the struct. The form [rsi + pt_y] reads as field y of the point RSI points at.",
                    "pt_y vale 8: el desplazamiento del campo y desde el inicio. La forma «[rsi + pt_y]» se lee «el campo y del punto apuntado por RSI»."
                ),
                t!(
                    "Écrivez « add rbx, [rsi + pt_y] » à la place du TODO.",
                    "Write add rbx, [rsi + pt_y] in place of the TODO.",
                    "Escriba «add rbx, [rsi + pt_y]» en lugar del TODO."
                ),
            ],
            takeaway: vec![
                t!(
                    "Un champ est un décalage constant depuis le début de la structure ; « equ » ne fait que lui donner un nom lisible.",
                    "A field is a constant offset from the start of the struct; equ merely gives it a readable name.",
                    "Un campo es un desplazamiento constante desde el inicio; «equ» solo le da un nombre legible."
                ),
                t!(
                    "Passer à l'élément suivant, c'est ajouter la taille complète de la structure — d'où l'intérêt de la nommer elle aussi.",
                    "Moving to the next element means adding the full size of the struct — which is why it deserves a name too.",
                    "Pasar al siguiente elemento es sumar el tamaño completo de la estructura — por eso también merece un nombre."
                ),
                t!(
                    "Un pointeur sur structure et un pointeur sur son premier champ ont la même valeur : c'est ce qui rend la conversion gratuite en C.",
                    "A pointer to a struct and a pointer to its first field hold the same value: that is what makes the conversion free in C.",
                    "Un puntero a estructura y un puntero a su primer campo valen lo mismo: por eso la conversión es gratuita en C."
                ),
            ],
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
            why: Some(t!(
                "Une chaîne C ne range nulle part sa longueur : elle s'arrête au premier octet nul. Toute la famille des débordements de tampon vient de cette décision, prise pour économiser huit octets — et la mesurer soi-même une fois vaut mieux que dix explications sur strlen.",
                "A C string stores its length nowhere: it stops at the first null byte. The whole buffer-overflow family descends from that decision, taken to save eight bytes — and measuring one yourself once beats ten explanations of strlen.",
                "Una cadena C no guarda su longitud en ninguna parte: termina en el primer byte nulo. Toda la familia de desbordamientos de búfer viene de esa decisión, tomada para ahorrar ocho bytes — y medirla usted mismo una vez vale más que diez explicaciones de strlen."
            )),
            hints: vec![
                t!(
                    "Le garde-fou « cmp rbx, 64 » n'est pas la vraie condition d'arrêt : il est là pour que la boucle finisse quand même. La vraie condition manque.",
                    "The cmp rbx, 64 guard is not the real stopping condition: it is there so the loop ends anyway. The real condition is missing.",
                    "El límite «cmp rbx, 64» no es la condición de parada real: está para que el bucle acabe igualmente. Falta la condición verdadera."
                ),
                t!(
                    "AL vient de recevoir l'octet courant. « test al, al » positionne ZF sans rien modifier, et « jz .fin » sort quand l'octet est nul.",
                    "AL has just received the current byte. test al, al sets ZF without modifying anything, and jz .fin exits when the byte is zero.",
                    "AL acaba de recibir el byte actual. «test al, al» posiciona ZF sin modificar nada, y «jz .fin» sale cuando el byte es cero."
                ),
                t!(
                    "Écrivez « test al, al » puis, à la ligne suivante, « jz .fin ».",
                    "Write test al, al then, on the next line, jz .fin.",
                    "Escriba «test al, al» y, en la línea siguiente, «jz .fin»."
                ),
            ],
            takeaway: vec![
                t!(
                    "« test al, al » est la façon idiomatique de demander « cet octet est-il nul ? » : un ET logique jeté, dont on ne garde que les drapeaux.",
                    "test al, al is the idiomatic way to ask is this byte zero: a discarded logical AND, of which only the flags are kept.",
                    "«test al, al» es la forma idiomática de preguntar si un byte es cero: un AND lógico descartado del que solo se guardan las banderas."
                ),
                t!(
                    "Lire un octet demande un registre d'un octet : « mov al, [rsi + rbx] », pas « mov rax ».",
                    "Reading one byte needs a one-byte register: mov al, [rsi + rbx], not mov rax.",
                    "Leer un byte requiere un registro de un byte: «mov al, [rsi + rbx]», no «mov rax»."
                ),
                t!(
                    "Sans octet nul, la boucle continue dans la mémoire du voisin. C'est très exactement le mécanisme des débordements de tampon.",
                    "Without a null byte, the loop walks on into the neighbour's memory. That is exactly the buffer overflow mechanism.",
                    "Sin byte nulo, el bucle sigue por la memoria del vecino. Ese es exactamente el mecanismo del desbordamiento de búfer."
                ),
            ],
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
            why: Some(t!(
                "Un exécutable n'est pas une suite d'instructions posée sur le disque : c'est un fichier structuré, que le noyau lit avant de vous donner la main. Savoir où sont l'entrée, les segments et les sections, c'est pouvoir répondre soi-même à « pourquoi ce binaire ne démarre pas », au lieu d'attendre qu'un outil le dise.",
                "An executable is not a stream of instructions sitting on disk: it is a structured file the kernel reads before handing control to you. Knowing where the entry, the segments and the sections are means answering why does this binary not start yourself, instead of waiting for a tool to say it.",
                "Un ejecutable no es una secuencia de instrucciones en el disco: es un archivo estructurado que el núcleo lee antes de darle el control. Saber dónde están la entrada, los segmentos y las secciones permite responder usted mismo a «por qué no arranca este binario»."
            )),
            hints: vec![
                t!(
                    "Le nombre magique est déjà lu et déjà juste : c'est la seconde moitié qui manque. RBX contient e_entry, l'adresse que le noyau a sautée pour démarrer.",
                    "The magic number is already read and already right: it is the second half that is missing. RBX holds e_entry, the address the kernel jumped to at startup.",
                    "El número mágico ya está leído y correcto: falta la segunda mitad. RBX contiene e_entry, la dirección a la que saltó el núcleo."
                ),
                t!(
                    "Si e_entry vaut bien l'adresse de _start, leur différence est nulle. « _start » sans crochets est justement cette adresse.",
                    "If e_entry really is the address of _start, their difference is zero. _start without brackets is precisely that address.",
                    "Si e_entry es la dirección de _start, su diferencia es cero. «_start» sin corchetes es justamente esa dirección."
                ),
                t!(
                    "Écrivez « sub rbx, _start » à la place du TODO.",
                    "Write sub rbx, _start in place of the TODO.",
                    "Escriba «sub rbx, _start» en lugar del TODO."
                ),
            ],
            takeaway: vec![
                t!(
                    "L'en-tête ELF est mappé en mémoire avec le programme : un exécutable peut se relire lui-même, sans ouvrir aucun fichier.",
                    "The ELF header is mapped into memory along with the program: an executable can read itself, without opening any file.",
                    "La cabecera ELF se mapea en memoria con el programa: un ejecutable puede leerse a sí mismo, sin abrir ningún archivo."
                ),
                t!(
                    "Segments et sections ne servent pas au même lecteur : le noyau charge des segments, ld assemble des sections. Le même octet appartient aux deux découpes.",
                    "Segments and sections serve different readers: the kernel loads segments, ld assembles sections. The same byte belongs to both cuts.",
                    "Segmentos y secciones sirven a lectores distintos: el núcleo carga segmentos, ld ensambla secciones. El mismo byte pertenece a ambos cortes."
                ),
                t!(
                    "Lire 7F 45 4C 46 comme un entier donne 0x464C457F : le petit-boutisme n'est pas une curiosité de cours, il se voit ici sur un cas réel.",
                    "Reading 7F 45 4C 46 as an integer gives 0x464C457F: little-endianness is not a textbook curiosity, you see it here on a real case.",
                    "Leer 7F 45 4C 46 como entero da 0x464C457F: el little-endian no es una curiosidad de manual, se ve aquí en un caso real."
                ),
            ],
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
            why: Some(t!(
                "nasm ne voit qu'un fichier à la fois : il ignore où atterriront les sections et quelle taille elles auront une fois tout rassemblé. Tout ce qu'il ne peut pas savoir, il le laisse en trou avec une consigne — et c'est ld qui remplit. Comprendre ce partage, c'est savoir lire une erreur d'édition de liens plutôt que la subir.",
                "nasm sees one file at a time: it does not know where sections will land nor how big they will be once everything is gathered. Whatever it cannot know it leaves as a hole with an instruction — and ld fills it in. Understanding that split means reading a link error instead of enduring it.",
                "nasm solo ve un archivo a la vez: ignora dónde caerán las secciones y qué tamaño tendrán al reunirlo todo. Lo que no puede saber lo deja como hueco con una consigna — y ld lo rellena. Entender ese reparto es saber leer un error de enlazado."
            )),
            hints: vec![
                t!(
                    "Les deux symboles dont vous avez besoin sont déclarés en haut, et aucun fichier ne les définit : c'est ld qui les fabrique.",
                    "The two symbols you need are declared at the top, and no file defines them: ld manufactures them.",
                    "Los dos símbolos que necesita están declarados arriba y ningún archivo los define: los fabrica ld."
                ),
                t!(
                    "RBX contient déjà _end, la fin de l'image mémoire. La taille de .bss est la distance entre son début et cette fin.",
                    "RBX already holds _end, the end of the memory image. The size of .bss is the distance from its start to that end.",
                    "RBX ya contiene _end, el final de la imagen en memoria. El tamaño de .bss es la distancia desde su inicio hasta ese final."
                ),
                t!(
                    "Écrivez « sub rbx, __bss_start » à la place du TODO.",
                    "Write sub rbx, __bss_start in place of the TODO.",
                    "Escriba «sub rbx, __bss_start» en lugar del TODO."
                ),
            ],
            takeaway: vec![
                t!(
                    "ld fabrique des symboles qu'aucun fichier ne définit : __ehdr_start, __bss_start, _end. Ils ne coûtent pas un octet dans le fichier.",
                    "ld manufactures symbols no file defines: __ehdr_start, __bss_start, _end. They cost not one byte in the file.",
                    "ld fabrica símbolos que ningún archivo define: __ehdr_start, __bss_start, _end. No cuestan ni un byte en el archivo."
                ),
                t!(
                    "Un symbole n'est qu'une adresse : « mov rbx, _end » charge un nombre, pas un contenu. Les crochets liraient la mémoire à cet endroit.",
                    "A symbol is only an address: mov rbx, _end loads a number, not a content. Brackets would read the memory there.",
                    "Un símbolo es solo una dirección: «mov rbx, _end» carga un número, no un contenido. Los corchetes leerían la memoria allí."
                ),
                t!(
                    ".bss ne pèse rien dans le fichier et tout en mémoire : « resb » réserve sans écrire, et le noyau met la page à zéro au chargement.",
                    ".bss weighs nothing in the file and everything in memory: resb reserves without writing, and the kernel zeroes the page at load time.",
                    ".bss no pesa nada en el archivo y todo en memoria: «resb» reserva sin escribir, y el núcleo pone la página a cero al cargar."
                ),
            ],
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
            why: Some(t!(
                "Quand un programme appelle une fonction d'une bibliothèque partagée, il ne sait pas où elle sera : la même bibliothèque est chargée à des adresses différentes dans chaque processus. L'indirection par une table est ce qui rend cela possible — et c'est aussi la cible favorite de qui veut détourner un appel.",
                "When a program calls a shared library function, it does not know where it will be: the same library is loaded at different addresses in each process. Indirection through a table is what makes this possible — and it is also the favourite target of anyone wanting to hijack a call.",
                "Cuando un programa llama a una función de una biblioteca compartida, no sabe dónde estará: la misma biblioteca se carga en direcciones distintas en cada proceso. La indirección por tabla lo hace posible — y es también el blanco favorito de quien quiere secuestrar una llamada."
            )),
            hints: vec![
                t!(
                    "La table contient deux adresses, dans l'ordre où elles sont écrites : double d'abord, triple ensuite. L'appel actuel prend la première.",
                    "The table holds two addresses, in the order written: double first, triple second. The current call takes the first one.",
                    "La tabla contiene dos direcciones, en el orden escrito: double primero, triple después. La llamada actual toma la primera."
                ),
                t!(
                    "Une adresse fait 8 octets : la deuxième entrée se trouve donc huit octets plus loin que la première.",
                    "An address is 8 bytes: the second entry therefore sits eight bytes past the first.",
                    "Una dirección ocupa 8 bytes: la segunda entrada está pues ocho bytes más allá."
                ),
                t!(
                    "Écrivez « call [rsi + 8] » à la place de « call [rsi] ».",
                    "Write call [rsi + 8] in place of call [rsi].",
                    "Escriba «call [rsi + 8]» en lugar de «call [rsi]»."
                ),
            ],
            takeaway: vec![
                t!(
                    "Les crochets font toute la différence : « call rsi » saute DANS la table, « call [rsi] » appelle l'adresse rangée là.",
                    "Brackets make all the difference: call rsi jumps INTO the table, call [rsi] calls the address stored there.",
                    "Los corchetes lo cambian todo: «call rsi» salta DENTRO de la tabla, «call [rsi]» llama a la dirección guardada allí."
                ),
                t!(
                    "La PLT est un talon qui lit la GOT ; la GOT n'est qu'un tableau d'adresses. Rien de plus que ce que vous venez d'écrire à la main.",
                    "The PLT is a stub that reads the GOT; the GOT is just an array of addresses. Nothing more than what you just wrote by hand.",
                    "La PLT es un talón que lee la GOT; la GOT es solo un arreglo de direcciones. Nada más que lo que acaba de escribir a mano."
                ),
                t!(
                    "Cette indirection permet de résoudre l'adresse au premier appel seulement — et d'écrire dans la table pour détourner tous les suivants.",
                    "This indirection allows resolving the address at the first call only — and writing into the table to hijack every later one.",
                    "Esa indirección permite resolver la dirección solo en la primera llamada — y escribir en la tabla para secuestrar todas las siguientes."
                ),
            ],
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
            why: Some(t!(
                "Un même programme doit pouvoir être chargé n'importe où en mémoire — c'est ce que fait le noyau à chaque exécution, pour rendre les attaques plus difficiles. Ce qui le permet tient dans la différence entre désigner une adresse absolue et désigner un écart : la première fige, la seconde suit.",
                "The same program must be loadable anywhere in memory — which is what the kernel does at every run, to make attacks harder. What allows it lies in the difference between naming an absolute address and naming an offset: the first pins, the second follows.",
                "Un mismo programa debe poder cargarse en cualquier parte de la memoria — es lo que hace el núcleo en cada ejecución, para dificultar los ataques. Lo que lo permite está en la diferencia entre nombrar una dirección absoluta y nombrar un desplazamiento."
            )),
            hints: vec![
                t!(
                    "Les deux lignes doivent désigner le MÊME octet par deux chemins différents. Relisez-les : elles ne nomment pas la même chose.",
                    "The two lines must name the SAME byte by two different routes. Read them again: they do not name the same thing.",
                    "Las dos líneas deben nombrar el MISMO byte por dos caminos distintos. Reléalas: no nombran lo mismo."
                ),
                t!(
                    "La première vise « valeur », la seconde vise « _start ». Corrigez la seconde en gardant la forme « rel », qui est ce que la leçon enseigne.",
                    "The first aims at valeur, the second at _start. Fix the second while keeping the rel form, which is what the lesson teaches.",
                    "La primera apunta a «valeur», la segunda a «_start». Corrija la segunda conservando la forma «rel», que es lo que enseña la lección."
                ),
                t!(
                    "Écrivez « lea rcx, [rel valeur] ».",
                    "Write lea rcx, [rel valeur].",
                    "Escriba «lea rcx, [rel valeur]»."
                ),
            ],
            takeaway: vec![
                t!(
                    "Une relocation est une consigne laissée par nasm à ld : « écris ici l'adresse que tu finiras par connaître ». « readelf -r » les montre.",
                    "A relocation is an instruction nasm leaves for ld: write here the address you will eventually know. readelf -r shows them.",
                    "Una reubicación es una consigna que nasm deja a ld: «escribe aquí la dirección que acabarás conociendo». «readelf -r» las muestra."
                ),
                t!(
                    "R_X86_64_64 écrit une adresse absolue et fige le programme ; R_X86_64_PC32 écrit un écart depuis RIP et le rend déplaçable.",
                    "R_X86_64_64 writes an absolute address and pins the program; R_X86_64_PC32 writes an offset from RIP and makes it relocatable.",
                    "R_X86_64_64 escribe una dirección absoluta y fija el programa; R_X86_64_PC32 escribe un desplazamiento desde RIP y lo hace reubicable."
                ),
                t!(
                    "Tout code PIE ou partagé n'utilise que la forme relative : c'est pour cela que « rel » revient partout dans le code moderne.",
                    "All PIE or shared code uses only the relative form: that is why rel appears everywhere in modern code.",
                    "Todo código PIE o compartido usa solo la forma relativa: por eso «rel» aparece por todas partes en el código moderno."
                ),
            ],
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
            why: Some(t!(
                "Additionner quatre nombres demande quatre instructions — ou une seule, si on les met côte à côte dans un registre large. C'est ainsi que sont écrits le décodage vidéo, le traitement d'images et l'algèbre des bibliothèques de calcul : non pas en allant plus vite, mais en faisant plusieurs choses à la fois.",
                "Adding four numbers takes four instructions — or one, if you place them side by side in a wide register. That is how video decoding, image processing and the linear algebra of compute libraries are written: not by going faster, but by doing several things at once.",
                "Sumar cuatro números requiere cuatro instrucciones — o una sola, si se ponen lado a lado en un registro ancho. Así se escriben la decodificación de vídeo, el procesado de imágenes y el álgebra de las bibliotecas de cálculo: no yendo más rápido, sino haciendo varias cosas a la vez."
            )),
            hints: vec![
                t!(
                    "Les deux chargements sont faits : ouvrez le panneau SSE / FPU et regardez xmm0 et xmm1, découpés en quatre entiers de 32 bits.",
                    "Both loads are done: open the SSE / FPU panel and look at xmm0 and xmm1, split into four 32-bit integers.",
                    "Ambas cargas están hechas: abra el panel SSE / FPU y mire xmm0 y xmm1, divididos en cuatro enteros de 32 bits."
                ),
                t!(
                    "« paddd » additionne quatre doublewords à la fois. Le d final dit la découpe, et c'est elle qui compte : avec « paddw », la retenue de 65535 + 1 serait perdue.",
                    "paddd adds four doublewords at once. The trailing d names the split, and the split is what matters: with paddw the carry of 65535 + 1 would be lost.",
                    "«paddd» suma cuatro doublewords a la vez. La d final indica el corte, y el corte es lo que importa: con «paddw» se perdería el acarreo de 65535 + 1."
                ),
                t!(
                    "Écrivez « paddd xmm0, xmm1 » à la place du TODO.",
                    "Write paddd xmm0, xmm1 in place of the TODO.",
                    "Escriba «paddd xmm0, xmm1» en lugar del TODO."
                ),
            ],
            takeaway: vec![
                t!(
                    "Un registre XMM fait 128 bits : quatre entiers de 32, huit de 16, ou seize octets. La dernière lettre de l'instruction dit la découpe choisie.",
                    "An XMM register is 128 bits: four 32-bit integers, eight 16-bit ones, or sixteen bytes. The instruction's last letter names the chosen split.",
                    "Un registro XMM tiene 128 bits: cuatro enteros de 32, ocho de 16 o dieciséis bytes. La última letra de la instrucción indica el corte elegido."
                ),
                t!(
                    "Chaque tranche s'arrête à son bord : une retenue ne passe jamais dans la voisine. Se tromper de découpe donne un résultat faux, pas une erreur.",
                    "Each lane stops at its edge: a carry never crosses into its neighbour. Choosing the wrong split gives a wrong result, not an error.",
                    "Cada carril se detiene en su borde: un acarreo nunca pasa al vecino. Equivocar el corte da un resultado erróneo, no un error."
                ),
                t!(
                    "« movdqa » exige une adresse multiple de 16 et plante sinon ; « movdqu » accepte tout, un peu plus lentement. D'où le « align 16 ».",
                    "movdqa demands a 16-byte aligned address and faults otherwise; movdqu accepts anything, slightly slower. Hence the align 16.",
                    "«movdqa» exige una dirección múltiplo de 16 y falla si no; «movdqu» acepta cualquiera, algo más lento. De ahí el «align 16»."
                ),
            ],
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
            why: Some(t!(
                "Un compilateur qui remplace une multiplication par une addition d'adresse ne triche pas : il utilise un circuit qui était déjà là, gratuit, prévu pour calculer des adresses. Reconnaître ces formes dans du code compilé, c'est cesser de le trouver illisible — la plupart de ses bizarreries sont exactement cela.",
                "A compiler replacing a multiplication with an address addition is not cheating: it uses a circuit that was already there, free, meant for computing addresses. Recognising these forms in compiled code is what stops it from looking unreadable — most of its oddities are exactly this.",
                "Un compilador que sustituye una multiplicación por una suma de direcciones no hace trampa: usa un circuito que ya estaba ahí, gratis, previsto para calcular direcciones. Reconocer esas formas en código compilado es dejar de encontrarlo ilegible."
            )),
            hints: vec![
                t!(
                    "« imul » est interdit par la leçon, et l'attente le vérifie : la bonne valeur obtenue par ce moyen sera refusée. Il faut passer par « lea ».",
                    "imul is forbidden by the lesson, and the check enforces it: the right value obtained that way will be rejected. You must go through lea.",
                    "«imul» está prohibido por la lección, y la comprobación lo verifica: el valor correcto obtenido así será rechazado. Hay que pasar por «lea»."
                ),
                t!(
                    "L'en-tête donne la décomposition : ×10, c'est ×5 puis doublé. Et ×5 s'écrit « [rax + rax*4] » — une base plus un index à l'échelle 4.",
                    "The header gives the decomposition: ×10 is ×5 then doubled. And ×5 is written [rax + rax*4] — a base plus an index at scale 4.",
                    "La cabecera da la descomposición: ×10 es ×5 y luego doblado. Y ×5 se escribe «[rax + rax*4]» — una base más un índice a escala 4."
                ),
                t!(
                    "Écrivez « lea rbx, [rax + rax*4] » au premier TODO, puis « add rbx, rbx » au second.",
                    "Write lea rbx, [rax + rax*4] at the first TODO, then add rbx, rbx at the second.",
                    "Escriba «lea rbx, [rax + rax*4]» en el primer TODO y «add rbx, rbx» en el segundo."
                ),
            ],
            takeaway: vec![
                t!(
                    "« lea » calcule sans lire la mémoire : c'est l'additionneur-multiplieur du processeur, disponible sans toucher aux flags.",
                    "lea computes without reading memory: it is the processor's adder-multiplier, available without touching the flags.",
                    "«lea» calcula sin leer memoria: es el sumador-multiplicador del procesador, disponible sin tocar las banderas."
                ),
                t!(
                    "Les échelles possibles sont 1, 2, 4 et 8 : de quoi fabriquer ×2, ×3, ×4, ×5, ×8 et ×9 en une instruction, et le reste en deux.",
                    "The available scales are 1, 2, 4 and 8: enough to build ×2, ×3, ×4, ×5, ×8 and ×9 in one instruction, and the rest in two.",
                    "Las escalas posibles son 1, 2, 4 y 8: bastan para ×2, ×3, ×4, ×5, ×8 y ×9 en una instrucción, y el resto en dos."
                ),
                t!(
                    "Doubler s'écrit « add rbx, rbx » aussi bien que « shl rbx, 1 » : à ce niveau, plusieurs écritures donnent le même circuit.",
                    "Doubling is written add rbx, rbx as well as shl rbx, 1: at this level several spellings map to the same circuit.",
                    "Doblar se escribe «add rbx, rbx» igual que «shl rbx, 1»: a este nivel varias escrituras dan el mismo circuito."
                ),
            ],
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
            why: Some(t!(
                "Sans les sources, il ne reste que ce que le programme fait. C'est la situation de qui analyse un logiciel malveillant, vérifie un binaire qu'on lui a livré, ou cherche à comprendre un format que personne n'a documenté — et le XOR est ce qu'on y rencontre en premier, parce qu'il est réversible et tient en une instruction.",
                "Without sources, all that remains is what the program does. That is the situation of anyone analysing malware, checking a binary they were handed, or working out an undocumented format — and XOR is the first thing you meet there, because it is reversible and fits in one instruction.",
                "Sin las fuentes, solo queda lo que el programa hace. Es la situación de quien analiza software malicioso, verifica un binario que le entregaron o intenta entender un formato sin documentar — y el XOR es lo primero que aparece, por ser reversible y caber en una instrucción."
            )),
            hints: vec![
                t!(
                    "La boucle lit bien chaque octet et les additionne : lancez-la telle quelle, RBX donne la somme des octets CHIFFRÉS. Il manque une étape entre les deux.",
                    "The loop does read each byte and sum them: run it as is, RBX gives the sum of the ENCRYPTED bytes. One step is missing in between.",
                    "El bucle lee cada byte y los suma: ejecútelo tal cual, RBX da la suma de los bytes CIFRADOS. Falta un paso entre medias."
                ),
                t!(
                    "Le XOR est sa propre inverse : déchiffrer, c'est refaire exactement la même opération avec la même clé, qui est nommée « cle » juste au-dessus.",
                    "XOR is its own inverse: decrypting is redoing the exact same operation with the same key, named cle just above.",
                    "El XOR es su propia inversa: descifrar es repetir la misma operación con la misma clave, llamada «cle» arriba."
                ),
                t!(
                    "Écrivez « xor al, cle » à la place du TODO.",
                    "Write xor al, cle in place of the TODO.",
                    "Escriba «xor al, cle» en lugar del TODO."
                ),
            ],
            takeaway: vec![
                t!(
                    "Le XOR est sa propre inverse : la même instruction chiffre et déchiffre, ce qui explique sa présence partout en analyse.",
                    "XOR is its own inverse: the same instruction encrypts and decrypts, which explains why it is everywhere in analysis.",
                    "El XOR es su propia inversa: la misma instrucción cifra y descifra, lo que explica su omnipresencia en análisis."
                ),
                t!(
                    "« xor al, al » met à zéro, « xor al, cle » chiffre : même instruction, deux rôles, et c'est l'opérande qui tranche.",
                    "xor al, al zeroes, xor al, cle encrypts: same instruction, two roles, and the operand decides.",
                    "«xor al, al» pone a cero, «xor al, cle» cifra: la misma instrucción, dos papeles, y el operando decide."
                ),
                t!(
                    "Analyser, c'est lire ce que le programme FAIT, pas ce qu'il dit faire : ici, la boucle trahit le chiffrement avant même qu'on ait la clé.",
                    "Analysing means reading what the program DOES, not what it claims to do: here the loop gives the encryption away before you even have the key.",
                    "Analizar es leer lo que el programa HACE, no lo que dice hacer: aquí el bucle delata el cifrado antes de tener la clave."
                ),
            ],
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
            why: Some(t!(
                "Rien, dans un fichier, ne distingue le code des données : c'est le découpage en instructions qui décide, et ce découpage dépend du point de départ. Se tromper d'un octet fait lire une suite d'instructions parfaitement plausible et entièrement fausse — c'est sur cette ambiguïté que reposent la plupart des tours d'obscurcissement.",
                "Nothing in a file distinguishes code from data: the split into instructions decides, and that split depends on the starting point. Being one byte off yields a perfectly plausible and entirely false instruction stream — most obfuscation tricks rest on that ambiguity.",
                "Nada en un archivo distingue el código de los datos: lo decide el corte en instrucciones, y ese corte depende del punto de partida. Errar un byte produce una secuencia de instrucciones plausible y totalmente falsa — en esa ambigüedad se apoyan casi todos los trucos de ofuscación."
            )),
            hints: vec![
                t!(
                    "Ces sept octets ne sont pas des données : ouvrez l'onglet Désassemblage, ASM Studio les relit comme une instruction et vous montre laquelle.",
                    "These seven bytes are not data: open the Disassembly tab, ASM Studio reads them back as an instruction and shows you which.",
                    "Esos siete bytes no son datos: abra la pestaña Desensamblado, ASM Studio los relee como instrucción y le muestra cuál."
                ),
                t!(
                    "Les trois premiers octets décrivent l'instruction, les quatre derniers portent le nombre. 42 s'écrit 0x2A, et le petit-boutisme le place en premier.",
                    "The first three bytes describe the instruction, the last four carry the number. 42 is 0x2A, and little-endianness puts it first.",
                    "Los tres primeros bytes describen la instrucción, los cuatro últimos llevan el número. 42 es 0x2A, y el little-endian lo pone primero."
                ),
                t!(
                    "Remplacez le quatrième octet, 0x00, par 0x2a : la ligne devient « db 0x48, 0xc7, 0xc3, 0x2a, 0x00, 0x00, 0x00 ».",
                    "Replace the fourth byte, 0x00, with 0x2a: the line becomes db 0x48, 0xc7, 0xc3, 0x2a, 0x00, 0x00, 0x00.",
                    "Sustituya el cuarto byte, 0x00, por 0x2a: la línea queda «db 0x48, 0xc7, 0xc3, 0x2a, 0x00, 0x00, 0x00»."
                ),
            ],
            takeaway: vec![
                t!(
                    "Un désassembleur reconnaît une instruction à son premier octet, en déduit sa longueur, et sait ainsi où commence la suivante.",
                    "A disassembler recognises an instruction by its first byte, derives its length, and thereby knows where the next one starts.",
                    "Un desensamblador reconoce una instrucción por su primer byte, deduce su longitud y así sabe dónde empieza la siguiente."
                ),
                t!(
                    "REX.W (0x48) annonce un opérande de 64 bits ; l'octet ModR/M désigne les registres. Ce sont les deux pièces qu'on retrouve partout.",
                    "REX.W (0x48) announces a 64-bit operand; the ModR/M byte names the registers. Those are the two pieces you meet everywhere.",
                    "REX.W (0x48) anuncia un operando de 64 bits; el byte ModR/M nombra los registros. Son las dos piezas que aparecen siempre."
                ),
                t!(
                    "Se tromper d'un octet au point d'entrée décale tout le désassemblage qui suit : les frontières glissent, et le code lu devient une fiction.",
                    "Being one byte off at the entry point shifts the whole disassembly that follows: boundaries slide, and the code you read becomes fiction.",
                    "Errar un byte en el punto de entrada desplaza todo el desensamblado: las fronteras se corren y el código leído se vuelve ficción."
                ),
            ],
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
            why: Some(t!(
                "Deux programmes qui doivent se parler ne partagent pas de mémoire : le noyau leur ouvre un canal, et chacun n'en tient qu'un bout. Le tube est le plus simple de ces canaux, celui que le shell installe entre deux commandes reliées par une barre verticale — et s'y tromper de bout est une erreur qu'aucun message ne rattrape.",
                "Two programs that must talk share no memory: the kernel opens a channel and each holds one end. The pipe is the simplest such channel, the one the shell installs between two commands joined by a vertical bar — and grabbing the wrong end is a mistake no message rescues.",
                "Dos programas que deben hablarse no comparten memoria: el núcleo les abre un canal y cada uno sostiene un extremo. La tubería es el más simple de esos canales, el que instala la shell entre dos órdenes unidas por una barra vertical — y equivocar el extremo es un error que ningún mensaje corrige."
            )),
            hints: vec![
                t!(
                    "Regardez le panneau Appels système après le read : il a échoué. Le noyau n'a pas planté, il a simplement refusé — et RAX porte un nombre négatif.",
                    "Look at the System calls panel after the read: it failed. The kernel did not crash, it simply refused — and RAX holds a negative number.",
                    "Mire el panel Llamadas al sistema tras el read: falló. El núcleo no se cayó, simplemente lo rechazó — y RAX lleva un número negativo."
                ),
                t!(
                    "Les deux appels utilisent le même descripteur, celui de fds+4. Or un tube a deux bouts : on n'écrit pas et on ne lit pas sur le même.",
                    "Both calls use the same descriptor, the one at fds+4. But a pipe has two ends: you do not write to and read from the same one.",
                    "Ambas llamadas usan el mismo descriptor, el de fds+4. Pero una tubería tiene dos extremos: no se escribe y se lee por el mismo."
                ),
                t!(
                    "Dans le read, remplacez « mov edi, [rel fds + 4] » par « mov edi, [rel fds] ».",
                    "In the read, replace mov edi, [rel fds + 4] with mov edi, [rel fds].",
                    "En el read, sustituya «mov edi, [rel fds + 4]» por «mov edi, [rel fds]»."
                ),
            ],
            takeaway: vec![
                t!(
                    "fds[0] est l'extrémité de LECTURE, fds[1] celle d'ÉCRITURE. Deux entiers de 32 bits, d'où le décalage de 4 octets pour atteindre le second.",
                    "fds[0] is the READ end, fds[1] the WRITE end. Two 32-bit integers, hence the 4-byte offset to reach the second.",
                    "fds[0] es el extremo de LECTURA, fds[1] el de ESCRITURA. Dos enteros de 32 bits, de ahí el desplazamiento de 4 bytes."
                ),
                t!(
                    "Un appel système qui échoue ne plante pas le programme : il rend une valeur négative que personne ne vous force à regarder. C'est la source d'erreurs la plus discrète du système.",
                    "A failing system call does not crash the program: it returns a negative value nobody forces you to look at. It is the system's quietest source of bugs.",
                    "Una llamada fallida no bloquea el programa: devuelve un valor negativo que nadie le obliga a mirar. Es la fuente de errores más silenciosa del sistema."
                ),
                t!(
                    "Un descripteur de fichier est un entier de 32 bits : « mov edi », pas « mov rdi » — et le noyau ne lit que la moitié basse.",
                    "A file descriptor is a 32-bit integer: mov edi, not mov rdi — and the kernel reads only the low half.",
                    "Un descriptor de archivo es un entero de 32 bits: «mov edi», no «mov rdi» — y el núcleo solo lee la mitad baja."
                ),
            ],
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
            why: Some(t!(
                "Du code injecté dans un programme déjà lancé ne dispose de rien : ni section de données, ni adresse fixe, et souvent pas le droit de contenir un octet nul, puisque c'est lui qui termine la saisie qui le transporte. Écrire sous ces contraintes montre mieux qu'un cours ce que l'encodage des instructions a de concret.",
                "Code injected into a running program has nothing: no data section, no fixed address, and often no right to contain a null byte, since that is what terminates the input carrying it. Writing under those constraints shows better than any lecture how concrete instruction encoding is.",
                "El código inyectado en un programa en marcha no dispone de nada: ni sección de datos, ni dirección fija, y a menudo sin derecho a contener un byte nulo, pues es el que termina la entrada que lo transporta. Escribir bajo esas restricciones enseña mejor que cualquier clase lo concreta que es la codificación."
            )),
            hints: vec![
                t!(
                    "Le zéro terminal est déjà empilé : il ne manque que les deux lettres, et elles doivent arriver AVANT lui dans la mémoire, donc être empilées après.",
                    "The terminating zero is already pushed: only the two letters are missing, and they must sit BEFORE it in memory, hence be pushed afterwards.",
                    "El cero final ya está apilado: solo faltan las dos letras, que deben quedar ANTES en memoria, es decir apilarse después."
                ),
                t!(
                    "'H' vaut 0x48 et 'i' vaut 0x69. En petit-boutisme, l'octet de poids faible est le premier en mémoire : la valeur à charger est donc 0x6948.",
                    "H is 0x48 and i is 0x69. In little-endian the low byte comes first in memory: the value to load is therefore 0x6948.",
                    "'H' vale 0x48 e 'i' vale 0x69. En little-endian el byte bajo va primero en memoria: el valor a cargar es 0x6948."
                ),
                t!(
                    "Écrivez « mov rax, 0x6948 » puis, à la ligne suivante, « push rax ».",
                    "Write mov rax, 0x6948 then, on the next line, push rax.",
                    "Escriba «mov rax, 0x6948» y, en la línea siguiente, «push rax»."
                ),
            ],
            takeaway: vec![
                t!(
                    "Un shellcode transporte ses données sur la pile parce qu'il n'a aucune section à lui et ne connaît aucune adresse fixe.",
                    "A shellcode carries its data on the stack because it owns no section and knows no fixed address.",
                    "Un shellcode lleva sus datos en la pila porque no tiene sección propia ni conoce dirección fija."
                ),
                t!(
                    "« mov rax, 60 » contient des octets nuls, « xor rax, rax » suivi de « mov al, 60 » n'en contient aucun. Le panneau Instruction montre les deux encodages.",
                    "mov rax, 60 contains null bytes, xor rax, rax followed by mov al, 60 contains none. The Instruction panel shows both encodings.",
                    "«mov rax, 60» contiene bytes nulos, «xor rax, rax» seguido de «mov al, 60» no. El panel Instrucción muestra ambas codificaciones."
                ),
                t!(
                    "Une chaîne construite sur la pile se lit à l'adresse que RSP porte : c'est la seule adresse qu'un code injecté connaisse à coup sûr.",
                    "A string built on the stack is read at the address RSP holds: that is the only address injected code knows for certain.",
                    "Una cadena construida en la pila se lee en la dirección que lleva RSP: la única dirección que el código inyectado conoce con certeza."
                ),
            ],
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
            why: Some(t!(
                "L'adresse de retour d'une fonction est rangée sur la pile, juste au-dessus des variables locales, et « ret » lui obéit sans jamais la vérifier. Voir une fois, en miniature et sous contrôle, comment un débordement l'atteint, c'est comprendre du même coup à quoi servent le canari, la pile non exécutable et l'ASLR.",
                "A function's return address sits on the stack, just above the local variables, and ret obeys it without ever checking. Seeing once, in miniature and under control, how an overflow reaches it is understanding at the same time what the canary, non-executable stacks and ASLR are for.",
                "La dirección de retorno está en la pila, justo encima de las variables locales, y «ret» la obedece sin comprobarla. Ver una vez, en miniatura y bajo control, cómo la alcanza un desbordamiento es entender a la vez para qué sirven el canario, la pila no ejecutable y el ASLR."
            )),
            hints: vec![
                t!(
                    "Le schéma de la pile est donné dans l'en-tête du programme : trois lignes, trois décalages depuis RBP. L'écriture actuelle vise le mauvais.",
                    "The stack layout is given in the program header: three lines, three offsets from RBP. The current write aims at the wrong one.",
                    "El esquema de la pila está en la cabecera: tres líneas, tres desplazamientos desde RBP. La escritura actual apunta al equivocado."
                ),
                t!(
                    "[rbp + 0] est l'ancien RBP sauvegardé — l'écraser ne détourne rien. L'adresse de retour est huit octets plus haut.",
                    "[rbp + 0] is the saved old RBP — overwriting it hijacks nothing. The return address is eight bytes higher.",
                    "[rbp + 0] es el RBP antiguo guardado — sobrescribirlo no secuestra nada. La dirección de retorno está ocho bytes más arriba."
                ),
                t!(
                    "Remplacez « mov [rbp + 0], rax » par « mov [rbp + 8], rax ».",
                    "Replace mov [rbp + 0], rax with mov [rbp + 8], rax.",
                    "Sustituya «mov [rbp + 0], rax» por «mov [rbp + 8], rax»."
                ),
            ],
            takeaway: vec![
                t!(
                    "Après le prologue : le tampon sous RBP, l'ancien RBP en [rbp + 0], l'adresse de retour en [rbp + 8]. Ce plan explique tous les débordements de pile.",
                    "After the prologue: buffer below RBP, saved RBP at [rbp + 0], return address at [rbp + 8]. That map explains every stack overflow.",
                    "Tras el prólogo: el búfer bajo RBP, el RBP guardado en [rbp + 0], la dirección de retorno en [rbp + 8]. Ese plano explica todos los desbordamientos."
                ),
                t!(
                    "« ret » n'est qu'un saut à l'adresse dépilée : il ne vérifie rien, et c'est très exactement là que tient la vulnérabilité.",
                    "ret is just a jump to the popped address: it checks nothing, and that is precisely where the vulnerability lies.",
                    "«ret» es solo un salto a la dirección desapilada: no comprueba nada, y ahí está exactamente la vulnerabilidad."
                ),
                t!(
                    "Trois défenses répondent à cela : le canari (détecte l'écrasement), NX (interdit d'exécuter la pile), ASLR (rend les adresses imprévisibles).",
                    "Three defences answer it: the canary (detects the overwrite), NX (forbids executing the stack), ASLR (makes addresses unpredictable).",
                    "Tres defensas responden: el canario (detecta la sobrescritura), NX (prohíbe ejecutar la pila), ASLR (hace impredecibles las direcciones)."
                ),
            ],
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
            why: Some(t!(
                "On récrit toujours le mauvais morceau tant qu'on n'a pas mesuré. Ce que cette leçon apprend n'est pas une astuce de plus mais l'ordre des opérations : compter d'abord les instructions réellement exécutées, choisir ensuite — et savoir que la réponse dépend de la machine, si bien qu'aucune recette ne vaut la mesure.",
                "You always rewrite the wrong part until you have measured. What this lesson teaches is not one more trick but the order of operations: first count the instructions actually executed, then choose — and know that the answer depends on the machine, so no recipe beats measuring.",
                "Siempre se reescribe la parte equivocada mientras no se mide. Lo que enseña esta lección no es un truco más sino el orden: contar primero las instrucciones realmente ejecutadas, elegir después — y saber que la respuesta depende de la máquina, así que ninguna receta vale más que medir."
            )),
            hints: vec![
                t!(
                    "« imul » est interdit par la leçon et l'attente le vérifie : obtenir 72 par une multiplication sera refusé.",
                    "imul is forbidden by the lesson and the check enforces it: getting 72 through a multiplication will be rejected.",
                    "«imul» está prohibido y la comprobación lo verifica: obtener 72 mediante una multiplicación será rechazado."
                ),
                t!(
                    "Multiplier par 8, c'est décaler de trois rangs vers la gauche, puisque 8 vaut 2 puissance 3. La ligne « add rax, 0 » est là pour être remplacée.",
                    "Multiplying by 8 means shifting three places left, since 8 is 2 to the power 3. The add rax, 0 line is there to be replaced.",
                    "Multiplicar por 8 es desplazar tres posiciones a la izquierda, pues 8 es 2 elevado a 3. La línea «add rax, 0» está para ser sustituida."
                ),
                t!(
                    "Remplacez « add rax, 0 » par « shl rax, 3 ».",
                    "Replace add rax, 0 with shl rax, 3.",
                    "Sustituya «add rax, 0» por «shl rax, 3»."
                ),
            ],
            takeaway: vec![
                t!(
                    "Multiplier par une puissance de deux, c'est décaler : « shl rax, 3 » fait ×8 en un cycle et sans dépendance.",
                    "Multiplying by a power of two is shifting: shl rax, 3 does ×8 in one cycle with no dependency.",
                    "Multiplicar por una potencia de dos es desplazar: «shl rax, 3» hace ×8 en un ciclo y sin dependencia."
                ),
                t!(
                    "La Timeline compte les instructions réellement exécutées : c'est la seule donnée qui ne se discute pas.",
                    "The Timeline counts the instructions actually executed: it is the one figure that cannot be argued with.",
                    "La Línea de tiempo cuenta las instrucciones realmente ejecutadas: el único dato indiscutible."
                ),
                t!(
                    "La règle n'est pas « fuir imul » mais « mesurer » : sur un multiplicateur quelconque, imul redevient le bon choix.",
                    "The rule is not avoid imul but measure: for an arbitrary multiplier, imul is the right choice again.",
                    "La regla no es «huir de imul» sino «medir»: con un multiplicador cualquiera, imul vuelve a ser la opción correcta."
                ),
            ],
        },
        Lesson {
            id: "win_premier",
            level: Level::Windows,
            title: t!(
                "Premier programme Windows",
                "First Windows program",
                "Primer programa Windows"
            ),
            goal: t!(
                "Assembler un exécutable Windows et le terminer proprement, sans appel système.",
                "Assemble a Windows executable and end it cleanly, without a system call.",
                "Ensamblar un ejecutable de Windows y terminarlo limpiamente, sin llamada al sistema."
            ),
            steps: vec![
                t!(
                    "Choisis la cible « Windows — PE64 console » (menu Exécution ▸ Cible). Elle est déjà posée par cette leçon.",
                    "Pick the \"Windows — PE64 console\" target (Run ▸ Target menu). This lesson already sets it.",
                    "Elige el destino «Windows — PE64 consola» (menú Ejecución ▸ Destino). Esta lección ya lo pone."
                ),
                t!(
                    "Sous Linux, terminer un programme c'est « mov rax, 60 » puis « syscall ». Sous Windows, c'est appeler ExitProcess, une fonction de kernel32.dll.",
                    "On Linux, ending a program is \"mov rax, 60\" then \"syscall\". On Windows, it is calling ExitProcess, a function of kernel32.dll.",
                    "En Linux, terminar un programa es «mov rax, 60» y «syscall». En Windows, es llamar a ExitProcess, una función de kernel32.dll."
                ),
                t!(
                    "« extern ExitProcess » ne contient pas de code : il demande au lieur d'inscrire ce nom dans la table d'import du .exe.",
                    "\"extern ExitProcess\" holds no code: it asks the linker to record that name in the .exe import table.",
                    "«extern ExitProcess» no contiene código: pide al enlazador que inscriba ese nombre en la tabla de importación del .exe."
                ),
                t!(
                    "Assemble avec Ctrl+B. Si Wine est installé, F5 exécute le programme et son code de sortie apparaît dans la console. Sinon, le panneau FORMAT montre ce que contient le fichier produit.",
                    "Assemble with Ctrl+B. If Wine is installed, F5 runs the program and its exit code shows in the console. Otherwise, the FORMAT panel shows what the produced file holds.",
                    "Ensambla con Ctrl+B. Si Wine está instalado, F5 ejecuta el programa y su código de salida aparece en la consola. Si no, el panel FORMATO muestra lo que contiene el archivo."
                ),
                t!(
                    "Il n'y a pas de pas-à-pas ici : Wine exécute le programme, il ne le déroule pas instruction par instruction. Les registres et la timeline restent à la cible Linux.",
                    "There is no single-stepping here: Wine runs the program, it does not walk it instruction by instruction. Registers and timeline stay with the Linux target.",
                    "Aquí no hay paso a paso: Wine ejecuta el programa, no lo recorre instrucción por instrucción. Los registros y la línea de tiempo quedan en el destino Linux."
                ),
            ],
            panels: vec!["editor", "console", "format"],
            starter: Some(L_WIN_PREMIER),
            why: Some(t!(
                "Le même processeur, les mêmes instructions, et pourtant un programme Linux ne tourne pas sous Windows. Ce qui change n'est pas le langage mais la façon de demander un service au système : un numéro et « syscall » d'un côté, l'appel d'une fonction de DLL de l'autre. Voir cette frontière, c'est comprendre ce qu'un système d'exploitation est vraiment.",
                "The same processor, the same instructions, and yet a Linux program does not run on Windows. What changes is not the language but the way you ask the system for a service: a number and syscall on one side, a DLL function call on the other. Seeing that border is understanding what an operating system really is.",
                "El mismo procesador, las mismas instrucciones, y sin embargo un programa Linux no funciona en Windows. Lo que cambia no es el lenguaje sino cómo se pide un servicio al sistema: un número y «syscall» de un lado, la llamada a una función de DLL del otro."
            )),
            hints: vec![
                t!(
                    "Le code de sortie est déjà passé à ExitProcess : la ligne existe, elle porte simplement la mauvaise valeur.",
                    "The exit code is already passed to ExitProcess: the line exists, it merely carries the wrong value.",
                    "El código de salida ya se pasa a ExitProcess: la línea existe, solo lleva el valor equivocado."
                ),
                t!(
                    "Windows attend son premier argument dans RCX — ici son moitié basse, ECX, puisqu'un code de sortie tient sur 32 bits.",
                    "Windows expects its first argument in RCX — here its low half, ECX, since an exit code fits in 32 bits.",
                    "Windows espera su primer argumento en RCX — aquí su mitad baja, ECX, pues un código de salida cabe en 32 bits."
                ),
                t!(
                    "Écrivez « mov ecx, 7 ».",
                    "Write mov ecx, 7.",
                    "Escriba «mov ecx, 7»."
                ),
            ],
            takeaway: vec![
                t!(
                    "Pas de « syscall » sous Windows : on appelle une fonction d'une DLL, déclarée par « extern » et inscrite par le lieur dans la table d'import.",
                    "No syscall on Windows: you call a DLL function, declared with extern and written by the linker into the import table.",
                    "Sin «syscall» en Windows: se llama a una función de DLL, declarada con «extern» e inscrita por el enlazador en la tabla de importación."
                ),
                t!(
                    "Le point d'entrée s'appelle « main » et non « _start » : c'est le nom que le lieur cherche pour un exécutable console.",
                    "The entry point is called main, not _start: that is the name the linker looks for in a console executable.",
                    "El punto de entrada se llama «main», no «_start»: es el nombre que busca el enlazador en un ejecutable de consola."
                ),
                t!(
                    "ExitProcess ne rend jamais la main, exactement comme l'appel exit de Linux : ce qui suit ne sera jamais exécuté.",
                    "ExitProcess never returns, exactly like the Linux exit call: whatever follows will never run.",
                    "ExitProcess nunca devuelve el control, igual que la llamada exit de Linux: lo que sigue nunca se ejecuta."
                ),
            ],
        },
        Lesson {
            id: "win_appel",
            level: Level::Windows,
            title: t!(
                "La convention d'appel Microsoft",
                "The Microsoft calling convention",
                "La convención de llamada de Microsoft"
            ),
            goal: t!(
                "Passer les arguments dans les registres qu'attend Windows, et non ceux de Linux.",
                "Pass arguments in the registers Windows expects, not Linux's.",
                "Pasar los argumentos en los registros que espera Windows, no los de Linux."
            ),
            steps: vec![
                t!(
                    "Linux passe les six premiers arguments par RDI, RSI, RDX, RCX, R8, R9. Windows par RCX, RDX, R8, R9, puis la pile.",
                    "Linux passes the first six arguments in RDI, RSI, RDX, RCX, R8, R9. Windows uses RCX, RDX, R8, R9, then the stack.",
                    "Linux pasa los seis primeros argumentos por RDI, RSI, RDX, RCX, R8, R9. Windows usa RCX, RDX, R8, R9 y luego la pila."
                ),
                t!(
                    "Rien ne signale l'erreur : le programme s'assemble, se lie et s'exécute — avec la mauvaise valeur. C'est le premier piège du passage d'un monde à l'autre.",
                    "Nothing flags the mistake: the program assembles, links and runs — with the wrong value. That is the first trap when moving between the two worlds.",
                    "Nada señala el error: el programa se ensambla, se enlaza y se ejecuta — con el valor equivocado. Es la primera trampa al pasar de un mundo a otro."
                ),
                t!(
                    "Le cinquième argument et les suivants vont sur la pile, APRÈS l'espace d'ombre — c'est le « mov qword [rsp + 32] » qu'on voit dans les appels à WriteFile.",
                    "The fifth argument onwards goes on the stack, AFTER the shadow space — that is the \"mov qword [rsp + 32]\" seen in WriteFile calls.",
                    "El quinto argumento en adelante va a la pila, DESPUÉS del espacio de sombra — es el «mov qword [rsp + 32]» que se ve en las llamadas a WriteFile."
                ),
                t!(
                    "Le retour se lit dans RAX des deux côtés : c'est le seul point commun des deux conventions.",
                    "The return value is read from RAX on both sides: the only thing the two conventions share.",
                    "El valor de retorno se lee en RAX en ambos lados: lo único que comparten las dos convenciones."
                ),
            ],
            panels: vec!["editor", "console", "disasm"],
            starter: Some(L_WIN_APPEL),
            why: Some(t!(
                "C'est l'erreur la plus fréquente en passant d'un monde à l'autre, et la plus difficile à voir : le programme s'assemble, se lie et s'exécute sans se plaindre — il travaille simplement sur une valeur qui n'est pas la bonne. Aucun outil ne signalera jamais un argument mis dans le registre du voisin.",
                "It is the most frequent mistake when crossing from one world to the other, and the hardest to see: the program assembles, links and runs without complaint — it simply works on the wrong value. No tool will ever flag an argument placed in the neighbouring register.",
                "Es el error más frecuente al pasar de un mundo a otro, y el más difícil de ver: el programa ensambla, enlaza y se ejecuta sin quejarse — simplemente trabaja con un valor equivocado. Ninguna herramienta señalará un argumento puesto en el registro del vecino."
            )),
            hints: vec![
                t!(
                    "Le programme s'assemble et se lie sans une erreur, et rend pourtant le mauvais code : la valeur 42 est bien écrite, mais pas là où ExitProcess la cherche.",
                    "The program assembles and links without a single error, yet returns the wrong code: 42 is written, but not where ExitProcess looks for it.",
                    "El programa ensambla y enlaza sin error y devuelve el código equivocado: 42 está escrito, pero no donde ExitProcess lo busca."
                ),
                t!(
                    "Le tableau de l'en-tête donne les deux ordres. RDI est le premier registre de Linux ; celui de Windows est ailleurs dans la liste.",
                    "The table in the header gives both orders. RDI is Linux's first register; Windows' first one is elsewhere in the list.",
                    "La tabla de la cabecera da ambos órdenes. RDI es el primer registro de Linux; el de Windows está en otro sitio de la lista."
                ),
                t!(
                    "Remplacez « mov edi, 42 » par « mov ecx, 42 ».",
                    "Replace mov edi, 42 with mov ecx, 42.",
                    "Sustituya «mov edi, 42» por «mov ecx, 42»."
                ),
            ],
            takeaway: vec![
                t!(
                    "Windows : RCX, RDX, R8, R9, puis la pile. Linux : RDI, RSI, RDX, RCX, R8, R9. Les deux listes partagent des registres, dans un ordre différent — ce qui rend l'erreur silencieuse.",
                    "Windows: RCX, RDX, R8, R9, then the stack. Linux: RDI, RSI, RDX, RCX, R8, R9. Both lists share registers in a different order — which is what makes the mistake silent.",
                    "Windows: RCX, RDX, R8, R9, luego la pila. Linux: RDI, RSI, RDX, RCX, R8, R9. Ambas listas comparten registros en distinto orden — por eso el error es silencioso."
                ),
                t!(
                    "Une convention d'appel n'est pas imposée par le processeur : rien ne l'empêche d'être violée, et rien ne le signale.",
                    "A calling convention is not enforced by the processor: nothing prevents it from being violated, and nothing reports it.",
                    "Una convención de llamada no la impone el procesador: nada impide violarla, y nada lo avisa."
                ),
                t!(
                    "Au-delà du quatrième argument, Windows passe par la pile — et ces valeurs-là viennent APRÈS l'espace d'ombre.",
                    "Beyond the fourth argument, Windows goes through the stack — and those values come AFTER the shadow space.",
                    "Más allá del cuarto argumento, Windows usa la pila — y esos valores van DESPUÉS del espacio de sombra."
                ),
            ],
        },
        Lesson {
            id: "win_pile",
            level: Level::Windows,
            title: t!("L'espace d'ombre", "The shadow space", "El espacio de sombra"),
            goal: t!(
                "Comprendre pourquoi tout appel Windows commence par réserver 40 octets.",
                "Understand why every Windows call starts by reserving 40 bytes.",
                "Entender por qué toda llamada de Windows empieza reservando 40 bytes."
            ),
            steps: vec![
                t!(
                    "L'appelant doit réserver 32 octets pour l'appelé, même si celui-ci ne s'en sert pas : c'est l'espace d'ombre, de la place où ranger les quatre premiers arguments.",
                    "The caller must reserve 32 bytes for the callee, even if it never uses them: that is the shadow space, room to spill the first four arguments.",
                    "El llamador debe reservar 32 bytes para el llamado, aunque no los use: es el espacio de sombra, sitio para volcar los cuatro primeros argumentos."
                ),
                t!(
                    "40 et non 32 : RSP doit être multiple de 16 au moment du « call », et l'adresse de retour empilée à l'entrée de main l'a décalé de 8.",
                    "40 rather than 32: RSP must be a multiple of 16 at the \"call\", and the return address pushed on entry to main shifted it by 8.",
                    "40 y no 32: RSP debe ser múltiplo de 16 en el «call», y la dirección de retorno apilada al entrar en main lo desplazó 8."
                ),
                t!(
                    "Un appel mal aligné plante sur les instructions SSE alignées que la bibliothèque système utilise — donc rarement à l'endroit où l'erreur a été commise.",
                    "A misaligned call faults on the aligned SSE instructions the system library uses — so rarely where the mistake was made.",
                    "Una llamada mal alineada falla en las instrucciones SSE alineadas que usa la biblioteca del sistema — rara vez donde se cometió el error."
                ),
                t!(
                    "Linux n'a pas d'espace d'ombre, mais exige le même alignement sur 16 octets : la moitié de la règle est commune.",
                    "Linux has no shadow space, but demands the same 16-byte alignment: half the rule is shared.",
                    "Linux no tiene espacio de sombra, pero exige la misma alineación de 16 bytes: la mitad de la regla es común."
                ),
            ],
            panels: vec!["editor", "console", "disasm"],
            starter: Some(L_WIN_PILE),
            why: Some(t!(
                "Trente-deux octets que l'appelant réserve et que l'appelé n'utilisera peut-être jamais : l'espace d'ombre paraît absurde tant qu'on ne l'a pas oublié une fois. Le programme plante alors dans une fonction système, très loin de la ligne fautive, et rien dans le message ne renvoie à la pile.",
                "Thirty-two bytes the caller reserves and the callee may never use: the shadow space looks absurd until you have forgotten it once. The program then crashes inside a system function, far from the offending line, and nothing in the message points back to the stack.",
                "Treinta y dos bytes que el llamante reserva y el llamado quizá nunca use: el espacio de sombra parece absurdo hasta que se olvida una vez. El programa se cae dentro de una función del sistema, lejos de la línea culpable, y nada en el mensaje remite a la pila."
            )),
            hints: vec![
                t!(
                    "Le « sub rsp, 0 » ne réserve rien du tout : c'est un emplacement laissé vide, et l'attente exige le nombre exact.",
                    "The sub rsp, 0 reserves nothing at all: it is a placeholder, and the check demands the exact number.",
                    "«sub rsp, 0» no reserva nada: es un hueco, y la comprobación exige el número exacto."
                ),
                t!(
                    "32 octets d'espace d'ombre, plus 8 pour réaligner RSP sur 16 après l'adresse de retour empilée par le call.",
                    "32 bytes of shadow space, plus 8 to realign RSP on 16 after the return address pushed by the call.",
                    "32 bytes de espacio de sombra, más 8 para realinear RSP a 16 tras la dirección de retorno apilada por el call."
                ),
                t!(
                    "Écrivez « sub rsp, 40 ».",
                    "Write sub rsp, 40.",
                    "Escriba «sub rsp, 40»."
                ),
            ],
            takeaway: vec![
                t!(
                    "L'espace d'ombre, c'est 32 octets que l'appelant doit à l'appelé, qu'il s'en serve ou non.",
                    "The shadow space is 32 bytes the caller owes the callee, used or not.",
                    "El espacio de sombra son 32 bytes que el llamante debe al llamado, los use o no."
                ),
                t!(
                    "40 et non 32 : RSP doit être multiple de 16 au moment du « call », et l'adresse de retour l'a déjà décalé de 8.",
                    "40 rather than 32: RSP must be a multiple of 16 at the call, and the return address has already shifted it by 8.",
                    "40 y no 32: RSP debe ser múltiplo de 16 en el «call», y la dirección de retorno ya lo desplazó 8."
                ),
                t!(
                    "Oublier ces huit octets fait planter les fonctions qui utilisent des instructions SSE alignées — c'est-à-dire beaucoup, et loin de chez vous.",
                    "Forgetting those eight bytes crashes functions using aligned SSE instructions — that is, many of them, far away from your code.",
                    "Olvidar esos ocho bytes hace fallar las funciones que usan instrucciones SSE alineadas — es decir, muchas, y lejos de su código."
                ),
            ],
        },
        Lesson {
            id: "win_imports",
            level: Level::Windows,
            title: t!(
                "Importer une fonction d'une DLL",
                "Importing a function from a DLL",
                "Importar una función de una DLL"
            ),
            goal: t!(
                "Appeler du code qui n'est pas dans le programme, et voir comment le .exe le réclame.",
                "Call code that is not in the program, and see how the .exe asks for it.",
                "Llamar a código que no está en el programa, y ver cómo el .exe lo reclama."
            ),
            steps: vec![
                t!(
                    "Le .exe ne contient pas le code de strlen : il contient son NOM, et une case vide que Windows remplira à son adresse au chargement (l'IAT).",
                    "The .exe holds no code for strlen: it holds its NAME, and an empty slot Windows fills with its address at load time (the IAT).",
                    "El .exe no contiene el código de strlen: contiene su NOMBRE y una casilla vacía que Windows rellena con su dirección al cargar (la IAT)."
                ),
                t!(
                    "Assemble, puis regarde le panneau FORMAT : les fonctions importées y sont listées avec leur DLL. C'est exactement ce que lit le chargeur de Windows.",
                    "Assemble, then look at the FORMAT panel: imported functions are listed with their DLL. That is exactly what the Windows loader reads.",
                    "Ensambla y mira el panel FORMATO: las funciones importadas aparecen con su DLL. Es exactamente lo que lee el cargador de Windows."
                ),
                t!(
                    "Dans le désassemblage, un « call strlen » ne saute pas dans le vide : il atteint un petit relais « jmp [rip+…] » qui lit la case de l'IAT.",
                    "In the disassembly, a \"call strlen\" does not jump into the void: it reaches a small \"jmp [rip+…]\" thunk that reads the IAT slot.",
                    "En el desensamblado, un «call strlen» no salta al vacío: llega a un pequeño relevo «jmp [rip+…]» que lee la casilla de la IAT."
                ),
                t!(
                    "ASM Studio connaît les fonctions usuelles de kernel32, user32 et msvcrt. Pour une autre DLL, le nom la porte : « extern gdi32$CreatePen ».",
                    "ASM Studio knows the usual functions of kernel32, user32 and msvcrt. For another DLL, the name carries it: \"extern gdi32$CreatePen\".",
                    "ASM Studio conoce las funciones habituales de kernel32, user32 y msvcrt. Para otra DLL, el nombre la lleva: «extern gdi32$CreatePen»."
                ),
            ],
            panels: vec!["editor", "format", "console"],
            starter: Some(L_WIN_IMPORTS),
            why: Some(t!(
                "Un « extern » ne contient pas une ligne de code : il laisse un nom que le lieur inscrit dans la table d'import, et que Windows remplace par une adresse au chargement. C'est le mécanisme qui permet à un .exe de quelques kilo-octets d'utiliser tout le système — et le premier endroit qu'on regarde pour savoir ce qu'un binaire inconnu sait faire.",
                "An extern holds not a line of code: it leaves a name the linker writes into the import table, which Windows replaces with an address at load time. That mechanism lets a few-kilobyte .exe use the whole system — and it is the first place you look to learn what an unknown binary can do.",
                "Un «extern» no contiene ni una línea de código: deja un nombre que el enlazador inscribe en la tabla de importación y que Windows sustituye por una dirección al cargar. Ese mecanismo permite a un .exe de unos pocos kilobytes usar todo el sistema."
            )),
            hints: vec![
                t!(
                    "Les deux chaînes sont déclarées côte à côte, et une seule est mesurée. Comptez leurs lettres : le code de sortie attendu en désigne une.",
                    "Both strings are declared side by side, and only one is measured. Count their letters: the expected exit code names one of them.",
                    "Ambas cadenas están declaradas juntas y solo se mide una. Cuente sus letras: el código de salida esperado señala una."
                ),
                t!(
                    "strlen lit l'adresse qu'on lui donne dans RCX. Il faut donc lui donner celle de « mot », qui compte sept lettres.",
                    "strlen reads the address given in RCX. So give it the address of mot, which has seven letters.",
                    "strlen lee la dirección dada en RCX. Hay que darle la de «mot», que tiene siete letras."
                ),
                t!(
                    "Remplacez « lea rcx, [autre] » par « lea rcx, [mot] ».",
                    "Replace lea rcx, [autre] with lea rcx, [mot].",
                    "Sustituya «lea rcx, [autre]» por «lea rcx, [mot]»."
                ),
            ],
            takeaway: vec![
                t!(
                    "La table d'import liste ce qu'un exécutable emprunte au système, DLL par DLL. Le panneau FORMAT la montre après l'assemblage.",
                    "The import table lists what an executable borrows from the system, DLL by DLL. The FORMAT panel shows it after assembling.",
                    "La tabla de importación lista lo que el ejecutable toma prestado del sistema, DLL por DLL. El panel FORMATO la muestra tras ensamblar."
                ),
                t!(
                    "Au chargement, Windows écrit les adresses réelles dans l'IAT : c'est l'équivalent exact de la GOT du monde ELF.",
                    "At load time Windows writes the real addresses into the IAT: the exact counterpart of the ELF world's GOT.",
                    "Al cargar, Windows escribe las direcciones reales en la IAT: el equivalente exacto de la GOT del mundo ELF."
                ),
                t!(
                    "Pour une fonction hors des DLL usuelles, le nom porte sa bibliothèque : « extern gdi32$CreatePen ».",
                    "For a function outside the usual DLLs, the name carries its library: extern gdi32$CreatePen.",
                    "Para una función fuera de las DLL habituales, el nombre lleva su biblioteca: «extern gdi32$CreatePen»."
                ),
            ],
        },
        Lesson {
            id: "win_format",
            level: Level::Windows,
            title: t!("Ce que contient un .exe", "What an .exe holds", "Lo que contiene un .exe"),
            goal: t!(
                "Lire un exécutable Windows comme on lit un ELF : sections, entrée, imports.",
                "Read a Windows executable the way you read an ELF: sections, entry, imports.",
                "Leer un ejecutable de Windows como se lee un ELF: secciones, entrada, importaciones."
            ),
            steps: vec![
                t!(
                    "Les deux formats répondent aux mêmes questions : où commence l'exécution, quel morceau est du code, lequel est modifiable, ce qui vient d'ailleurs.",
                    "Both formats answer the same questions: where execution starts, which part is code, which is writable, what comes from elsewhere.",
                    "Ambos formatos responden a las mismas preguntas: dónde empieza la ejecución, qué parte es código, cuál es modificable, qué viene de fuera."
                ),
                t!(
                    "Le panneau FORMAT montre .bss avec zéro octet dans le fichier et de la place en mémoire — vrai des deux côtés, et c'est toute l'idée de cette section.",
                    "The FORMAT panel shows .bss with zero bytes in the file and room in memory — true on both sides, and that is the whole point of that section.",
                    "El panel FORMATO muestra .bss con cero bytes en el archivo y sitio en memoria — cierto en ambos lados, y esa es toda la idea de esa sección."
                ),
                t!(
                    "Le point d'entrée d'un PE est une RVA, relative à la base de l'image (0x140000000). Celui d'un ELF est une adresse absolue.",
                    "A PE entry point is an RVA, relative to the image base (0x140000000). An ELF's is an absolute address.",
                    "El punto de entrada de un PE es una RVA, relativa a la base de la imagen (0x140000000). El de un ELF es una dirección absoluta."
                ),
                t!(
                    "Assemble le même source pour les deux cibles et compare les deux panneaux FORMAT : c'est le meilleur résumé de ce parcours.",
                    "Assemble the same source for both targets and compare the two FORMAT panels: it is the best summary of this path.",
                    "Ensambla el mismo código para ambos destinos y compara los dos paneles FORMATO: es el mejor resumen de este recorrido."
                ),
            ],
            panels: vec!["editor", "format", "memmap"],
            starter: Some(L_WIN_FORMAT),
            why: Some(t!(
                "Un PE et un ELF répondent aux mêmes questions avec d'autres mots : où est le code, où sont les données, qu'est-ce qui est emprunté à l'extérieur, par où commence-t-on. Qui a compris l'un a compris les trois quarts de l'autre — et c'est aussi le seul chose qu'un IDE Linux puisse offrir d'un .exe qu'il ne peut pas exécuter.",
                "A PE and an ELF answer the same questions in different words: where is the code, where is the data, what is borrowed from outside, where do we start. Whoever understood one has understood three quarters of the other — and it is also the only thing a Linux IDE can offer of an .exe it cannot run.",
                "Un PE y un ELF responden a las mismas preguntas con otras palabras: dónde está el código, dónde los datos, qué se toma prestado, por dónde se empieza. Quien ha entendido uno ha entendido tres cuartos del otro."
            )),
            hints: vec![
                t!(
                    "Deux valeurs sont déclarées, douze et treize, et le programme lit la seconde. Le code attendu dit laquelle il fallait.",
                    "Two values are declared, twelve and thirteen, and the program reads the second. The expected code says which one was wanted.",
                    "Se declaran dos valores, doce y trece, y el programa lee el segundo. El código esperado dice cuál se quería."
                ),
                t!(
                    "Les crochets lisent le contenu, exactement comme sous Linux : le format du fichier change, pas la façon d'accéder à la mémoire.",
                    "Brackets read the contents, exactly as on Linux: the file format changes, not the way you reach memory.",
                    "Los corchetes leen el contenido, igual que en Linux: cambia el formato del archivo, no la forma de acceder a la memoria."
                ),
                t!(
                    "Remplacez « mov rcx, [treize] » par « mov rcx, [douze] ».",
                    "Replace mov rcx, [treize] with mov rcx, [douze].",
                    "Sustituya «mov rcx, [treize]» por «mov rcx, [douze]»."
                ),
            ],
            takeaway: vec![
                t!(
                    ".text le code, .data les variables, .bss la place réservée sans octets dans le fichier : ces trois-là sont communes aux deux formats.",
                    ".text the code, .data the variables, .bss space reserved with no bytes in the file: those three are common to both formats.",
                    "«.text» el código, «.data» las variables, «.bss» el espacio reservado sin bytes en el archivo: los tres son comunes a ambos formatos."
                ),
                t!(
                    ".idata est la table d'import du PE ; le monde ELF répond à la même question avec .plt et .got.",
                    ".idata is the PE import table; the ELF world answers the same question with .plt and .got.",
                    "«.idata» es la tabla de importación del PE; el mundo ELF responde con «.plt» y «.got»."
                ),
                t!(
                    "Le point d'entrée d'un PE est une RVA, relative à la base de l'image ; celui d'un ELF est une adresse absolue. Même information, deux façons de la ranger.",
                    "A PE's entry point is an RVA, relative to the image base; an ELF's is an absolute address. Same information, two ways of storing it.",
                    "El punto de entrada de un PE es una RVA, relativa a la base de la imagen; el de un ELF es absoluta. La misma información, dos formas de guardarla."
                ),
            ],
        },
    ]
}

// ======================================================================
//  Le pont entre les leçons et les exercices
// ======================================================================
//
//  Deux parcours vivaient côte à côte sans se connaître : vingt-neuf leçons
//  d'un côté, trente-six exercices auto-corrigés de l'autre, semés dans un
//  dossier que l'élève ouvrait à la main. Une leçon finie ne menait nulle
//  part, et un exercice ouvert ne disait pas de quelle notion il relevait —
//  chacun avait sa progression, aucun n'avait la moitié de l'histoire.
//
//  La table ci-dessous les relie, dans les deux sens : une leçon propose ses
//  exercices d'application, un exercice rappelle la leçon dont il vient. Elle
//  est déclarée à part plutôt qu'en champ des leçons pour une raison qui se
//  vérifie : c'est le seul endroit où l'on peut voir, d'un coup d'œil, qu'un
//  exercice n'est rattaché à rien. Deux tests l'imposent — pas de lien mort,
//  pas d'exercice orphelin.

/// Exercices d'application, par identifiant de leçon. Les noms sont ceux des
/// fichiers semés dans `~/.local/share/asm_studio/examples/`.
const PRACTICE: &[(&str, &[&str])] = &[
    ("premier_programme", &["ex_code_sortie.asm", "ex_c1_bases.asm", "ex_c2_1_code_retour.asm"]),
    ("registres", &["ex_c3_2_copier_registres.asm", "ex_somme.asm"]),
    ("tailles", &["ex_c3_1_tailles.asm"]),
    ("memoire", &["ex_maximum.asm", "ex_moyenne.asm"]),
    ("flags", &["ex_c6_3_pair_impair.asm", "ex_c6_1_plus_petit.asm"]),
    ("pile", &["ex_c5_1_echange.asm", "ex_c5_2_trois_valeurs.asm"]),
    ("sauts", &["ex_c6_2_trois_nombres.asm"]),
    (
        "boucles",
        &[
            "ex_factorielle.asm",
            "ex_c7_1_compte_rebours.asm",
            "ex_c7_3_multiplication.asm",
            "ex_fibonacci.asm",
        ],
    ),
    ("mul_div", &["ex_puissance.asm", "ex_c4_1_calculette.asm", "ex_c4_2_division_signee.asm"]),
    ("fonctions", &["ex_c8_1_soustraire.asm", "ex_c8_2_somme_jusqua.asm"]),
    ("system_v", &["ex_c8_3_trois_appels.asm"]),
    (
        "syscalls",
        &[
            "ex_c2_2_mon_message.asm",
            "ex_c7_2_etoiles.asm",
            "ex_c10_1_triple.asm",
            "ex_c10_2_somme_saisie.asm",
        ],
    ),
    ("tableaux", &["ex_tableau.asm", "ex_c9_1_tableau_min.asm", "ex_c9_3_compter_pairs.asm"]),
    ("chaines", &["ex_longueur.asm", "ex_c11_4_palindrome.asm"]),
    ("optimisation", &["ex_bits.asm", "ex_c11_2_tri_decroissant.asm"]),
    ("performance", &["ex_c11_5_premiers.asm", "ex_c11_3_fizzbuzz.asm"]),
];

/// Exercices qui mettent en pratique une leçon (vide si elle n'en a pas).
pub fn practice_for(lesson_id: &str) -> &'static [&'static str] {
    PRACTICE
        .iter()
        .find(|(id, _)| *id == lesson_id)
        .map(|(_, files)| *files)
        .unwrap_or(&[])
}

/// Leçon dont relève un exercice, d'après son nom de fichier.
///
/// C'est la réciproque : un élève qui ouvre `ex_c7_2_etoiles.asm` sans savoir
/// par où le prendre doit pouvoir remonter à la leçon qui l'explique.
pub fn lesson_of_exercise(file_name: &str) -> Option<Lesson> {
    let id = PRACTICE
        .iter()
        .find(|(_, files)| files.contains(&file_name))
        .map(|(id, _)| *id)?;
    find(id)
}

/// Leçons d'un niveau, dans l'ordre.
pub fn lessons_of(level: Level) -> Vec<Lesson> {
    catalogue().into_iter().filter(|l| l.level == level).collect()
}

/// Le parcours tel qu'il est réellement proposé : le catalogue, moins les
/// leçons Windows quand l'assemblage PE est désactivé.
///
/// C'est ce parcours-là que compte la progression et que parcourent les boutons
/// « précédente » et « suivante » : annoncer « leçon 12 sur 29 » puis sauter les
/// cinq leçons Windows au moment d'avancer serait un compte faux.
pub fn path(with_pe: bool) -> Vec<Lesson> {
    catalogue()
        .into_iter()
        .filter(|l| with_pe || !l.level.needs_pe())
        .collect()
}

/// Rang d'une leçon dans le parcours (0 pour la première), et sa longueur.
/// `None` si la leçon n'en fait pas partie — un identifiant périmé, ou une
/// leçon Windows alors que le parcours Windows est éteint.
pub fn position(id: &str, with_pe: bool) -> Option<(usize, usize)> {
    let p = path(with_pe);
    let i = p.iter().position(|l| l.id == id)?;
    Some((i, p.len()))
}

/// Leçons qui précèdent et qui suivent celle-ci dans le parcours.
pub fn neighbours(id: &str, with_pe: bool) -> (Option<Lesson>, Option<Lesson>) {
    let p = path(with_pe);
    let Some(i) = p.iter().position(|l| l.id == id) else {
        return (None, None);
    };
    let prev = if i > 0 { p.get(i - 1).cloned() } else { None };
    (prev, p.get(i + 1).cloned())
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

    /// (terminées, total) sur tout le parcours, pour l'indicateur de
    /// progression : les tallies par niveau disent où l'on en est dans un
    /// chapitre, pas où l'on en est dans le livre.
    pub fn overall(&self, with_pe: bool) -> (usize, usize) {
        let l = path(with_pe);
        (l.iter().filter(|x| self.is_done(x.id)).count(), l.len())
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

    /// Jusqu'où le parcours est ouvert : le rang de la leçon terminée la plus
    /// avancée, plus un. Zéro quand rien n'est terminé — la première leçon est
    /// ouverte, et elle seule.
    ///
    /// Le front se lit sur la leçon la plus AVANCÉE, pas sur le nombre de
    /// leçons terminées : une progression trouée, faite du temps où l'on
    /// pouvait sauter une leçon, ne doit pas se refermer sur celui qui l'a. Qui
    /// avait atteint la vingtième la garde.
    pub fn reach(&self, with_pe: bool) -> usize {
        path(with_pe)
            .iter()
            .enumerate()
            .filter(|(_, l)| self.is_done(l.id))
            .map(|(i, _)| i + 1)
            .max()
            .unwrap_or(0)
    }

    /// Cette leçon est-elle ouverte ? On apprend l'assembleur dans l'ordre :
    /// une leçon s'ouvre quand celle qui la précède est validée, et pas avant.
    ///
    /// Une leçon hors parcours — identifiant périmé, leçon Windows alors que le
    /// parcours Windows est éteint — n'est pas verrouillée : il n'y a rien
    /// devant elle à valider, et la refuser serait un cul-de-sac.
    pub fn is_unlocked(&self, id: &str, with_pe: bool) -> bool {
        match position(id, with_pe) {
            Some((i, _)) => i <= self.reach(with_pe),
            None => true,
        }
    }
}

/// Sérialisation pour les réglages : identifiants séparés par des virgules.
/// Réciproque de [`Progress::parse`].
impl std::fmt::Display for Progress {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.done.join(","))
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
            (Level::Intermediate, 8),
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
    ///
    /// Chacun pour SA cible : un starter du parcours Windows passe par
    /// `nasm -f win64` et le lieur PE, sans quoi son `extern ExitProcess` ferait
    /// échouer un `ld` qui n'a jamais entendu parler de kernel32.
    #[test]
    fn every_starter_assembles() {
        use std::path::Path;
        std::fs::create_dir_all("build/tutorial").ok();
        for l in catalogue() {
            let Some(src) = l.starter else { continue };
            let path = format!("build/tutorial/{}.asm", l.id);
            std::fs::write(&path, src).expect("écriture");
            let out = crate::assemble::assemble_for(
                Path::new(&path),
                Path::new("build/tutorial"),
                &[],
                l.target(),
                Lang::Fr,
            );
            assert!(out.is_ok(), "{} ne s'assemble pas : {:?}", l.id, out.err());
        }
    }

    /// Le parcours Windows ne peut promettre que ce qu'il sait vérifier. Sans
    /// débogueur, une attente sur un registre ne serait jamais contrôlée : ces
    /// leçons ne portent donc que sur le code de sortie et sur le texte du
    /// programme. Le test l'impose, pour qu'aucune leçon future ne triche.
    #[test]
    fn windows_lessons_only_expect_what_can_be_checked() {
        for l in lessons_of(Level::Windows) {
            let src = l.starter.unwrap_or_else(|| panic!("{} : leçon Windows sans programme", l.id));
            let ex = crate::exercise::parse(src);
            assert!(ex.is_exercise(), "{} : aucune attente", l.id);
            for e in &ex.expectations {
                assert!(
                    matches!(e.subject, crate::exercise::Subject::ExitCode),
                    "{} : « {} » porte sur un registre, invérifiable sans débogueur",
                    l.id,
                    e.label()
                );
            }
            assert_eq!(l.target(), crate::assemble::Target::Windows);
        }
    }

    /// Aucun lien mort : chaque exercice cité par une leçon existe vraiment, et
    /// chaque leçon citée existe aussi. Un bouton « s'entraîner » qui ouvre le
    /// vide est pire que pas de bouton.
    #[test]
    fn every_practice_link_points_at_something_real() {
        for (lesson_id, files) in PRACTICE {
            assert!(find(lesson_id).is_some(), "leçon inconnue dans la table : {lesson_id}");
            for f in *files {
                let path = std::path::Path::new("examples_seed").join(f);
                assert!(path.exists(), "{lesson_id} renvoie à {f}, qui n'existe pas");
                // Et c'est bien un exercice : sans attentes, rien à corriger.
                let src = std::fs::read_to_string(&path).expect("lecture");
                assert!(
                    crate::exercise::parse(&src).is_exercise(),
                    "{f} est proposé comme exercice mais ne déclare aucune attente"
                );
            }
        }
    }

    /// Aucun exercice orphelin : tout ce qui est semé est rattaché à une leçon.
    ///
    /// C'est le contrôle qui tient la promesse du parcours. Trente-six exercices
    /// vivaient dans un dossier, sans lien avec les vingt-neuf leçons ; l'élève
    /// qui finissait une leçon n'apprenait pas qu'un exercice l'attendait, et
    /// celui qui ouvrait un exercice ne savait pas de quelle notion il relevait.
    #[test]
    fn no_seeded_exercise_is_left_out_of_the_path() {
        let mut orphans = Vec::new();
        for entry in std::fs::read_dir("examples_seed").expect("dossier des exemples").flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            // Les démonstrations (hello_world, boucle…) ne sont pas des
            // exercices : elles n'ont rien à corriger, donc rien à rattacher.
            if !name.starts_with("ex_") || !name.ends_with(".asm") {
                continue;
            }
            if lesson_of_exercise(&name).is_none() {
                orphans.push(name);
            }
        }
        orphans.sort();
        assert!(
            orphans.is_empty(),
            "ces exercices ne sont rattachés à aucune leçon : {orphans:?}"
        );
    }

    /// Le lien se lit dans les deux sens, et la réciproque est cohérente : un
    /// exercice renvoie à la leçon qui le propose.
    #[test]
    fn the_link_between_lesson_and_exercise_works_both_ways() {
        for (lesson_id, files) in PRACTICE {
            for f in *files {
                let back = lesson_of_exercise(f).expect("un exercice cité a sa leçon");
                assert_eq!(&back.id, lesson_id, "{f} : aller-retour incohérent");
            }
        }
        assert!(practice_for("inconnue").is_empty(), "leçon inconnue : aucune pratique");
        assert!(lesson_of_exercise("pas_un_exercice.asm").is_none());
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

    /// Le parcours s'ouvre au fur et à mesure : au départ la première leçon et
    /// elle seule, puis une de plus à chaque validation. C'est la règle qui
    /// donne son sens à l'ordre des leçons — la boucle après le saut, le saut
    /// après la comparaison.
    #[test]
    fn the_path_opens_one_lesson_at_a_time() {
        let p0 = path(false);
        let (first, second, third) = (p0[0].id, p0[1].id, p0[2].id);

        let mut p = Progress::default();
        assert_eq!(p.reach(false), 0, "rien de terminé : le front est à zéro");
        assert!(p.is_unlocked(first, false), "la première leçon est toujours ouverte");
        assert!(!p.is_unlocked(second, false), "la deuxième attend que la première soit validée");
        assert!(!p.is_unlocked(third, false));

        p.mark_done(first);
        assert!(p.is_unlocked(second, false), "validée : la suivante s'ouvre");
        assert!(!p.is_unlocked(third, false), "et une seule à la fois");

        // Revenir en arrière ne referme rien : une leçon déjà lue reste ouverte.
        p.mark_done(second);
        assert!(p.is_unlocked(first, false));
        assert!(p.is_unlocked(third, false));
    }

    /// Une progression trouée — héritée du temps où « Suivante » ne demandait
    /// rien — ne doit pas se refermer sur celui qui la porte : le front suit la
    /// leçon la plus avancée, pas le compte des leçons terminées.
    #[test]
    fn a_gapped_progress_keeps_what_it_reached() {
        let p0 = path(false);
        let far = p0[10].id;

        let mut p = Progress::default();
        p.mark_done(far);

        assert_eq!(p.reach(false), 11);
        assert!(p.is_unlocked(p0[11].id, false), "la suite de la plus avancée reste ouverte");
        assert!(p.is_unlocked(p0[3].id, false), "et tout ce qui la précède aussi");
        assert!(!p.is_unlocked(p0[12].id, false), "mais pas au-delà");
    }

    /// Chaque leçon dit à quoi sa notion sert et ce qu'il faut en retenir, dans
    /// les trois langues. Une leçon ajoutée sans cela repart d'un écran qui
    /// n'explique que le comment — c'est précisément ce que ce niveau de
    /// contenu est venu corriger.
    #[test]
    fn every_lesson_says_what_it_is_for_and_what_to_remember() {
        for l in catalogue() {
            let why = l.why.as_ref().unwrap_or_else(|| {
                panic!("{} : pas de « à quoi ça sert »", l.id)
            });
            assert!(
                l.takeaway.len() >= 2,
                "{} : au moins deux points à retenir attendus, {} trouvé(s)",
                l.id,
                l.takeaway.len()
            );
            for lang in [Lang::Fr, Lang::En, Lang::Es] {
                assert!(!why.get(lang).trim().is_empty(), "{} : « à quoi ça sert » vide en {lang:?}", l.id);
                for (i, p) in l.takeaway.iter().enumerate() {
                    assert!(!p.get(lang).trim().is_empty(), "{} : à retenir n° {i} vide en {lang:?}", l.id);
                }
            }
        }
    }

    /// Toute leçon qui porte un programme porte aussi de quoi le finir. Le
    /// parcours ne s'ouvre qu'en validant : sans indices, une leçon sur laquelle
    /// on bute n'est plus une gêne, c'est la fin du parcours.
    #[test]
    fn every_lesson_with_a_program_can_be_unstuck() {
        for l in catalogue().into_iter().filter(|l| l.has_starter()) {
            assert!(
                l.hints.len() >= 2,
                "{} : au moins deux indices attendus, {} trouvé(s)",
                l.id,
                l.hints.len()
            );
            for lang in [Lang::Fr, Lang::En, Lang::Es] {
                for (i, h) in l.hints.iter().enumerate() {
                    assert!(!h.get(lang).trim().is_empty(), "{} : indice n° {i} vide en {lang:?}", l.id);
                }
            }
        }
    }

    /// Une leçon hors parcours ne se verrouille pas : le parcours Windows
    /// éteint, ses leçons ne sont plus rangées nulle part, et les refuser
    /// enfermerait qui y arrive par un autre chemin.
    #[test]
    fn a_lesson_outside_the_path_is_never_locked() {
        let p = Progress::default();
        assert!(p.is_unlocked("identifiant-qui-n-existe-pas", false));

        let windows = catalogue()
            .into_iter()
            .find(|l| l.level.needs_pe())
            .expect("le catalogue porte des leçons Windows");
        assert!(!p.is_unlocked(windows.id, true), "dans le parcours : verrouillée comme les autres");
        assert!(p.is_unlocked(windows.id, false), "hors parcours : ouverte");
    }
}
