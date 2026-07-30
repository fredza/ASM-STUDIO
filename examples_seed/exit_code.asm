; Code de sortie — illustre le syscall exit avec un code non nul
; Le shell affiche le code via :  echo $?

section .text
    global _start

_start:
    mov rax, 60         ; syscall : exit
    mov rdi, 42         ; code de sortie = 42
    syscall
