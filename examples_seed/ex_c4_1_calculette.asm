;@titre Calculette à la main (cours 4.1)
;@enonce Calcule (6 + 4) × 3 dans RBX, puis 50 ÷ 7 avec « div » : le quotient
;@enonce dans R14, le reste dans R15.
;@enonce « div » divise le couple RDX:RAX par son opérande, laisse le quotient
;@enonce dans RAX et le reste dans RDX — d'où le « xor rdx, rdx » obligatoire
;@enonce avant, sinon un RDX traînant fausserait tout.
;@attendu rbx == 30
;@attendu r14 == 7
;@attendu r15 == 1
;@attendu exit == 30

section .text
    global _start

_start:
    ; (6 + 4) * 3
    mov rbx, 6
    add rbx, 4
    ; TODO : multiplier RBX par 3  (« imul rbx, 3 »)

    ; 50 / 7
    mov rax, 50
    xor rdx, rdx                ; la moitié haute du dividende : zéro
    mov rcx, 7
    div rcx
    ; TODO : recopier le quotient dans R14 et le reste dans R15,
    ; TODO : avant que le « mov rax, 60 » de la sortie n'écrase RAX.

    mov rax, 60
    mov rdi, rbx                ; le code de sortie = (6+4)*3
    syscall
