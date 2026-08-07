;@titre Longueur d'une chaîne
;@enonce Compte les caractères de la chaîne, sans le zéro final, dans RBX.
;@enonce Une chaîne C se termine au premier octet nul.
;@attendu rbx == 7
;@attendu exit == 0

section .data
    texte db "Bonjour", 0   ; 7 lettres, puis le zéro qui termine

section .text
    global _start

_start:
    mov rsi, texte
    xor rbx, rbx            ; longueur en construction

.boucle:
    mov al, [rsi + rbx]     ; UN octet
    ; TODO : si AL vaut 0, la chaîne est finie : sauter à .fin
    ;        (« test al, al » puis « jz .fin »)
    inc rbx
    cmp rbx, 32             ; garde-fou : sans le test ci-dessus, on partirait
    jb .boucle              ; au loin dans la mémoire

.fin:
    mov rax, 60
    xor rdi, rdi
    syscall
