;@titre Copier des registres (cours 3.2)
;@enonce Mets 5 dans RBX, copie RBX dans RCX, puis change RBX à 99.
;@enonce Que vaut RCX, et pourquoi ? « mov » COPIE la valeur : les deux registres
;@enonce sont indépendants après coup, changer l'un ne touche pas l'autre.
;@enonce Renvoie RCX comme code de sortie.
;@attendu rbx == 99
;@attendu rcx == 5
;@attendu exit == 5

section .text
    global _start

_start:
    mov rbx, 5
    ; TODO : copier RBX dans RCX
    mov rbx, 99                 ; RCX bouge-t-il ?

    mov rax, 60
    ; TODO : mettre RCX dans RDI (le code de sortie)
    mov rdi, 0
    syscall
