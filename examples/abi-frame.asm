section .text
    global _start
_start:
    mov rdi, 7
    call f
    mov rax, 60
    xor rdi, rdi
    syscall
f:
    push rbp
    mov rbp, rsp
    sub rsp, 16
    mov qword [rbp-8], 0x1234
    leave
    ret
