; test.asm — exemple pour ASM Studio (M1)
; Assemble : nasm -f elf64 test.asm -o test.o ; ld -o test test.o

section .data
    msg db "Bonjour de la part de frédéric ASM", 10           ; 10 = \n

section .text
    global _start

_start:
    mov     rax, 5                   ; rax = 5
    push    rax                      ; empile 5
    mov     rbx, 8                   ; rbx = 8
    cmp     rax, rbx                 ; compare rax et rbx -> pose les flags (ZF/CF/SF/OF)
    pop     rcx                      ; rcx = 5 (dépile)

    mov     rax, 1                   ; sys_write
    mov     rdi, 1                   ; fd = stdout
    mov     rsi, msg                 ; buffer
    mov     rdx, 10                  ; longueur
    syscall                          ; écrit "Hello ASM\n"

    mov     rax, 60                  ; sys_exit
    xor     rdi, rdi                 ; code de sortie = 0
    syscall
