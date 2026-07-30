section .text
    global _start
_start:
    call f
    mov rax, 60
    xor rdi, rdi
    syscall
f:
    push rax                ; push sans pop -> ret saute dans le vide
    ret
