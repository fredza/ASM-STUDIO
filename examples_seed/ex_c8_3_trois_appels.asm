;@titre Trois appels (cours 8.3)
;@enonce Une fonction « afficher » écrite une fois, appelée autant qu'on veut :
;@enonce c'est tout l'intérêt du chapitre. Ajoute le troisième message et son appel.
;@enonce La sortie se lit dans la boîte « Sortie du programme ».
;@enonce R14 doit finir à 3.
;@attendu r14 == 3
;@attendu exit == 0

section .data
    m1     db "Premier message.", 10
    m1_len equ $ - m1
    m2     db "Deuxieme message.", 10
    m2_len equ $ - m2
    m3     db "Troisieme message.", 10
    m3_len equ $ - m3

section .text
    global _start

; afficher(RDI = adresse, RSI = longueur) — et compte l'appel dans R14.
afficher:
    mov rdx, rsi                ; longueur
    mov rsi, rdi                ; adresse
    mov rax, 1                  ; write
    mov rdi, 1                  ; stdout
    syscall
    inc r14
    ret

_start:
    xor r14, r14

    mov rdi, m1
    mov rsi, m1_len
    call afficher

    mov rdi, m2
    mov rsi, m2_len
    call afficher

    ; TODO : le troisième appel, avec m3 et m3_len

    mov rax, 60
    xor rdi, rdi
    syscall
