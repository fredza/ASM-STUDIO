;@titre Puissance de deux
;@enonce Place 2^8 = 256 dans RBX, sans multiplication : un décalage à gauche
;@enonce (« shl ») multiplie par deux à chaque position décalée.
;@attendu rbx == 256
;@attendu exit == 0

section .text
    global _start

_start:
    mov rbx, 1

    ; TODO : décaler RBX de 8 positions vers la gauche
    shl rbx, 0          ; 0 position = aucun effet

    mov rax, 60
    xor rdi, rdi
    syscall
