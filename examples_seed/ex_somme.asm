;@titre Somme de 1 à 10
;@enonce Complète la boucle pour que RBX contienne 1+2+…+10 = 55.
;@enonce Ne modifie pas RBX après la boucle : c'est lui qui est vérifié.
;@attendu rbx == 55
;@attendu exit == 0

section .text
    global _start

_start:
    xor rbx, rbx        ; accumulateur
    mov rcx, 10         ; compteur : 10, 9, 8 … 1

.boucle:
    ; TODO : ajouter rcx à rbx
    dec rcx
    jnz .boucle

    mov rax, 60
    xor rdi, rdi
    syscall
