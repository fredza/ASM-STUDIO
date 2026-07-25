; heap.asm — démonstration du tas (heap) via l'appel système brk.
; Ouvrez ce fichier, Lancez, avancez pas à pas : l'onglet « Tas » se remplit
; dès que le second brk a agrandi le segment [heap].

section .text
    global _start

_start:
    ; brk(0) : récupère l'adresse courante de fin du tas dans rax
    mov     rax, 12          ; sys_brk
    xor     rdi, rdi
    syscall

    ; brk(fin + 4096) : agrandit le tas de 4 Kio -> le segment [heap] apparaît
    mov     rdi, rax
    add     rdi, 4096
    mov     rax, 12          ; sys_brk
    syscall

    ; exit(0)
    mov     rax, 60
    xor     rdi, rdi
    syscall
