; Fibonacci — calcule les 12 premiers termes
; Résultat final (F12 = 144) dans rax, visible dans le débogueur

section .text
    global _start

_start:
    xor rax, rax        ; F(0) = 0
    mov rbx, 1          ; F(1) = 1
    mov rcx, 10         ; 10 itérations supplémentaires (→ F12)

.suivant:
    add rax, rbx        ; rax = rax + rbx
    xchg rax, rbx       ; swap : rbx = nouveau terme, rax = ancien
    dec rcx
    jnz .suivant

    ; rax contient F(12) = 144 — exit(rax & 0xFF) pour le voir avec echo $?
    mov rdi, rax
    and rdi, 0xFF
    mov rax, 60
    syscall
