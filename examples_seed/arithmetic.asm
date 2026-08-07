; Arithmétique de base : add, sub, imul, idiv
; Résultat final dans rax (visible dans les registres du débogueur)

section .text
    global _start

_start:
    mov rax, 10         ; rax = 10
    add rax, 5          ; rax = 15
    sub rax, 3          ; rax = 12
    imul rax, 4         ; rax = 48

    ; division : rax / 6  =>  rax = quotient, rdx = reste
    xor rdx, rdx        ; vider rdx avant idiv (obligatoire)
    mov rcx, 6
    idiv rcx            ; rax = 8, rdx = 0

    ; exit(rax) — le code de sortie = résultat (8)
    mov rdi, rax
    mov rax, 60
    syscall
