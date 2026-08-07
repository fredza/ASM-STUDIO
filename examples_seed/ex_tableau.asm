;@titre Somme d'un tableau
;@enonce Additionne les cinq entiers du tableau et laisse le total — 150 —
;@enonce dans RBX. L'adressage « [t + rcx*8] » atteint l'élément d'indice RCX.
;@attendu rbx == 150
;@attendu exit == 0

section .data
    t dq 10, 20, 30, 40, 50
    n equ ($ - t) / 8   ; laisse l'assembleur compter : 5

section .text
    global _start

_start:
    xor rbx, rbx        ; somme
    xor rcx, rcx        ; indice

.boucle:
    ; TODO : ajouter l'élément courant à RBX  (« add rbx, [t + rcx*8] »)
    inc rcx
    cmp rcx, n
    jb .boucle          ; jb = below, non signé : un indice n'est jamais négatif

    mov rax, 60
    xor rdi, rdi
    syscall
