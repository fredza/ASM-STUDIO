; Longueur de chaîne — compte les octets jusqu'au zéro terminal
; Démontre : scasb, repne, accès mémoire indirect

section .data
    texte db "Bonjour ASM Studio", 0

section .text
    global _start

_start:
    lea rdi, [texte]    ; pointeur vers le début
    xor al, al          ; octet cherché : 0 (fin de chaîne)
    mov rcx, -1         ; compteur maximal
    repne scasb         ; avance rdi jusqu'au 0
    ; rcx = -1 - longueur - 1  =>  longueur = -rcx - 2
    not rcx             ; rcx = longueur + 1
    dec rcx             ; rcx = longueur exacte (18)

    ; exit(longueur) — visible avec echo $?
    mov rdi, rcx
    mov rax, 60
    syscall
