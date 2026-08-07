; Hello World — affiche "Hello, World!" sur stdout
; syscall write(1, msg, 13)  puis  exit(0)

section .data
    msg db "Hello, World!", 10   ; 10 = '\n'
    len equ $ - msg

section .text
    global _start

_start:
    ; write(1, msg, len)
    mov rax, 1          ; numéro syscall : write
    mov rdi, 1          ; fd = stdout
    mov rsi, msg        ; adresse du message
    mov rdx, len        ; longueur
    syscall

    ; exit(0)
    mov rax, 60         ; numéro syscall : exit
    xor rdi, rdi        ; code de retour = 0
    syscall
