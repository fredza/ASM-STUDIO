;@titre Le plus petit du tableau (cours 9.1)
;@enonce Ce parcours retient le plus GRAND élément. Retourne la comparaison pour
;@enonce qu'il retienne le plus petit.
;@enonce Le tableau contient 17, 3, 42, 8, 25 : RBX doit finir à 3.
;@attendu rbx == 3
;@attendu exit == 3

section .data
    t dq 17, 3, 42, 8, 25
    n equ ($ - t) / 8

section .text
    global _start

_start:
    xor rcx, rcx
    mov rbx, [t + rcx*8]        ; le champion provisoire : le premier élément
    inc rcx                     ; on compare à partir du deuxième

.boucle:
    cmp rcx, n
    jae .fait                   ; jae : non signé, un indice n'est jamais négatif

    mov rdx, [t + rcx*8]
    cmp rdx, rbx
    ; TODO : « jle » ignore les candidats plus petits — donc garde le plus grand.
    ; TODO : quel saut faut-il pour garder le plus petit ?
    jle .suivant
    mov rbx, rdx                ; nouveau champion

.suivant:
    inc rcx
    jmp .boucle

.fait:
    mov rax, 60
    mov rdi, rbx
    syscall
