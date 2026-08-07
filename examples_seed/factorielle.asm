; Factorielle itérative — calcule 10! = 3628800
; Démontre : imul, boucle, registres

section .text
    global _start

_start:
    mov rax, 1          ; accumulateur
    mov rcx, 10         ; n = 10

.boucle:
    imul rax, rcx       ; rax *= rcx
    dec rcx
    jnz .boucle         ; jusqu'à rcx == 0

    ; rax = 3628800 (10!)
    ; exit code tronqué à 8 bits (3628800 % 256 = 0) — observer rax dans le débogueur
    mov rdi, rax
    and rdi, 0xFF
    mov rax, 60
    syscall
