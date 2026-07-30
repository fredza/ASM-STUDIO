;@titre Le code de sortie
;@enonce Fais en sorte que le programme se termine avec le code 7.
;@enonce Indice : pour sys_exit (RAX = 60), le code de sortie se met dans RDI.
;@attendu exit == 7

section .text
    global _start

_start:
    mov rax, 60         ; sys_exit
    xor rdi, rdi        ; TODO : mettre 7 ici au lieu de 0
    syscall
