;@titre Fonction avec une boucle (cours 8.2)
;@enonce Écris « somme_jusqua » : elle reçoit n dans RDI et renvoie dans RAX la
;@enonce somme de 1 à n, avec une boucle À L'INTÉRIEUR de la fonction.
;@enonce Appelle-la avec n = 10 : tu dois obtenir 55.
;@requis call
;@attendu r14 == 55
;@attendu exit == 55

section .text
    global _start

; somme_jusqua(RDI = n) → RAX
somme_jusqua:
    xor rax, rax                ; la somme
    mov rcx, rdi                ; le compteur part de n

.boucle:
    ; TODO : ajouter RCX à RAX
    dec rcx
    jnz .boucle                 ; jusqu'à ce que le compteur s'épuise
    ret

_start:
    mov rdi, 10
    ; TODO : appeler somme_jusqua
    mov r14, rax

    mov rax, 60
    mov rdi, r14
    syscall
