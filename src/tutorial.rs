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
//  Les niveaux Débutant et Intermédiaire sont écrits intégralement :
//  programme de départ, étapes, panneaux et attentes vérifiables. Avancé
//  et Expert déclarent leur plan — titre et objectif — et se compléteront
//  de la même façon. Annoncer un parcours vide serait pire que de le
//  montrer en construction.
//
//  Un programme de départ doit ÉCHOUER tel quel et PASSER une fois son
//  TODO appliqué : les deux moitiés sont vérifiées par le test
//  `every_intermediate_lesson_fails_then_passes`, qui applique à la lettre
//  la correction que le commentaire dicte.

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
        planned("elf", Level::Advanced,
            t!("Le format ELF", "The ELF format", "El formato ELF"),
            t!("Reconnaître les en-têtes et les sections d'un exécutable Linux.",
               "Recognise the headers and sections of a Linux executable.",
               "Reconocer las cabeceras y secciones de un ejecutable Linux."),
            &["editor", "memmap"]),
        planned("linking", Level::Advanced,
            t!("Édition de liens", "Linking", "Enlazado"),
            t!("Comprendre ce que ld assemble entre le fichier objet et l'exécutable.",
               "Understand what ld puts together between object file and executable.",
               "Entender qué une ld entre el archivo objeto y el ejecutable."),
            &["editor", "console"]),
        planned("plt_got", Level::Advanced,
            t!("PLT et GOT", "PLT and GOT", "PLT y GOT"),
            t!("Suivre un appel de bibliothèque partagée à travers ses tables d'indirection.",
               "Follow a shared-library call through its indirection tables.",
               "Seguir una llamada a biblioteca compartida por sus tablas de indirección."),
            &["disasm", "memmap"]),
        planned("relocations", Level::Advanced,
            t!("Relocations", "Relocations", "Relocalizaciones"),
            t!("Voir quelles adresses sont corrigées au chargement, et pourquoi.",
               "See which addresses are fixed up at load time, and why.",
               "Ver qué direcciones se corrigen al cargar, y por qué."),
            &["disasm", "memory"]),
        planned("simd", Level::Advanced,
            t!("SIMD et AVX", "SIMD and AVX", "SIMD y AVX"),
            t!("Traiter plusieurs valeurs par instruction avec les registres vectoriels.",
               "Process several values per instruction with vector registers.",
               "Procesar varios valores por instrucción con registros vectoriales."),
            &["editor", "registers", "instruction"]),
        planned("optimisation", Level::Advanced,
            t!("Optimisations", "Optimisations", "Optimizaciones"),
            t!("Mesurer avant de récrire : coût des instructions et prédiction de branchement.",
               "Measure before rewriting: instruction cost and branch prediction.",
               "Medir antes de reescribir: coste de instrucciones y predicción de saltos."),
            &["disasm", "instruction", "timeline"]),
        // ---------------- Expert ----------------
        planned("reverse", Level::Expert,
            t!("Rétro-ingénierie", "Reverse engineering", "Ingeniería inversa"),
            t!("Reconstituer l'intention d'un binaire dont on n'a pas les sources.",
               "Reconstruct the intent of a binary you have no source for.",
               "Reconstruir la intención de un binario sin fuentes."),
            &["disasm", "memmap", "callstack"]),
        planned("desassemblage", Level::Expert,
            t!("Désassemblage", "Disassembly", "Desensamblado"),
            t!("Lire le code machine sans étiquettes, et retrouver les frontières d'instruction.",
               "Read machine code without labels, and find instruction boundaries.",
               "Leer código máquina sin etiquetas y hallar los límites de instrucción."),
            &["disasm", "instruction"]),
        planned("syscalls_avances", Level::Expert,
            t!("Appels système avancés", "Advanced system calls", "Llamadas al sistema avanzadas"),
            t!("Processus, signaux, mémoire partagée : au-delà de read et write.",
               "Processes, signals, shared memory: beyond read and write.",
               "Procesos, señales, memoria compartida: más allá de read y write."),
            &["editor", "syscalls"]),
        planned("shellcode", Level::Expert,
            t!("Shellcode", "Shellcode", "Shellcode"),
            t!("Écrire du code sans octet nul ni adresse absolue, et comprendre les contraintes.",
               "Write code with no null byte and no absolute address, and understand the constraints.",
               "Escribir código sin bytes nulos ni direcciones absolutas, y entender las restricciones."),
            &["editor", "disasm", "memmap"]),
        planned("exploitation", Level::Expert,
            t!("Exploitation de binaires", "Binary exploitation", "Explotación de binarios"),
            t!("Comprendre comment un débordement de pile détourne l'adresse de retour — et ce qui l'en empêche.",
               "Understand how a stack overflow hijacks the return address — and what prevents it.",
               "Entender cómo un desbordamiento de pila secuestra la dirección de retorno — y qué lo impide."),
            &["stack", "callstack", "memmap"]),
        planned("performance", Level::Expert,
            t!("Analyse de performances", "Performance analysis", "Análisis de rendimiento"),
            t!("Compter les cycles, repérer les dépendances, et distinguer le coût réel du coût supposé.",
               "Count cycles, spot dependencies, and tell real cost from assumed cost.",
               "Contar ciclos, detectar dependencias y distinguir coste real de coste supuesto."),
            &["disasm", "instruction", "timeline"]),
    ]
}

/// Leçon annoncée mais dont le contenu reste à écrire.
///
/// Elle apparaît dans le parcours avec son objectif : l'élève voit où il va.
/// Masquer les leçons à venir donnerait l'illusion d'un parcours plus court
/// qu'il ne l'est.
fn planned(
    id: &'static str,
    level: Level,
    title: Text,
    goal: Text,
    panels: &'static [PanelKey],
) -> Lesson {
    Lesson {
        id,
        level,
        title,
        goal,
        steps: Vec::new(),
        panels: panels.to_vec(),
        starter: None,
    }
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

    /// Le niveau Intermédiaire est écrit lui aussi : sept leçons, toutes
    /// pratiques — aucune n'est purement explicative à ce palier.
    #[test]
    fn the_intermediate_level_is_fully_written() {
        let m = lessons_of(Level::Intermediate);
        assert_eq!(m.len(), 7, "sept leçons intermédiaires attendues, vu {}", m.len());
        for l in &m {
            assert!(l.steps.len() >= 3, "{} : moins de trois étapes", l.id);
            assert!(l.has_starter(), "{} n'a pas de programme de départ", l.id);
            assert!(!l.panels.is_empty(), "{} n'ouvre aucun panneau", l.id);
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
