;@titre Le plus grand des deux
;@enonce RSI vaut 42 et R8 vaut 17. Place le plus grand des deux dans RBX.
;@enonce Indice : « cmp » positionne les flags, puis un saut conditionnel choisit.
;@attendu rbx == 42
;@attendu exit == 0

section .text
    global _start

_start:
    mov rsi, 42
    mov r8, 17

    ; TODO : comparer RSI et R8, mettre le plus grand dans RBX.
    mov rbx, r8         ; faux : prend toujours R8

    mov rax, 60
    xor rdi, rdi
    syscall
