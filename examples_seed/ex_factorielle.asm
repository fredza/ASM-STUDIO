;@titre Factorielle de 5
;@enonce Complète la boucle pour que RBX contienne 5! = 5×4×3×2×1 = 120.
;@enonce Ne modifie pas RBX après la boucle : c'est lui qui est vérifié.
;@attendu rbx == 120
;@attendu exit == 0

section .text
    global _start

_start:
    mov rbx, 1          ; produit accumulé
    mov rcx, 5          ; 5, 4, 3, 2, 1

.boucle:
    ; TODO : multiplier RBX par RCX  (« imul rbx, rcx »)
    dec rcx
    jnz .boucle

    mov rax, 60
    xor rdi, rdi
    syscall
