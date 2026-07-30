section .rodata
    msg db "hello", 0
section .text
    global _start
_start:
    mov byte [msg], 'H'     ; écriture dans .rodata
    mov rax, 60
    xor rdi, rdi
    syscall
