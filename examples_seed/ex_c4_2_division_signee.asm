;@titre Division signée (cours 4.2)
;@enonce Calcule à la main le quotient et le reste de -20 ÷ 3, en arrondissant
;@enonce vers zéro, puis écris le programme avec « idiv » et vérifie ta réponse.
;@enonce « idiv » est la version signée de « div » ; il faut étendre le signe de
;@enonce RAX dans RDX avec « cqo » — pas « xor rdx, rdx », qui donnerait un
;@enonce énorme nombre positif au lieu de -20.
;@requis idiv
;@attendu r14 == -6
;@attendu r15 == -2
;@attendu exit == 0

section .text
    global _start

_start:
    mov rax, -20
    ; TODO : étendre le signe de RAX dans RDX  (« cqo »)
    mov rcx, 3
    ; TODO : diviser  (« idiv rcx »)

    mov r14, rax                ; quotient
    mov r15, rdx                ; reste

    mov rax, 60
    xor rdi, rdi
    syscall
