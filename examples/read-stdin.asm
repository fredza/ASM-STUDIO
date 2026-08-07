; read-stdin.asm — lit une ligne sur l'entrée standard et la renvoie telle quelle.
;
; C'est le programme qui figeait l'IDE avant que l'attente d'un pas ne devienne
; non bloquante : `read` ne rend la main qu'une fois la saisie faite, et le
; débogueur restait suspendu dedans avec toute l'interface derrière lui.

section .bss
    buf resb 64                      ; tampon de réception

section .text
    global _start

_start:
    mov     rax, 0                   ; sys_read
    mov     rdi, 0                   ; fd = stdin
    mov     rsi, buf                 ; où déposer les octets lus
    mov     rdx, 64                  ; taille du tampon
    syscall                          ; rax = nombre d'octets lus

    mov     rdx, rax                 ; longueur à réécrire = ce qu'on a lu
    mov     rax, 1                   ; sys_write
    mov     rdi, 1                   ; fd = stdout
    mov     rsi, buf
    syscall

    mov     rax, 60                  ; sys_exit
    xor     rdi, rdi
    syscall
