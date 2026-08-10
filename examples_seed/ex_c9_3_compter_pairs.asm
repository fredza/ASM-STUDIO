;@titre Compter les nombres pairs (cours 9.3)
;@enonce Reprends le « test rax, 1 » du chapitre 6 et compte combien d'éléments du
;@enonce tableau sont pairs.
;@enonce Le tableau contient 17, 3, 42, 8, 25 : deux pairs (42 et 8), donc RBX = 2.
;@requis test
;@attendu rbx == 2
;@attendu exit == 2

section .data
    t dq 17, 3, 42, 8, 25
    n equ ($ - t) / 8

section .text
    global _start

_start:
    xor rbx, rbx                ; le compte
    xor rcx, rcx                ; l'indice

.boucle:
    cmp rcx, n
    jae .fait

    mov rdx, [t + rcx*8]
    ; TODO : tester le bit du bas de RDX  (« test rdx, 1 »)
    ; TODO : sauter à .suivant s'il vaut 1 (nombre impair)
    ; TODO : sinon compter ce pair dans RBX

.suivant:
    inc rcx
    jmp .boucle

.fait:
    mov rax, 60
    mov rdi, rbx
    syscall
