; hello_asmstd.asm — utilise la bibliothèque asmstd (aucun numéro de syscall !).
; Activez « Bibliothèque asmstd » dans Réglages, ou gardez asmstd.inc à côté.

%include "asmstd.inc"

section .data
    msg db "Hello via asmstd!", 10
    len equ $ - msg

section .text
    global _start

_start:
    mov     rdi, 1          ; fd = stdout
    mov     rsi, msg
    mov     rdx, len
    call    asm.write       ; au lieu de : mov rax,1 ; syscall

    xor     rdi, rdi        ; code de sortie 0
    call    asm.exit        ; au lieu de : mov rax,60 ; syscall
