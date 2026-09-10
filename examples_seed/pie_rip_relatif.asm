; PIE et adressage RIP-relatif — un programme qui se moque de son adresse
; Démontre : default rel, lea reg, [rel étiquette], lien « ld -pie »
;
; À cocher avant d'assembler : Exécution ▸ Lien position-indépendant (-pie).
; Le binaire produit est alors de type DYN, et le noyau le charge où il veut.
;
; La règle tient en une phrase : plus aucune adresse écrite en dur. Une étiquette
; ne vaut plus un nombre connu à l'avance, seulement une distance depuis
; l'instruction en cours — c'est ce que « default rel » et « lea [rel … ] »
; calculent pour nous, et que le processeur additionne à RIP à l'exécution.

default rel                     ; [étiquette] veut désormais dire [rel étiquette]

section .data
    message  db "Ce programme tourne a n'importe quelle adresse.", 10
    long_msg equ $ - message
    compteur dq 3               ; en .data : lu et écrit par adresse relative

section .text
    global _start

_start:
    ; lea + [rel …] : « adresse de message » calculée depuis RIP.
    ; Écrire « mov rsi, message » demanderait l'adresse absolue, que le lieur
    ; refuse net en -pie (relocation R_X86_64_32S, « recompile with -fPIE »).
    lea rsi, [rel message]

.repeter:
    ; write(1, message, long_msg) — rsi porte déjà l'adresse calculée
    mov rax, 1
    mov rdi, 1
    mov rdx, long_msg
    push rsi                    ; le syscall abîme rcx et r11, pas rsi ; on le
    syscall                     ;   garde tout de même, pour l'habitude
    pop rsi

    ; Lire ET écrire une donnée : [rel compteur] des deux côtés.
    dec qword [rel compteur]
    jnz .repeter

    ; exit(0)
    mov rax, 60
    xor rdi, rdi
    syscall
