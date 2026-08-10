;@titre Empiler trois valeurs (cours 5.2)
;@enonce Empile 1, puis 2, puis 3 — dans cet ordre — puis dépile trois fois dans
;@enonce RBX, RCX et RDX. Dans quel ordre récupères-tu 1, 2 et 3 ?
;@enonce La pile est une pile d'assiettes : le dernier posé est le premier repris.
;@enonce RBX doit donc valoir 3, RCX 2 et RDX 1.
;@attendu rbx == 3
;@attendu rcx == 2
;@attendu rdx == 1
;@attendu exit == 0

section .text
    global _start

_start:
    push 1
    ; TODO : empiler 2, puis 3

    pop rbx
    ; TODO : dépiler dans RCX, puis dans RDX

    mov rax, 60
    xor rdi, rdi
    syscall
