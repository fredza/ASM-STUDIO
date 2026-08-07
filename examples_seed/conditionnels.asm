; Conditionnels — compare deux valeurs, affiche ">" "<" ou "="
; Démontre : cmp, je, jl, jg

section .data
    msg_gt  db ">", 10
    msg_lt  db "<", 10
    msg_eq  db "=", 10

section .text
    global _start

_start:
    mov rax, 7          ; première valeur
    mov rbx, 5          ; deuxième valeur
    cmp rax, rbx        ; compare rax et rbx

    je  .egal
    jl  .inferieur

.superieur:
    mov rsi, msg_gt
    jmp .afficher

.inferieur:
    mov rsi, msg_lt
    jmp .afficher

.egal:
    mov rsi, msg_eq

.afficher:
    mov rax, 1          ; write
    mov rdi, 1          ; stdout
    mov rdx, 2          ; longueur (char + newline)
    syscall

    mov rax, 60         ; exit(0)
    xor rdi, rdi
    syscall
