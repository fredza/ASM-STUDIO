;@titre Table de multiplication (cours 7.3)
;@enonce Calcule 7 × 6 sans « imul » : uniquement avec une boucle qui additionne
;@enonce 7 à lui-même six fois. C'est exactement ce que la multiplication veut
;@enonce dire — le processeur, lui, sait le faire d'un coup.
;@enonce RBX doit valoir 42.
;@interdit imul
;@interdit mul
;@attendu rbx == 42
;@attendu exit == 42

section .text
    global _start

_start:
    xor rbx, rbx                ; le produit en construction
    mov rcx, 6                  ; six additions

.boucle:
    ; TODO : ajouter 7 à RBX
    dec rcx
    jnz .boucle

    mov rax, 60
    mov rdi, rbx
    syscall
