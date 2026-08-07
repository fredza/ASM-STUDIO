section .text
    global _start
_start:
    mov rax, 10
    xor rdx, rdx
    xor rcx, rcx
    div rcx                 ; division par zéro
    mov rax, 60
    xor rdi, rdi
    syscall
