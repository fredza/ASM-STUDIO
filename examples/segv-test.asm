section .text
    global _start
_start:
    xor rax, rax
    mov rbx, [rax]
    mov rax, 60
    xor rdi, rdi
    syscall
