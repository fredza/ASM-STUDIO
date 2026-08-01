;@titre Le dixième de Fibonacci
;@enonce La suite : 0, 1, 1, 2, 3, 5, 8… chaque terme est la somme des deux
;@enonce précédents. Complète la boucle pour laisser F(10) = 55 dans RBX.
;@attendu rbx == 55
;@attendu exit == 0

section .text
    global _start

_start:
    xor rax, rax        ; terme précédent : F(0) = 0
    mov rbx, 1          ; terme courant   : F(1) = 1
    mov rcx, 9          ; neuf pas pour aller de F(1) à F(10)

.boucle:
    ; TODO : avancer d'un terme.
    ;   RAX = RAX + RBX   (« add rax, rbx »)  puis échanger les deux
    ;   (« xchg rax, rbx ») pour que RBX porte toujours le terme courant.
    dec rcx
    jnz .boucle

    mov rax, 60
    xor rdi, rdi
    syscall
