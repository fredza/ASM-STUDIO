;@titre Moyenne d'un tableau
;@enonce La somme des cinq entiers est déjà calculée dans RBX. Divise-la par
;@enonce leur nombre (n = 5) pour laisser la moyenne — 30 — dans RBX.
;@enonce « div » divise RDX:RAX par son opérande ; pense à mettre RDX à 0.
;@attendu rbx == 30
;@attendu exit == 0

section .data
    t dq 10, 20, 30, 40, 50
    n equ ($ - t) / 8

section .text
    global _start

_start:
    xor rbx, rbx
    xor rcx, rcx
.somme:
    add rbx, [t + rcx*8]
    inc rcx
    cmp rcx, n
    jb .somme
    ; RBX = 150 ici.

    mov rax, rbx        ; le dividende va dans RAX
    xor rdx, rdx        ; RDX = 0 : sinon « div » lit RDX:RAX comme un nombre 128 bits
    mov r8, n           ; le diviseur (« div » n'accepte pas d'immédiat)
    ; TODO : diviser par R8  (« div r8 ») ; le quotient revient dans RAX
    mov rbx, rax        ; la moyenne

    mov rax, 60
    xor rdi, rdi
    syscall
